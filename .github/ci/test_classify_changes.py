#!/usr/bin/env python3
"""Regression checks for CI selection, including real Git rename/deletion diffs."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from classify_changes import changed_paths, classify_paths


SCRIPT = Path(__file__).with_name("classify_changes.py").resolve()


class ClassifyPathsTests(unittest.TestCase):
    def test_documentation_only(self):
        self.assertEqual(
            classify_paths(["docs/tutorials/example.md", "README.md", "LICENSE", "LICENSE.md"]),
            {"rom": False, "rust": False, "installer": False},
        )

    def test_rust_inputs(self):
        for path in ("coop/crates/app/src/lib.rs", "Cargo.toml", "Cargo.lock", "android/app/build.gradle", "deploy/coop/Dockerfile"):
            with self.subTest(path=path):
                self.assertEqual(classify_paths([path]), {"rom": False, "rust": True, "installer": False})

    def test_mixed_installer_and_rust_changes(self):
        self.assertEqual(
            classify_paths(["docs/guide.md", "coop/crates/app/src/lib.rs", "installer/windows/Package.wxs"]),
            {"rom": False, "rust": True, "installer": True},
        )

    def test_unknown_or_build_inputs_require_every_check(self):
        for path in ("AGENTS.md", "CONTRIBUTING.md", "src/battle.c", "Makefile", ".github/workflows/build.yml", "tools/tool.py", "new-area/file.txt"):
            with self.subTest(path=path):
                self.assertEqual(classify_paths(["docs/guide.md", path]), {"rom": True, "rust": True, "installer": True})


class GitDiffTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.repo = Path(self.directory.name)
        self.git("init", "--quiet")
        self.git("config", "user.email", "ci-classifier@example.invalid")
        self.git("config", "user.name", "CI classifier tests")
        (self.repo / "docs").mkdir()
        (self.repo / "src").mkdir()
        (self.repo / "docs" / "renamed file.txt").write_text("rename this file\n", encoding="utf-8")
        (self.repo / "src" / "deleted.c").write_text("delete this file\n", encoding="utf-8")
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "fixture base")
        self.base = self.git("rev-parse", "HEAD")

    def git(self, *args):
        return subprocess.run(
            ["git", *args], cwd=self.repo, check=True, capture_output=True, text=True
        ).stdout.strip()

    def cli(self, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args], cwd=self.repo, capture_output=True, text=True
        )

    def test_deletion_and_both_rename_paths_are_classified(self):
        self.git("mv", "docs/renamed file.txt", "src/renamed file.txt")
        self.git("rm", "src/deleted.c")
        self.git("commit", "--quiet", "-m", "fixture changes")
        paths = changed_paths(self.base, cwd=self.repo)
        self.assertCountEqual(paths, ["docs/renamed file.txt", "src/renamed file.txt", "src/deleted.c"])
        self.assertEqual(classify_paths(paths), {"rom": True, "rust": True, "installer": True})
        result = self.cli("--base", self.base)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "rom=true\nrust=true\ninstaller=true\n")

    def test_explicit_head_and_documentation_only_diff(self):
        (self.repo / "docs" / "guide.md").write_text("new guide\n", encoding="utf-8")
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "fixture documentation")
        result = self.cli("--base", self.base, "--head", self.git("rev-parse", "HEAD"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "rom=false\nrust=false\ninstaller=false\n")

    def test_unavailable_base_fails_without_selection_outputs(self):
        result = self.cli("--base", "0" * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn("Cannot classify changes", result.stderr)

    def test_invalid_revision_fails_without_selection_outputs(self):
        result = self.cli("--base", "--help")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")

    def test_all_does_not_need_base(self):
        result = self.cli("--all")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "rom=true\nrust=true\ninstaller=true\n")


if __name__ == "__main__":
    unittest.main()
