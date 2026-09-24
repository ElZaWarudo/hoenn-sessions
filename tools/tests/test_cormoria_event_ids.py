"""Checks for the pinned Cormoria event-ID preview."""

from __future__ import annotations

import copy
import unittest
from unittest import mock

from tools.cormoria import import_world, register_event_ids


class CormoriaEventIdTests(unittest.TestCase):
    def test_installed_header_matches_pinned_ledger(self) -> None:
        installed = register_event_ids.ROOT / register_event_ids.OUTPUT
        self.assertEqual(installed.read_bytes(), register_event_ids.render_event_ids())

    def test_all_campaign_ids_are_namespaced_and_contiguous(self) -> None:
        text = register_event_ids.render_event_ids().decode("utf-8")
        definitions = [line for line in text.splitlines() if line.startswith("#define Cormoria_")]
        self.assertEqual(len(definitions), 483 + 43 + 195 + 17)
        self.assertIn("#define Cormoria_FLAG_ANCIENT_FIRST_TIME 0x8000", text)
        self.assertIn("#define Cormoria_TRAINER_CERAMBASECAMPGYM_A 0x5000", text)
        self.assertNotIn("#define Cormoria_VAR_0x8000", text)

    def test_duplicate_or_out_of_range_flag_is_rejected(self) -> None:
        manifest = import_world.load_manifests()
        for replacement in (0x8000, 0x9000):
            with self.subTest(replacement=replacement):
                tampered = copy.deepcopy(manifest)
                tampered[1]["flags"][1]["target_id"] = replacement
                with mock.patch.object(register_event_ids.import_world, "load_manifests",
                                       return_value=tampered):
                    with self.assertRaises(register_event_ids.EventIdError):
                        register_event_ids.render_event_ids()


if __name__ == "__main__":
    unittest.main()
