"""Authenticated Cormoria map/layout registration and installed-prefix checks."""

from __future__ import annotations

import copy
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.cormoria import import_world, register_maps

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_STAGE = Path.home() / ".codex/cormoria-swarm-artifacts/content-stage-20260923-v5"


class CormoriaMapRegistrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.region, _, _ = import_world.load_manifests(ROOT)
        cls.stage = Path(os.environ.get("CORMORIA_STAGE", DEFAULT_STAGE))

    def test_all_maps_keep_pinned_group_and_index(self) -> None:
        ordered = register_maps._ordered_maps(self.region)
        self.assertEqual(len(ordered), 165)
        self.assertEqual([(item["group"], item["index"]) for item in ordered],
                         [(group["host_group"], index)
                          for group in self.region["groups"]
                          for index in range(len(group["maps"]))])
        self.assertEqual(ordered[0]["target_name"], "Cormoria_CarabrueTown")

        tampered = copy.deepcopy(self.region)
        tampered["maps"][0]["index"] = 1
        with self.assertRaises(register_maps.MapRegistrationError):
            register_maps._ordered_maps(tampered)

    def test_host_map_or_layout_reorder_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for path in (register_maps.HOST_GROUPS, register_maps.HOST_LAYOUTS):
                destination = root / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes((ROOT / path).read_bytes())
            base_groups, base_layouts = register_maps._host_registries(root)
            self.assertEqual(len(base_groups["group_order"]), 79)
            self.assertEqual(len(base_layouts["layouts"]), 1192)

            groups_path = root / register_maps.HOST_GROUPS
            layouts_path = root / register_maps.HOST_LAYOUTS
            later_groups = json.loads(groups_path.read_text(encoding="utf-8"))
            later_groups["group_order"].append("gMapGroup_Future")
            later_groups["gMapGroup_Future"] = ["Future_Map"]
            groups_path.write_text(json.dumps(later_groups), encoding="utf-8")
            later_layouts = json.loads(layouts_path.read_text(encoding="utf-8"))
            later_layouts["layouts"].append({"id": "LAYOUT_FUTURE"})
            layouts_path.write_text(json.dumps(later_layouts), encoding="utf-8")
            self.assertEqual(register_maps._host_registries(root), (base_groups, base_layouts))

            groups_path.write_bytes((ROOT / register_maps.HOST_GROUPS).read_bytes())
            layouts_path.write_bytes((ROOT / register_maps.HOST_LAYOUTS).read_bytes())
            groups = json.loads(groups_path.read_text(encoding="utf-8"))
            groups[groups["group_order"][0]][:2] = reversed(groups[groups["group_order"][0]][:2])
            groups_path.write_text(json.dumps(groups), encoding="utf-8")
            with self.assertRaises(register_maps.MapRegistrationError):
                register_maps._host_registries(root)

            groups_path.write_bytes((ROOT / register_maps.HOST_GROUPS).read_bytes())
            layouts = json.loads(layouts_path.read_text(encoding="utf-8"))
            layouts["layouts"][:2] = reversed(layouts["layouts"][:2])
            layouts_path.write_text(json.dumps(layouts), encoding="utf-8")
            with self.assertRaises(register_maps.MapRegistrationError):
                register_maps._host_registries(root)

    def test_object_movement_radius_must_fit_packed_template(self) -> None:
        mapped = {"name": "Cormoria_FutureMap", "object_events": [{
            "movement_range_x": 0, "movement_range_y": 16,
        }]}
        with self.assertRaises(register_maps.MapRegistrationError):
            register_maps._adapt_object_ranges(mapped)

    def test_authenticated_stage_builds_full_registry_preview(self) -> None:
        if not self.stage.is_dir():
            self.skipTest("pinned Cormoria stage is unavailable")
        files = register_maps.build_registration(self.stage, ROOT)
        self.assertEqual(len(files), 497)
        groups = json.loads(files["data/maps/map_groups.json"])
        layouts = json.loads(files["data/layouts/layouts.json"])
        host_groups = json.loads((ROOT / register_maps.HOST_GROUPS).read_text(encoding="utf-8"))
        host_layouts = json.loads((ROOT / register_maps.HOST_LAYOUTS).read_text(encoding="utf-8"))
        self.assertEqual(groups["group_order"][:79], host_groups["group_order"][:79])
        self.assertEqual(groups["gMapGroup_Cormoria_Phase1"][0], "Cormoria_CarabrueTown")
        self.assertEqual([len(groups[name]) for name in groups["group_order"][79:]],
                         [28, 32, 40, 31, 30, 4])
        self.assertEqual(layouts["layouts"][:1192], host_layouts["layouts"][:1192])
        self.assertEqual(len(layouts["layouts"]), 1357)
        self.assertEqual(layouts["layouts"][1192]["border_filepath"],
                         "data/cormoria/layouts/CarabrueTown/border.bin")
        self.assertEqual(layouts["layouts"][1192]["rom_world"], "cormoria")
        pelluca = json.loads(files["data/maps/Cormoria_PellucaCity/map.json"])
        self.assertEqual(pelluca["object_events"][3]["movement_range_y"], 4)
        self.assertEqual(pelluca["region"], "REGION_CORMORIA")
        self.assertTrue(all(json.loads(data)["region"] == "REGION_CORMORIA"
                            for path, data in files.items() if path.startswith("data/maps/Cormoria_")))
        berry_ids = [event["trainer_sight_or_berry_tree_id"]
                     for path, data in files.items() if path.startswith("data/maps/Cormoria_")
                     for event in json.loads(data)["object_events"]
                     if event["movement_type"] == "MOVEMENT_TYPE_BERRY_TREE_GROWTH"]
        self.assertEqual(len(berry_ids), 28)
        self.assertEqual(len(set(berry_ids)), 25)
        self.assertTrue(all(value.startswith("Cormoria_BERRY_TREE_") for value in berry_ids))

        original = register_maps._staged_bytes

        def altered(stage: Path, relative: str) -> bytes:
            data = original(stage, relative)
            return data + b" " if relative == "maps/Cormoria_CarabrueTown/map.json" else data

        with mock.patch.object(register_maps, "_staged_bytes", side_effect=altered):
            with self.assertRaises(register_maps.MapRegistrationError):
                register_maps.build_registration(self.stage, ROOT)

        read_bytes = Path.read_bytes
        binary_path = (self.stage / "source/data/layouts/CarabrueTown/border.bin").resolve()

        def altered_binary(path: Path) -> bytes:
            data = read_bytes(path)
            return data + b"\0" if path.resolve() == binary_path else data

        with mock.patch.object(Path, "read_bytes", altered_binary):
            with self.assertRaises(import_world.ImportError):
                register_maps.build_registration(self.stage, ROOT)

    def test_output_must_be_new_and_outside_source_trees(self) -> None:
        if not self.stage.is_dir():
            self.skipTest("pinned Cormoria stage is unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            existing = Path(temporary) / "existing"
            existing.mkdir()
            for output in (existing, ROOT / "data/cormoria/unsafe-registration",
                           self.stage / "unsafe-registration"):
                with self.subTest(output=output):
                    with self.assertRaises(register_maps.MapRegistrationError):
                        register_maps.write_registration(self.stage, output, ROOT)

    def test_authentic_region_map_grid_and_graphics(self) -> None:
        if not self.stage.is_dir():
            self.skipTest("pinned Cormoria stage is unavailable")
        output = register_maps.build_region_map(self.stage, ROOT)
        layout = output[register_maps.REGION_MAP_LAYOUT].decode("utf-8")
        rows = [register_maps.SECTION_TOKEN.findall(line) for line in layout.splitlines()
                if line.startswith("    {")]
        self.assertEqual(len(rows), 15)
        self.assertEqual({len(row) for row in rows}, {28})
        self.assertEqual(rows[3][25], "MAPSEC_CORMORIA_MT_CERAM")
        self.assertEqual(rows[8][18], "MAPSEC_CORMORIA_CARABRUE_TOWN")
        self.assertNotIn("MAPSEC_CERAM_PEAK,", layout)
        self.assertEqual(output["graphics/pokenav/region_map/map_cormoria.png"],
                         (self.stage / "source" / register_maps.REGION_MAP_PNG).read_bytes())
        self.assertEqual(output["graphics/pokenav/region_map/map_cormoria.bin"],
                         (self.stage / "source" / register_maps.REGION_MAP_TILEMAP).read_bytes())


@unittest.skipUnless(shutil.which("g++"), "mapjson integration needs g++")
class CormoriaMapHeaderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temp.cleanup)
        cls.directory = Path(cls.temp.name)
        cls.executable = cls.directory / "mapjson.exe"
        subprocess.run(["g++", "-std=c++17", "-O0", "mapjson.cpp", "json11.cpp",
                        "-o", str(cls.executable)], cwd=ROOT / "tools/mapjson",
                       check=True, capture_output=True, text=True)

    def test_cormoria_requires_explicit_matching_engine_region(self) -> None:
        stage = Path(os.environ.get("CORMORIA_STAGE", DEFAULT_STAGE))
        if not stage.is_dir():
            self.skipTest("pinned Cormoria stage is unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            sections = register_maps._load(stage / "source/src/data/region_map/region_map_sections.json")
            host = register_maps._load(ROOT / "src/data/region_map/region_map_sections.json")
            region, _, _ = import_world.load_manifests(ROOT)
            from tools.cormoria import register_sections
            all_sections = register_sections.append_sections(host["map_sections"], sections["map_sections"], region["sections"])
            section_path = work / "src/data/region_map/region_map_sections.json"
            section_path.parent.mkdir(parents=True)
            section_path.write_text(json.dumps({"map_sections": all_sections}), encoding="utf-8")
            group_path = work / "include/constants/map_groups.h"
            group_path.parent.mkdir(parents=True)
            group_path.write_text("MAP_LITTLEROOT_TOWN = (0\n", encoding="utf-8")
            source = json.loads((ROOT / "data/maps/LittlerootTown/map.json").read_text())
            source["region_map_section"] = "MAPSEC_CORMORIA_CARABRUE_TOWN"
            source["rom_world"] = "cormoria"
            layout_path = work / "layouts.json"
            layout_path.write_bytes((ROOT / "data/layouts/layouts.json").read_bytes())
            source_path = work / "map.json"

            def generate() -> subprocess.CompletedProcess:
                source_path.write_text(json.dumps(source), encoding="utf-8")
                return subprocess.run([str(self.executable), "map", "emerald",
                                       source_path.as_posix(), layout_path.as_posix(), work.as_posix()],
                                      cwd=work, capture_output=True, text=True)

            source["region"] = "REGION_CORMORIA"
            result = generate()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("\t.byte 3\n\tmap_header_flags", (work / "header.inc").read_text())
            source.pop("region")
            self.assertNotEqual(generate().returncode, 0)
            source["region"] = "REGION_HOENN"
            self.assertNotEqual(generate().returncode, 0)
            source["region"] = "REGION_CORMORIA"
            source["rom_world"] = "future_world"
            self.assertNotEqual(generate().returncode, 0)


if __name__ == "__main__":
    unittest.main()
