"""Checks for the authenticated, non-linked Cormoria script preview."""

from __future__ import annotations

import json
import os
import struct
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.cormoria import register_scripts


STAGE = Path(os.environ.get(
    "CORMORIA_STAGE", Path.home() / ".codex/cormoria-swarm-artifacts/content-stage-20260923-v5"))


class PortalScriptHandoffTests(unittest.TestCase):
    def test_both_ferries_start_saving_without_another_dialogue_gate(self) -> None:
        for relative, request in (
            ("data/maps/LilycoveCity_Harbor/scripts.inc", "CoopNetBridge_ScriptTravelToCormoria"),
            ("data/cormoria/maps/RivetshoreCity_Harbor/scripts.inc", "CoopNetBridge_ScriptTravelToMain"),
            ("tools/cormoria/rivetshore_portal_overlay.inc", "CoopNetBridge_ScriptTravelToMain"),
        ):
            with self.subTest(relative=relative):
                script = (register_scripts.ROOT / relative).read_text(encoding="utf-8")
                after_request = script.split(f"callnative {request}", 1)[1]
                before_save = after_request.split("special CoopPortalSaveGame", 1)[0]
                self.assertIn("goto_if_eq VAR_RESULT, FALSE", before_save)
                self.assertNotIn("msgbox", before_save)


