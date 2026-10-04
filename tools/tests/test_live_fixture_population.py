"""Unit byte fixtures only; these are never uploaded as ROM-written saves."""
import struct
import sys
import unittest
from itertools import permutations
from unittest import mock
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
from live_fixture_population import box_species, check_population
from live_harness_oracles import OracleFailure, read_flash


class FixturePopulationTests(unittest.TestCase):
    def record(self, personality):
        record = bytearray(80)
        trainer = 0x1234ABCD
        struct.pack_into("<II", record, 0, personality, trainer)
        record[19] = 2
        words = [0] * 12
        # Independent canonical order, not the decoder's offset table.
        order = tuple(permutations((0, 1, 2, 3)))[personality % 24]
        words[order.index(0) * 3] = 25 | (31 << 11)
        struct.pack_into("<H", record, 28, sum(w + (w >> 16) for w in words) & 0xFFFF)
        struct.pack_into("<12I", record, 32, *(w ^ personality ^ trainer for w in words))
        return bytes(record)

    def test_all_growth_permutations_decode_packed_species(self):
        for personality in range(24):
            with self.subTest(personality=personality):
                self.assertEqual(box_species(self.record(personality)), 25)

    def test_checksum_corruption_and_bad_egg_are_rejected(self):
        record = bytearray(self.record(17))
        record[32] ^= 1
        with self.assertRaisesRegex(OracleFailure, "checksum"):
            box_species(bytes(record))
        record = bytearray(self.record(17))
        record[19] |= 1
        with self.assertRaisesRegex(OracleFailure, "Bad Egg"):
            box_species(bytes(record))
        self.assertIsNone(box_species(bytes(80)))

    def test_existing_empty_rom_save_cannot_pass_population_recipe(self):
        save = read_flash(ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav")
        descriptor = (ROOT / "coop/crates/coop-save/src/fixtures/player_transfer_v3.bin").read_bytes()
        recipe = {"abi": "hoenn-box80-v1", "party_species": [1], "party_level": 5,
                  "pc_species": [7], "bag_items": [[2, 3]]}
        with self.assertRaisesRegex(OracleFailure, "party species"):
            check_population(save, descriptor, recipe)
        with self.assertRaisesRegex(OracleFailure, "supported ABI"):
            check_population(save, descriptor, dict(recipe, abi="unknown-family"))

    def test_complete_recipe_and_each_damaged_boundary(self):
        save = read_flash(ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav")
        descriptor = (ROOT / "coop/crates/coop-save/src/fixtures/player_transfer_v3.bin").read_bytes()
        party = bytearray(604)
        party[0] = 6
        for i in range(6):
            party[4 + i * 100:84 + i * 100] = self.record(i)
            party[88 + i * 100] = 5
        pc = self.record(23) + bytes(33600 - 80)
        bag = struct.pack("<HHHH", 2, 3, 0, 0)
        recipe = {"abi": "hoenn-box80-v1", "party_species": [25] * 6,
                  "party_level": 5, "pc_species": [25], "bag_items": [[2, 3]]}
        def verify(party_bytes, pc_bytes, bag_bytes):
            with mock.patch("live_fixture_population.logical_field", side_effect=[party_bytes, pc_bytes, bag_bytes]):
                return check_population(save, descriptor, recipe)
        self.assertEqual(len(verify(bytes(party), pc, bag)["shared_witnesses"]), 3)
        damaged = bytearray(party)
        damaged[88] = 6
        with self.assertRaisesRegex(OracleFailure, "levels"):
            verify(bytes(damaged), pc, bag)
        with self.assertRaisesRegex(OracleFailure, "PC species"):
            verify(bytes(party), bytes(33600), bag)
        with self.assertRaisesRegex(OracleFailure, "item/quantity"):
            verify(bytes(party), pc, struct.pack("<HHHH", 2, 4, 0, 0))
        with self.assertRaisesRegex(OracleFailure, "item/quantity"):
            verify(bytes(party), pc, struct.pack("<HHHH", 2, 3, 2, 7))


if __name__ == "__main__":
    unittest.main()
