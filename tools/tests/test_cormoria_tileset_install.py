"""The installed Cormoria tileset inputs must remain pinned and isolated."""

from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path

from tools.cormoria import install_tilesets, materialize_tileset_preview


class CormoriaTilesetInstallTests(unittest.TestCase):
    def test_installed_tree_is_self_contained_and_hash_bound(self) -> None:
        self.assertEqual(install_tilesets.check_installed(), 1058)

    def test_namespaced_rules_preserve_donor_tile_count(self) -> None:
        root = install_tilesets.ROOT
        plan = json.loads((root / install_tilesets.PLAN).read_text(encoding="utf-8"))
        rules = (root / install_tilesets.RULES).read_text(encoding="utf-8")
        self.assertEqual(rules.count("-Wnum_tiles"), 28)
        self.assertIn("data/tilesets/cormoria/secondary/shop/tiles.4bpp: %.4bpp: %.png\n"
                      "\t$(GFX) $< $@ -num_tiles 502 -Wnum_tiles", rules)
        self.assertEqual(rules.encode(), install_tilesets.rules_from_plan(plan))

    def test_source_drift_is_rejected_by_local_check(self) -> None:
        source_root = install_tilesets.ROOT
        with tempfile.TemporaryDirectory() as directory:
            target_root = Path(directory)
            for relative in (install_tilesets.PLAN, install_tilesets.HEADER, install_tilesets.RULES):
                target = target_root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((source_root / relative).read_bytes())
            plan = json.loads((target_root / install_tilesets.PLAN).read_text())
            first = plan["recipes"][0]
            relative = Path(materialize_tileset_preview.source_target(first))
            target = target_root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(b"drift")
            with self.assertRaisesRegex(install_tilesets.InstallError, "installed source hash mismatch"):
                install_tilesets.check_installed(target_root)

    @unittest.skipUnless(os.environ.get("CORMORIA_STAGE") and os.environ.get("CORMORIA_GENERAL_FOUNDATION")
                         and os.environ.get("CORMORIA_DONOR_RULES") and os.environ.get("CORMORIA_DONOR_ANIMS"),
                         "external authenticated donor inputs unavailable")
    def test_installer_refuses_differing_existing_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan = json.loads((install_tilesets.ROOT / install_tilesets.PLAN).read_text())
            relative = Path(materialize_tileset_preview.source_target(plan["recipes"][0]))
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(b"do not overwrite")
            with self.assertRaisesRegex(install_tilesets.InstallError, "refusing to overwrite"):
                install_tilesets.install(
                    Path(os.environ["CORMORIA_STAGE"]), Path(os.environ["CORMORIA_GENERAL_FOUNDATION"]),
                    Path(os.environ["CORMORIA_DONOR_RULES"]), Path(os.environ["CORMORIA_DONOR_ANIMS"]),
                    output_root=root)
            self.assertEqual(target.read_bytes(), b"do not overwrite")
            self.assertFalse((root / install_tilesets.HEADER).exists())


if __name__ == "__main__":
    unittest.main()
