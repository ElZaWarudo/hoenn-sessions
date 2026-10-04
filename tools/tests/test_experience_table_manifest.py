import hashlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.coop.experience_table_manifest import (
    EXPECTED_SIZE,
    build_manifest,
    require_same_experience_tables,
    symbol_from_nm,
)
from tools.coop.player_transfer_manifest import ManifestError


class ExperienceTableManifestTests(unittest.TestCase):
    def test_reads_linked_pointer_free_table_from_rom(self):
        payload = bytes(index % 256 for index in range(EXPECTED_SIZE))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rom = root / "game.gba"
            elf = root / "game.elf"
            rom.write_bytes(bytes(128) + payload)
            elf.write_bytes(b"test")
            completed = type("Result", (), {
                "returncode": 0,
                "stdout": f"gExperienceTables R 08000080 {EXPECTED_SIZE:x}\n",
                "stderr": "",
            })()
            with patch("tools.coop.experience_table_manifest.subprocess.run", return_value=completed):
                manifest = build_manifest(elf, rom, "arm-none-eabi-nm")
        self.assertEqual(manifest["address"], 0x08000080)
        self.assertEqual(manifest["size"], EXPECTED_SIZE)
        self.assertEqual(manifest["sha256"], hashlib.sha256(payload).hexdigest())

    def test_rejects_missing_duplicate_and_wrong_sized_symbols(self):
        good = f"gExperienceTables R 08000080 {EXPECTED_SIZE:x}\n"
        for output in ("", good + good, "gExperienceTables R 08000080 10\n"):
            with self.subTest(output=output), self.assertRaises(ManifestError):
                symbol_from_nm(output)

    def test_rejects_incompatible_world_progression(self):
        with self.assertRaises(ManifestError):
            require_same_experience_tables({"main": {"sha256": "a"}, "cormoria": {"sha256": "b"}})
        require_same_experience_tables({"main": {"sha256": "a"}, "cormoria": {"sha256": "a"}})


if __name__ == "__main__":
    unittest.main()
