"""The Android client must accept the bridge header the ROM writes.

emulator.c refuses to expose the bridge unless the header matches its own
constants, and BridgeFrame.java validates it again. The ROM bumped its game
protocol from 1 to 5 while emulator.c still required 1, so every Android
session stopped with "Bridge no disponible" ten seconds after the game
started. These checks fail CI when the three copies drift apart.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
ROM_HEADER = REPO_ROOT / "include/coop/net_bridge.h"
EMULATOR_C = REPO_ROOT / "android/app/src/main/cpp/emulator.c"
BRIDGE_FRAME = REPO_ROOT / "android/app/src/main/java/io/hoenn/sessions/BridgeFrame.java"


def c_defines(path: Path, prefix: str) -> dict[str, int]:
    pattern = re.compile(rf"^#define {prefix}(\w+) (0x[0-9A-Fa-f]+|\d+)u?\s*$", re.M)
    return {name: int(value, 0) for name, value in pattern.findall(path.read_text(encoding="utf-8"))}


def java_int(path: Path, name: str) -> int:
    match = re.search(rf"static final int {name}\s*=\s*(\d+);", path.read_text(encoding="utf-8"))
    if match is None:
        raise AssertionError(f"{name} not found in {path.name}")
    return int(match.group(1))


class AndroidBridgeConstantsTest(unittest.TestCase):
    def test_native_header_check_matches_rom(self) -> None:
        rom = c_defines(ROM_HEADER, "COOP_NET_BRIDGE_")
        native = c_defines(EMULATOR_C, "BRIDGE_")
        for field in ("MAGIC", "ABI_VERSION", "GAME_PROTOCOL_VERSION", "GAME_BUILD_ID"):
            with self.subTest(field=field):
                self.assertEqual(native[field], rom[field])

    def test_native_check_uses_the_named_constants(self) -> None:
        body = EMULATOR_C.read_text(encoding="utf-8").split("static bool valid_bridge(void)", 1)[1]
        body = body.split("}", 1)[0]
        for name in ("BRIDGE_MAGIC", "BRIDGE_ABI_VERSION", "BRIDGE_GAME_PROTOCOL_VERSION",
                     "BRIDGE_GAME_BUILD_ID"):
            with self.subTest(name=name):
                self.assertIn(name, body)

    def test_java_frame_constants_match_rom(self) -> None:
        rom = c_defines(ROM_HEADER, "COOP_NET_BRIDGE_")
        self.assertEqual(java_int(BRIDGE_FRAME, "BRIDGE_ABI"), rom["ABI_VERSION"])
        self.assertEqual(java_int(BRIDGE_FRAME, "PROTOCOL_VERSION"), rom["GAME_PROTOCOL_VERSION"])


if __name__ == "__main__":
    unittest.main()
