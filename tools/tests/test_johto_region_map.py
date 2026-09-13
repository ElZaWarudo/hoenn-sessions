"""Regression tests for the deterministic Johto region-map import boundary."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.johto import import_region_map as region_map


DONOR = Path(os.environ["JOHTO_DONOR"]) if os.environ.get("JOHTO_DONOR") else None


class JohtoRegionMapTests(unittest.TestCase):
    def test_checked_in_assets_have_audited_hashes_and_formats(self):
        png = (region_map.ROOT / region_map.ASSET_DIRECTORY / "johtomap.png").read_bytes()
        tilemap = (region_map.ROOT / region_map.ASSET_DIRECTORY / "johtomap.bin").read_bytes()
        self.assertEqual(hashlib.sha256(png).hexdigest(), region_map.PNG_SHA256)
        self.assertEqual(hashlib.sha256(tilemap).hexdigest(), region_map.BIN_SHA256)
        region_map.validate_png(png)
        width, height = struct.unpack(">II", png[16:24])
        self.assertEqual(width * height, 16384)
        self.assertEqual(len(tilemap), 4096)

    def test_generated_layout_is_canonical_and_uses_host_symbols(self):
        layout = (region_map.ROOT / region_map.OUTPUT_LAYOUT_PATH).read_text(encoding="utf-8")
        self.assertNotIn("\r", layout)
        rows = region_map.parse_layout(layout)
        self.assertEqual((len(rows), len(rows[0])), (15, 28))
        self.assertIn("sRegionMapSections_Johto", layout)
        self.assertNotIn("MAPSEC_TRAINER_HILL", layout)
        self.assertIn("MAPSEC_JOHTO_ROUTE_29", layout)

    def test_generated_layout_is_forced_to_lf_by_repository_attributes(self):
        attributes = (region_map.ROOT / ".gitattributes").read_text(encoding="utf-8")
        rule = "/src/data/region_map/region_map_layout_johto.h text eol=lf"
        self.assertIn(rule, attributes.splitlines())
        result = subprocess.check_output(
            [
                "git", "-C", str(region_map.ROOT), "check-attr", "eol", "--",
                region_map.OUTPUT_LAYOUT_PATH.as_posix(),
            ],
            text=True,
        ).strip()
        self.assertEqual(
            result,
            "src/data/region_map/region_map_layout_johto.h: eol: lf",
        )

    def test_graphics_rules_define_host_compressed_formats(self):
        rules = (region_map.ROOT / "graphics_file_rules.mk").read_text(encoding="utf-8")
        self.assertIn("graphics/pokenav/region_map/johtomap.8bpp:", rules)
        rule = "graphics/pokenav/region_map/johtomap.8bpp: %.8bpp: %.png\n\t$(GFX) $< $@"
        self.assertIn(rule, rules)
        self.assertNotIn("johtomap.8bpp: %.8bpp: %.png\n\t$(GFX) $< $@ -num_tiles", rules)
        self.assertIn("johtomap.8bpp.smol: graphics/pokenav/region_map/johtomap.8bpp", rules)
        self.assertIn("johtomap.bin.smolTM: graphics/pokenav/region_map/johtomap.bin", rules)

    @unittest.skipUnless(DONOR, "JOHTO_DONOR is required for donor integration tests")
    def test_build_is_deterministic_and_check_is_idempotent(self):
        assert DONOR is not None
        first = region_map.build(DONOR, region_map.ROOT)
        second = region_map.build(DONOR, region_map.ROOT)
        self.assertEqual(first, second)
        self.assertEqual(region_map.run(DONOR, region_map.ROOT, check=True), 0)

    @unittest.skipUnless(DONOR, "JOHTO_DONOR is required for donor integration tests")
    def test_check_accepts_windows_checkout_eol_but_rejects_semantic_mutation(self):
        assert DONOR is not None
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for path in (region_map.MANIFEST_PATH, region_map.HOST_SECTIONS_PATH):
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(region_map.ROOT / path, target)
            region_map.run(DONOR, root, check=False)
            layout = root / region_map.OUTPUT_LAYOUT_PATH
            lf = layout.read_bytes()
            self.assertNotIn(b"\r", lf)
            layout.write_bytes(lf.replace(b"\n", b"\r\n"))
            self.assertEqual(region_map.run(DONOR, root, check=True), 0)
            layout.write_bytes(layout.read_bytes().replace(b"MAPSEC_NONE", b"MAPSEC_MUTATION", 1))
            with self.assertRaisesRegex(region_map.RegionMapImportError, "output drift"):
                region_map.run(DONOR, root, check=True)

    @unittest.skipUnless(DONOR, "JOHTO_DONOR is required for donor integration tests")
    def test_source_mutation_is_rejected_by_hash(self):
        assert DONOR is not None
        with tempfile.TemporaryDirectory() as directory:
            donor = Path(directory) / "donor"
            for path in (
                region_map.SOURCE_LAYOUT_PATH,
                region_map.SOURCE_SECTIONS_PATH,
                region_map.SOURCE_PNG_PATH,
                region_map.SOURCE_BIN_PATH,
            ):
                target = donor / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(DONOR / path, target)
            png = donor / region_map.SOURCE_PNG_PATH
            png.write_bytes(png.read_bytes() + b"mutation")
            with self.assertRaisesRegex(region_map.RegionMapImportError, "hash mismatch"):
                region_map.build(donor, region_map.ROOT)

    @unittest.skipUnless(DONOR, "JOHTO_DONOR is required for donor integration tests")
    def test_manifest_provenance_mutation_is_rejected(self):
        assert DONOR is not None
        manifest = region_map.load_json(region_map.ROOT / region_map.MANIFEST_PATH)
        manifest["provenance"]["donor_tree"] = "0" * 40
        with self.assertRaisesRegex(region_map.RegionMapImportError, "provenance drifted"):
            region_map._manifest_mapping(manifest)

    @unittest.skipUnless(DONOR, "JOHTO_DONOR is required for donor integration tests")
    def test_alias_target_symbol_mutation_fails_before_writes(self):
        assert DONOR is not None
        manifest = region_map.load_json(region_map.ROOT / region_map.MANIFEST_PATH)
        alias = next(
            entry for entry in manifest["sections"]["entries"]
            if entry["classification"] == "johto_alias"
        )
        alias["target_symbol"] = "MAPSEC_MUTATED_ALIAS"
        self._assert_manifest_rejected_before_writes(manifest, "missing host section")

    @unittest.skipUnless(DONOR, "JOHTO_DONOR is required for donor integration tests")
    def test_adapter_target_id_mutation_fails_before_writes(self):
        assert DONOR is not None
        manifest = region_map.load_json(region_map.ROOT / region_map.MANIFEST_PATH)
        adapter = next(
            entry for entry in manifest["sections"]["entries"]
            if entry["classification"] == "required_host_adapter"
        )
        adapter["target_id"] += 1
        self._assert_manifest_rejected_before_writes(manifest, "host section id drifted")

    def _assert_manifest_rejected_before_writes(
        self, manifest: dict, message: str
    ) -> None:
        assert DONOR is not None
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / region_map.MANIFEST_PATH
            manifest_path.parent.mkdir(parents=True, exist_ok=True)
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            sections_path = root / region_map.HOST_SECTIONS_PATH
            sections_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(region_map.ROOT / region_map.HOST_SECTIONS_PATH, sections_path)
            with self.assertRaisesRegex(region_map.RegionMapImportError, message):
                region_map.run(DONOR, root, check=False)
            self.assertFalse((root / region_map.ASSET_DIRECTORY).exists())

    @unittest.skipUnless(DONOR, "JOHTO_DONOR is required for donor integration tests")
    def test_mutated_checked_in_asset_is_not_overwritten(self):
        assert DONOR is not None
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for path in (region_map.MANIFEST_PATH, region_map.HOST_SECTIONS_PATH):
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(region_map.ROOT / path, target)
            target = root / region_map.ASSET_DIRECTORY / "johtomap.png"
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(b"user mutation")
            with self.assertRaisesRegex(region_map.RegionMapImportError, "refusing to overwrite"):
                region_map.run(DONOR, root, check=False)


if __name__ == "__main__":
    unittest.main()