@unittest.skipUnless(STAGE.is_dir(), "authenticated local Cormoria stage is unavailable")
class ScriptPreviewTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.preview = register_scripts.build_preview(STAGE)
        cls.metadata = json.loads(cls.preview["registration.json"])

    def test_complete_ordered_source_set(self) -> None:
        order = self.metadata["include_order"]
        self.assertEqual(self.metadata["counts"], {"maps": 168, "scripts": 12, "text": 7})
        self.assertEqual(len(order), 187)
        self.assertEqual(len(set(order)), len(order))
        self.assertEqual(order[0], "data/maps/Route117/scripts.inc")
        self.assertEqual(order[-1], "data/maps/Rivetshore_Lookout/scripts.inc")
        wrapper = self.preview["data/cormoria/scripts.inc"].decode()
        self.assertEqual(register_scripts.INCLUDE.findall(wrapper),
                         [f"data/cormoria/{path.removeprefix('data/')}" for path in order])

    def test_isolation_and_host_globals(self) -> None:
        host = register_scripts._host_labels(register_scripts.ROOT)
        all_labels: set[str] = set()
        for path, data in self.preview.items():
            if not path.endswith(".inc") or path == "data/cormoria/scripts.inc":
                continue
            self.assertTrue(path.startswith("data/cormoria/"))
            self.assertNotIn(b'.include "data/script_cmd_table.inc"', data)
            labels = register_scripts._definitions(data.decode(), path)
            self.assertTrue(all(label.startswith("Cormoria_") for label in labels))
            self.assertFalse(all_labels.intersection(labels))
            all_labels.update(labels)
        self.assertFalse(all_labels.intersection(host))
        self.assertNotIn("gSpecialVars::", self.preview["data/cormoria/scripts.inc"].decode())

    def test_donor_shiny_gifts_keep_forced_semantics(self) -> None:
        script = self.preview["data/cormoria/maps/CarabrueTown_TenebrisLab/scripts.inc"].decode()
        self.assertNotIn("isShiny =", script)
        self.assertEqual(script.count("shinyMode=SHINY_MODE_ALWAYS"), 3)
        self.assertEqual(script.count("shinyMode=SHINY_MODE_NEVER"), 3)

    def test_local_ids_do_not_collide_with_host_map_macros(self) -> None:
        script = self.preview["data/cormoria/maps/Route117/scripts.inc"].decode()
        self.assertIn(".set Cormoria_maps_Route117_scripts_LOCALID_DAYCARE_MAN, 3", script)
        self.assertNotIn(".set LOCALID_DAYCARE_MAN,", script)

    def test_starter_capacity_does_not_consume_choice_before_delivery(self) -> None:
        relative = "data/cormoria/maps/CarabrueTown_TenebrisLab/scripts.inc"
        script = self.preview[relative].decode()
        self.assertEqual(script, (register_scripts.ROOT / relative).read_text(encoding="utf-8"))
        prefix = "Cormoria_CarabrueTown_TenebrisLab_EventScript"
        failure = prefix + "_StarterStorageFull"
        for choice, object_id in (("One", 4), ("Two", 7), ("Three", 6)):
            entry = prefix + "_Pokeball_" + choice
            body = script.split(entry + "::\n", 1)[1].split(entry + "_1:", 1)[0]
            self.assertLess(body.index("goto_if_set Cormoria_FLAG_UNUSED_0x020"),
                            body.index("call " + prefix + "_StarterCapacity"))
            self.assertLess(body.index("call " + prefix + "_StarterCapacity"), body.index("showmonpic"))
            accepted = script.split(entry + "_5:", 1)[1].split(entry + "_8:", 1)[0]
            self.assertNotIn("removeobject", accepted)
            success = script.split(entry + "_8:\n", 1)[1].split(entry + "_9:", 1)[0]
            guard = f"goto_if_eq VAR_RESULT, MON_CANT_GIVE, {failure}"
            self.assertLess(success.index(guard), success.index(f"removeobject {object_id}"))
            self.assertLess(success.index(guard), success.index("setflag Cormoria_FLAG_SYS_POKEMON_GET"))
            self.assertLess(success.index(guard), success.index("setflag Cormoria_FLAG_UNUSED_0x020"))
        check = script.split(prefix + "_StarterCapacity::\n", 1)[1].split(failure + "::\n", 1)[0]
        self.assertLess(check.index("goto_if_ne VAR_RESULT, PARTY_SIZE"),
                        check.index("specialvar VAR_RESULT, ScriptCheckFreePokemonStorageSpace"))
        failed = script.split(failure + "::\n", 1)[1].split(failure + "_Text_0:", 1)[0]
        self.assertIn("releaseall\n\tend", failed)
        self.assertNotIn("setflag", failed)
        self.assertNotIn("removeobject", failed)
        with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "overlay drift"):
            register_scripts._adapt_carabrue_starter_capacity(script)

    def test_gardevoir_berry_is_claimed_only_after_delivery(self) -> None:
        relative = "data/cormoria/maps/CarabrueTown_TenebrisLab/scripts.inc"
        script = self.preview[relative].decode()
        self.assertEqual(script, (register_scripts.ROOT / relative).read_text(encoding="utf-8"))
        prefix = "Cormoria_CarabrueTown_TenebrisLab_Gardevoir"
        body = script.split(prefix + "::\n", 1)[1].split(prefix + "_1:", 1)[0]
        guard = f"goto_if_eq VAR_RESULT, FALSE, {prefix}_ItemFull"
        self.assertLess(body.index("giveitem ITEM_STARF_BERRY"), body.index(guard))
        self.assertLess(body.index(guard), body.index("setflag Cormoria_FLAG_TENEBRIS_GARDEVOIR"))
        self.assertLess(body.index(guard), body.index(f"msgbox {prefix}_Text_2"))
        failure = script.split(prefix + "_ItemFull::\n", 1)[1].split("\n\n", 1)[0]
        self.assertEqual(failure.strip(), "releaseall\n\tend")
        with self.assertRaises(register_scripts.ScriptRegistrationError):
            register_scripts._adapt_carabrue_gardevoir_reward(script)
        with self.assertRaises(register_scripts.ScriptRegistrationError):
            register_scripts._adapt_carabrue_gardevoir_reward(script.replace("ITEM_STARF_BERRY", "ITEM_ORAN_BERRY"))

    def test_storage_clefable_uses_its_own_map_object(self) -> None:
        script = self.preview["data/cormoria/maps/SSElegant_Storage/scripts.inc"].decode()
        self.assertNotIn("LOCALID_CLEF", script)
        self.assertIn("applymovement 8, Cormoria_SSElegant_Storage_GabConfronts_Movement_10", script)
        self.assertIn("applymovement 8, Cormoria_SSElegant_Storage_GabConfronts_Movement_11", script)

    def test_donor_trades_pass_identity_and_party_slot_to_engine(self) -> None:
        for map_name, trade_id in (
            ("CeramBaseCamp_Main", "INGAME_TRADE_CORMORIA_WIMPOD"),
            ("PellucaCityRestaurant", "INGAME_TRADE_CORMORIA_HOUNDOUR"),
            ("Rivetshore_RangerInstitute_Interior", "INGAME_TRADE_CORMORIA_PINSIR"),
        ):
            with self.subTest(map_name=map_name):
                relative = f"data/cormoria/maps/{map_name}/scripts.inc"
                script = self.preview[relative].decode()
                self.assertEqual(script.replace("\r\n", "\n"),
                                 (register_scripts.ROOT / relative).read_text(encoding="utf-8").replace("\r\n", "\n"))
                before_species = script.split(f"setvar VAR_0x8008, {trade_id}", 1)[1].split(
                    "specialvar VAR_RESULT, GetInGameTradeSpeciesInfo", 1)[0]
                self.assertIn("copyvar VAR_0x8005, VAR_0x8008", before_species)
                before_create = script.split("special CreateInGameTradePokemon", 1)[0].rsplit(
                    "specialvar VAR_RESULT, GetTradeSpecies", 1)[1]
                self.assertIn("copyvar VAR_0x8004, VAR_0x800A", before_create)
                self.assertIn("copyvar VAR_0x8005, VAR_0x8008", before_create)

    def test_pelluca_rescue_failures_share_regenerated_cleanup(self) -> None:
        relative = "data/cormoria/maps/PellucaCity/scripts.inc"
        script = self.preview[relative].decode().replace("\r\n", "\n")
        installed = (register_scripts.ROOT / relative).read_text(encoding="utf-8").replace("\r\n", "\n")
        self.assertEqual(script, installed)
        cleanup = "Cormoria_PellucaCityFlooded_EventScript_FailCleanup::"
        self.assertEqual(script.count(cleanup), 1)
        self.assertIn("\tmsgbox Cormoria_PellucaCityFlooded_EventScript_TimesUp_Text_0\n" + cleanup,
                      script)
        custom = (register_scripts.ROOT / "data/cormoria/scripts/pelluca_safari.inc").read_text(encoding="utf-8")
        self.assertEqual(custom.count("goto Cormoria_PellucaCityFlooded_EventScript_FailCleanup"), 1)
        self.assertEqual(custom.count("goto_if_eq VAR_RESULT, YES, Cormoria_PellucaCityFlooded_EventScript_FailCleanup"), 1)

    def test_championship_first_clear_fix_survives_regeneration(self) -> None:
        relative = "data/cormoria/maps/Championship_R5/scripts.inc"
        script = self.preview[relative].decode().replace("\r\n", "\n")
        installed = (register_scripts.ROOT / relative).read_text(encoding="utf-8").replace("\r\n", "\n")
        self.assertEqual(script, installed)
        self.assertNotIn("setflag Cormoria_FLAG_SYS_GAME_CLEAR", script.split("special GameClear", 1)[0])

    def test_critical_hm_gifts_retry_after_full_bag(self) -> None:
        cases = (
            (
                "data/cormoria/maps/GalecrestCity/scripts.inc",
                "Cormoria_GalecrestCity_NPC_5::",
                "ITEM_HM_ROCK_SMASH",
                "Cormoria_FLAG_GALECREST_ROCKSMASH",
                "Cormoria_GalecrestCity_NPC_5_ItemFull",
                "end",
            ),
            (
                "data/cormoria/maps/WinterlilyHollow/scripts.inc",
                "Cormoria_WinterlilyHollow_NPC_SurfMan::",
                "ITEM_HM03",
                "Cormoria_FLAG_WINTERLILY_HOLLOW_SURF",
                "Cormoria_WinterlilyHollow_NPC_SurfMan_ItemFull",
                "end",
            ),
        )
        for relative, entry, item, flag, failure, terminal in cases:
            with self.subTest(relative=relative):
                script = self.preview[relative].decode().replace("\r\n", "\n")
                installed = (register_scripts.ROOT / relative).read_text(encoding="utf-8").replace(
                    "\r\n", "\n")
                self.assertEqual(script, installed)
                self.assertEqual(script.count(f"\tgiveitem {item}\n"), 1)
                self.assertEqual(script.count(f"\tsetflag {flag}\n"), 1)
                entry_body = script.split(entry + "\n", 1)[1]
                gift = entry_body.index(f"\tgiveitem {item}\n")
                failed = entry_body.index(f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n")
                self.assertLess(gift, failed)
                self.assertLess(failed, entry_body.index(f"\tsetflag {flag}\n"))
                self.assertIn(f"{failure}::\n", script)
                failure_body = script.split(f"{failure}::\n", 1)[1]
                self.assertIn("\treleaseall\n", failure_body)
                self.assertIn(f"\t{terminal}\n", failure_body)
                self.assertNotIn(f"\tgiveitem {item}\n", failure_body)
                self.assertNotIn(f"\tsetflag {flag}\n", failure_body)
                self.assertNotIn("\tgoto ", failure_body)

    def test_story_credentials_remain_retryable_with_full_bag(self) -> None:
        for map_name, item, flag, failure in (
            ("SilversunCityGym", "ITEM_BACKSTAGE_PASS",
             "Cormoria_FLAG_SILVERSUN_BACKSTAGEPASS_GET",
             "Cormoria_SilversunCityGym_EventScript_BackstagePass_ItemFull"),
            ("GalecrestCity_DetectiveAcademy", "ITEM_DETECTIVE_STUDENT_ID",
             "Cormoria_FLAG_GALECREST_STUDENTID_GET",
             "Cormoria_GalecrestCity_DetectiveAcademy_Receptionist_StudentID_ItemFull"),
        ):
            with self.subTest(map_name=map_name):
                relative = f"data/cormoria/maps/{map_name}/scripts.inc"
                script = self.preview[relative].decode().replace("\r\n", "\n")
                installed = (register_scripts.ROOT / relative).read_text(encoding="utf-8").replace(
                    "\r\n", "\n")
                self.assertEqual(script, installed)
                self.assertEqual(script.count(f"\tgiveitem {item}\n"), 1)
                gift = script.index(f"\tgiveitem {item}\n")
                branch = script.index(f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n")
                flag_set = script.index(f"\tsetflag {flag}\n", gift)
                self.assertLess(gift, branch)
                self.assertLess(branch, flag_set)
                failure_body = script.split(f"{failure}::\n", 1)[1].split(
                    f"{failure}_Text_0:", 1)[0]
                self.assertIn("\treleaseall\n\treturn\n", failure_body)
                self.assertNotIn("\tsetflag ", failure_body)
                self.assertNotIn("\tgiveitem ", failure_body)
                if item == "ITEM_DETECTIVE_STUDENT_ID":
                    self.assertLess(branch, script.index("\tremovemoney 1000\n", gift))

    def test_existing_story_credentials_reconcile_without_duplicate_or_fee(self) -> None:
        silversun = self.preview["data/cormoria/maps/SilversunCityGym/scripts.inc"].decode()
        entry = silversun.split("Cormoria_SilversunCityGym_EventScript_BackstagePass::\n", 1)[1]
        entry = entry.split("Cormoria_SilversunCityGym_EventScript_BackstagePass_1:", 1)[0]
        self.assertLess(entry.index("\tcheckitem ITEM_BACKSTAGE_PASS\n"),
                        entry.index("\tgiveitem ITEM_BACKSTAGE_PASS\n"))
        self.assertIn(
            "\tgoto_if_eq VAR_RESULT, TRUE, "
            "Cormoria_SilversunCityGym_EventScript_BackstagePass_AlreadyOwned\n", entry)
        owned = silversun.split(
            "Cormoria_SilversunCityGym_EventScript_BackstagePass_AlreadyOwned::\n", 1)[1]
        owned = owned.split("Cormoria_SilversunCityGym_EventScript_BackstagePass_ItemFull::", 1)[0]
        self.assertIn("\tsetflag Cormoria_FLAG_SILVERSUN_BACKSTAGEPASS_GET\n", owned)
        self.assertNotIn("\tgiveitem ", owned)

        galecrest = self.preview[
            "data/cormoria/maps/GalecrestCity_DetectiveAcademy/scripts.inc"].decode()
        owned = galecrest.split(
            "Cormoria_GalecrestCity_DetectiveAcademy_Receptionist_StudentID_2:\n", 1)[1]
        owned = owned.split(
            "Cormoria_GalecrestCity_DetectiveAcademy_Receptionist_StudentID_5:", 1)[0]
        self.assertIn("\tsetflag Cormoria_FLAG_GALECREST_STUDENTID_GET\n", owned)
        self.assertNotIn("\tgiveitem ", owned)
        self.assertNotIn("\tremovemoney ", owned)

    def test_story_credential_overlays_reject_donor_flow_drift(self) -> None:
        cases = (
            ("SilversunCityGym", register_scripts._adapt_silversun_backstage_pass,
             "\tsetflag Cormoria_FLAG_SILVERSUN_BACKSTAGEPASS_GET\n"),
            ("GalecrestCity_DetectiveAcademy", register_scripts._adapt_galecrest_student_id,
             "\tremovemoney 1000\n"),
        )
        for map_name, transform, marker in cases:
            with self.subTest(map_name=map_name):
                donor = (STAGE / "namespaced_scripts/data/maps" / map_name / "scripts.inc").read_text()
                with self.assertRaises(register_scripts.ScriptRegistrationError):
                    transform(donor.replace(marker, "", 1))

    def test_winterlily_surf_entry_reaches_authenticated_gift_flow(self) -> None:
        script = self.preview["data/cormoria/maps/WinterlilyHollow/scripts.inc"].decode()
        entry = script.split("Cormoria_WinterlilyHollow_NPC_SurfMan::\n", 1)[1].split(
            "Cormoria_WinterlilyHollow_NPC_SurfMan_2:\n", 1)[0]
        self.assertNotIn("\tend\n", entry.split("\tlockall\n", 1)[0])
        self.assertLess(
            entry.index("msgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_0, MSGBOX_NPC"),
            entry.index("\tlockall\n"),
        )
        self.assertLess(entry.index("\tlockall\n"), entry.index("\tfaceplayer\n"))
        self.assertLess(
            entry.index("\tfaceplayer\n"),
            entry.index(
                "\tgoto_if_set Cormoria_FLAG_WINTERLILY_HOLLOW_SURF, "
                "Cormoria_WinterlilyHollow_NPC_SurfMan_2\n"
            ),
        )
        self.assertLess(
            entry.index("Cormoria_WinterlilyHollow_NPC_SurfMan_Text_2, MSGBOX_YESNO"),
            entry.index("\tcheckitem ITEM_HEART_SCALE\n"),
        )
        self.assertLess(
            entry.index("\tcheckitem ITEM_HEART_SCALE\n"),
            entry.index("\tgiveitem ITEM_HM03\n"),
        )
        self.assertIn(
            "\tgiveitem ITEM_HM03\n"
            "\tgoto_if_eq VAR_RESULT, FALSE, Cormoria_WinterlilyHollow_NPC_SurfMan_ItemFull\n",
            entry,
        )

    def test_winterlily_surf_transaction_and_repeat_guard(self) -> None:
        script = self.preview["data/cormoria/maps/WinterlilyHollow/scripts.inc"].decode()
        entry = script.split("Cormoria_WinterlilyHollow_NPC_SurfMan::\n", 1)[1]
        completed = entry.split("Cormoria_WinterlilyHollow_NPC_SurfMan_2:\n", 1)[1].split(
            "Cormoria_WinterlilyHollow_NPC_SurfMan_5:\n", 1)[0]
        self.assertIn("msgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_1\n", completed)
        self.assertIn("\treleaseall\n", completed)
        self.assertIn("\tend\n", completed)
        self.assertNotIn("\tgoto Cormoria_WinterlilyHollow_NPC_SurfMan_1\n", completed)
        self.assertNotIn("\tgiveitem ITEM_HM03\n", completed)

        self.assertEqual(script.count("\tgiveitem ITEM_HM03\n"), 1)
        self.assertEqual(script.count("\tremoveitem ITEM_HEART_SCALE\n"), 1)
        self.assertEqual(script.count("\tsetflag Cormoria_FLAG_WINTERLILY_HOLLOW_SURF\n"), 1)
        gift = entry.index("\tgiveitem ITEM_HM03\n")
        failed = entry.index(
            "\tgoto_if_eq VAR_RESULT, FALSE, "
            "Cormoria_WinterlilyHollow_NPC_SurfMan_ItemFull\n"
        )
        removed = entry.index("\tremoveitem ITEM_HEART_SCALE\n")
        dialogue_5 = entry.index(
            "msgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_5, MSGBOX_SIGN\n"
        )
        dialogue_6 = entry.index(
            "msgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_6\n"
        )
        dialogue_7 = entry.index(
            "msgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_7\n"
        )
        flagged = entry.index("\tsetflag Cormoria_FLAG_WINTERLILY_HOLLOW_SURF\n")
        self.assertLess(gift, failed)
        self.assertLess(failed, removed)
        self.assertLess(removed, dialogue_5)
        self.assertLess(dialogue_5, dialogue_6)
        self.assertLess(dialogue_6, dialogue_7)
        self.assertLess(removed, flagged)

        failure = "Cormoria_WinterlilyHollow_NPC_SurfMan_ItemFull"
        failure_body = script.split(f"{failure}::\n", 1)[1]
        self.assertNotIn("\tremoveitem ITEM_HEART_SCALE\n", failure_body)
        self.assertNotIn("\tsetflag Cormoria_FLAG_WINTERLILY_HOLLOW_SURF\n", failure_body)
        self.assertNotIn("Text_5", failure_body)
        self.assertNotIn("Text_6", failure_body)
        self.assertIn("The Bag is full. Make room for", failure_body)

        branches = (
            ("Cormoria_WinterlilyHollow_NPC_SurfMan_1", "Cormoria_WinterlilyHollow_NPC_SurfMan_2"),
            ("Cormoria_WinterlilyHollow_NPC_SurfMan_2", "Cormoria_WinterlilyHollow_NPC_SurfMan_5"),
            ("Cormoria_WinterlilyHollow_NPC_SurfMan_5", "Cormoria_WinterlilyHollow_NPC_SurfMan_8"),
            ("Cormoria_WinterlilyHollow_NPC_SurfMan_8", "Cormoria_WinterlilyHollow_NPC_SurfManAlt::"),
        )
        surf = script.split("Cormoria_WinterlilyHollow_NPC_SurfMan::\n", 1)[1]
        for start, end in branches:
            with self.subTest(branch=start):
                branch_body = surf.split(f"{start}:\n", 1)[1].split(
                    f"{end}\n" if end.endswith("::") else f"{end}:\n", 1
                )[0]
                self.assertTrue(branch_body.rstrip().endswith("\treleaseall\n\tend"))
                self.assertNotIn("\treturn\n", branch_body)

    def test_critical_hm_reward_transforms_reject_donor_drift(self) -> None:
        cases = (
            (
                "data/cormoria/maps/GalecrestCity/scripts.inc",
                register_scripts._adapt_galecrest_rock_smash,
                "ITEM_HM_ROCK_SMASH",
                "Cormoria_GalecrestCity_NPC_5_ItemFull",
                '# 345 "data//maps/GalecrestCity/scripts.pory"',
            ),
            (
                "data/cormoria/maps/WinterlilyHollow/scripts.inc",
                register_scripts._adapt_winterlily_surf,
                "ITEM_HM03",
                "Cormoria_WinterlilyHollow_NPC_SurfMan_ItemFull",
                '# 298 "data//maps/WinterlilyHollow/scripts.pory"',
            ),
        )
        for relative, transform, item, failure, marker in cases:
            with self.subTest(relative=relative):
                rendered = self.preview[relative].decode()
                overlay_start = f"\n{failure}::\n"
                self.assertIn(overlay_start, rendered)
                donor = rendered.split(overlay_start, 1)[0]
                donor = donor.replace(
                    f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n", "", 1)
                if relative == "data/cormoria/maps/WinterlilyHollow/scripts.inc":
                    donor = donor.replace(
                        "\tremoveitem ITEM_HEART_SCALE\n", "", 1)
                    donor = donor.replace(
                        "# 298 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tgiveitem ITEM_HM03\n"
                        "# 296 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_5, MSGBOX_SIGN\n"
                        "# 297 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_6\n",
                        "# 296 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_5, MSGBOX_SIGN\n"
                        "# 297 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_6\n"
                        "# 298 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tgiveitem ITEM_HM03\n",
                        1,
                    )
                    donor = donor.replace(
                        "# 272 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_0, MSGBOX_NPC\n"
                        "# 276 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tlockall\n",
                        "# 272 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_0, MSGBOX_NPC\n"
                        "# 273 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tend\n"
                        "# 276 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tlockall\n",
                        1,
                    )
                    donor = donor.replace(
                        "# 280 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_1\n"
                        "\treleaseall\n"
                        "\tend\n",
                        "# 280 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_1\n"
                        "\tgoto Cormoria_WinterlilyHollow_NPC_SurfMan_1\n",
                        1,
                    )
                    donor = donor.replace(
                        "# 286 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_3, MSGBOX_NPC\n"
                        "\treleaseall\n"
                        "\tend\n",
                        "# 286 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_3, MSGBOX_NPC\n"
                        "\tend\n",
                        1,
                    )
                    donor = donor.replace(
                        "# 292 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_4, MSGBOX_NPC\n"
                        "\treleaseall\n"
                        "\tend\n",
                        "# 292 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_4, MSGBOX_NPC\n"
                        "\tend\n",
                        1,
                    )
                    donor = donor.replace(
                        "# 301 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tsetflag Cormoria_FLAG_WINTERLILY_HOLLOW_SURF\n"
                        "\treleaseall\n"
                        "\tend\n",
                        "# 301 \"data//maps/WinterlilyHollow/scripts.pory\"\n"
                        "\tsetflag Cormoria_FLAG_WINTERLILY_HOLLOW_SURF\n"
                        "\treturn\n",
                        1,
                    )
                self.assertEqual(transform(donor), rendered)
                if relative == "data/cormoria/maps/WinterlilyHollow/scripts.inc":
                    with self.assertRaisesRegex(register_scripts.ScriptRegistrationError,
                                                "flow drift"):
                        transform(donor.replace(
                            '# 273 "data//maps/WinterlilyHollow/scripts.pory"',
                            '# drift',
                            1,
                        ))
                    with self.assertRaisesRegex(register_scripts.ScriptRegistrationError,
                                                "flow drift"):
                        transform(donor.replace(
                            "\tgoto Cormoria_WinterlilyHollow_NPC_SurfMan_1\n",
                            "\tend\n",
                            1,
                        ))
                with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "flow drift"):
                    transform(donor.replace(marker, "# drift", 1))
                with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "flow drift"):
                    transform(donor.replace(f"\tgiveitem {item}\n", "", 1))

    def test_gastree_gifts_retry_after_full_bag(self) -> None:
        relative = "data/cormoria/maps/GastreeGym/scripts.inc"
        script = self.preview[relative].decode().replace("\r\n", "\n")
        installed = (register_scripts.ROOT / relative).read_text(encoding="utf-8").replace("\r\n", "\n")
        self.assertEqual(script, installed)

        leader_failure = "Cormoria_GastreeGym_LeaderBattle_RareShardItemFull"
        leader_retry = "Cormoria_GastreeGym_LeaderBattle_RareShardRetry"
        leader_received = "Cormoria_FLAG_GASTREEGYM_LEADER_RARE_SHARD_RECEIVED"
        self.assertIn(
            f"#define {leader_received} (WORLD_EVENT_FLAG_START + 486)",
            (register_scripts.ROOT / "include/cormoria/extra_flags.h").read_text(encoding="utf-8"),
        )
        victory = script.split("Cormoria_GastreeGym_EventScript_Victory::\n", 1)[1].split(
            "Cormoria_GastreeGym_Red::", 1)[0]
        self.assertLess(victory.index("setflag Cormoria_FLAG_BADGE01_GET"),
                        victory.index("giveitem ITEM_RARE_SHARD"))
        self.assertIn(
            f"giveitem ITEM_RARE_SHARD\n"
            f"\tgoto_if_eq VAR_RESULT, FALSE, {leader_failure}\n"
            f"\tsetflag {leader_received}",
            victory,
        )
        repeat = script.split("Cormoria_GastreeGym_LeaderBattle_2:\n", 1)[1].split(
            "Cormoria_GastreeGym_EventScript_Victory::", 1)[0]
        self.assertIn(
            f"goto_if_set {leader_received}, Cormoria_GastreeGym_LeaderBattle_1",
            repeat,
        )
        self.assertNotIn("checkitem ITEM_RARE_SHARD", repeat)
        self.assertNotIn("checkpcitem ITEM_RARE_SHARD", repeat)
        self.assertIn(f"goto {leader_retry}", repeat)
        retry = script.split(f"{leader_retry}::\n", 1)[1].split(
            f"{leader_failure}::\n", 1)[0]
        self.assertEqual(retry.count("giveitem ITEM_RARE_SHARD"), 1)
        self.assertEqual(retry.count(f"goto_if_eq VAR_RESULT, FALSE, {leader_failure}"), 2)
        self.assertEqual(retry.count(f"setflag {leader_received}"), 1)
        self.assertIn("checkitemspace ITEM_RARE_SHARD, 1", retry)
        self.assertLess(retry.index("checkitemspace ITEM_RARE_SHARD, 1"),
                        retry.index("giveitem ITEM_RARE_SHARD"))
        self.assertLess(retry.index("giveitem ITEM_RARE_SHARD"),
                        retry.index(f"setflag {leader_received}"))
        self.assertNotIn("setflag Cormoria_FLAG_BADGE01_GET", retry)
        self.assertNotIn("clearflag", retry)
        self.assertIn(
            f"{leader_failure}::\n\tmsgbox {leader_failure}_Text_0, MSGBOX_DEFAULT\n"
            "\treleaseall\n\tend\n",
            script,
        )

        failure = "Cormoria_GastreeGym_Red_ItemFull"
        self.assertEqual(script.count(failure + "::"), 1)
        self.assertEqual(script.count(f"goto_if_eq VAR_RESULT, FALSE, {failure}"), 2)
        self.assertIn(f"{failure}::\n\treleaseall\n\tend\n", script)

        water = script.split("Cormoria_GastreeGym_Red::\n", 1)[1].split(
            "Cormoria_GastreeGym_Red_1:", 1)[0]
        self.assertIn("goto_if_set Cormoria_FLAG_GASTREEGYM_SPENSER_WATER", water)
        self.assertLess(water.index("giveitem ITEM_FRESH_WATER"),
                        water.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"))
        self.assertLess(water.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"),
                        water.index("msgbox Cormoria_GastreeGym_Red_Text_4"))
        self.assertLess(water.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"),
                        water.index("setflag Cormoria_FLAG_GASTREEGYM_SPENSER_WATER"))
        self.assertLess(water.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"),
                        water.index("clearflag Cormoria_FLAG_HIDE_ROUTE3_UNDERPASS_GYM"))

        reward_gate = script.split("Cormoria_GastreeGym_Red_2:\n", 1)[1].split(
            "Cormoria_GastreeGym_Red_5:", 1)[0]
        self.assertIn("goto_if_set Cormoria_FLAG_GASTREEGYM_SPENSER_REWARD", reward_gate)
        reward = script.split("Cormoria_GastreeGym_Red_Reward_Give_1:\n", 1)[1].split(
            "Cormoria_GastreeGym_Red_Reward_End::", 1)[0]
        self.assertLess(reward.index("giveitem VAR_0x8006"),
                        reward.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"))
        self.assertLess(reward.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"),
                        reward.index("setflag Cormoria_FLAG_GASTREEGYM_SPENSER_REWARD"))
        self.assertLess(reward.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"),
                        reward.index("goto Cormoria_GastreeGym_Red_Reward_End"))

    def test_gastree_reward_transform_rejects_drift(self) -> None:
        relative = "data/cormoria/maps/GastreeGym/scripts.inc"
        script = self.preview[relative].decode()
        failure = "Cormoria_GastreeGym_Red_ItemFull"
        donor = script.replace(f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n", "")
        donor = donor.replace(f"\n{failure}::\n\treleaseall\n\tend\n", "", 1)
        leader_failure = "Cormoria_GastreeGym_LeaderBattle_RareShardItemFull"
        leader_retry = "Cormoria_GastreeGym_LeaderBattle_RareShardRetry"
        leader_received = "Cormoria_FLAG_GASTREEGYM_LEADER_RARE_SHARD_RECEIVED"
        donor = donor.replace(
            f"\tgoto_if_eq VAR_RESULT, FALSE, {leader_failure}\n", "", 1)
        donor = donor.replace(f"\tsetflag {leader_received}\n", "", 2)
        donor = donor.replace(
            f"\tgoto_if_set {leader_received}, Cormoria_GastreeGym_LeaderBattle_1\n"
            f"\tgoto {leader_retry}\n",
            "\tgoto Cormoria_GastreeGym_LeaderBattle_1\n",
            1,
        )
        overlay_start = f"\n{leader_retry}::\n"
        self.assertIn(overlay_start, donor)
        donor = donor.split(overlay_start, 1)[0]
        self.assertEqual(register_scripts._adapt_gastree_item_rewards(donor), script)
        with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "reward flow drift"):
            register_scripts._adapt_gastree_item_rewards(
                donor.replace("\tmsgbox Cormoria_GastreeGym_Red_Text_4\n", "", 1))
        with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "leader reward flow drift"):
            register_scripts._adapt_gastree_item_rewards(
                donor.replace(
                    '# 78 "data//maps/GastreeGym/scripts.pory"\n'
                    "\tspeakername Cormoria_GastreeGym_LeaderBattle_Text_1\n",
                    "",
                    1,
                ))

    def test_welcome_package_keeps_object_and_flag_on_bag_failure(self) -> None:
        relative = "data/cormoria/maps/CarabrueTown_Home2F/scripts.inc"
        script = self.preview[relative].decode().replace("\r\n", "\n")
        installed = (register_scripts.ROOT / relative).read_text(encoding="utf-8").replace("\r\n", "\n")
        self.assertEqual(script, installed)
        failure = "Cormoria_CarabrueTown_Home2F_EventScript_PickUpBag_ItemFull"
        reward = script.split("Cormoria_CarabrueTown_Home2F_EventScript_PickUpBag::\n", 1)[1].split(
            "Cormoria_CarabrueTown_Home2F_EventScript_PC::", 1)[0]
        self.assertLess(reward.index("giveitem ITEM_LAB_WELCOMEPACKAGE"),
                        reward.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"))
        self.assertLess(reward.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"),
                        reward.index("setflag Cormoria_FLAG_TENEBRIS_POLICE_PRESCENCE"))
        self.assertLess(reward.index(f"goto_if_eq VAR_RESULT, FALSE, {failure}"),
                        reward.index("removeobject 1"))
        self.assertIn(f"{failure}::\n\tmsgbox {failure}_Text_0, MSGBOX_DEFAULT\n"
                      "\treleaseall\n\treturn\n", reward)
        self.assertIn("Make room for the", reward)
        self.assertIn("then try again", reward)

    def test_starter_supplies_preflight_all_three_distinct_pockets(self) -> None:
        relative = "data/cormoria/maps/CarabrueTown_TenebrisLab/scripts.inc"
        script = self.preview[relative].decode().replace("\r\n", "\n")
        installed = (register_scripts.ROOT / relative).read_text(encoding="utf-8").replace("\r\n", "\n")
        self.assertEqual(script, installed)
        failure = "Cormoria_CarabrueTown_TenebrisLab_EventScript_Start_ItemFull"
        start = script.split("Cormoria_CarabrueTown_TenebrisLab_EventScript_Start::\n", 1)[1].split(
            "Cormoria_CarabrueTown_TenebrisLab_EventScript_ByTheWayGiveToDetectives::", 1)[0]
        checks = ["checkitemspace ITEM_POKE_BALL, 5", "checkitemspace ITEM_POTION, 1",
                  "checkitemspace ITEM_TOWN_MAP, 1"]
        self.assertEqual(start.count(f"goto_if_eq VAR_RESULT, FALSE, {failure}"), 3)
        for check in checks:
            self.assertLess(start.index(check), start.index("msgbox Cormoria_CarabrueTown_TenebrisLab_EventScript_Start_Text_1"))
            self.assertIn(check + f"\n\tgoto_if_eq VAR_RESULT, FALSE, {failure}", start)
        self.assertLess(start.index(checks[-1]), start.index("giveitem ITEM_POKE_BALL, 5"))
        self.assertLess(start.index("giveitem ITEM_TOWN_MAP"), start.index("removeobject 5"))
        self.assertLess(start.index("giveitem ITEM_TOWN_MAP"),
                        start.index("completesubquest QUEST_LAB_FIRST_DAY, SUB_QUEST_2"))
        self.assertIn(f"{failure}::\n\tmsgbox {failure}_Text_0, MSGBOX_DEFAULT\n"
                      "\treleaseall\n\twarp MAP_CORMORIA_CARABRUE_TOWN, 8, 18\n\tend\n", start)
        self.assertIn("then return to the lab", start)
        self.assertIn("map_script_2 Cormoria_VAR_LAB_STATE, 0, "
                      "Cormoria_CarabrueTown_TenebrisLab_EventScript_Start", script)
        failure_flow = start.split(f"{failure}::\n", 1)[1].split(f"{failure}_Text_0:", 1)[0]
        for mutation in ("setvar Cormoria_VAR_LAB_STATE", "setflag", "removeobject",
                         "completequest", "completesubquest"):
            self.assertNotIn(mutation, failure_flow)

        town = json.loads((STAGE / "source/data/maps/CarabrueTown/map.json").read_text())
        self.assertFalse(any((warp["x"], warp["y"]) == (8, 18)
                             for warp in town["warp_events"]))
        layout = next(layout for layout in json.loads(
            (register_scripts.ROOT / "data/layouts/layouts.json").read_text()
        )["layouts"] if layout["id"] == "LAYOUT_CORMORIA_CARABRUE_TOWN")
        blockdata = (register_scripts.ROOT / layout["blockdata_filepath"]).read_bytes()
        tile = struct.unpack_from("<H", blockdata, 2 * (18 * layout["width"] + 8))[0]
        self.assertEqual(tile & 0x0C00, 0, "return tile must be passable")

    def test_early_reward_transforms_reject_donor_drift(self) -> None:
        home = self.preview["data/cormoria/maps/CarabrueTown_Home2F/scripts.inc"].decode()
        lab = self.preview["data/cormoria/maps/CarabrueTown_TenebrisLab/scripts.inc"].decode()
        home_failure = "Cormoria_CarabrueTown_Home2F_EventScript_PickUpBag_ItemFull"
        lab_failure = "Cormoria_CarabrueTown_TenebrisLab_EventScript_Start_ItemFull"
        home_donor = home.replace(f"\tgoto_if_eq VAR_RESULT, FALSE, {home_failure}\n", "", 1)
        home_donor = home_donor.replace(
            f"\n{home_failure}::\n\tmsgbox {home_failure}_Text_0, MSGBOX_DEFAULT\n"
            "\treleaseall\n\treturn\n"
            f"\n{home_failure}_Text_0:\n"
            '\t.string "The Bag is full. Make room for the\\n"\n'
            '\t.string "Lab Package, then try again.$"\n', "", 1)
        lab_donor = lab.replace("".join(
            f"\tcheckitemspace {item}, {count}\n"
            f"\tgoto_if_eq VAR_RESULT, FALSE, {lab_failure}\n"
            for item, count in (("ITEM_POKE_BALL", 5), ("ITEM_POTION", 1), ("ITEM_TOWN_MAP", 1))
        ), "", 1)
        lab_donor = lab_donor.replace(
            f"\n{lab_failure}::\n\tmsgbox {lab_failure}_Text_0, MSGBOX_DEFAULT\n"
            "\treleaseall\n\twarp MAP_CORMORIA_CARABRUE_TOWN, 8, 18\n\tend\n"
            f"\n{lab_failure}_Text_0:\n"
            '\t.string "The Bag is full. Make room for the\\n"\n'
            '\t.string "supplies, then return to the lab.$"\n', "", 1)
        self.assertEqual(register_scripts._adapt_carabrue_welcome_package(home_donor), home)
        self.assertEqual(register_scripts._adapt_carabrue_starter_supplies(lab_donor), lab)
        with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "welcome package flow drift"):
            register_scripts._adapt_carabrue_welcome_package(
                home_donor.replace("giveitem ITEM_LAB_WELCOMEPACKAGE", "giveitem ITEM_POTION", 1))
        with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "starter supplies flow drift"):
            register_scripts._adapt_carabrue_starter_supplies(
                lab_donor.replace("giveitem ITEM_POKE_BALL, 5", "giveitem ITEM_POKE_BALL, 4", 1))

    def test_cutscene_hms_remain_claimable_after_full_bag(self) -> None:
        for name, item, retry, terminal_flag in (
            ("SilversunCity", "ITEM_HM04", "StrengthBagFull", "Cormoria_FLAG_SILVERSUN_NEXTQUEST"),
            ("CarabrueTown_TenebrisLab_Finale", "ITEM_HM07", "WaterfallBagFull",
             "Cormoria_FLAG_POST_FINALE_CUTSCENE"),
        ):
            relative = f"data/cormoria/maps/{name}/scripts.inc"
            script = self.preview[relative].decode()
            installed = (register_scripts.ROOT / relative).read_text(encoding="utf-8")
            with self.subTest(name=name):
                self.assertEqual(script.replace("\r\n", "\n"), installed.replace("\r\n", "\n"))
                self.assertIn(f"\tgoto_if_set {terminal_flag}", script)
                self.assertIn(f"\tgiveitem {item}\n\tgoto_if_eq VAR_RESULT, FALSE", script)
                self.assertIn(f"{retry}::", script)
                self.assertIn(f"\tcheckitem {item}", script)


    def test_route6_uses_appended_gabrielle_partner(self) -> None:
        script = self.preview["data/cormoria/maps/Route6/scripts.inc"].decode()
        self.assertIn(
            "multi_2_vs_2 Cormoria_TRAINER_ROUTE6_GRUNT1, "
            "Cormoria_Route6_Gabrielle_DoBattle_Text_0, "
            "Cormoria_TRAINER_ROUTE6_GRUNT2, "
            "Cormoria_Route6_Gabrielle_DoBattle_Text_1, "
            "PARTNER_CORMORIA_GABRIELLE", script)
        self.assertNotIn("PARTNER_ROUTE6_GAB", script)

    def test_galecrest_uses_appended_number_input_menu(self) -> None:
        script = self.preview["data/cormoria/maps/GalecrestCityGym/scripts.inc"].decode()
        self.assertIn("MULTI_CORMORIA_NUMBER_INPUT", script)
        self.assertNotIn("MULTI_NUMBER_INPUT", script)

    def test_gacha_token_settlement_is_owned_only_by_minigame(self) -> None:
        donor = (STAGE / "source" / "data/maps/GalecrestCity_GameCorner/scripts.inc").read_text()
        script = self.preview[
            "data/cormoria/maps/GalecrestCity_GameCorner/scripts.inc"
        ].decode()
        self.assertEqual(donor.count("removeitem ITEM_GACHA_TOKEN"), 4)
        self.assertNotIn("removeitem ITEM_GACHA_TOKEN", script)
        for tier in ("Basic", "Great", "Ultra", "Master"):
            target = f"Cormoria_GalecrestCity_GameCorner_Gacha_{tier}_4"
            self.assertIn(
                f"\tgoto {target}",
                script,
            )

    def test_donor_extra_flag_is_world_local(self) -> None:
        script = self.preview["data/cormoria/scripts/debug.inc"].decode()
        self.assertIn("setflag Cormoria_FLAG_VISITED_RIVETSHORE_RANGER", script)
        self.assertNotIn("setflag FLAG_VISITED_RIVETSHORE_RANGER", script)

    def test_rivetshore_return_portal_survives_regeneration(self) -> None:
        relative = "data/cormoria/maps/RivetshoreCity_Harbor/scripts.inc"
        script = self.preview[relative]
        installed = (register_scripts.ROOT / relative).read_bytes()
        self.assertEqual(script.replace(b"\r\n", b"\n"), installed.replace(b"\r\n", b"\n"))
        self.assertIn(b"callnative CoopNetBridge_ScriptTravelToMain", script)
        self.assertIn(b"Cormoria_RivetshoreCity_Harbor_Attendant_Original::", script)

    def test_deterministic_bytes(self) -> None:
        self.assertEqual(self.preview, register_scripts.build_preview(STAGE))

    def test_stage_drift_rejected(self) -> None:
        original = register_scripts._stage_bytes

        def changed(stage: Path, relative: str, records: dict) -> bytes:
            data = original(stage, relative, records)
            if relative == "namespaced_scripts/data/maps/CarabrueTown/scripts.inc":
                return data + b"\n"
            return data

        with mock.patch.object(register_scripts, "_stage_bytes", side_effect=changed):
            with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "source drift"):
                register_scripts.build_preview(STAGE)

    def test_duplicate_labels_and_unsafe_path_rejected(self) -> None:
        with self.assertRaisesRegex(register_scripts.ScriptRegistrationError, "duplicate label"):
            register_scripts._definitions("Cormoria_One:\nCormoria_One::\n", "test.inc")
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "source path escape"):
                register_scripts._stage_bytes(Path(temporary), "../outside", {"../outside": {}})


if __name__ == "__main__":
    unittest.main()
