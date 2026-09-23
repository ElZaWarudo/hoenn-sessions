"""Assemble generated map tables for both worlds and compare their stable IDs."""
from __future__ import annotations

import json
import shutil
import struct
import subprocess
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
        for value in ("coromria", "", 2, None):
            with self.subTest(value=value):
                self.write("data/maps/Main/map.json", {"id": "MAP_MAIN", "rom_world": value})
                result = self.groups(check=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("rom_world", result.stderr)

    def test_legacy_maps_default_to_main_world(self):
        for name in self.names:
            self.write(f"data/maps/{name}/map.json", {"id": "MAP_" + name.upper()})
        self.groups()
        source = "".join(f".equ {name}, {101 + index}\n" for index, name in enumerate(self.names))
        source += (self.root / "data/maps/groups.inc").read_text()
        self.assertEqual(struct.unpack("<4I", self.assemble(source, 1)[:16]), (101, 102, 103, 104))
        self.assertEqual(struct.unpack("<4I", self.assemble(source, 2)[:16]), (0, 0, 0, 0))

    def test_layout_slots_and_constants_survive_world_selection(self):
        layouts = []
        for name, world in zip(self.names, self.worlds):
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
        source = "".join(f".equ Layout{name}, {201 + index}\n" for index, name in enumerate(self.names))
        source += (self.root / "data/layouts/layouts_table.inc").read_text()
        self.assertEqual(struct.unpack("<4I", self.assemble(source, 1)[:16]), (201, 0, 203, 204))
        self.assertEqual(struct.unpack("<4I", self.assemble(source, 2)[:16]), (0, 202, 203, 0))
        constants = (self.root / "include/constants/layouts.h").read_text()
        for index, name in enumerate(self.names):
            self.assertIn(f"#define LAYOUT_{name.upper()} {index + 1}\n", constants)
        bodies = ".equ Primary, 0\n.equ Secondary, 0\n.equ FALSE, 0\n.equ TRUE, 1\n"
        bodies += (self.root / "data/layouts/layouts.inc").read_text()
        self.assertEqual(len(self.assemble(bodies, 1)), 3 * 44)
        self.assertEqual(len(self.assemble(bodies, 2)), 2 * 44)


if __name__ == "__main__":
    unittest.main()
