"""Checks for the authenticated, non-linked Cormoria script preview."""

from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.cormoria import register_scripts


STAGE = Path(os.environ.get(
    "CORMORIA_STAGE", Path.home() / ".codex/cormoria-swarm-artifacts/content-stage-20260923-v5"))


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

    def test_storage_clefable_uses_its_own_map_object(self) -> None:
        script = self.preview["data/cormoria/maps/SSElegant_Storage/scripts.inc"].decode()
        self.assertNotIn("LOCALID_CLEF", script)
        self.assertIn("applymovement 8, Cormoria_SSElegant_Storage_GabConfronts_Movement_10", script)
        self.assertIn("applymovement 8, Cormoria_SSElegant_Storage_GabConfronts_Movement_11", script)

    def test_donor_trade_uses_appended_shared_identity(self) -> None:
        script = self.preview["data/cormoria/maps/CeramBaseCamp_Main/scripts.inc"].decode()
        self.assertIn("INGAME_TRADE_CORMORIA_WIMPOD", script)
        self.assertNotIn("INGAME_TRADE_WIMPOD", script)

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
        self.assertEqual(script, (register_scripts.ROOT / relative).read_bytes())
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
