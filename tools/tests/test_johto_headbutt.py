import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DONOR = ROOT.parent.parent.parent / "johto-hns"
if not DONOR.is_dir():
    DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")


def blocks(source):
    result = {}
    current = None
    for raw in source.splitlines():
        line = raw.strip()
        if not line or line.startswith("@"):
            continue
        if re.fullmatch(r"\w+:{1,2}", line):
            current = line.rstrip(":")
            result[current] = []
        elif current:
            result[current].append(line)
    return result


class HeadbuttTests(unittest.TestCase):
    def setUp(self):
        self.ledger = json.loads((ROOT / "data/johto/headbutt.json").read_text())
        self.script = (ROOT / "data/scripts/johto_field_moves.inc").read_text()
        self.source = (ROOT / "src/johto/field_moves.c").read_text()
        self.avatar = (ROOT / "src/field_control_avatar.c").read_text()

    def test_pinned_donor_and_provenance(self):
        donor_path = DONOR / self.ledger["donor"]["path"]
        donor_source = donor_path.read_text(encoding="utf-8")
        self.assertEqual(hashlib.sha256(donor_source.encode()).hexdigest(),
                         self.ledger["donor"]["normalized_sha256"])
        self.assertEqual(self.ledger["donor"]["lines"], [114, 164])
        self.assertEqual(self.ledger["donor"]["revision"],
                         "751823abaf677020bcd72c45fe3e7cb2b8a576e4")
        self.assertTrue(self.ledger["campaign_ready"])
        self.assertIsNone(self.ledger["pending_binding"])
        self.assertEqual(self.ledger["policy"]["unlock"], "JOHTO_FLAG_GET_HEADBUTT")
        self.assertEqual(self.ledger["policy"]["move"], "MOVE_HEADBUTT")

    def test_donor_intent_is_adapted_without_destructive_object_movement(self):
        donor = blocks((DONOR / self.ledger["donor"]["path"]).read_text(encoding="utf-8"))
        self.assertEqual(donor["EventScript_Headbutt"][:5], [
            "lockall",
            "goto_if_unset FLAG_GET_HEADBUTT, EventScript_CantHeadbuttTree",
            "checkpartymove MOVE_HEADBUTT",
            "goto_if_eq VAR_RESULT, PARTY_SIZE, EventScript_CantHeadbuttTree",
            "bufferpartymonnick STR_VAR_1, VAR_RESULT",
        ])
        self.assertIn("lockall", self.script)
        self.assertIn("goto_if_unset JOHTO_FLAG_GET_HEADBUTT, Johto_EventScript_CantHeadbutt", self.script)
        self.assertIn("callnative Script_JohtoCheckHeadbutt, requests_effects=1", self.script)
        self.assertNotIn("applymovement VAR_LAST_TALKED", self.script)
        self.assertNotIn("rock_smash_break", self.script)
        self.assertNotIn("setflag FLAG_SAFE_FOLLOWER_MOVEMENT", self.script)

    def test_feedback_and_battle_branches_have_reachable_release_paths(self):
        entry = blocks(self.script)["Johto_EventScript_Headbutt"]
        self.assertEqual(entry[:5], [
            "lockall",
            "goto_if_unset JOHTO_FLAG_GET_HEADBUTT, Johto_EventScript_CantHeadbutt",
            "callnative Script_JohtoCheckHeadbutt, requests_effects=1",
            "goto_if_eq VAR_RESULT, PARTY_SIZE, Johto_EventScript_CantHeadbutt",
            "bufferpartymonnick STR_VAR_1, VAR_RESULT",
        ])
        self.assertEqual(entry[8:15], [
            "setvar VAR_0x8004, 0",
            "setvar VAR_0x8005, 1",
            "setvar VAR_0x8006, 4",
            "setvar VAR_0x8007, 2",
            "special ShakeCamera",
            "waitstate",
            "playse SE_M_HEADBUTT",
        ])
        encounter = entry.index("special RockSmashWildEncounter")
        self.assertEqual(entry[encounter + 1],
                         "goto_if_eq VAR_RESULT, FALSE, Johto_EventScript_EndHeadbutt")
        self.assertEqual(entry[encounter + 2], "waitstate")
        self.assertEqual(blocks(self.script)["Johto_EventScript_EndHeadbutt"],
                         ["releaseall", "end"])
        self.assertEqual(blocks(self.script)["Johto_EventScript_CantHeadbutt"], [
            "msgbox Johto_Text_CantHeadbutt, MSGBOX_DEFAULT",
            "releaseall",
            "end",
        ])
        self.assertIn("ordinary battle return", self.script)
        self.assertIn("field or whiteout callback", self.ledger["policy"]["encounter"])

    def test_selector_is_in_real_metatile_path_and_isolated_from_water(self):
        metatile_start = self.avatar.index("static const u8 *GetInteractedMetatileScript(struct MapPosition *position")
        water_start = self.avatar.index("static const u8 *GetInteractedWaterScript(struct MapPosition *unused1")
        metatile = self.avatar[metatile_start:water_start]
        self.assertIn("return JohtoFieldMoves_GetHeadbuttScript(metatileBehavior);", metatile)
        interaction_start = self.avatar.index("static const u8 *GetInteractionScript(struct MapPosition *position")
        interaction = self.avatar[interaction_start:metatile_start]
        self.assertLess(interaction.index("GetInteractedObjectEventScript"),
                        interaction.index("GetInteractedBackgroundEventScript"))
        self.assertLess(interaction.index("GetInteractedBackgroundEventScript"),
                        interaction.index("GetInteractedMetatileScript"))
        self.assertLess(interaction.index("GetInteractedMetatileScript"),
                        interaction.index("GetInteractedWaterScript"))
        self.assertNotIn("JohtoFieldMoves_GetHeadbuttScript", interaction)
        runtime = json.loads((ROOT / "data/johto/metatile_runtime.json").read_text(encoding="utf-8"))
        self.assertEqual(runtime["semantics"]["MB_JOHTO_HEADBUTT_TREE"]["consumer"],
                         "JohtoFieldMoves_GetHeadbuttScript")

    def test_native_and_selector_boundaries_are_source_grounded(self):
        for required in (
            "if (count > PARTY_SIZE)",
            "count = PARTY_SIZE",
            "MON_DATA_SPECIES",
            "SPECIES_NONE",
            "MON_DATA_IS_EGG",
            "MAX_MON_MOVES",
            "MON_DATA_MOVE1 + moveSlot",
            "MOVE_HEADBUTT",
            "Script_RequestEffects(SCREFF_V1)",
            "gPlayerPartyCount",
            "MB_JOHTO_HEADBUTT_TREE",
            "Johto_EventScript_Headbutt",
        ):
            self.assertIn(required, self.source)
        self.assertNotIn("MON_DATA_HP", self.source)
        self.assertLess(self.source.index("Script_RequestEffects(SCREFF_V1)"),
                         self.source.index("gSpecialVar_Result ="))

    def test_engine_fixture_covers_waiting_camera_and_host_battle_return(self):
        fixture = (ROOT / "test/johto/headbutt.c").read_text()
        for required in (
            "ScriptContext_SetupScript",
            "ScriptContext_RunScript",
            "ScriptContext_IsEnabled",
            "InstallCameraPanAheadCallback",
            "GetCameraOffsetWithPan",
            "BattleSetup_StartWildBattle",
            "gMain.savedCallback",
            "CB2_ReturnToField",
            "CB2_WhiteOut",
            "FieldCB_ReturnToFieldNoScriptCheckMusic",
            "gObjectEvents[1].frozen",
        ):
            self.assertIn(required, fixture)

    def test_script_is_linked_once_and_whirlpool_alias_remains_untouched(self):
        assembly = (ROOT / "data/event_scripts.s").read_text(encoding="utf-8")
        self.assertEqual(assembly.count('.include "data/scripts/johto_field_moves.inc"'), 1)
        field_moves = (ROOT / "data/johto/field_moves.json").read_text()
        self.assertIn('"EventScript_Whirlpool": "Johto_EventScript_Whirlpool"', field_moves)
        whirlpool = blocks(self.script)["Johto_EventScript_Whirlpool"]
        self.assertEqual(whirlpool[:4], [
            "lockall",
            "goto_if_unset JOHTO_FLAG_BADGE07_GET, Johto_EventScript_CantWhirlpool",
            "callnative Script_JohtoCheckWhirlpool, requests_effects=1",
            "goto_if_eq VAR_RESULT, PARTY_SIZE, Johto_EventScript_CantWhirlpool",
        ])


if __name__ == "__main__":
    unittest.main()
