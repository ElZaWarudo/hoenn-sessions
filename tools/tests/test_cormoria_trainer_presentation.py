"""Cormoria trainer presentation coverage and pinned-source reproducibility."""

from __future__ import annotations

import os
import hashlib
import re
import struct
import unittest
from pathlib import Path

from tools.cormoria.import_trainer_presentation import (
    CLASSES, PICS, ROOT, GABRIELLE_BACK_OUTPUT, GABRIELLE_BACK_SHA256, run,
)


class CormoriaTrainerPresentationTests(unittest.TestCase):
    def test_roster_classes_and_portraits_are_declared(self) -> None:
        roster = (ROOT / "src/data/cormoria/trainers.h").read_text(encoding="utf-8")
        constants = (ROOT / "include/constants/trainers.h").read_text(encoding="utf-8")
        battle = (ROOT / "src/battle_main.c").read_text(encoding="utf-8")
        graphics = (ROOT / "src/data/graphics/trainers.h").read_text(encoding="utf-8")
        declared_classes = set(re.findall(r"TRAINER_CLASS_[A-Z0-9_]+", constants))
        declared_pics = set(re.findall(r"TRAINER_PIC_[A-Z0-9_]+", constants))
        self.assertFalse(set(re.findall(r"TRAINER_CLASS_[A-Z0-9_]+", roster)) - declared_classes)
        self.assertFalse(set(re.findall(r"TRAINER_PIC_[A-Z0-9_]+", roster)) - declared_pics)
        self.assertEqual(len(CLASSES), 19)
        self.assertEqual(len(PICS), 32)
        for name in CLASSES:
            self.assertEqual(len(re.findall(rf"\[TRAINER_CLASS_{name}\]\s*=", battle)), 1)
        for name in PICS:
            self.assertEqual(len(re.findall(rf"\[TRAINER_PIC_{name}\]\s*=", graphics)), 1)
            self.assertIn(f"gCormoriaTrainerFrontPic_{name}", graphics)
            self.assertIn(f"gCormoriaTrainerPalette_{name}", graphics)

    def test_every_portrait_and_palette_has_source_art(self) -> None:
        graphics = (ROOT / "src/data/graphics/trainers.h").read_text(encoding="utf-8")
        imports = re.findall(r'INCBIN_U(?:16|32)\("(graphics/trainers/cormoria/[^\"]+)"\)', graphics)
        self.assertEqual(len(imports), 64)
        for built in imports:
            stem = built.removesuffix(".4bpp.smol").removesuffix(".gbapal")
            png = ROOT / f"{stem}.png"
            palette = ROOT / f"{stem}.pal"
            self.assertTrue(png.is_file() or palette.is_file(), built)
            if png.is_file():
                contents = png.read_bytes()
                self.assertEqual(contents[:8], b"\x89PNG\r\n\x1a\n")
                if built.endswith(".4bpp.smol"):
                    self.assertEqual(struct.unpack(">II", contents[16:24]), (64, 64))

    def test_gabrielle_back_sprite_has_pinned_source(self) -> None:
        source = ROOT / GABRIELLE_BACK_OUTPUT
        self.assertTrue(source.is_file())
        self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), GABRIELLE_BACK_SHA256)
        graphics = (ROOT / "src/data/graphics/trainers.h").read_text(encoding="utf-8")
        self.assertIn('graphics/cormoria/trainers/back_pics/gabrielle.4bpp', graphics)

    @unittest.skipUnless(os.environ.get("CORMORIA_DONOR_GIT"), "pinned donor Git object store unavailable")
    def test_exact_pinned_donor_sources(self) -> None:
        run(Path(os.environ["CORMORIA_DONOR_GIT"]), check=True)


if __name__ == "__main__":
    unittest.main()
