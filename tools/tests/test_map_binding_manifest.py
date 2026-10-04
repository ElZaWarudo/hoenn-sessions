"""ROM pointer-chain checks used to validate release arrival coordinates."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest import mock

from tools.coop import map_binding_manifest as binding


class MapBindingTests(unittest.TestCase):
    def setUp(self) -> None:
        image = bytearray(0x400)
        base = binding.ROM_START
        struct.pack_into("<I", image, 0x100, base + 0x120)  # group 0
        struct.pack_into("<I", image, 0x120, base + 0x140)  # map 0
        struct.pack_into("<I", image, 0x140, base + 0x180)  # header layout pointer
        struct.pack_into("<H", image, 0x152, 2)             # header layout ID
        struct.pack_into("<I", image, 0x200, base + 0x184)  # layout 1
        struct.pack_into("<I", image, 0x204, base + 0x180)  # layout 2
        self.image = image
        self.manifest = {
            "schema_version": 1, "rom_sha256": hashlib.sha256(image).hexdigest(),
            "gMapGroups": {"address": base + 0x100, "size": 4},
            "gMapLayouts": {"address": base + 0x200, "size": 8},
            "group_lengths": [1],
        }

    def verify(self, group: int = 0, number: int = 0, layout: int = 2) -> None:
        self.manifest["rom_sha256"] = hashlib.sha256(self.image).hexdigest()
        binding.verify_arrival(bytes(self.image), self.manifest, group, number, layout)

    def test_linked_header_matches_layout_table(self) -> None:
        self.verify()

    def test_save_and_catalog_can_agree_on_wrong_layout(self) -> None:
        with self.assertRaisesRegex(binding.MapBindingError, "map/layout does not match"):
            self.verify(layout=1)

    def test_header_pointer_disagrees_with_layout_table(self) -> None:
        struct.pack_into("<I", self.image, 0x140, binding.ROM_START + 0x184)
        with self.assertRaisesRegex(binding.MapBindingError, "map/layout does not match"):
            self.verify()

    def test_rejects_null_and_outside_rom_pointers(self) -> None:
        for address in (0, 0x0A000000):
            with self.subTest(address=address):
                struct.pack_into("<I", self.image, 0x120, address)
                with self.assertRaisesRegex(binding.MapBindingError, "outside shipped ROM"):
                    self.verify()

    def test_rejects_bad_map_and_layout_indexes(self) -> None:
        for group, number, layout in ((1, 0, 2), (0, 1, 2), (0, 0, 0), (0, 0, 3)):
            with self.subTest(group=group, number=number, layout=layout):
                with self.assertRaisesRegex(binding.MapBindingError, "outside linked tables"):
                    self.verify(group, number, layout)

    def test_rejects_signed_warpdata_map_coordinates(self) -> None:
        for value in (128, 255):
            for group, number in ((value, 0), (0, value)):
                with self.subTest(group=group, number=number):
                    with self.assertRaisesRegex(binding.MapBindingError, "signed WarpData range"):
                        self.verify(group, number)

    def test_rejects_missing_or_misaligned_linked_symbols(self) -> None:
        output = "gMapGroups R 08000100 4\ngMapLayouts R 08000200 8"
        self.assertEqual(binding.symbols_from_nm(output), {
            "gMapGroups": (binding.ROM_START + 0x100, 4),
            "gMapLayouts": (binding.ROM_START + 0x200, 8),
        })
        self.assertEqual(binding.symbols_from_nm("gMapGroups R 08000100\ngMapLayouts R 08000200"), {
            "gMapGroups": (binding.ROM_START + 0x100, 0),
            "gMapLayouts": (binding.ROM_START + 0x200, 0),
        })
        with self.assertRaisesRegex(binding.MapBindingError, "missing or invalid"):
            binding.symbols_from_nm(output + "\ngMapLayouts R 08000200 8")
        with self.assertRaisesRegex(binding.MapBindingError, "missing or invalid"):
            binding.symbols_from_nm("gMapGroups R 08000100 5\ngMapLayouts R 08000200 8")

    def test_build_manifest_derives_assembly_symbol_extents(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data" / "maps").mkdir(parents=True)
            (root / "data" / "layouts").mkdir(parents=True)
            (root / "border.bin").write_bytes(b"x")
            groups = root / "data" / "maps" / "map_groups.json"
            layouts = root / "data" / "layouts" / "layouts.json"
            groups.write_text(json.dumps({"group_order": ["group0"], "group0": ["map0"]}))
            layouts.write_text(json.dumps({"layouts_table_label": "gMapLayouts", "layouts": [
                {"border_filepath": "border.bin"}, {"border_filepath": "border.bin"}]}))
            rom = root / "game.gba"
            rom.write_bytes(self.image)
            output = "gMapGroups R 08000100 0\ngMapLayouts R 08000200 0"
            with mock.patch.object(binding.subprocess, "run", return_value=subprocess.CompletedProcess(
                    [], 0, output, "")):
                manifest = binding.build_manifest(root / "game.elf", rom, groups, layouts, "nm")
            self.assertEqual(manifest["gMapGroups"]["size"], 4)
            self.assertEqual(manifest["gMapLayouts"]["size"], 8)


if __name__ == "__main__":
    unittest.main()
