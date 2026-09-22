import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]


class KantoTravelMenuContractTests(unittest.TestCase):
    def test_menu_has_stable_three_way_order(self):
        menu_data = (ROOT / "src/data/script_menu.h").read_text(encoding="utf-8")
        start = menu_data.index("sMultichoiceList_KantoEras[]")
        end = menu_data.index("};", start)
        menu = menu_data[start:end]

        self.assertLess(menu.index('"Original Kanto"'), menu.index('"Three Years Later"'))
        self.assertLess(menu.index('"Three Years Later"'), menu.index('"Cancel"'))
        self.assertIn("[MULTI_KANTO_ERAS]", menu_data)

    def test_selector_returns_destination_values_and_handles_b(self):
        scripts = (ROOT / "data/event_scripts.s").read_text(encoding="utf-8")
        start = scripts.index("EventScript_ChooseKantoEra::")
        end = scripts.index("EventScript_UseFlightCall::", start)
        selector = scripts[start:end]

        self.assertIn("case 0, EventScript_ChooseKantoEra_Original", selector)
        self.assertIn("case 1, EventScript_ChooseKantoEra_Later", selector)
        self.assertIn("case 2, EventScript_ChooseKantoEra_Cancel", selector)
        self.assertIn("case MULTI_B_PRESSED, EventScript_ChooseKantoEra_Cancel", selector)
        self.assertIn("special Johto_SelectOriginalKanto", selector)
        self.assertIn("special Johto_SelectLaterKanto", selector)
        self.assertIn("special Johto_CancelKantoTravel", selector)

    def test_flight_call_selects_era_only_for_johto_departures(self):
        scripts = (ROOT / "data/event_scripts.s").read_text(encoding="utf-8")
        start = scripts.index("EventScript_UseFlightCall::")
        end = scripts.index('.include "data/johto/campaign_scripts.inc"', start)
        flight_call = scripts[start:end]

        self.assertIn("special Johto_GetCurrentTravelContext", flight_call)
        self.assertIn("call EventScript_ChooseKantoEra", flight_call)
        self.assertIn("special Special_FlightCallSelectedKantoEra", flight_call)
        self.assertIn("case 1, EventScript_FlightCall_Johto", flight_call)
        self.assertIn("case MULTI_B_PRESSED, EventScript_FlightCall_Cancel", flight_call)
        self.assertIn("special Johto_CancelKantoTravel", flight_call)

    def test_script_specials_are_registered(self):
        specials = (ROOT / "data/specials.inc").read_text(encoding="utf-8")
        for name in (
            "Special_FlightCallSelectedKantoEra",
            "Special_FlightCallJohto",
            "Johto_SelectOriginalKanto",
            "Johto_SelectLaterKanto",
            "Johto_ChooseJohto",
            "Johto_CancelKantoTravel",
            "Johto_RecordCurrentHeal",
            "Johto_PrepareKantoTravel",
            "Johto_CommitKantoTravel",
            "Johto_NeedsLaterKantoInitialization",
            "Johto_MarkLaterKantoInitialized",
        ):
            self.assertIn(f"def_special {name}", specials)

    def test_flight_map_b_cancels_pending_but_success_preserves_it(self):
        region_map = (ROOT / "src/region_map.c").read_text(encoding="utf-8")
        start = region_map.rindex("static void CB_ExitFlyMap(void)")
        end = region_map.index("u32 FilterFlyDestination", start)
        exit_callback = region_map[start:end]
        prepare = exit_callback.index("JohtoTravel_RecordCurrentHeal")
        success = exit_callback.index("if (sFlyMap->choseFlyLocation)", prepare)
        cancel = exit_callback.index("CancelFlightCall();")
        clear_forced = exit_callback.index("ClearForcedFlightRegion();")

        self.assertLess(prepare, success)
        self.assertLess(success, cancel)
        self.assertLess(cancel, clear_forced)
        self.assertNotIn("CancelFlightCall", exit_callback[success:cancel])
        self.assertIn("JohtoTravel_PrepareCrossing", exit_callback[prepare:success])

        field_specials = (ROOT / "src/field_specials.c").read_text(encoding="utf-8")
        open_start = field_specials.index("void Special_FlightCallSelectedKantoEra(void)")
        open_end = field_specials.index("void Special_FlightCallJohto(void)", open_start)
        self.assertNotIn("JohtoTravel_RecordCurrentHeal", field_specials[open_start:open_end])

    def test_allocation_failure_and_script_cancel_clear_all_flight_call_state(self):
        region_map = (ROOT / "src/region_map.c").read_text(encoding="utf-8")
        alloc_start = region_map.index("sFlyMap = Alloc(sizeof(*sFlyMap));")
        alloc_end = region_map.index("else", alloc_start)
        alloc_failure = region_map[alloc_start:alloc_end]
        self.assertIn("if (sFlyMap == NULL)", alloc_failure)
        self.assertIn("CancelFlightCall();", alloc_failure)

        field_specials = (ROOT / "src/field_specials.c").read_text(encoding="utf-8")
        cancel_start = field_specials.index("void Johto_CancelKantoTravel(void)")
        cancel_end = field_specials.index("void Johto_RecordCurrentHeal(void)", cancel_start)
        self.assertIn("CancelFlightCall();", field_specials[cancel_start:cancel_end])

        cancel_helper = region_map.index("void CancelFlightCall(void)")
        helper_end = region_map.index("static u8 GetActiveRegionMapType", cancel_helper)
        helper = region_map[cancel_helper:helper_end]
        self.assertIn("gFlightCallFromBag = FALSE;", helper)
        self.assertIn("JohtoTravel_Cancel", helper)
        self.assertIn("ClearForcedFlightRegion", helper)

    def test_fly_arrival_uses_non_destructive_arrival_hook(self):
        field_effect = (ROOT / "src/field_effect.c").read_text(encoding="utf-8")
        start = field_effect.rindex("static void FieldCallback_FlyIntoMap(void)")
        end = field_effect.index("#define taskState", start)
        fly_arrival = field_effect[start:end]
        self.assertIn("JohtoTravel_TryCommitArrival", fly_arrival)
        self.assertNotIn("JohtoTravel_CommitCrossing", fly_arrival)


if __name__ == "__main__":
    unittest.main()
