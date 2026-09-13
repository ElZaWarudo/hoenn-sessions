"""Selected scenery conversion and filesystem-boundary regression tests."""

import copy
import json
import os
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.johto import import_region_assets as assets


DONOR = Path(os.environ["JOHTO_DONOR"]) if os.environ.get("JOHTO_DONOR") else None
ORIGINAL_PREFIX_SHA256 = "165b842ab4d4a341a549cbac3e89c613c64253b8b528374738f21eb6b3739f10"


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
            (root / "border.bin").write_bytes(struct.pack("<HHHH", 0, 641, 0, 0))
            with self.assertRaisesRegex(assets.AssetError, "secondary tile index"):
                assets.validate_layout_assets(root, layout, 640, 1)
            (root / "map.bin").write_bytes(struct.pack("<H", 512))
            (root / "border.bin").write_bytes(b"\x00" * 8)
            with self.assertRaisesRegex(assets.AssetError, "primary tile index"):
                assets.validate_layout_assets(root, layout, 512, 1)
            (root / "map.bin").write_bytes(struct.pack("<H", 0))
            with self.assertRaisesRegex(assets.AssetError, "unsupported primary"):
                assets.validate_layout_assets(root, layout, 640, 1, {0}, set())
            (root / "border.bin").write_bytes(struct.pack("<HHHH", 640, 0, 0, 0))
            with self.assertRaisesRegex(assets.AssetError, "unsupported secondary"):
                assets.validate_layout_assets(root, layout, 640, 1, set(), {0})
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

    def test_full_manifest_matches_selection_and_keeps_global_runtime_gate(self):
        self.assertEqual(self.manifest, assets.build_manifest(DONOR))
        self.assertEqual(self.manifest["selection"],
                         {"layout_count": 407, "tileset_count": 97, "asset_count": 2366})
        self.assertEqual(len(self.mismatches), 22)
        ready_tilesets = {
            entry["symbol"] for entry in self.manifest["tilesets"]
            if entry["runtime_ready"]
        }
        self.assertEqual(ready_tilesets, {"gTileset_KantoLaterImported_General"})
        self.assertFalse(self.manifest["runtime_ready"])
        self.assertEqual(len(self.manifest["layouts"]), 407)
        self.assertEqual(len(self.manifest["tilesets"]), 97)
        self.assertEqual(sum(len(item["assets"]) for item in self.manifest["layouts"]), 814)
        self.assertEqual(sum(len(item["assets"]) for item in self.manifest["tilesets"]), 1552)
        self.assertEqual(json.loads(assets.OUTPUT_MANIFEST.read_text()), self.manifest)

    def test_original_prefix_and_later_target_namespaces_are_stable(self):
        prefix = assets.canonical_json({
            "layouts": self.manifest["layouts"][:239],
            "tilesets": self.manifest["tilesets"][:66],
        })
        self.assertEqual(assets.sha256(prefix.encode()), ORIGINAL_PREFIX_SHA256)
        self.assertEqual(
            [item["symbol"] for item in self.manifest["tilesets"][:66]],
            sorted(item["symbol"] for item in self.manifest["tilesets"][:66]),
        )
        later_layouts = self.manifest["layouts"][239:]
        self.assertEqual(len(later_layouts), 168)
        self.assertEqual(
            [item["target_layout"] for item in later_layouts],
            [item["identity_namespace"]["layout"]
             for item in json.loads((assets.REGION_MANIFEST).read_text())["maps"][239:]],
        )
        later_tilesets = self.manifest["tilesets"][66:]
        self.assertEqual(len(later_tilesets), 31)
        self.assertEqual(
            [item["symbol"] for item in later_tilesets],
            sorted(item["symbol"] for item in later_tilesets),
        )
        self.assertTrue(all(item["symbol"].startswith("gTileset_KantoLaterImported_")
                            for item in later_tilesets))
        self.assertTrue(all(item["source_symbol"].startswith("gTileset_")
                            for item in later_tilesets))
        source_to_target = {
            item.get("source_symbol", item["symbol"]): item["symbol"]
            for item in self.manifest["tilesets"]
        }
        for layout in later_layouts:
            for source_field, target_field in (
                ("primary_tileset", "target_primary_tileset"),
                ("secondary_tileset", "target_secondary_tileset"),
            ):
                source = layout[source_field]
                if source in source_to_target and source in {
                    item["symbol"] for item in self.manifest["tilesets"][:66]
                }:
                    expected_target = "gTileset_JohtoImported_" + source.removeprefix("gTileset_")
                else:
                    expected_target = "gTileset_KantoLaterImported_" + source.removeprefix("gTileset_")
                self.assertEqual(layout[target_field], expected_target)

    def test_general_table_and_readiness_gates_are_explicit(self):
        general = next(item for item in self.manifest["tilesets"]
                       if item["symbol"] == "gTileset_KantoLaterImported_General")
        self.assertEqual(general["kind"], "primary")
        self.assertEqual(general["callback"], "InitTilesetAnim_HoennGeneral")
        self.assertTrue(general["runtime_ready"])
        self.assertEqual(general["assets"][1]["metatile_count"], 512)
        self.assertEqual(general["assets"][2]["source_size"], 1024)
        self.assertEqual(self.manifest["conversion"]["primary_metatile_boundary"], 640)
        self.assertEqual(self.manifest["conversion"]["general_primary_metatile_count"], 512)
        pending = self.manifest["runtime_readiness"]["pending_general_layouts"]
        self.assertEqual(pending, [])
        safari_layouts = {
            item["symbol"]: item for item in self.manifest["layouts"]
            if item["symbol"] in {
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH",
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BRUSH",
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_MOUNTAIN",
            }
        }
        self.assertEqual(set(safari_layouts), {
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH",
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BRUSH",
            "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_MOUNTAIN",
        })
        for source, item in safari_layouts.items():
            self.assertEqual(item["primary_tileset"], "gTileset_General", source)
            self.assertEqual(item["target_primary_tileset"],
                             "gTileset_KantoLaterImported_General", source)
            self.assertEqual(item["target_layout"], source.replace(
                "LAYOUT_FUCHSIA_", "LAYOUT_KANTO_LATER_FUCHSIA_"), source)

    def test_general_runtime_readiness_drift_is_rejected_exactly(self):
        mutations = {}
        forged = copy.deepcopy(self.manifest)
        general = next(item for item in forged["tilesets"]
                       if item["symbol"] == "gTileset_KantoLaterImported_General")
        general["runtime_ready"] = False
        mutations["runtime readiness"] = forged
        forged = copy.deepcopy(self.manifest)
        general = next(item for item in forged["tilesets"]
                       if item["symbol"] == "gTileset_KantoLaterImported_General")
        general["callback"] = "InitTilesetAnim_General"
        mutations["callback identity"] = forged
        forged = copy.deepcopy(self.manifest)
        forged["runtime_readiness"]["pending_general_layouts"].append({
            "source_layout": "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH",
            "pending": "drift",
        })
        mutations["pending identity"] = forged
        forged = copy.deepcopy(self.manifest)
        beach = next(item for item in forged["layouts"]
                     if item["symbol"] == "LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH")
        beach["target_layout"] = "LAYOUT_KANTO_LATER_FUCHSIA_CITY_SAFARI_ZONE_BRUSH"
        mutations["target identity"] = forged

        with tempfile.TemporaryDirectory() as directory, \
             mock.patch.object(assets, "build_manifest", return_value=self.manifest):
            for label, forged in mutations.items():
                with self.subTest(label=label), self.assertRaisesRegex(
                    assets.AssetError, "supplied asset manifest"
                ):
                    assets.stage_assets(DONOR, Path(directory) / label, forged)

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

    def test_write_accepts_only_the_exact_predecessor_or_intended_output(self):
        predecessor = subprocess.check_output(
            ["git", "show", "HEAD:data/johto/asset_manifest.json"],
            cwd=assets.ROOT,
        )
        expected_manifest = {"selection": {"layout_count": 0, "tileset_count": 0}}
        expected = assets.canonical_json(expected_manifest).encode()
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "asset_manifest.json"
            with mock.patch.object(assets, "OUTPUT_MANIFEST", target), \
                 mock.patch.object(assets, "build_manifest", return_value=expected_manifest):
                target.write_bytes(predecessor)
                self.assertEqual(assets.main(["--donor", "unused", "--write"]), 0)
                self.assertEqual(target.read_bytes(), expected)
                target_before = b"arbitrary existing manifest bytes"
                target.write_bytes(target_before)
                self.assertEqual(assets.main(["--donor", "unused", "--write"]), 1)
                self.assertEqual(target.read_bytes(), target_before)
                target.write_bytes(expected)
                self.assertEqual(assets.main(["--donor", "unused", "--write"]), 0)
                self.assertEqual(target.read_bytes(), expected)

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

    def test_route7_repair_is_exact_and_source_bound(self):
        layout = next(item for item in self.manifest["layouts"]
                      if item["symbol"] == "LAYOUT_ROUTE7")
        map_asset = layout["assets"][0]
        raw = (DONOR / assets.ROUTE7_MAP_SOURCE_PATH).read_bytes()
        converted, repair = assets.convert_source_map(assets.ROUTE7_MAP_SOURCE_PATH, raw)
        self.assertEqual(map_asset["conversion"], "route7-forest-boundary-repair")
        self.assertEqual(map_asset["source_sha256"], assets.ROUTE7_MAP_SOURCE_SHA256)
        self.assertEqual(map_asset["output_sha256"], assets.ROUTE7_MAP_OUTPUT_SHA256)
        self.assertEqual(repair, map_asset["repair"])
        self.assertEqual(repair["changed_cell_count"], 12)
        expected = {
            170: (0x06E7, 0x0414), 171: (0x06E7, 0x0415),
            172: (0x06E7, 0x0414), 173: (0x06E7, 0x0415),
            174: (0x06E7, 0x0414), 175: (0x06E7, 0x0415),
            192: (0x0786, 0x041C), 193: (0x0786, 0x041D),
            194: (0x0786, 0x041C), 195: (0x0786, 0x041D),
            196: (0x0786, 0x041C), 197: (0x0786, 0x041D),
        }
        self.assertEqual(
            {entry["index"]: (int(entry["source_word"], 16), int(entry["output_word"], 16))
             for entry in repair["changed_cells"]},
            expected,
        )
        for index, (old, new) in expected.items():
            self.assertEqual(struct.unpack_from("<H", raw, index * 2)[0], old)
            self.assertEqual(struct.unpack_from("<H", converted, index * 2)[0], new)
            self.assertEqual(old & assets.ROUTE7_MAP_REPAIR_MASK,
                             new & assets.ROUTE7_MAP_REPAIR_MASK)
        changed = {index * 2 + offset for index in expected for offset in (0, 1)}
        self.assertEqual({i for i, (old, new) in enumerate(zip(raw, converted)) if old != new}, changed)
        with self.assertRaisesRegex(assets.AssetError, "hash mismatch"):
            assets.convert_source_map(assets.ROUTE7_MAP_SOURCE_PATH,
                                      bytes([raw[0] ^ 1]) + raw[1:])


if __name__ == "__main__":
    unittest.main()
