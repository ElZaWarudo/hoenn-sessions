"""Validate imported scenery and the pilot's escape path without an emulator."""
import hashlib
import json
from pathlib import Path
import re
import struct
import unittest

from tools.johto.import_new_bark_assets import (
    REPOSITORY, REVISION, asset_paths, convert_attributes,
)

ROOT = Path(__file__).resolve().parents[2]


class NewBarkImportTests(unittest.TestCase):
    def test_existing_region_names_keep_utf8_accents(self):
        sections = json.loads((ROOT / "src/data/region_map/region_map_sections.json")
                              .read_text(encoding="utf-8"))["map_sections"]
        names = {section["id"]: section.get("name") for section in sections}
        for place in ("MANSION", "LEAGUE", "TOWER"):
            self.assertEqual(names[f"MAPSEC_POKEMON_{place}"], f"POK\u00e9MON {place}")

    def test_attribute_conversion_preserves_layers_and_removes_headbutt_collision(self):
        self.assertEqual(convert_attributes(struct.pack("<3H", 0x0002, 0x1010, 0x20A1)),
                         struct.pack("<3I", 0x00000002, 0x20000010, 0x40000000))
        for unsupported in (0x0100, 0x3000):
            with self.assertRaises(ValueError):
                convert_attributes(struct.pack("<H", unsupported))
        with self.assertRaises(struct.error):
            convert_attributes(b"\x00")

    def test_registered_town_exits_are_reciprocal_or_controlled(self):
        town = json.loads((ROOT / "data/maps/NewBarkTown/map.json").read_text(encoding="utf-8"))
        groups = json.loads((ROOT / "data/maps/map_groups.json").read_text(encoding="utf-8"))
        self.assertIn("NewBarkTown", groups["gMapGroup_Johto"])
        self.assertEqual(town["region"], "REGION_JOHTO")

        map_names = {name for maps in groups.values() for name in maps}
        maps_by_id = {}
        for name in map_names:
            path = ROOT / "data/maps" / name / "map.json"
            if path.exists():
                map_data = json.loads(path.read_text(encoding="utf-8"))
                maps_by_id[map_data["id"]] = map_data

        expected_connections = {
            ("left", "MAP_ROUTE29", -5),
            ("right", "MAP_ROUTE27", -11),
        }
        self.assertEqual(
            {(edge["direction"], edge["map"], edge["offset"]) for edge in town["connections"]},
            expected_connections,
        )
        opposite = {"left": "right", "right": "left", "up": "down", "down": "up"}
        for edge in town["connections"]:
            with self.subTest(connection=edge["map"]):
                reverse_edges = maps_by_id[edge["map"]]["connections"]
                self.assertIn(
                    {
                        "map": town["id"],
                        "offset": -edge["offset"],
                        "direction": opposite[edge["direction"]],
                    },
                    reverse_edges,
                )

        # Five doors round-trip through their declared destination warp. The
        # remaining donor-authored roof transition is intentionally one-way,
        # but still resolves to a registered map and an in-range warp.
        controlled_one_way = {("MAP_TIN_TOWER_ROOF_DAY", "0")}
        observed_one_way = set()
        for index, warp in enumerate(town["warp_events"]):
            with self.subTest(warp=index):
                destination = maps_by_id[warp["dest_map"]]
                destination_index = int(warp["dest_warp_id"])
                self.assertLess(destination_index, len(destination["warp_events"]))
                reverse = destination["warp_events"][destination_index]
                if (reverse["dest_map"], reverse["dest_warp_id"]) != (town["id"], str(index)):
                    observed_one_way.add((warp["dest_map"], warp["dest_warp_id"]))
        self.assertEqual(observed_one_way, controlled_one_way)

    def test_pilot_guides_preserve_yes_no_script_contracts(self):
        # Static script contracts only: asynchronous message/warp execution still
        # needs emulator coverage; this does not simulate the event-script engine.
        routes = (
            ("OldaleTown_PokemonCenter_1F", "JohtoGuide", "VisitJohto",
             "StayInHoenn", "MAP_NEW_BARK_TOWN, 10, 15"),
            ("NewBarkTown", "Guide", "Return", "Stay",
             "MAP_OLDALE_TOWN_POKEMON_CENTER_1F, 10, 4"),
        )
        for map_name, guide, prompt, decline, destination in routes:
            with self.subTest(map=map_name):
                directory = ROOT / "data/maps" / map_name
                guide_label = f"{map_name}_EventScript_{guide}"
                decline_label = f"{map_name}_EventScript_{decline}"

                # The Hoenn-side guide remains a live object. Full-world import
                # replaces New Bark's preview object composition with donor
                # objects and provides its return paths through world topology.
                if map_name == "OldaleTown_PokemonCenter_1F":
                    map_data = json.loads((directory / "map.json").read_text(encoding="utf-8"))
                    self.assertTrue(any(
                        obj["script"] == guide_label
                        and (obj["x"], obj["y"], obj["flag"]) == (10, 3, "0")
                        for obj in map_data["object_events"]
                    ))

                script = (directory / "scripts.inc").read_text(encoding="utf-8")
                for label, expected in (
                    (guide_label, [
                        "lock", "faceplayer",
                        f"msgbox {map_name}_Text_{prompt}, MSGBOX_YESNO",
                        f"goto_if_eq VAR_RESULT, NO, {decline_label}",
                        f"warp {destination}", "waitstate", "release", "end",
                    ]),
                    (decline_label, ["release", "end"]),
                ):
                    blocks = re.findall(
                        rf"^{re.escape(label)}::?\s*\n(.*?)(?=^\w+:|\Z)",
                        script, flags=re.MULTILINE | re.DOTALL,
                    )
                    self.assertEqual(len(blocks), 1, label)
                    commands = [line.strip() for line in blocks[0].splitlines()
                                if line.strip()]
                    self.assertEqual(commands, expected, label)

    def test_assets_match_pinned_manifest_and_layout_format(self):
        manifest = json.loads((ROOT / "data/johto/new_bark_source.json").read_text())
        expected_paths = asset_paths()
        manifest_paths = [asset["path"] for asset in manifest["assets"]]
        # Independent fixed count catches matching omissions in helper/manifest.
        self.assertEqual(len(expected_paths), 34)
        self.assertEqual(len(set(expected_paths)), 34)
        self.assertEqual(len(manifest_paths), 34)
        self.assertEqual(len(set(manifest_paths)), 34)
        self.assertEqual(set(manifest_paths), set(expected_paths))
        self.assertEqual(manifest["repository"], REPOSITORY)
        self.assertEqual(manifest["revision"], REVISION)
        for asset in manifest["assets"]:
            with self.subTest(path=asset["path"]):
                data = (ROOT / asset["path"]).read_bytes()
                self.assertEqual(hashlib.sha256(data).hexdigest(), asset["import_sha256"])
        layouts = json.loads((ROOT / "data/layouts/layouts.json").read_text())["layouts"]
        layout = next(x for x in layouts if x["id"] == "LAYOUT_NEW_BARK_TOWN")
        self.assertEqual(layout["layout_version"], "frlg")
        data = (ROOT / layout["blockdata_filepath"]).read_bytes()
        self.assertEqual(len(data), layout["width"] * layout["height"] * 2)
        blocks = struct.unpack(f"<{len(data)//2}H", data)
        # Guide and landing positions must be walkable, at elevation 3.
        for x, y in ((10, 15), (11, 15)):
            self.assertEqual(blocks[y * layout["width"] + x] & 0xFC00, 0x3000)
        for tileset in ("primary/johto_general", "secondary/new_bark_town"):
            directory = ROOT / "data/tilesets" / tileset
            metatiles = (directory / "metatiles.bin").read_bytes()
            attributes = (directory / "metatile_attributes.bin").read_bytes()
            self.assertEqual(len(metatiles) // 16, len(attributes) // 4)
            self.assertEqual(len(attributes) % 4, 0)


if __name__ == "__main__":
    unittest.main()
