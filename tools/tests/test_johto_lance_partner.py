"""Source and asset checks for the Johto Lance partner foundation."""
from pathlib import Path
import hashlib
import json
import re
import struct
import unittest

ROOT = Path(__file__).resolve().parents[2]
DONOR = Path(r"C:\Users\Mayor\Documents\Caribbean\johto-hns")
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
PNG_SHA = "52f38cd618ddd115653860c61516f0e3c244fb72ec5d0b3ddbfc93f0eb795474"
PAL_SHA = "57bf312444230be21b0bc5e247f1cd39a5935dc85c917eb29d36dfd60e601069"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class JohtoLancePartnerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.party_path = ROOT / "src/data/battle_partners.party"
        cls.party = cls.party_path.read_text(encoding="utf-8")
        cls.lance = cls.party.split("=== PARTNER_LANCE ===", 1)[1]
        cls.meta = json.loads((ROOT / "data/johto/lance_partner.json").read_text(encoding="utf-8"))

    def test_partner_ids_are_append_only_and_steven_block_is_preserved(self):
        constants = (ROOT / "include/constants/battle_partner.h").read_text(encoding="utf-8")
        self.assertRegex(constants, r"#define PARTNER_NONE 0\s+#define PARTNER_STEVEN 1\s+#define PARTNER_LANCE 2\s+#define PARTNER_COUNT 3")
        expected_steven = """=== PARTNER_STEVEN ===
Name: STEVEN
Class: Rival
Pic: Steven
Gender: Male
Music: Male
AI: Basic Trainer

Metang
Brave Nature
Level: 42
IVs: 31 HP / 31 Atk / 31 Def / 31 SpA / 31 SpD / 31 Spe
EVs: 252 Atk / 252 Def / 6 SpA
- Light Screen
- Psychic
- Reflect
- Metal Claw

Skarmory
Impish Nature
Level: 43
IVs: 31 HP / 31 Atk / 31 Def / 31 SpA / 31 SpD / 31 Spe
EVs: 252 HP / 6 SpA / 252 SpD
- Toxic
- Aerial Ace
- Protect
- Steel Wing

Aggron
Adamant Nature
Level: 44
IVs: 31 HP / 31 Atk / 31 Def / 31 SpA / 31 SpD / 31 Spe
EVs: 252 Atk / 252 SpA / 6 SpD
- Thunder
- Protect
- Solar Beam
- Dragon Claw
"""
        before = self.party.split("=== PARTNER_LANCE ===", 1)[0]
        self.assertTrue(before.endswith(expected_steven + "\n"))

        trainers = (ROOT / "include/constants/trainers.h").read_text(encoding="utf-8")
        self.assertRegex(trainers, r"TRAINER_PIC_SILVER,\s+TRAINER_PIC_JOHTO_PARTNER_LANCE,\s+TRAINER_PIC_COUNT,")

    def test_lance_source_party_is_exact_and_has_no_invented_items(self):
        self.assertRegex(self.lance, r"Name: LANCE\s+Class: Elite Four\s+Pic: Johto Partner Lance\s+Gender: Male\s+Music: Elite Four\s+AI: Basic Trainer\s+Multi Party: Half")
        expected = [
            ("Dragonite", 42, "252 Atk / 252 Def / 6 SpA", ["Hyper Beam", "Thunder", "Safeguard", "Outrage"]),
            ("Dragonair", 35, "252 HP / 6 SpA / 252 SpD", ["Blizzard", "Thunder Wave", "Flamethrower", "Quick Attack"]),
            ("Charizard", 36, "252 Atk / 252 SpA / 6 SpD", ["Flamethrower", "Wing Attack", "Double Team", "Steel Wing"]),
        ]
        for species, level, evs, moves in expected:
            self.assertIn(species, self.lance)
            block = self.lance[self.lance.index(species):]
            block = block[:next((block.index(x) for x in ("\n\nDragonite", "\n\nDragonair", "\n\nCharizard") if x in block[1:]), len(block))]
            self.assertIn(f"Level: {level}", block)
            self.assertIn("Adamant Nature", block)
            self.assertIn("IVs: 31 HP / 31 Atk / 31 Def / 31 SpA / 31 SpD / 31 Spe", block)
            self.assertIn(f"EVs: {evs}", block)
            self.assertEqual(re.findall(r"^- (.+)$", block, re.MULTILINE), moves)
        self.assertNotRegex(self.lance, r"(?m)^(?:Held )?Item:")
        donor = (DONOR / "src/battle_tower.c").read_text(encoding="utf-8")
        source = donor[donor.index("sStevenMons"):donor.index("};", donor.index("sStevenMons"))]
        source_mons = re.findall(r"\{\s*\.species = (SPECIES_\w+),(.*?)\n    \}", source, re.S)
        self.assertEqual(len(source_mons), 3)
        for (species, body), mon in zip(source_mons, self.meta["partner"]["party"]):
            self.assertEqual(species, mon["species"])
            self.assertEqual(int(re.search(r"\.level = (\d+)", body)[1]), mon["level"])
            self.assertEqual(re.search(r"\.nature = (\w+)", body)[1], mon["nature"])
            self.assertEqual(re.search(r"\.fixedIV = (\w+)", body)[1], "MAX_PER_STAT_IVS")
            self.assertEqual(mon["ivs"], 31)
            evs = [int(value) for value in re.search(r"\.evs = \{([^}]+)\}", body)[1].split(",")]
            self.assertEqual(evs, [mon["evs"][stat] for stat in ("hp", "atk", "def", "speed", "spatk", "spdef")])
            moves = [value.strip() for value in re.search(r"\.moves = \{([^}]+)\}", body)[1].split(",")]
            self.assertEqual(moves, mon["moves"])

    def test_assets_are_pinned_and_palette_is_valid(self):
        png = ROOT / "graphics/johto/trainers/back_pics/lance.png"
        pal = ROOT / "graphics/johto/trainers/back_pics/lance.pal"
        self.assertEqual(sha256(png), PNG_SHA)
        self.assertEqual(sha256(pal), PAL_SHA)
        self.assertEqual(sha256(DONOR / "graphics/trainers/back_pics/steven.png"), PNG_SHA)
        self.assertEqual(sha256(DONOR / "graphics/trainers/palettes/steven_back.pal"), PAL_SHA)
        data = png.read_bytes()
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
        self.assertEqual(struct.unpack(">II", data[16:24]), (64, 256))
        lines = pal.read_text(encoding="ascii").splitlines()
        self.assertEqual(lines[:2], ["JASC-PAL", "0100"])
        self.assertEqual(int(lines[2]), 16)
        self.assertEqual(len(lines[3:]), 16)
        self.assertTrue(all(len(line.split()) == 3 and all(0 <= int(v) <= 255 for v in line.split()) for line in lines[3:]))

    def test_graphics_registration_uses_host_front_and_safe_throw(self):
        graphics = (ROOT / "src/data/graphics/trainers.h").read_text(encoding="utf-8")
        self.assertIn('gJohtoTrainerBackPic_Lance[] = INCBIN_U8("graphics/johto/trainers/back_pics/lance.4bpp")', graphics)
        self.assertIn('gJohtoTrainerBackPalette_Lance[16] = INCBIN_U16("graphics/johto/trainers/back_pics/lance.gbapal")', graphics)
        entry = graphics[graphics.index("[TRAINER_PIC_JOHTO_PARTNER_LANCE]"):]
        self.assertIn("gTrainerFrontPic_EliteFourLanceFrlg", entry)
        self.assertIn("gTrainerPalette_EliteFourLanceFrlg", entry)
        self.assertIn("TRAINER_BACK_PIC(4, gJohtoTrainerBackPic_Lance, gJohtoTrainerBackPalette_Lance, sBackAnims_Hoenn)", entry)
        animation = graphics[graphics.index("sAnimCmd_Hoenn"):graphics.index("};", graphics.index("sAnimCmd_Hoenn"))]
        self.assertEqual(re.findall(r"ANIMCMD_FRAME\((\d+), (\d+)\)", animation), [("0", "24"), ("1", "9"), ("2", "24"), ("0", "9"), ("3", "50")])
        self.assertIn("ANIMCMD_END", animation)

    def test_provenance_binds_source_and_keeps_campaign_gate_closed(self):
        self.assertEqual(self.meta["provenance"]["donor_revision"], DONOR_REVISION)
        self.assertFalse(self.meta["campaign_battle_ready"])
        self.assertEqual(self.meta["partner"]["party"][0]["held_item"], "ITEM_NONE")
        self.assertEqual(self.meta["presentation"]["throw_frames"], [0, 1, 2, 0, 3])
        self.assertEqual(self.meta["presentation"]["throw_durations"], [24, 9, 24, 9, 50])
        self.assertEqual(self.meta["presentation"]["neutral_frame"], 3)

if __name__ == "__main__":
    unittest.main()
