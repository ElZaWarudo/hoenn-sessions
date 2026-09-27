"""Contract tests for a release with more than two ROM worlds."""

from __future__ import annotations

import copy
import hashlib
import json
import struct
import tempfile
import unittest
from unittest import mock
from pathlib import Path

from tools import rom_release_catalog
from tools.rom_release_catalog import validate_catalog
from tools.coop import player_transfer_manifest as transfer_schema
from tools.coop import map_binding_manifest
from tools.coop.object_contract_manifest import (COUNT_PROBE, DESCRIPTOR, ROM_BASE, TABLES,
                                                  decode_descriptor, manifest_from_rom)
from tools.tests.test_object_contract_manifest import fixture as scalar_fixture


def synthetic_schema_payload() -> bytes:
    spans = (128, 128, 128, 128)
    cursors = [0] * transfer_schema.STORAGE_COUNT
    fields = []
    entries = list(transfer_schema.EXPECTED_FIELDS.items())
    for index, (field_id, (storage, owner)) in enumerate(entries):
        next_storage = entries[index + 1][1][0] if index + 1 < len(entries) else None
        size = spans[storage] - cursors[storage] if storage != next_storage else 1
        fields.append(transfer_schema.FIELD_STRUCT.pack(field_id, storage, owner,
                                                        cursors[storage], size, 0))
        cursors[storage] += size
    count = len(fields)
    header = transfer_schema.HEADER_STRUCT.pack(
        transfer_schema.SCHEMA_MAGIC, transfer_schema.SCHEMA_VERSION, count,
        transfer_schema.HEADER_SIZE + count * transfer_schema.FIELD_SIZE,
        transfer_schema.HEADER_SIZE, *spans, transfer_schema.HEADER_SIZE,
        transfer_schema.FIELD_SIZE, 0)
    return header + b"".join(fields)


class RomReleaseCatalogTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.build_path = self.root / "rom_worlds.json"
        self.catalog_path = self.root / "release.json"
        names = ("main", "cormoria", "third")
        self.builds = {"schema_version": 1, "default_world": "main", "worlds": [
            {"name": name, "world_id": index, "build_bit": 1 << (index - 1),
             "game_version": None if index == 1 else "EMERALD",
             "map_version": None if index == 1 else "emerald",
             "build_name": None if index == 1 else f"emerald-{name}",
             "title": None if index == 1 else name.upper(),
             "game_code": None if index == 1 else f"BP{index:02d}"}
            for index, name in enumerate(names, 1)]}
        entries = []
        payload = synthetic_schema_payload()
        decoded = transfer_schema.parse_schema_payload(payload)
        template_bytes = (Path(__file__).parent / "fixtures" / "arrival-v3-cormoria-carabrue.sav").read_bytes()
        for index, name in enumerate(names, 1):
            rom = f"{name}.gba"
            bridge = f"{name}.bridge.json"
            transfer = f"{name}.player-transfer.json"
            image = bytearray(0x8000)
            image[:len(name)] = name.encode()
            image[0x3500:0x3500 + len(payload)] = payload
            struct.pack_into("<I", image, 0x300 + 79 * 4, map_binding_manifest.ROM_START + 0x500)
            struct.pack_into("<I", image, 0x300 + 13 * 4, map_binding_manifest.ROM_START + 0x700)
            struct.pack_into("<I", image, 0x300 + 82 * 4, map_binding_manifest.ROM_START + 0x800)
            struct.pack_into("<I", image, 0x500 + 4, map_binding_manifest.ROM_START + 0x600)
            struct.pack_into("<I", image, 0x700 + 10 * 4, map_binding_manifest.ROM_START + 0x900)
            struct.pack_into("<I", image, 0x800 + 21 * 4, map_binding_manifest.ROM_START + 0xA00)
            struct.pack_into("<I", image, 0x600, map_binding_manifest.ROM_START + 0x1000)
            struct.pack_into("<H", image, 0x600 + 0x12, 1194)
            struct.pack_into("<I", image, 0x900, map_binding_manifest.ROM_START + 0x1100)
            struct.pack_into("<H", image, 0x900 + 0x12, 88)
            struct.pack_into("<I", image, 0xA00, map_binding_manifest.ROM_START + 0x1200)
            struct.pack_into("<H", image, 0xA00 + 0x12, 1314)
            struct.pack_into("<I", image, 0x2000 + (1194 - 1) * 4,
                             map_binding_manifest.ROM_START + 0x1000)
            struct.pack_into("<I", image, 0x2000 + (1193 - 1) * 4,
                             map_binding_manifest.ROM_START + 0x1300)
            struct.pack_into("<I", image, 0x2000 + (88 - 1) * 4,
                             map_binding_manifest.ROM_START + 0x1100)
            struct.pack_into("<I", image, 0x2000 + (1314 - 1) * 4,
                             map_binding_manifest.ROM_START + 0x1200)
            scalar_rom, scalar_symbols = scalar_fixture()
            scalar_offset = 0x5000
            image[scalar_offset:scalar_offset + len(scalar_rom)] = scalar_rom
            descriptor_address, descriptor_size = scalar_symbols[DESCRIPTOR]
            descriptor_start = descriptor_address - ROM_BASE
            layouts, _, _ = decode_descriptor(
                scalar_rom[descriptor_start:descriptor_start + descriptor_size])
            for table, (stride, pointers, _) in zip(TABLES, layouts):
                address, size = scalar_symbols[table]
                for row in range(size // stride):
                    for pointer_offset in pointers:
                        position = scalar_offset + address - ROM_BASE + row * stride + pointer_offset
                        pointer, = struct.unpack_from("<I", image, position)
                        if pointer:
                            struct.pack_into("<I", image, position, pointer + scalar_offset)
            scalar_symbols = {symbol: (address + scalar_offset, size)
                              for symbol, (address, size) in scalar_symbols.items()}
            (self.root / rom).write_bytes(image)
            (self.root / bridge).write_text(json.dumps({
                "game_build": {"rom_sha256": hashlib.sha256((self.root / rom).read_bytes()).hexdigest()},
                "save": {"schema_version": 2},
            }), encoding="utf-8")
            (self.root / transfer).write_text(json.dumps({
                "rom_sha256": hashlib.sha256((self.root / rom).read_bytes()).hexdigest(),
                "schema_version": transfer_schema.SCHEMA_VERSION,
                "sha256": hashlib.sha256(payload).hexdigest(),
                "address": transfer_schema.ROM_START + 0x3500,
                "size": len(payload),
                **decoded,
            }), encoding="utf-8")
            map_binding = f"{name}.map-binding.json"
            (self.root / map_binding).write_text(json.dumps({
                "schema_version": 1,
                "rom_sha256": hashlib.sha256(image).hexdigest(),
                "gMapGroups": {"address": map_binding_manifest.ROM_START + 0x300,
                               "size": 83 * 4},
                "gMapLayouts": {"address": map_binding_manifest.ROM_START + 0x2000,
                                "size": 1314 * 4},
                "group_lengths": [11 if n == 13 else 22 if n == 82 else 2 for n in range(83)],
            }), encoding="utf-8")
            object_scalar = f"{name}.object-scalar.json"
            (self.root / object_scalar).write_text(
                json.dumps(manifest_from_rom(bytes(image), scalar_symbols)), encoding="utf-8")
            arrivals = {}
            for arrival_id in ("from_previous", "from_next"):
                template_name = f"{name}-{arrival_id}.sav"
                (self.root / template_name).write_bytes(template_bytes)
                arrivals[arrival_id] = {
                    "map_group": 79, "map_number": 1, "warp_id": 255,
                    "map_layout_id": 1194,
                    "template_sav_path": template_name,
                    "template_sav_sha256": hashlib.sha256(template_bytes).hexdigest(),
                }
            entries.append({
                "name": name, "world_id": index, "save_namespace": f"save_{name}",
                "presence_regions": ["CORMORIA"],
                "shared_player_schema": 2, "regional_save_schema": 1,
                # A later third-world release needs a revised location codec.
                "location_codec": 3, "object_catalog_sha256": "a" * 64,
                "owned_location_sections": [0, 249] if index == 1 else [250, 300] if index == 2 else [301, 320],
                "rom_path": rom,
                "rom_sha256": hashlib.sha256((self.root / rom).read_bytes()).hexdigest(),
                "bridge_path": bridge,
                "bridge_sha256": hashlib.sha256((self.root / bridge).read_bytes()).hexdigest(),
                "player_transfer_path": transfer,
                "player_transfer_sha256": hashlib.sha256((self.root / transfer).read_bytes()).hexdigest(),
                "map_binding_path": map_binding,
                "map_binding_sha256": hashlib.sha256((self.root / map_binding).read_bytes()).hexdigest(),
                "object_scalar_path": object_scalar,
                "object_scalar_sha256": hashlib.sha256((self.root / object_scalar).read_bytes()).hexdigest(),
                "arrivals": arrivals,
                "portals": []})
        for index, entry in enumerate(entries):
            next_index = (index + 1) % 3
            previous_index = (index - 1) % 3
            entry["portals"] = [
                {"id": "to_next", "destination_world_id": next_index + 1,
                 "arrival_portal_id": "from_previous", "return_portal_id": "to_previous"},
                {"id": "to_previous", "destination_world_id": previous_index + 1,
                 "arrival_portal_id": "from_next", "return_portal_id": "to_next"},
            ]
        self.catalog = {"schema_version": 1, "worlds": entries}

    def validate(self, catalog: dict | None = None) -> dict:
        self.build_path.write_text(json.dumps(self.builds), encoding="utf-8")
        self.catalog_path.write_text(json.dumps(catalog or self.catalog), encoding="utf-8")
        trusted = hashlib.sha256(self.catalog_path.read_bytes()).hexdigest()
        return validate_catalog(self.build_path, self.catalog_path, trusted)

    def test_rejects_catalog_replaced_after_trust_is_set(self) -> None:
        self.validate()
        trusted = hashlib.sha256(self.catalog_path.read_bytes()).hexdigest()
        self.catalog_path.write_text("{}", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "trusted digest"):
            validate_catalog(self.build_path, self.catalog_path, trusted)

    def test_three_world_cycle_and_return_edges(self) -> None:
        self.assertEqual(set(self.validate()), {1, 2, 3})

    def test_rejects_changed_artifact_before_travel(self) -> None:
        (self.root / "third.gba").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "digest mismatch"):
            self.validate()

    def test_rejects_missing_arrival_template(self) -> None:
        self.catalog["worlds"][1]["arrivals"]["from_previous"].pop("template_sav_path")
        with self.assertRaisesRegex(ValueError, "release-relative"):
            self.validate()

    def test_rejects_wrong_arrival_template_size(self) -> None:
        arrival = self.catalog["worlds"][1]["arrivals"]["from_previous"]
        template = self.root / arrival["template_sav_path"]
        template.write_bytes(b"too short")
        arrival["template_sav_sha256"] = hashlib.sha256(template.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "invalid arrival save size"):
            self.validate()

    def test_rejects_arrival_template_with_wrong_saved_map(self) -> None:
        self.catalog["worlds"][1]["arrivals"]["from_previous"]["map_number"] = 2
        with self.assertRaisesRegex(ValueError, "outside linked tables"):
            self.validate()

    def test_rejects_arrival_template_with_wrong_saved_layout(self) -> None:
        self.catalog["worlds"][1]["arrivals"]["from_previous"]["map_layout_id"] = 1314
        with self.assertRaisesRegex(ValueError, "map/layout does not match shipped ROM"):
            self.validate()

    def test_rejects_save_and_catalog_agree_on_wrong_rom_layout(self) -> None:
        self.catalog["worlds"][1]["arrivals"]["from_previous"]["map_layout_id"] = 1193
        with mock.patch.object(rom_release_catalog, "_verify_arrival_save"):
            with self.assertRaisesRegex(ValueError, "map/layout does not match shipped ROM"):
                self.validate()

    def test_rejects_null_map_group_pointer(self) -> None:
        rom = self.root / "cormoria.gba"
        image = bytearray(rom.read_bytes())
        struct.pack_into("<I", image, 0x300 + 79 * 4, 0)
        rom.write_bytes(image)
        self._refresh_rom_hash(1)
        with self.assertRaisesRegex(ValueError, "map pointer is null"):
            self.validate()

    def test_rejects_out_of_window_map_header_pointer(self) -> None:
        rom = self.root / "cormoria.gba"
        image = bytearray(rom.read_bytes())
        struct.pack_into("<I", image, 0x500 + 4, 0x0A000000)
        rom.write_bytes(image)
        self._refresh_rom_hash(1)
        with self.assertRaisesRegex(ValueError, "outside shipped ROM"):
            self.validate()

    def test_rejects_map_number_outside_group(self) -> None:
        self.catalog["worlds"][1]["arrivals"]["from_previous"]["map_number"] = 2
        with self.assertRaisesRegex(ValueError, "outside linked tables"):
            self.validate()

    def test_rejects_signed_warpdata_map_coordinates(self) -> None:
        for field in ("map_group", "map_number"):
            for value in (128, 255):
                with self.subTest(field=field, value=value):
                    catalog = copy.deepcopy(self.catalog)
                    catalog["worlds"][1]["arrivals"]["from_previous"][field] = value
                    with self.assertRaisesRegex(ValueError, f"{field} must be an integer from 0 through 127"):
                        self.validate(catalog)

    def test_rejects_signed_warpdata_warp_ids(self) -> None:
        for warp_id in (128, 254):
            with self.subTest(warp_id=warp_id):
                catalog = copy.deepcopy(self.catalog)
                catalog["worlds"][1]["arrivals"]["from_previous"]["warp_id"] = warp_id
                with self.assertRaisesRegex(ValueError, "warp_id must be 0 through 127 or 255"):
                    self.validate(catalog)

    def test_accepts_warp_id_255_sentinel(self) -> None:
        self.catalog["worlds"][1]["arrivals"]["from_previous"]["warp_id"] = 255
        self.validate()

    def _refresh_rom_hash(self, world_index: int) -> None:
        entry = self.catalog["worlds"][world_index]
        entry["rom_sha256"] = hashlib.sha256((self.root / entry["rom_path"]).read_bytes()).hexdigest()
        for prefix in ("bridge", "player_transfer", "map_binding", "object_scalar"):
            path = self.root / entry[f"{prefix}_path"]
            data = json.loads(path.read_text(encoding="utf-8"))
            if prefix == "bridge":
                data["game_build"]["rom_sha256"] = entry["rom_sha256"]
            else:
                data["rom_sha256"] = entry["rom_sha256"]
            path.write_text(json.dumps(data), encoding="utf-8")
            entry[f"{prefix}_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()

    def test_rejects_object_scalar_manifest_for_another_rom(self) -> None:
        entry = self.catalog["worlds"][2]
        path = self.root / entry["object_scalar_path"]
        manifest = json.loads(path.read_text(encoding="utf-8"))
        manifest["rom_sha256"] = self.catalog["worlds"][0]["rom_sha256"]
        path.write_text(json.dumps(manifest), encoding="utf-8")
        entry["object_scalar_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "object scalar manifest does not match ROM"):
            self.validate()

    def test_rejects_changed_object_scalar_artifact(self) -> None:
        entry = self.catalog["worlds"][2]
        path = self.root / entry["object_scalar_path"]
        path.write_bytes(path.read_bytes() + b" ")
        with self.assertRaisesRegex(ValueError, "artifact digest mismatch"):
            self.validate()

    def test_rejects_different_world_object_scalars(self) -> None:
        entry = self.catalog["worlds"][2]
        path = self.root / entry["object_scalar_path"]
        manifest = json.loads(path.read_text(encoding="utf-8"))
        rom_path = self.root / entry["rom_path"]
        rom = bytearray(rom_path.read_bytes())
        rom[manifest["tables"]["gItemsInfo"]["address"] - ROM_BASE] ^= 1
        rom_path.write_bytes(rom)
        self._refresh_rom_hash(2)
        symbols = {DESCRIPTOR: (manifest["descriptor"]["address"], manifest["descriptor"]["size"]),
                   COUNT_PROBE: (manifest["count_probe"]["address"], manifest["count_probe"]["size"])}
        symbols.update({name: (manifest["tables"][name]["address"], manifest["tables"][name]["size"])
                        for name in TABLES})
        path.write_text(json.dumps(manifest_from_rom(bytes(rom), symbols)), encoding="utf-8")
        entry["object_scalar_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "world ROMs disagree on shared-object scalar"):
            self.validate()

    def test_accepts_fresh_main_and_cormoria_harbor_saves(self) -> None:
        self.catalog["worlds"][0]["presence_regions"] = ["HOENN", "CORMORIA"]
        for world_index, portal, fixture, group, number, layout in (
            (0, "from_next", "arrival-v3-main-lilycove.sav", 13, 10, 88),
            (1, "from_previous", "arrival-v3-cormoria-rivetshore.sav", 82, 21, 1314),
        ):
            arrival = self.catalog["worlds"][world_index]["arrivals"][portal]
            image = (Path(__file__).parent / "fixtures" / fixture).read_bytes()
            (self.root / arrival["template_sav_path"]).write_bytes(image)
            arrival.update(map_group=group, map_number=number, warp_id=255,
                           map_layout_id=layout,
                           template_sav_sha256=hashlib.sha256(image).hexdigest())
        self.validate()

    def test_rejects_template_replaced_between_hash_and_v2_parse(self) -> None:
        arrival = self.catalog["worlds"][1]["arrivals"]["from_previous"]
        template = self.root / arrival["template_sav_path"]
        valid_bytes = template.read_bytes()
        template.write_bytes(b"invalid before hash".ljust(131088, b"\0"))
        arrival["template_sav_sha256"] = hashlib.sha256(template.read_bytes()).hexdigest()
        original = rom_release_catalog._artifact
        changed = False

        def swap_after_hash(root: Path, name: object, digest: object) -> Path:
            nonlocal changed
            path = original(root, name, digest)
            if name == arrival["template_sav_path"] and not changed:
                changed = True
                template.write_bytes(valid_bytes)
            return path

        with mock.patch.object(rom_release_catalog, "_artifact", side_effect=swap_after_hash):
            with self.assertRaisesRegex(ValueError, "digest changed during validation"):
                self.validate()

    def test_rejects_noncanonical_artifact_path(self) -> None:
        arrival = self.catalog["worlds"][1]["arrivals"]["from_previous"]
        arrival["template_sav_path"] = ".//" + arrival["template_sav_path"]
        with self.assertRaisesRegex(ValueError, "release-relative"):
            self.validate()

    def test_rejects_bridge_from_another_rom(self) -> None:
        bridge = self.root / "third.bridge.json"
        bridge.write_text(json.dumps({"game_build": {"rom_sha256": "b" * 64},
                                      "save": {"schema_version": 2}}), encoding="utf-8")
        self.catalog["worlds"][2]["bridge_sha256"] = hashlib.sha256(bridge.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "bridge manifest does not match"):
            self.validate()

    def test_rejects_bridge_with_different_save_schema(self) -> None:
        bridge = self.root / "third.bridge.json"
        bridge.write_text(json.dumps({
            "game_build": {"rom_sha256": self.catalog["worlds"][2]["rom_sha256"]},
            "save": {"schema_version": 1},
        }), encoding="utf-8")
        self.catalog["worlds"][2]["bridge_sha256"] = hashlib.sha256(bridge.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "bridge manifest does not match"):
            self.validate()

    def test_rejects_malformed_bridge_manifest(self) -> None:
        bridge = self.root / "third.bridge.json"
        bridge.write_text("{}", encoding="utf-8")
        self.catalog["worlds"][2]["bridge_sha256"] = hashlib.sha256(bridge.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "invalid bridge manifest"):
            self.validate()

    def test_rejects_unknown_destination(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][0]["portals"][0]["destination_world_id"] = 4
        with self.assertRaisesRegex(ValueError, "unknown or self-referential"):
            self.validate(catalog)

    def test_rejects_mismatched_return_id(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][1]["portals"][1]["return_portal_id"] = "to_previous"
        with self.assertRaisesRegex(ValueError, "portal return does not name"):
            self.validate(catalog)

    def test_rejects_portal_ids_too_long_for_server_catalog(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][0]["portals"][0]["id"] = "p" * 97
        with self.assertRaisesRegex(ValueError, "invalid portal ID"):
            self.validate(catalog)

    def test_rejects_more_routes_than_server_catalog_can_hold(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        for index in range(129):
            forward = f"extra_to_cormoria_{index}"
            reverse = f"extra_to_main_{index}"
            catalog["worlds"][0]["portals"].append({
                "id": forward, "destination_world_id": 2,
                "arrival_portal_id": "from_main", "return_portal_id": reverse,
            })
            catalog["worlds"][1]["portals"].append({
                "id": reverse, "destination_world_id": 1,
                "arrival_portal_id": "from_cormoria", "return_portal_id": forward,
            })
        with self.assertRaisesRegex(ValueError, "too many world portals"):
            self.validate(catalog)

    def test_rejects_missing_reverse_portal(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][1]["portals"] = catalog["worlds"][1]["portals"][:1]
        with self.assertRaisesRegex(ValueError, "reciprocal return"):
            self.validate(catalog)

    def test_rejects_disconnected_future_region(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][0]["portals"] = catalog["worlds"][0]["portals"][:1]
        catalog["worlds"][1]["portals"] = catalog["worlds"][1]["portals"][1:]
        catalog["worlds"][2]["portals"] = []
        with self.assertRaisesRegex(ValueError, "unreachable from the default world"):
            self.validate(catalog)

    def test_rejects_mismatched_player_objects(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][2]["object_catalog_sha256"] = "b" * 64
        with self.assertRaisesRegex(ValueError, "shared-player contract"):
            self.validate(catalog)

    def test_rejects_different_transfer_layout_in_third_world(self) -> None:
        transfer = self.root / "third.player-transfer.json"
        manifest = json.loads(transfer.read_text(encoding="utf-8"))
        manifest["sha256"] = "d" * 64
        transfer.write_text(json.dumps(manifest), encoding="utf-8")
        self.catalog["worlds"][2]["player_transfer_sha256"] = hashlib.sha256(transfer.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "manifest does not match ROM descriptor"):
            self.validate()

    def test_rejects_stale_manifest_for_changed_third_rom(self) -> None:
        rom = self.root / "third.gba"
        changed = bytearray(rom.read_bytes())
        changed[0x3500 + transfer_schema.HEADER_SIZE + 3] = transfer_schema.OWNER_SHARED_PLAYER
        rom.write_bytes(changed)
        self.catalog["worlds"][2]["rom_sha256"] = hashlib.sha256(rom.read_bytes()).hexdigest()
        bridge = self.root / "third.bridge.json"
        bridge_data = json.loads(bridge.read_text(encoding="utf-8"))
        bridge_data["game_build"]["rom_sha256"] = self.catalog["worlds"][2]["rom_sha256"]
        bridge.write_text(json.dumps(bridge_data), encoding="utf-8")
        self.catalog["worlds"][2]["bridge_sha256"] = hashlib.sha256(bridge.read_bytes()).hexdigest()
        transfer = self.root / "third.player-transfer.json"
        transfer_data = json.loads(transfer.read_text(encoding="utf-8"))
        transfer_data["rom_sha256"] = self.catalog["worlds"][2]["rom_sha256"]
        transfer.write_text(json.dumps(transfer_data), encoding="utf-8")
        self.catalog["worlds"][2]["player_transfer_sha256"] = hashlib.sha256(transfer.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "invalid player-transfer ROM descriptor"):
            self.validate()

    def test_rejects_transfer_manifest_for_another_rom(self) -> None:
        transfer = self.root / "third.player-transfer.json"
        manifest = json.loads(transfer.read_text(encoding="utf-8"))
        manifest["rom_sha256"] = self.catalog["worlds"][0]["rom_sha256"]
        transfer.write_text(json.dumps(manifest), encoding="utf-8")
        self.catalog["worlds"][2]["player_transfer_sha256"] = hashlib.sha256(transfer.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "player-transfer manifest does not match"):
            self.validate()

    def test_rejects_path_escape(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][0]["rom_path"] = "../main.gba"
        with self.assertRaisesRegex(ValueError, "release-relative"):
            self.validate(catalog)

    def test_rejects_symlink_outside_release(self) -> None:
        outside = self.root.parent / f"{self.root.name}-outside.gba"
        outside.write_bytes(b"outside")
        self.addCleanup(outside.unlink)
        link = self.root / "outside.gba"
        try:
            link.symlink_to(outside)
        except OSError as exc:
            self.skipTest(f"symlinks unavailable: {exc}")
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][0]["rom_path"] = link.name
        catalog["worlds"][0]["rom_sha256"] = hashlib.sha256(outside.read_bytes()).hexdigest()
        with self.assertRaisesRegex(ValueError, "escapes release"):
            self.validate(catalog)

    def test_rejects_duplicate_save_namespace(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][2]["save_namespace"] = "save_main"
        with self.assertRaisesRegex(ValueError, "duplicate or mismatched"):
            self.validate(catalog)

    def test_rejects_overlapping_third_world_location_sections(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][2]["owned_location_sections"] = [300, 320]
        with self.assertRaisesRegex(ValueError, "location sections overlap"):
            self.validate(catalog)

    def test_rejects_reversed_location_section_range(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][2]["owned_location_sections"] = [320, 301]
        with self.assertRaisesRegex(ValueError, "reversed location-section range"):
            self.validate(catalog)

    def test_rejects_missing_location_section_allocation(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        del catalog["worlds"][2]["owned_location_sections"]
        with self.assertRaisesRegex(ValueError, "owned location-section range"):
            self.validate(catalog)

    def test_rejects_missing_world_presence_regions(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        del catalog["worlds"][1]["presence_regions"]
        with self.assertRaisesRegex(ValueError, "invalid presence regions"):
            self.validate(catalog)

    def test_rejects_arrival_outside_world_presence_regions(self) -> None:
        catalog = copy.deepcopy(self.catalog)
        catalog["worlds"][0]["presence_regions"] = ["HOENN"]
        with self.assertRaisesRegex(ValueError, "arrival map is outside presence regions"):
            self.validate(catalog)


if __name__ == "__main__":
    unittest.main()
