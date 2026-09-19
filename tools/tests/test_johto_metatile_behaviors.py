"""Source-backed checks for the reserved Johto metatile runtime values."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from tools.johto import import_region_assets as assets


class JohtoMetatileRuntimeTests(unittest.TestCase):
    def test_reservations_match_parser_and_runtime_contract(self):
        runtime = json.loads((ROOT / "data/johto/metatile_runtime.json").read_text(encoding="utf-8"))
        expected = {
            "MB_JOHTO_HEADBUTT_TREE": 0xF0,
            "MB_JOHTO_WATER_NORTH_ARROW_WARP": 0xF1,
            "MB_JOHTO_INERT": 0xF2,
            "MB_JOHTO_DEOXYS_ATTACK": 0xF3,
        }
        self.assertEqual(runtime["reserved_behaviors"], expected)
        self.assertEqual({key: assets.JOHTO_RESERVATIONS[key] for key in expected}, expected)
        host = assets.parse_host_behaviors(ROOT / "include/constants/metatile_behaviors.h")
        for symbol, value in expected.items():
            self.assertEqual(host[symbol], value)
        self.assertEqual(host["MB_ROCK_CLIMB"], 0xEF)
        self.assertEqual(runtime["host_tail"]["MB_ROCK_CLIMB"], 0xEF)
        self.assertEqual(runtime["table_length"], 0xF4)
        self.assertEqual(runtime["invalid_behavior"], 0xFF)
        self.assertEqual(runtime["semantics"]["MB_JOHTO_HEADBUTT_TREE"]["consumer"],
                         "JohtoFieldMoves_GetHeadbuttScript")
        avatar = (ROOT / "src/field_control_avatar.c").read_text(encoding="utf-8")
        self.assertIn("return JohtoFieldMoves_GetHeadbuttScript(metatileBehavior);", avatar)

    def test_parser_rejects_reservation_drift_and_host_occupancy(self):
        original = (ROOT / "include/constants/metatile_behaviors.h").read_text(encoding="utf-8")
        with tempfile.TemporaryDirectory() as directory:
            header = Path(directory) / "metatile_behaviors.h"
            header.write_text(original.replace("MB_JOHTO_INERT,", "MB_JOHTO_INERT = 0xF1,"), encoding="utf-8")
            with self.assertRaises(assets.AssetError):
                assets.parse_host_behaviors(header)


if __name__ == "__main__":
    unittest.main()
