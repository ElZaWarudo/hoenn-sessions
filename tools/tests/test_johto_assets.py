"""Selected scenery conversion and filesystem-boundary regression tests."""

import copy
import json
import os
import struct
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.johto import import_region_assets as assets


DONOR = Path(os.environ["JOHTO_DONOR"]) if os.environ.get("JOHTO_DONOR") else None


class AssetFormatTests(unittest.TestCase):
    def test_strict_converter_rejects_unknown_reserved_and_unsupported_values(self):
        mapping = {0: {"target_value": 0}, 0xA1: {"target_value": 0xF0}}
        self.assertEqual(assets.convert_attributes(struct.pack("<HH", 0, 0x20A1), mapping),
                         struct.pack("<II", 0, 0x400000F0))
        for raw in (b"\x00", struct.pack("<H", 0x0100), struct.pack("<H", 0x3000),
                    struct.pack("<H", 0x4000), struct.pack("<H", 0x00FE)):
            with self.subTest(raw=raw), self.assertRaises(assets.AssetError):
                assets.convert_attributes(raw, mapping)

    def test_layout_rejects_bad_geometry_border_and_tile_references(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            layout = {"id": "TEST", "width": 1, "height": 1,
                      "blockdata_filepath": "map.bin", "border_filepath": "border.bin"}
            (root / "map.bin").write_bytes(struct.pack("<H", 640))
            (root / "border.bin").write_bytes(b"\x00" * 8)
            assets.validate_layout_assets(root, layout, 640, 1)
            with self.assertRaisesRegex(assets.AssetError, "bounds"):
                assets.validate_layout_assets(root, layout, 640, 0)
            with self.assertRaisesRegex(assets.AssetError, "geometry"):
                assets.validate_layout_assets(root, dict(layout, width=2), 640, 1)
            (root / "border.bin").write_bytes(b"\x00" * 6)
            with self.assertRaisesRegex(assets.AssetError, "border"):
                assets.validate_layout_assets(root, layout, 640, 1)
            for path in ("../map.bin", str(root / "map.bin")):
                with self.assertRaises(assets.AssetError):
                    assets.safe_relative(path)

    def test_repository_worktree_and_donor_outputs_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            donor = root / "donor"
            donor.mkdir()
            for output in (donor, donor / "stage", root, assets.ROOT / "stage"):
                with self.subTest(output=output), self.assertRaises(assets.AssetError):
                    assets._ensure_output_root(output, donor)
            worktree = root / "worktree"
            worktree.mkdir()
            (worktree / ".git").write_text("gitdir: unused\n")
            with self.assertRaisesRegex(assets.AssetError, "repository"):
                assets._ensure_output_root(worktree / "new-stage", donor)
            self.assertEqual(assets._ensure_output_root(root / "safe", donor), root / "safe")

    def test_wrong_donor_pin_fails_before_loading_assets(self):
        with mock.patch.object(assets, "git_pin", return_value=("wrong", "tree")):
            with self.assertRaisesRegex(assets.AssetError, "pin mismatch"):
                assets.build_manifest("unused")


@unittest.skipUnless(DONOR, "set JOHTO_DONOR to run the pinned corpus tests")
class AssetCorpusTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = assets.build_manifest(DONOR)
        cls.mapping, cls.mismatches = assets._behavior_map(
            DONOR / "include/constants/metatile_behaviors.h",
            assets.ROOT / "include/constants/metatile_behaviors.h")

    def test_full_manifest_matches_selection_and_remains_pending_runtime(self):
        self.assertEqual(self.manifest, assets.build_manifest(DONOR))
        self.assertEqual(self.manifest["selection"],
                         {"layout_count": 239, "tileset_count": 66, "asset_count": 1534})
        self.assertEqual(len(self.mismatches), 22)
        self.assertTrue(all(not entry["runtime_ready"] for entry in self.manifest["tilesets"]))
        self.assertEqual(json.loads(assets.OUTPUT_MANIFEST.read_text()), self.manifest)

    def test_symbolic_behavior_mapping_does_not_reuse_conflicting_host_meanings(self):
        for source, target in ((0xA1, 0xF0), (0xEF, 0xF1), (0xA3, 0xF2),
                               (0x2D, 0xF3), (0xEB, 0xC8), (0xEC, 0xC9), (0xFF, 0xFF)):
            with self.subTest(source=source):
                self.assertEqual(self.mapping[source]["target_value"], target)
        self.assertNotIn(0xFE, self.mapping)

    def test_exceptions_are_source_hash_bound_and_preserve_layer(self):
        for relative, expected_sha in assets.PINNED_ATTRIBUTE_EXCEPTION_SHAS.items():
            with self.subTest(relative=relative):
                raw = (DONOR / relative).read_bytes()
                self.assertEqual(assets.sha256(raw), expected_sha)
                converted, note = assets.convert_source_attributes(relative, raw, self.mapping)
                self.assertIsNotNone(note)
                old = [x[0] for x in struct.iter_unpack("<H", raw)]
                new = [x[0] for x in struct.iter_unpack("<I", converted)]
                self.assertEqual([value >> 12 for value in old], [value >> 29 for value in new])
                for index in assets.PINNED_UNDEFINED_BEHAVIORS.get(relative, {}):
                    self.assertEqual(new[index] & 0xFF, 0xF2)
                with self.assertRaisesRegex(assets.AssetError, "hash mismatch"):
                    assets.convert_source_attributes(relative, bytes([raw[0] ^ 1]) + raw[1:], self.mapping)
        for relative in ("data/tilesets/primary/kanto_general/tiles.png",
                         "data/tilesets/primary/johto_general/tiles.png"):
            self.assertIn(assets.parse_png((DONOR / relative).read_bytes())["bit_depth"], (4, 8))

    def test_check_is_read_only_and_rejects_stale_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "manifest.json"
            target.write_text(assets.canonical_json(self.manifest))
            with mock.patch.object(assets, "OUTPUT_MANIFEST", target):
                before = target.read_bytes()
                self.assertEqual(assets.main(["--donor", str(DONOR), "--check"]), 0)
                self.assertEqual(target.read_bytes(), before)
                target.write_bytes(b"{}\n")
                self.assertEqual(assets.main(["--donor", str(DONOR), "--check"]), 1)
                self.assertEqual(target.read_bytes(), b"{}\n")

    def test_staging_checks_every_output_hash_and_rejects_unrelated_overwrites(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "stage"
            forged = copy.deepcopy(self.manifest)
            forged["provenance"]["donor_revision"] = "forged"
            with self.assertRaisesRegex(assets.AssetError, "supplied asset manifest"):
                assets.stage_assets(DONOR, target, forged)
            self.assertFalse(target.exists())
            first = self.manifest["layouts"][0]["assets"][0]["path"]
            conflict = target / first
            conflict.parent.mkdir(parents=True)
            conflict.write_bytes(b"unrelated")
            with self.assertRaisesRegex(assets.AssetError, "unrelated"):
                assets.stage_assets(DONOR, target)
            self.assertEqual([p for p in target.rglob("*") if p.is_file()], [conflict])
            self.assertEqual(conflict.read_bytes(), b"unrelated")
            conflict.unlink()
            outputs = assets.stage_assets(DONOR, target)
            expected = {}
            for kind in ("layouts", "tilesets"):
                for record in self.manifest[kind]:
                    for asset in record["assets"]:
                        expected[asset["path"]] = asset["output_sha256"]
            self.assertEqual(set(outputs), set(expected))
            for relative, digest in expected.items():
                self.assertEqual(assets.sha256((target / relative).read_bytes()), digest, relative)
            self.assertEqual(set(assets.stage_assets(DONOR, target)), set(expected))


if __name__ == "__main__":
    unittest.main()
