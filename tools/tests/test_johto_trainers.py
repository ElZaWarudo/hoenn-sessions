"""Focused conversion tests for the strict Johto trainer importer."""
from pathlib import Path
import tempfile
import json
import re
import unittest

from tools.johto import import_trainers


DONOR = Path(r"C:\Users\Mayor\Documents\Caribbean\johto-hns")


class JohtoTrainerImporterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.roster = import_trainers.load_roster(DONOR)
        cls.by_symbol = {trainer.symbol: trainer for trainer in cls.roster}

    def write_fixture_root(self, root, ledger, presentation=None):
        data = root / "data/johto"
        data.mkdir(parents=True)
        (data / "content_symbols.json").write_text(json.dumps(ledger))
        if presentation is None:
            (data / "trainer_presentation.json").write_bytes(
                (import_trainers.ROOT / "data/johto/trainer_presentation.json").read_bytes()
            )
        else:
            (data / "trainer_presentation.json").write_text(json.dumps(presentation))
        constants = root / "include/constants"
        constants.mkdir(parents=True)
        for name in ("trainers.h", "species.h", "items.h", "moves.h"):
            (constants / name).write_bytes(
                (import_trainers.ROOT / "include/constants" / name).read_bytes()
            )

    def test_full_roster_and_ledger_identity(self):
        self.assertEqual(len(self.roster), 412)
        self.assertEqual(sum(len(t.party) for t in self.roster), 1103)
        self.assertEqual(self.roster[0].symbol, "TRAINER_JOEY")
        self.assertEqual(self.roster[0].ordinal, 0)
        self.assertEqual(self.roster[-1].symbol, "TRAINER_ZEKE")
        self.assertEqual(self.roster[-1].ordinal, 411)
        self.assertEqual([t.ordinal for t in self.roster], list(range(412)))
        self.assertEqual(len(self.by_symbol), 412)

    def test_deterministic_render_and_check(self):
        first = import_trainers.render_header(self.roster)
        second = import_trainers.render_header(import_trainers.load_roster(DONOR))
        self.assertEqual(first, second)
        generated = import_trainers.generated_path()
        self.assertEqual(generated.read_text(encoding="utf-8"), first)
        self.assertEqual(import_trainers.run(DONOR, import_trainers.ROOT, True), 0)

    def test_exact_iv_scaling_boundaries(self):
        self.assertEqual([import_trainers._scaled_iv(x) for x in (0, 20, 100, 200, 255)], [0, 2, 12, 24, 31])
        values = {mon.iv for trainer in self.roster for mon in trainer.party}
        self.assertTrue(values.issubset({0, 20, 100, 200, 255}))

    def test_lance_opponents_are_the_only_half_teams_and_keep_parties(self):
        header = import_trainers.generated_path().read_text(encoding="utf-8")
        blocks = re.findall(
            r"\[(\d+)\] = /\* (TRAINER_[A-Z0-9_]+) \*/\n        \{(.*?)\n        \},",
            header, re.S,
        )
        self.assertEqual(len(blocks), 412)
        expected_half = {"TRAINER_ARIANA_1", "TRAINER_GRUNT_23"}
        actual_half = set()
        for ordinal, symbol, body in blocks:
            trainer = self.by_symbol[symbol]
            self.assertEqual(int(ordinal), trainer.ordinal)
            team_size = re.search(r"\.multiTeamSize = (\w+),", body).group(1)
            expected = "MULTI_TEAM_SIZE_HALF" if symbol in expected_half else "MULTI_TEAM_SIZE_FULL"
            self.assertEqual(team_size, expected, symbol)
            if team_size == "MULTI_TEAM_SIZE_HALF":
                actual_half.add(symbol)
            party_name = f"sJohtoParty_{trainer.ordinal:03d}"
            self.assertIn(f".party = {party_name},", body)
            self.assertIn(f".partySize = ARRAY_COUNT({party_name}),", body)
            party = re.search(
                rf"static const struct TrainerMon {party_name}\[\] =\n\{{(.*?)\n\}};",
                header, re.S,
            ).group(1)
            self.assertEqual(re.findall(r"\.species = (\w+),", party), [m.species for m in trainer.party])
            self.assertEqual([int(x) for x in re.findall(r"\.lvl = (\d+),", party)], [m.level for m in trainer.party])
            self.assertEqual(re.findall(r"\.heldItem = (\w+),", party), [m.held_item for m in trainer.party])
            self.assertEqual(
                [tuple(x.strip() for x in moves.split(",")) for moves in re.findall(r"\.moves = \{ (.*?) \},", party)],
                [m.moves for m in trainer.party],
            )
        self.assertEqual(actual_half, expected_half)
        self.assertEqual(header.count(".multiTeamSize = MULTI_TEAM_SIZE_FULL,"), 410)

    def test_all_runtime_symbols_resolve_to_host_constants(self):
        constants = {
            "trainer": (import_trainers.ROOT / "include/constants/trainers.h").read_text(encoding="utf-8"),
            "species": (import_trainers.ROOT / "include/constants/species.h").read_text(encoding="utf-8"),
            "item": (import_trainers.ROOT / "include/constants/items.h").read_text(encoding="utf-8"),
            "move": (import_trainers.ROOT / "include/constants/moves.h").read_text(encoding="utf-8"),
        }
        for trainer in self.roster:
            for symbol in (trainer.trainer_class, trainer.music, trainer.portrait):
                self.assertRegex(constants["trainer"], rf"\b{re.escape(symbol)}\b", symbol)
            for symbol in trainer.items:
                self.assertRegex(constants["item"], rf"\b{re.escape(symbol)}\b", symbol)
            for mon in trainer.party:
                self.assertRegex(constants["species"], rf"\b{re.escape(mon.species)}\b", mon.species)
                self.assertRegex(constants["item"], rf"\b{re.escape(mon.held_item)}\b", mon.held_item)
                for symbol in mon.moves:
                    self.assertRegex(constants["move"], rf"\b{re.escape(symbol)}\b", symbol)
        self.assertNotIn(
            "MOVE_SMELLING_SALT",
            {move for trainer in self.roster for mon in trainer.party for move in mon.moves},
        )

    def test_all_party_layouts_and_real_fields(self):
        self.assertFalse(self.by_symbol["TRAINER_JOEY"].custom_moves)
        falkner = self.by_symbol["TRAINER_FALKNER_1"]
        self.assertTrue(falkner.custom_moves)
        self.assertEqual(falkner.party[0].held_item, "ITEM_NONE")
        self.assertEqual(falkner.party[1].held_item, "ITEM_SITRUS_BERRY")
        self.assertTrue(any(t.custom_moves and any(m.held_item != "ITEM_NONE" for m in t.party) for t in self.roster))
        double = self.by_symbol["TRAINER_AMY_AND_MAY"]
        self.assertTrue(double.double_battle)
        self.assertEqual(double.name_expr, '_("AMY&MAY")')
        self.assertEqual(self.by_symbol["TRAINER_ARCHER"].trainer_class, "TRAINER_CLASS_ROCKET_ADMIN")
        self.assertEqual(self.by_symbol["TRAINER_RED_2"].music, "TRAINER_ENCOUNTER_MUSIC_ELITE_FOUR")

    def test_rival_name_marker_and_unknown_placeholder(self):
        self.assertEqual(import_trainers._parse_name('_("{B_RIVAL_NAME}")'), '_("{RIVAL}")')
        with self.assertRaises(import_trainers.ImportErrorStrict):
            import_trainers._parse_name('_("{UNKNOWN_NAME}")')

    def test_duplicate_ledger_symbol_rejected(self):
        ledger = json.loads((import_trainers.ROOT / "data/johto/content_symbols.json").read_text())
        ledger["identities"]["trainers"][1]["symbol"] = ledger["identities"]["trainers"][2]["symbol"]
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.write_fixture_root(root, ledger)
            with self.assertRaisesRegex(import_trainers.ImportErrorStrict, "duplicate"):
                import_trainers.load_roster(DONOR, root)

    def test_middle_ledger_identity_swap_rejected(self):
        ledger = json.loads((import_trainers.ROOT / "data/johto/content_symbols.json").read_text())
        trainers = ledger["identities"]["trainers"]
        trainers[200]["symbol"], trainers[201]["symbol"] = trainers[201]["symbol"], trainers[200]["symbol"]
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.write_fixture_root(root, ledger)
            with self.assertRaisesRegex(import_trainers.ImportErrorStrict, "identity order"):
                import_trainers.load_roster(DONOR, root)

    def test_invalid_presentation_alias_targets_rejected(self):
        ledger = json.loads((import_trainers.ROOT / "data/johto/content_symbols.json").read_text())
        cases = (
            ("class_aliases", "TRAINER_CLASS_BIKER", "class"),
            ("portrait_aliases", "TRAINER_PIC_LEADER_BLAINE", "portrait"),
            ("music_aliases", "TRAINER_ENCOUNTER_MUSIC_HG_BOY_1", "music"),
        )
        for aliases, source, kind in cases:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temp:
                presentation = json.loads(
                    (import_trainers.ROOT / "data/johto/trainer_presentation.json").read_text()
                )
                presentation[aliases][source] = f"INVALID_{kind.upper()}_TARGET"
                root = Path(temp)
                self.write_fixture_root(root, ledger, presentation)
                with self.assertRaisesRegex(import_trainers.ImportErrorStrict, f"unmapped {kind}"):
                    import_trainers.load_roster(DONOR, root)

    def test_pinned_donor_and_unknown_expression_rejected(self):
        with self.assertRaises(import_trainers.ImportErrorStrict):
            import_trainers.load_roster(Path(tempfile.gettempdir()) / "not-a-donor")
        with self.assertRaises(import_trainers.ImportErrorStrict):
            import_trainers._parse_ai("AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_UNKNOWN")
        with self.assertRaises(import_trainers.ImportErrorStrict):
            import_trainers._parse_party_array(
                "TrainerMonNoItemDefaultMoves",
                "{ .iv = 256, .lvl = 1, .species = SPECIES_RATTATA, }",
                "fixture",
            )


if __name__ == "__main__":
    unittest.main()
