"""Guard the supplemental General art against substitutions and source drift."""

from __future__ import annotations

import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.cormoria import stage_general_foundation as foundation


class GeneralFoundationTests(unittest.TestCase):
    def test_manifest_names_exact_foundation_inputs(self) -> None:
        manifest = json.loads(foundation.MANIFEST.read_text(encoding="utf-8"))
        self.assertEqual(manifest["provenance"], {"repository": foundation.REPOSITORY,
                         "revision": foundation.REVISION, "tree": foundation.TREE})
        self.assertEqual([entry["path"] for entry in manifest["files"]], list(foundation.PATHS))
        self.assertEqual(len(manifest["files"]), 18)
        self.assertTrue(all(len(entry["git_blob"]) == 40 and len(entry["sha256"]) == 64
                            for entry in manifest["files"]))

    def test_mismatched_inventory_fails_before_writing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            donor = root / "donor"
            donor.mkdir()
            output = root / "output"
            with patch.object(foundation, "expected_manifest", return_value={"files": []}):
                with self.assertRaisesRegex(foundation.FoundationError, "committed General inventory"):
                    foundation.stage(donor, output)
            self.assertFalse(output.exists())

    def test_wrong_git_revision_is_rejected(self) -> None:
        with patch.object(foundation, "git", return_value=b"not-the-pinned-revision\n"):
            with self.assertRaisesRegex(foundation.FoundationError, "HEAD differs"):
                foundation.object_inventory(Path("donor"))

    def test_output_inside_repository_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            donor = Path(directory) / "donor"
            donor.mkdir()
            output = foundation.ROOT / "unwanted-general-stage"
            with self.assertRaisesRegex(foundation.FoundationError, "outside the repository"):
                foundation.stage(donor, output)
            self.assertFalse(output.exists())

    @unittest.skipUnless(os.environ.get("CORMORIA_DONOR_GIT"), "pinned donor Git checkout unavailable")
    def test_git_objects_match_archive_and_stage(self) -> None:
        donor = Path(os.environ["CORMORIA_DONOR_GIT"])
        archive_value = os.environ.get("CORMORIA_DONOR_ARCHIVE")
        archive = Path(archive_value) if archive_value else None
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "supplement"
            self.assertEqual(foundation.stage(donor, output, archive), 18)
            self.assertEqual((output / "source/src/graphics.c").stat().st_size,
                             next(item["bytes"] for item in json.loads(foundation.MANIFEST.read_text())
                                  ["files"] if item["path"] == "src/graphics.c"))
            self.assertTrue((output / "source/data/tilesets/primary/general/tiles.png").is_file())
            self.assertFalse((output / "data/tilesets/primary/general/tiles.png").exists())
            with self.assertRaisesRegex(foundation.FoundationError, "output must be fresh"):
                foundation.stage(donor, output, archive)

            if archive is not None:
                altered = Path(directory) / "altered-archive"
                for relative in foundation.PATHS:
                    destination = altered / relative
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(archive / relative, destination)
                palette = altered / foundation.PATHS[2]
                palette.write_bytes(palette.read_bytes() + b"altered")
                with self.assertRaisesRegex(foundation.FoundationError, "archive differs"):
                    foundation.stage(donor, Path(directory) / "rejected", altered)
                self.assertFalse((Path(directory) / "rejected").exists())


if __name__ == "__main__":
    unittest.main()
