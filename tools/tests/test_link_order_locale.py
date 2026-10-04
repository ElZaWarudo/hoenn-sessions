"""The ROM link order must not depend on the builder's locale.

GNU make 4.3 returns $(wildcard) matches in LC_COLLATE order. The source
lists in the Makefile are the link order, so an en_US.UTF-8 build linked
pokemon_animation.o before pokemon.o while CI (C.UTF-8) linked it after,
moving ROM data and EWRAM symbols and changing the ROM hash. The Makefile now
sorts each pattern bytewise; these checks keep it that way.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
PATTERNS = (
    ("src", "*.c"), ("src", "*/*.c"), ("src", "*/*/*.c"),
    ("src", "*.s"), ("src", "*/*.s"), ("src", "*/*/*.s"),
    ("asm", "*.s"),
    ("data", "*.s"),
)


def link_order(locale: str, world: str) -> list[str]:
    env = dict(os.environ, LC_ALL=locale, LANG=locale)
    result = subprocess.run(
        ["make", "-s", "--no-print-directory", "SETUP_PREREQS=0", "NODEP=1",
         f"ROM_WORLD={world}",
         "--eval", "print-link-order: ; @printf '%s\\n' $(OBJS_REL)", "print-link-order"],
        cwd=REPO_ROOT, env=env, capture_output=True, text=True, check=True)
    return result.stdout.split()


def bytewise_expected() -> list[str]:
    """Each wildcard pattern sorted by raw bytes, patterns kept in Makefile order."""
    objects = []
    for root, pattern in PATTERNS:
        matches = sorted((path.relative_to(REPO_ROOT).as_posix()
                          for path in (REPO_ROOT / root).glob(pattern)),
                         key=lambda name: name.encode("utf-8"))
        for name in matches:
            if root == "src" and name.endswith(".c") and ".inc.c" in name:
                continue
            objects.append(name.rsplit(".", 1)[0] + ".o")
    return objects


@unittest.skipUnless(shutil.which("make") and os.name != "nt", "needs GNU make on Linux")
class LinkOrderLocaleTest(unittest.TestCase):
    def test_link_order_is_identical_across_locales(self) -> None:
        for world in ("main", "cormoria"):
            with self.subTest(world=world):
                c_order = link_order("C.UTF-8", world)
                self.assertEqual(link_order("en_US.UTF-8", world), c_order)
                self.assertEqual(link_order("C", world), c_order)

    def test_code_and_data_objects_link_in_bytewise_order(self) -> None:
        order = [name for name in link_order("en_US.UTF-8", "main")
                 if not name.startswith("sound/songs/midi/")]
        self.assertEqual(order, bytewise_expected())
        # The pair that moved the 2026-10-04 release ROMs.
        self.assertLess(order.index("src/pokemon.o"), order.index("src/pokemon_animation.o"))

    def test_midi_objects_link_in_bytewise_order(self) -> None:
        midi = [name for name in link_order("en_US.UTF-8", "cormoria")
                if name.startswith("sound/songs/midi/")]
        self.assertEqual(midi, sorted(midi, key=lambda name: name.encode("utf-8")))
        self.assertGreater(len(midi), 0)


if __name__ == "__main__":
    unittest.main()
