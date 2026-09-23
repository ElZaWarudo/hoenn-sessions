"""Assemble generated map tables for both worlds and compare their stable IDs."""
from __future__ import annotations

import json
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which("g++") and shutil.which("objcopy"), "requires GNU build tools")
class RomWorldTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.compiler_dir = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.compiler_dir.cleanup)
        cls.executable = Path(cls.compiler_dir.name) / "mapjson.exe"
        subprocess.run(["g++", "-std=c++17", "-O0", "mapjson.cpp", "json11.cpp",
                        "-o", str(cls.executable)], cwd=ROOT / "tools/mapjson",
                       check=True, capture_output=True, text=True)

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for path in ("data/maps", "data/layouts", "src/data", "include/constants", "tools/mapjson"):
            (self.root / path).mkdir(parents=True)
        self.write("tools/mapjson/required_map_defines.json", {"required_maps": [], "required_layouts": []})
        self.write("src/data/heal_locations.json", {})
        self.registry = {"schema_version": 1, "default_world": "main", "worlds": [
            {"name": "main", "world_id": 1, "build_bit": 1, "game_version": None,
             "map_version": None, "build_name": None, "title": None, "game_code": None},
            {"name": "cormoria", "world_id": 2, "build_bit": 2, "game_version": "EMERALD",
             "map_version": "emerald", "build_name": "emerald-cormoria",
             "title": "CORMORIA", "game_code": "BPCO"},
        ]}
        self.write("data/rom_worlds.json", self.registry)
        self.names = ("Main", "Cormoria", "Shared", "MainAfter")
        self.worlds = ("main", "cormoria", "shared", "main")
        self.write("data/maps/map_groups.json", {
            "group_order": ["Group0", "Group1"],
            "Group0": list(self.names[:3]), "Group1": [self.names[3]],
        })
        for name, world in zip(self.names, self.worlds):
            self.write(f"data/maps/{name}/map.json", {"id": "MAP_" + name.upper(), "rom_world": world})

    def write(self, path, value):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(value) + "\n")

    def groups(self, check=True):
        return subprocess.run([str(self.executable), "groups", "emerald", "data/maps/map_groups.json",
                               *[f"data/maps/{name}/map.json" for name in self.names],
                               "data/maps", "include/constants"],
                              cwd=self.root, check=check, capture_output=True, text=True)

    def resolve(self, selector, version="EMERALD", check=True):
        return subprocess.run([sys.executable, str(ROOT / "tools/rom_world_registry.py"),
                               "data/rom_worlds.json", selector, version], cwd=self.root,
                              check=check, capture_output=True, text=True)

    def assemble(self, source, world):
        path = self.root / "tables.s"
        source = "\n".join(line for line in source.splitlines() if not line.lstrip().startswith("@"))
        path.write_text(f".section .rodata\n.equ ROM_WORLD, {world}\n.equ NULL, 0\n" + source.replace("::", ":"))
        subprocess.run(["g++", "-c", str(path), "-o", "tables.o"], cwd=self.root,
                       check=True, capture_output=True, text=True)
        subprocess.run(["objcopy", "-O", "binary", "-j", ".rodata", "tables.o", "tables.bin"],
                       cwd=self.root, check=True, capture_output=True, text=True)
        return (self.root / "tables.bin").read_bytes()

    def test_maps_keep_slots_and_global_constants_across_worlds(self):
        self.groups()
        definitions = "".join(f".equ {name}, {101 + index}\n" for index, name in enumerate(self.names))
        source = definitions + (self.root / "data/maps/groups.inc").read_text()
        self.assertEqual(struct.unpack("<4I", self.assemble(source, 1)[:16]), (101, 0, 103, 104))
        self.assertEqual(struct.unpack("<4I", self.assemble(source, 2)[:16]), (0, 102, 103, 0))
        constants = (self.root / "include/constants/map_groups.h").read_text()
        for symbol, number, group in (("MAP_MAIN", 0, 0), ("MAP_CORMORIA", 1, 0),
                                      ("MAP_SHARED", 2, 0), ("MAP_MAINAFTER", 0, 1)):
            self.assertRegex(constants, rf"{symbol}\s*= \({number} \| \({group} << 8\)\)")

    def test_only_selected_world_includes_are_assembled(self):
        self.groups()
        for index, name in enumerate(self.names):
            for filename in ("header", "events", "connections"):
                (self.root / f"data/maps/{name}/{filename}.inc").write_text(f".4byte {101 + index}\n")
        for filename in ("headers", "events", "connections"):
            source = (self.root / f"data/maps/{filename}.inc").read_text()
            self.assertEqual(self.assemble(source, 1), struct.pack("<3I", 101, 103, 104))
            self.assertEqual(self.assemble(source, 2), struct.pack("<2I", 102, 103))

    def test_unknown_or_non_string_world_is_rejected(self):
        for value in ("coromria", "", 2, None, [], ["main", "main"], ["main", "coromria"]):
            with self.subTest(value=value):
                self.write("data/maps/Main/map.json", {"id": "MAP_MAIN", "rom_world": value})
                result = self.groups(check=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("rom_world", result.stderr)

    def test_registry_selects_worlds_and_rejects_invalid_builds(self):
        self.assertEqual(self.resolve("main").stdout.strip(), "1 - - - -")
        self.assertEqual(self.resolve("2").stdout.strip(),
                         "2 emerald-cormoria CORMORIA BPCO emerald")
        for selector, version in (("3", "EMERALD"), ("unknown", "EMERALD"),
                                  ("cormoria", "FIRERED")):
            with self.subTest(selector=selector, version=version):
                self.assertNotEqual(self.resolve(selector, version, check=False).returncode, 0)

    def test_make_world_selection_uses_registry(self):
        if not shutil.which("make"):
            self.skipTest("GNU make is required")
        shutil.copyfile(ROOT / "tools/rom_world_registry.py",
                        self.root / "tools/rom_world_registry.py")
        selector_block = (ROOT / "Makefile").read_text().split("# GBA rom header", 1)[0]
        (self.root / "selector.mk").write_text(selector_block + "\n.PHONY: selected\nselected:\n"
            "\t@echo ROM_WORLD=$(ROM_WORLD) BUILD_NAME=$(BUILD_NAME) TITLE=$(TITLE) "
            "GAME_CODE=$(GAME_CODE) MAP_VERSION=$(MAP_VERSION)\n")

        def selected(*variables):
            return subprocess.run(["make", "-s", "-f", "selector.mk", "selected", *variables],
                                  cwd=self.root, capture_output=True, text=True)

        self.assertIn("ROM_WORLD=1 BUILD_NAME=emerald", selected("ROM_WORLD=main").stdout)
        self.assertIn("ROM_WORLD=2 BUILD_NAME=emerald-cormoria TITLE=CORMORIA",
                      selected("ROM_WORLD=2").stdout)
        self.assertNotEqual(selected("ROM_WORLD=3").returncode, 0)
        self.assertNotEqual(selected("ROM_WORLD=cormoria", "GAME_VERSION=FIRERED").returncode, 0)
        self.registry["worlds"].append({"name": "third", "world_id": 7, "build_bit": 4,
                                        "game_version": "EMERALD", "map_version": "emerald",
                                        "build_name": "emerald-third", "title": "THIRD WORLD",
                                        "game_code": "BPTH"})
        self.write("data/rom_worlds.json", self.registry)
        self.assertIn("ROM_WORLD=4 BUILD_NAME=emerald-third TITLE=THIRD WORLD GAME_CODE=BPTH",
                      selected("ROM_WORLD=third").stdout)
        self.assertIn("TITLE=THIRD WORLD GAME_CODE=BPTH MAP_VERSION=emerald",
                      selected("ROM_WORLD=third", "TITLE=WRONG", "GAME_CODE=XXXX",
                               "MAP_VERSION=firered").stdout)

    def test_third_world_and_explicit_membership(self):
        self.registry["worlds"].append({"name": "third", "world_id": 7, "build_bit": 4,
                                        "game_version": "EMERALD", "map_version": "emerald",
                                        "build_name": "emerald-third", "title": "THIRD WORLD",
                                        "game_code": "BPTH"})
        self.write("data/rom_worlds.json", self.registry)
        self.names += ("Third", "TwoWorlds")
        groups = json.loads((self.root / "data/maps/map_groups.json").read_text())
        groups["Group1"].extend(("Third", "TwoWorlds"))
        self.write("data/maps/map_groups.json", groups)
        self.write("data/maps/Third/map.json", {"id": "MAP_THIRD", "rom_world": "third"})
        self.write("data/maps/TwoWorlds/map.json", {"id": "MAP_TWOWORLDS",
                                                    "rom_world": ["cormoria", "third"]})
        self.groups()
        definitions = "".join(f".equ {name}, {101 + index}\n" for index, name in enumerate(self.names))
        source = definitions + (self.root / "data/maps/groups.inc").read_text()
        self.assertEqual(struct.unpack("<6I", self.assemble(source, 1)[:24]),
                         (101, 0, 103, 104, 0, 0))
        self.assertEqual(struct.unpack("<6I", self.assemble(source, 2)[:24]),
                         (0, 102, 103, 0, 0, 106))
        self.assertEqual(struct.unpack("<6I", self.assemble(source, 4)[:24]),
                         (0, 0, 103, 0, 105, 106))
        self.assertEqual(struct.unpack("<6I", self.assemble(source, 7)[:24]),
                         (101, 102, 103, 104, 105, 106))
        self.assertEqual(self.resolve("third").stdout.strip(),
                         "4 emerald-third THIRD~WORLD BPTH emerald")

    def test_invalid_registry_rejected_by_build_and_generator(self):
        cases = (
            ("duplicate bit", {"build_bit": 1}),
            ("non power of two", {"build_bit": 3}),
            ("duplicate world ID", {"world_id": 1}),
            ("missing build metadata", {"game_code": None}),
            ("main build name alias", {"build_name": "emerald"}),
            ("main game code alias", {"game_code": "BPEE"}),
        )
        for label, change in cases:
            with self.subTest(label=label):
                entry = self.registry["worlds"][1]
                original = {key: entry[key] for key in change}
                entry.update(change)
                self.write("data/rom_worlds.json", self.registry)
                self.assertNotEqual(self.resolve("cormoria", check=False).returncode, 0)
                self.assertNotEqual(self.groups(check=False).returncode, 0)
                entry.update(original)

        default = self.registry["worlds"][0]
        for field, value in (("build_name", "emerald"), ("game_code", "BPEE")):
            with self.subTest(field=field):
                default[field] = value
                self.write("data/rom_worlds.json", self.registry)
                self.assertNotEqual(self.resolve("main", check=False).returncode, 0)
                self.assertNotEqual(self.groups(check=False).returncode, 0)
                default[field] = None

    def test_registry_rejects_artifact_aliases_between_added_worlds(self):
        third = {"name": "third", "world_id": 7, "build_bit": 4,
                 "game_version": "EMERALD", "map_version": "emerald",
                 "build_name": "emerald-cormoria", "title": "THIRD", "game_code": "BPTH"}
        self.registry["worlds"].append(third)
        for field, value in (("build_name", "emerald-cormoria"), ("game_code", "BPCO")):
            with self.subTest(field=field):
                third["build_name"] = "emerald-third"
                third["game_code"] = "BPTH"
                third[field] = value
                self.write("data/rom_worlds.json", self.registry)
                self.assertNotEqual(self.resolve("third", check=False).returncode, 0)
                self.assertNotEqual(self.groups(check=False).returncode, 0)

    def test_legacy_maps_default_to_main_world(self):
        for name in self.names:
            self.write(f"data/maps/{name}/map.json", {"id": "MAP_" + name.upper()})
        self.groups()
        source = "".join(f".equ {name}, {101 + index}\n" for index, name in enumerate(self.names))
        source += (self.root / "data/maps/groups.inc").read_text()
        self.assertEqual(struct.unpack("<4I", self.assemble(source, 1)[:16]), (101, 102, 103, 104))
        self.assertEqual(struct.unpack("<4I", self.assemble(source, 2)[:16]), (0, 0, 0, 0))

    def test_layout_slots_and_constants_survive_world_selection(self):
        self.registry["worlds"].append({"name": "third", "world_id": 7, "build_bit": 4,
                                        "game_version": "EMERALD", "map_version": "emerald",
                                        "build_name": "emerald-third", "title": "THIRD WORLD",
                                        "game_code": "BPTH"})
        self.write("data/rom_worlds.json", self.registry)
        names = (*self.names, "Third", "TwoWorlds")
        worlds = (*self.worlds, "third", ["cormoria", "third"])
        layouts = []
        for name, world in zip(names, worlds):
            binary = f"data/layouts/{name}.bin"
            (self.root / binary).write_bytes(b"\x00\x00" * 4)
            layouts.append({"id": "LAYOUT_" + name.upper(), "name": "Layout" + name,
                            "rom_world": world, "width": 2, "height": 2,
                            "border_filepath": binary, "blockdata_filepath": binary,
                            "primary_tileset": "Primary", "secondary_tileset": "Secondary"})
        self.write("data/layouts/layouts.json", {"layouts_table_label": "gMapLayouts", "layouts": layouts})
        subprocess.run([str(self.executable), "layouts", "emerald", "data/layouts/layouts.json",
                        "data/layouts", "include/constants"], cwd=self.root,
                       check=True, capture_output=True, text=True)
        source = "".join(f".equ Layout{name}, {201 + index}\n" for index, name in enumerate(names))
        source += (self.root / "data/layouts/layouts_table.inc").read_text()
        self.assertEqual(struct.unpack("<6I", self.assemble(source, 1)[:24]),
                         (201, 0, 203, 204, 0, 0))
        self.assertEqual(struct.unpack("<6I", self.assemble(source, 2)[:24]),
                         (0, 202, 203, 0, 0, 206))
        self.assertEqual(struct.unpack("<6I", self.assemble(source, 4)[:24]),
                         (0, 0, 203, 0, 205, 206))
        constants = (self.root / "include/constants/layouts.h").read_text()
        for index, name in enumerate(names):
            self.assertIn(f"#define LAYOUT_{name.upper()} {index + 1}\n", constants)
        bodies = ".equ Primary, 0\n.equ Secondary, 0\n.equ FALSE, 0\n.equ TRUE, 1\n"
        bodies += (self.root / "data/layouts/layouts.inc").read_text()
        self.assertEqual(len(self.assemble(bodies, 1)), 3 * 44)
        self.assertEqual(len(self.assemble(bodies, 2)), 3 * 44)
        self.assertEqual(len(self.assemble(bodies, 4)), 3 * 44)


if __name__ == "__main__":
    unittest.main()
