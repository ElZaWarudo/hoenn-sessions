import copy
import hashlib
import importlib.util
import io
import json
import re
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.johto import import_region_assets


ROOT = Path(__file__).resolve().parents[2]
DONOR_ROOT = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")
SPEC = importlib.util.spec_from_file_location("scenery", ROOT / "tools/johto/import_scenery.py")
scenery = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(scenery)


def load(path: str):
    local = ROOT / path
    if local.is_file():
        return json.loads(local.read_text(encoding="utf-8"))
    return json.loads(head_bytes(path))


def head_bytes(path: str) -> bytes:
    return subprocess.run(
        ["git", "show", f"HEAD:{path}"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout


class JohtoSceneryTest(unittest.TestCase):
    def test_complete_selection_and_append_only_registration(self):
        manifest = load("data/johto/asset_manifest.json")
        registration = load("data/johto/scenery_registration.json")
        self.assertEqual(manifest["selection"], {"layout_count": 407, "tileset_count": 97, "asset_count": 2366})
        self.assertEqual(len(manifest["layouts"]), 407)
        self.assertEqual(len(manifest["tilesets"]), 97)
        self.assertEqual(len(registration["layouts"]), 407)
        self.assertEqual(len(registration["tilesets"]), 97)
        self.assertEqual(registration["provenance"]["selection"], manifest["selection"])
        self.assertEqual(registration["layouts"][0]["target_layout_id"], "LAYOUT_NEW_BARK_TOWN")
        self.assertEqual(registration["layouts"][238]["ordinal"], 1023)
        self.assertEqual(registration["layouts"][239]["ordinal"], 1024)
        self.assertEqual(registration["layouts"][-1]["ordinal"], 1191)
        self.assertEqual(registration["layouts"][239]["target_layout_id"], manifest["layouts"][239]["identity_namespace"]["layout"])
        self.assertTrue(all(item["target_layout_id"].startswith("LAYOUT_KANTO_LATER_") for item in registration["layouts"][239:]))
        self.assertEqual(len({item["target_layout_id"] for item in registration["layouts"]}), 407)
        self.assertEqual(len({item["target_name"] for item in registration["layouts"]}), 407)

    def test_layout_table_keeps_1024_rows_and_appends_168(self):
        manifest = load("data/johto/asset_manifest.json")
        registration = load("data/johto/scenery_registration.json")
        layouts = load("data/layouts/layouts.json")
        self.assertEqual(len(layouts["layouts"]), 1192)
        self.assertEqual(scenery._identity(layouts["layouts"][:1024]), scenery.HOST_LAYOUT_TABLE_SHA256)
        self.assertEqual(scenery._identity(layouts["layouts"][:785]), scenery.HOST_LAYOUT_PREFIX_SHA256)
        for index, selected in enumerate(manifest["layouts"][239:]):
            actual = layouts["layouts"][1024 + index]
            accepted = registration["layouts"][239 + index]
            self.assertEqual(actual["id"], selected["identity_namespace"]["layout"])
            self.assertEqual(actual["name"], "KantoLater_" + selected["name"])
            self.assertEqual(actual["width"], selected["width"])
            self.assertEqual(actual["height"], selected["height"])
            self.assertEqual(actual["primary_tileset"], selected["target_primary_tileset"])
            self.assertEqual(actual["secondary_tileset"], selected["target_secondary_tileset"])
            self.assertEqual(actual["border_filepath"], "data/johto/scenery/" + selected["assets"][1]["path"])
            self.assertEqual(actual["blockdata_filepath"], "data/johto/scenery/" + selected["assets"][0]["path"])
            self.assertEqual(accepted["ordinal"], 1024 + index)
            self.assertEqual(accepted["map_layout_id"], 1025 + index)
            self.assertEqual(accepted["identity_namespace"], selected["identity_namespace"])

    def test_all_selected_assets_have_pinned_bytes_and_exact_growth(self):
        manifest = load("data/johto/asset_manifest.json")
        assets = [asset for group in ("layouts", "tilesets") for entry in manifest[group] for asset in entry["assets"]]
        self.assertEqual(len(assets), 2366)
        existing = 0
        for asset in assets:
            path = ROOT / "data/johto/scenery" / asset["path"]
            if path.is_file():
                existing += 1
                raw = path.read_bytes()
                self.assertEqual(hashlib.sha256(raw).hexdigest(), asset["output_sha256"], asset["path"])
                self.assertEqual(len(raw), asset["output_size"])
        self.assertEqual(existing, 2366)
        self.assertEqual(sum(len(entry["assets"]) for entry in manifest["layouts"][239:]), 336)
        self.assertEqual(sum(len(entry["assets"]) for entry in manifest["tilesets"][66:]), 496)

    def test_later_source_hashes_are_bound_to_the_pinned_donor(self):
        manifest = load("data/johto/asset_manifest.json")
        self.assertTrue(DONOR_ROOT.is_dir())
        later_assets = [
            asset
            for group, start in (("layouts", 239), ("tilesets", 66))
            for entry in manifest[group][start:]
            for asset in entry["assets"]
        ]
        self.assertEqual(len(later_assets), 832)
        for asset in later_assets:
            source = DONOR_ROOT / asset["path"]
            output = ROOT / "data/johto/scenery" / asset["path"]
            self.assertEqual(source.stat().st_size, asset["source_size"], asset["path"])
            self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), asset["source_sha256"], asset["path"])
            self.assertEqual(output.stat().st_size, asset["output_size"], asset["path"])
            self.assertEqual(hashlib.sha256(output.read_bytes()).hexdigest(), asset["output_sha256"], asset["path"])

    def test_later_tileset_identity_and_header_prefix(self):
        manifest = load("data/johto/asset_manifest.json")
        registration = load("data/johto/scenery_registration.json")
        header = (ROOT / "src/data/tilesets/johto_imported.h").read_text(encoding="utf-8").replace("\r\n", "\n")
        old_symbols = [f"gTileset_JohtoImported_{item['symbol'].removeprefix('gTileset_')}" for item in manifest["tilesets"][:66]]
        found = re.findall(r"^const struct Tileset (gTileset_\w+) =", header, flags=re.M)
        self.assertEqual(found[:66], old_symbols)
        later = manifest["tilesets"][66:]
        self.assertEqual(len(later), 31)
        self.assertEqual([item["target_symbol"] for item in registration["tilesets"][66:]], [item["symbol"] for item in later])
        self.assertTrue(all(item["target_symbol"].startswith("gTileset_KantoLaterImported_") for item in registration["tilesets"][66:]))
        self.assertEqual(set(found[66:]), {item["symbol"] for item in later})

    def test_route7_repair_and_general_bounds(self):
        manifest = load("data/johto/asset_manifest.json")
        route7 = next(item for item in manifest["layouts"] if item["symbol"] == "LAYOUT_ROUTE7")
        route7_map = next(asset for asset in route7["assets"] if asset["path"].endswith("/map.bin"))
        self.assertEqual(route7_map["conversion"], "route7-forest-boundary-repair")
        self.assertEqual(route7_map["source_sha256"], "558557c41d23981e78967839798c78d31a004174e9d7e14aec0de69c524d4ff9")
        self.assertEqual(route7_map["output_sha256"], "333f993fa119a62a3e80c6e1135ad107125286bbdac11c572742f043daca88dd")
        self.assertEqual(route7_map["repair"]["changed_cell_count"], 12)
        with self.assertRaises(scenery.ImportError):
            scenery._validate_table_words(b"\x00\x02", scenery.GENERAL_PRIMARY_COUNT, scenery.GENERAL_SECONDARY_COUNT, "general-gap")
        scenery._validate_table_words(b"\xff\x01", scenery.GENERAL_PRIMARY_COUNT, scenery.GENERAL_SECONDARY_COUNT, "general-primary-end")
        scenery._validate_table_words(b"\x80\x02", scenery.GENERAL_PRIMARY_COUNT, scenery.GENERAL_SECONDARY_COUNT, "general-secondary-zero")
        with self.assertRaises(scenery.ImportError):
            scenery._validate_table_words(b"\xff\x03", scenery.GENERAL_PRIMARY_COUNT, 383, "general-secondary-overflow")

    def test_route7_repair_has_the_independent_twelve_cell_mask(self):
        route7 = DONOR_ROOT / "data/layouts/Route7/map.bin"
        raw = route7.read_bytes()
        converted, repair = import_region_assets.convert_source_map("data/layouts/Route7/map.bin", raw)
        self.assertIsNotNone(repair)
        self.assertEqual(hashlib.sha256(raw).hexdigest(), "558557c41d23981e78967839798c78d31a004174e9d7e14aec0de69c524d4ff9")
        self.assertEqual(hashlib.sha256(converted).hexdigest(), "333f993fa119a62a3e80c6e1135ad107125286bbdac11c572742f043daca88dd")
        source_words = list(struct.unpack("<" + "H" * (len(raw) // 2), raw))
        output_words = list(struct.unpack("<" + "H" * (len(converted) // 2), converted))
        expected_words = source_words[:]
        expected_indices = list(range(170, 176)) + list(range(192, 198))
        for index in range(170, 176):
            expected_words[index] = 0x0414 if index % 2 == 0 else 0x0415
        for index in range(192, 198):
            expected_words[index] = 0x041C if index % 2 == 0 else 0x041D
        self.assertEqual([index for index, (before, after) in enumerate(zip(source_words, output_words)) if before != after], expected_indices)
        self.assertEqual(output_words, expected_words)

    def test_later_attribute_exceptions_have_an_independent_byte_oracle(self):
        paths = sorted([
            "data/tilesets/secondary/cave_green/metatile_attributes.bin",
            "data/tilesets/secondary/cave_mt_moon/metatile_attributes.bin",
            "data/tilesets/secondary/cave_sandy/metatile_attributes.bin",
            "data/tilesets/secondary/celadon_apartments/metatile_attributes.bin",
            "data/tilesets/secondary/cerulean_city/metatile_attributes.bin",
            "data/tilesets/secondary/indigo_plateau/metatile_attributes.bin",
            "data/tilesets/secondary/saffron_city_dojo_vip/metatile_attributes.bin",
            "data/tilesets/secondary/silph_co/metatile_attributes.bin",
            "data/tilesets/secondary/soul_house/metatile_attributes.bin",
            "data/tilesets/secondary/viridian_city_gym/metatile_attributes.bin",
        ])
        manifest = load("data/johto/asset_manifest.json")
        assets = {
            asset["path"]: asset
            for entry in manifest["tilesets"][66:]
            for asset in entry["assets"]
        }

        define_pattern = re.compile(r"^\s*#define\s+(MB_[A-Za-z0-9_]+)\s+([^\s/]+)")

        def numeric_defines(text):
            values = {}
            for line in text.splitlines():
                match = define_pattern.match(line)
                if match:
                    try:
                        values[match.group(1)] = int(match.group(2), 0)
                    except ValueError:
                        pass
            return values

        donor_values = numeric_defines(
            (DONOR_ROOT / "include/constants/metatile_behaviors.h").read_text(encoding="utf-8")
        )
        host_text = head_bytes("include/constants/metatile_behaviors.h").decode("utf-8")
        host_values = numeric_defines(host_text)
        in_enum = False
        next_value = 0
        for line in host_text.splitlines():
            stripped = line.split("//", 1)[0].strip()
            if stripped.startswith("enum"):
                in_enum = True
                next_value = 0
                continue
            if in_enum and stripped.startswith("};"):
                in_enum = False
                continue
            if not in_enum:
                continue
            match = re.fullmatch(r"(MB_[A-Za-z0-9_]+)(?:\s*=\s*([^,]+))?\s*,?", stripped)
            if not match:
                continue
            symbol, explicit = match.groups()
            if explicit is not None:
                next_value = int(explicit, 0)
            host_values[symbol] = next_value
            next_value += 1
        host_values.setdefault("MB_INVALID", 0xFF)

        source_words = {
            path: list(struct.iter_unpack("<H", (DONOR_ROOT / path).read_bytes()))
            for path in paths
        }
        used_behaviors = {word & 0xFF for words in source_words.values() for (word,) in words}
        special_targets = {
            "MB_HEADBUTT_TREE": 0xF0,
            "MB_WATER_NORTH_ARROW_WARP": 0xF1,
            "MB_UNUSED_2D": 0xF3,
        }
        inert_symbols = {
            "MB_UNUSED_1E", "MB_UNUSED_23", "MB_UNUSED_58", "MB_UNUSED_A3",
            "MB_UNUSED_A4", "MB_UNUSED_A5", "MB_UNUSED_A6", "MB_UNUSED_A8",
            "MB_UNUSED_AB", "MB_UNUSED_AC", "MB_UNUSED_AE", "MB_UNUSED_AF",
            "MB_UNUSED_C8", "MB_UNUSED_C9", "MB_UNUSED_CA", "MB_UNUSED_EE",
        }
        behavior_mapping = {}
        for symbol, source_value in donor_values.items():
            if source_value not in used_behaviors:
                continue
            if symbol == "MB_INVALID":
                target = 0xFF
            elif symbol in special_targets:
                target = special_targets[symbol]
            elif symbol in inert_symbols:
                target = 0xF2
            elif symbol in host_values:
                target = host_values[symbol]
            else:
                continue
            behavior_mapping[source_value] = target
        self.assertEqual(len(behavior_mapping), 168)
        used_behavior_mapping = {
            f"{source:02x}": target
            for source, target in behavior_mapping.items()
        }
        mapping_digest = hashlib.sha256(
            json.dumps(
                used_behavior_mapping,
                sort_keys=True,
                separators=(",", ":"),
                ensure_ascii=True,
            ).encode("utf-8")
        ).hexdigest()
        self.assertEqual(
            mapping_digest,
            "2fb15d23a93be1e58d76ff4053e8fd45db0cafee4fea624be8369a5427e4e209",
        )

        expected_counts = {
            "cave_green": (0, 250, 0),
            "cave_mt_moon": (53, 0, 0),
            "cave_sandy": (0, 4, 5),
            "celadon_apartments": (0, 27, 14),
            "cerulean_city": (0, 9, 6),
            "indigo_plateau": (0, 10, 3),
            "saffron_city_dojo_vip": (0, 1, 0),
            "silph_co": (0, 0, 4),
            "soul_house": (39, 0, 0),
            "viridian_city_gym": (0, 32, 29),
        }
        records = []
        reference_outputs = []
        for path in paths:
            source = (DONOR_ROOT / path).read_bytes()
            checked = (ROOT / "data/johto/scenery" / path).read_bytes()
            output = bytearray()
            reserved = []
            layer3 = []
            undefined = []
            for index, (original,) in enumerate(source_words[path]):
                if original & 0x0F00:
                    reserved.append([index, f"{original:#06x}"])
                cleared = original & ~0x0F00
                layer = cleared >> 12
                if layer == 3:
                    layer3.append([index, f"{cleared:#06x}"])
                source_behavior = cleared & 0xFF
                target_behavior = behavior_mapping.get(source_behavior)
                if target_behavior is None:
                    undefined.append([index, f"{original:#06x}"])
                    target_behavior = 0xF2
                output.extend(struct.pack("<I", target_behavior | (layer << 29)))
            reference = bytes(output)
            self.assertEqual(reference, checked, path)
            stem = Path(path).parent.name
            self.assertEqual((len(reserved), len(layer3), len(undefined)), expected_counts[stem], path)
            asset = assets[path]
            records.append({
                "path": path,
                "source_sha256": hashlib.sha256(source).hexdigest(),
                "output_sha256": hashlib.sha256(reference).hexdigest(),
                "source_size": len(source),
                "output_size": len(reference),
                "reserved": reserved,
                "layer3": layer3,
                "undefined": undefined,
            })
            self.assertEqual(records[-1]["source_sha256"], asset["source_sha256"], path)
            self.assertEqual(records[-1]["output_sha256"], asset["output_sha256"], path)
            reference_outputs.append(reference)
        records_digest = hashlib.sha256(
            json.dumps(records, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        self.assertEqual(records_digest, "f17075968650f13f5d3c292cfd0adebc31cd8cf98004441ffb18adf3623d7387")
        self.assertEqual(
            hashlib.sha256(b"".join(reference_outputs)).hexdigest(),
            "29b1ce056dda88625869bbebe83e84851a53d7cdef7d6871158ca2d9cb9d7a0d",
        )

    def _isolated_registration_run(self, document, mode, *, asset_manifest_bytes=None, region_manifest_bytes=None):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            registration_path = root / "scenery_registration.json"
            asset_manifest_path = root / "asset_manifest.json"
            region_manifest_path = root / "region_manifest.json"
            original = scenery._canonical(document).encode()
            registration_path.write_bytes(original)
            asset_manifest_path.write_bytes(
                asset_manifest_bytes
                if asset_manifest_bytes is not None
                else (ROOT / "data/johto/asset_manifest.json").read_bytes()
            )
            region_manifest_path.write_bytes(
                region_manifest_bytes
                if region_manifest_bytes is not None
                else (ROOT / "data/johto/region_manifest.json").read_bytes()
            )
            manifest = json.loads(asset_manifest_path.read_bytes())
            complete = load("data/johto/scenery_registration.json")

            def plan(_donor):
                registration = scenery._registration(
                    manifest,
                    complete["layouts"][scenery.OLD_LAYOUT_COUNT:],
                    complete["tilesets"][scenery.OLD_TILESET_COUNT:],
                )
                return ({registration_path: scenery._canonical(registration).encode()}, {}, registration)

            with patch.multiple(
                scenery,
                REGISTRATION_JSON=registration_path,
                ASSET_MANIFEST=asset_manifest_path,
                REGION_MANIFEST=region_manifest_path,
            ), patch.object(scenery, "_plan", side_effect=plan):
                stderr = io.StringIO()
                with patch.object(scenery.sys, "stderr", stderr):
                    result = scenery.main(["--donor-root", folder, mode])
            return result, original, registration_path.read_bytes(), stderr.getvalue()

    def _isolated_registration_write(self, document):
        result, original, written, _ = self._isolated_registration_run(document, "--write")
        return result, original, written

    def test_write_upgrades_the_exact_complete_predecessor_in_isolation(self):
        predecessor = json.loads(head_bytes("data/johto/scenery_registration.json"))
        self.assertEqual(
            scenery._identity(predecessor),
            scenery.PREDECESSOR_COMPLETE_REGISTRATION_SHA256,
        )
        result, original, written = self._isolated_registration_write(predecessor)
        self.assertEqual(result, 0)
        self.assertNotEqual(written, original)
        registration = json.loads(written)
        self.assertEqual(registration, load("data/johto/scenery_registration.json"))
        self.assertEqual(len(registration["layouts"]), 407)
        self.assertEqual(len(registration["tilesets"]), 97)
        self.assertEqual(registration["provenance"]["selection"], {
            "layout_count": 407,
            "tileset_count": 97,
            "asset_count": 2366,
        })

    def test_write_upgrades_raw_provenance_predecessor_and_rejects_mutation(self):
        predecessor = load("data/johto/scenery_registration.json")
        predecessor["provenance"]["region_manifest_sha256"] = (
            "b6e86075e617caece5405a9cfbeae0645361ba66ce543b93aa2c161db7c6ddc6"
        )
        predecessor["provenance"]["asset_manifest_sha256"] = (
            "0e2ed2a12f19f93fa0be68ee825b4ab45ed71410b0c06599614361f0d28904d5"
        )
        self.assertEqual(
            scenery._identity(predecessor),
            scenery.PREDECESSOR_RAW_PROVENANCE_REGISTRATION_SHA256,
        )
        result, original, written = self._isolated_registration_write(predecessor)
        self.assertEqual(result, 0)
        self.assertNotEqual(written, original)
        self.assertEqual(json.loads(written), load("data/johto/scenery_registration.json"))

        changed = copy.deepcopy(predecessor)
        changed["provenance"]["asset_manifest_sha256"] = "0" * 64
        result, original, written = self._isolated_registration_write(changed)
        self.assertEqual(result, 1)
        self.assertEqual(written, original)

    def test_write_rejects_adversarial_complete_predecessor_drift_without_mutation(self):
        predecessor = json.loads(head_bytes("data/johto/scenery_registration.json"))
        mutations = {}
        changed = copy.deepcopy(predecessor)
        changed["scope"]["purpose"] = "untrusted purpose"
        mutations["purpose"] = changed
        changed = copy.deepcopy(predecessor)
        changed["unexpected"] = True
        mutations["key"] = changed
        changed = copy.deepcopy(predecessor)
        changed["provenance"]["donor_tree"] = "0" * 40
        mutations["provenance"] = changed
        changed = copy.deepcopy(predecessor)
        changed["layouts"].append(copy.deepcopy(changed["layouts"][-1]))
        mutations["count"] = changed
        changed = copy.deepcopy(predecessor)
        changed["runtime_readiness"]["general_pending_layouts"].reverse()
        mutations["pending identity order"] = changed
        changed = copy.deepcopy(predecessor)
        general = next(
            entry for entry in changed["tilesets"]
            if entry["target_symbol"] == "gTileset_KantoLaterImported_General"
        )
        general["callback"] = "InitTilesetAnim_General"
        mutations["General callback"] = changed
        changed = copy.deepcopy(predecessor)
        changed["provenance"]["asset_manifest_sha256"] = "0" * 64
        mutations["asset provenance"] = changed
        for label, document in mutations.items():
            with self.subTest(label=label):
                result, original, written = self._isolated_registration_write(document)
                self.assertEqual(result, 1)
                self.assertEqual(written, original)

    def test_json_manifest_provenance_is_format_independent_and_semantic(self):
        registration = load("data/johto/scenery_registration.json")
        asset = load("data/johto/asset_manifest.json")
        region = load("data/johto/region_manifest.json")
        formats = (
            json.dumps(asset, indent=4, sort_keys=True).encode(),
            (json.dumps(asset, separators=(",", ":")) + "\r\n").encode(),
        )
        for index, formatted_asset in enumerate(formats):
            with self.subTest(format=index):
                result, original, written, stderr = self._isolated_registration_run(
                    registration,
                    "--check",
                    asset_manifest_bytes=formatted_asset,
                    region_manifest_bytes=(json.dumps(region, sort_keys=True) + "\r\n").encode(),
                )
                self.assertEqual(result, 0, stderr)
                self.assertEqual(written, original)

        changed_asset = copy.deepcopy(asset)
        changed_asset["selection"]["asset_count"] += 1
        result, original, written, stderr = self._isolated_registration_run(
            registration,
            "--check",
            asset_manifest_bytes=json.dumps(changed_asset, sort_keys=True).encode(),
        )
        self.assertEqual(result, 1)
        self.assertEqual(written, original)
        self.assertIn("manifest provenance drifted", stderr)

    def test_check_and_write_reject_boolean_registration_schema_without_mutation(self):
        registration = load("data/johto/scenery_registration.json")
        registration["schema_version"] = True
        for mode in ("--check", "--write"):
            with self.subTest(mode=mode):
                result, original, written, stderr = self._isolated_registration_run(
                    registration, mode
                )
                self.assertEqual(result, 1)
                self.assertEqual(written, original)
                self.assertIn("schema version drifted", stderr)

    def test_layout_and_registration_preflight_rejects_prefix_mutations(self):
        manifest = load("data/johto/asset_manifest.json")
        layouts = load("data/layouts/layouts.json")
        changed = copy.deepcopy(layouts)
        changed["layouts"][0], changed["layouts"][1] = changed["layouts"][1], changed["layouts"][0]
        with self.assertRaises(scenery.ImportError):
            scenery._layout_records(manifest, changed)
        registration = load("data/johto/scenery_registration.json")
        changed_registration = copy.deepcopy(registration)
        changed_registration["layouts"][0]["target_layout_id"] = "LAYOUT_KANTO_LATER_DRIFT"
        changed_count = copy.deepcopy(registration)
        changed_count["layouts"].pop()
        with tempfile.TemporaryDirectory() as folder:
            asset_manifest = Path(folder) / "asset_manifest.json"
            region_manifest = Path(folder) / "region_manifest.json"
            asset_manifest.write_bytes(head_bytes("data/johto/asset_manifest.json"))
            region_manifest.write_bytes(head_bytes("data/johto/region_manifest.json"))
            for document in (changed_registration, changed_count):
                document["provenance"]["asset_manifest_sha256"] = scenery._identity(
                    json.loads(asset_manifest.read_bytes())
                )
                document["provenance"]["region_manifest_sha256"] = scenery._identity(
                    json.loads(region_manifest.read_bytes())
                )
            with patch.multiple(
                scenery,
                ASSET_MANIFEST=asset_manifest,
                REGION_MANIFEST=region_manifest,
            ), patch.object(
                scenery,
                "_load",
                side_effect=lambda path: changed_registration
                if path == scenery.REGISTRATION_JSON
                else json.loads(path.read_text(encoding="utf-8")),
            ):
                with self.assertRaises(scenery.ImportError):
                    scenery._registration(manifest, [], [])
            with patch.multiple(
                scenery,
                ASSET_MANIFEST=asset_manifest,
                REGION_MANIFEST=region_manifest,
            ), patch.object(
                scenery,
                "_load",
                side_effect=lambda path: changed_count
                if path == scenery.REGISTRATION_JSON
                else json.loads(path.read_text(encoding="utf-8")),
            ):
                with self.assertRaisesRegex(scenery.ImportError, "counts drifted"):
                    scenery._registration(manifest, [], [])

    def test_check_and_write_refuse_drift_without_mutation(self):
        output = ROOT / "data/johto/scenery/data/layouts/CeladonCity/map.bin"
        original = output.read_bytes()
        drifted = original[:-1] + bytes([original[-1] ^ 0x01])
        output.write_bytes(drifted)
        try:
            self.assertEqual(scenery.main(["--donor-root", str(DONOR_ROOT), "--write"]), 1)
            self.assertEqual(output.read_bytes(), drifted)
        finally:
            output.write_bytes(original)

    def test_metadata_rejects_unknown_fields_and_unmapped_callbacks(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "src/data/tilesets/headers.h"
            path.parent.mkdir(parents=True)
            path.write_text("const struct Tileset gTileset_Test = { .unknown = 1, };")
            with self.assertRaises(scenery.ImportError):
                scenery._tileset_metadata(Path(folder), "gTileset_Test", {"kind": "primary"}, {})
            path.write_text("const struct Tileset gTileset_Test = { .swapPalettes = SWAP_PAL(8), };")
            with self.assertRaises(scenery.ImportError):
                scenery._tileset_metadata(Path(folder), "gTileset_Test", {"kind": "primary", "callback": "InitTilesetAnim_Unknown"}, {})
            callback, metadata = scenery._tileset_metadata(Path(folder), "gTileset_Test", {"kind": "primary", "callback": None}, {})
            self.assertIsNone(callback)
            self.assertEqual(metadata["swapPalettes"], 2)

    def test_source_drift_rejects_before_any_output_write(self):
        with tempfile.TemporaryDirectory() as folder:
            donor = Path(folder)
            (donor / "source.bin").write_bytes(b"drift")
            asset = {"path": "source.bin", "conversion": "identity", "source_sha256": hashlib.sha256(b"original").hexdigest(), "output_sha256": hashlib.sha256(b"original").hexdigest(), "source_size": 8, "output_size": 8}

            class AssetModule:
                @staticmethod
                def _behavior_map(*args):
                    return {}, {}

            tilesets = [{"symbol": f"gTileset_Test_{index}", "kind": "secondary", "assets": [{"metatile_count": 1}]} for index in range(97)]
            with self.assertRaisesRegex(scenery.ImportError, "pinned source hash/size mismatch"):
                scenery._source_assets({"layouts": [{"primary_tileset": "gTileset_Test_0", "secondary_tileset": "gTileset_Test_0", "assets": [asset]}], "tilesets": tilesets}, donor, AssetModule)
            self.assertEqual(list(donor.iterdir()), [donor / "source.bin"])

    def test_pending_runtime_gate_is_explicit(self):
        registration = load("data/johto/scenery_registration.json")
        self.assertFalse(registration["runtime_readiness"]["ready"])
        self.assertEqual(set(registration["runtime_readiness"]["general_pending_layouts"]), set(scenery.PENDING_GENERAL_LAYOUTS))
        self.assertTrue(any("animation" in item.lower() for item in registration["runtime_readiness"]["pending"]))


if __name__ == "__main__":
    unittest.main()
