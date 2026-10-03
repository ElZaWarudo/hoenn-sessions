"""Preparation must stop credential input on focus loss and retain caches."""
import sys
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_prepare_clients as prepare


class PreparationTests(unittest.TestCase):
    def test_keyboard_focus_loss_stops_remaining_secret_characters(self):
        adapter = mock.Mock()
        adapter.user32.VkKeyScanW.side_effect = lambda char: ord(char.upper())
        adapter.foreground.side_effect = [100, 100, 200]
        with mock.patch.object(prepare.time, "sleep"):
            with self.assertRaisesRegex(prepare.HarnessFailure, "lost focus"):
                prepare.type_text("abc", adapter, 100)
        downs = [call.args[0] for call in adapter.key.call_args_list if call.args[1]]
        self.assertEqual(downs, [ord("A")])
        self.assertIn(mock.call(ord("A"), False), adapter.key.call_args_list)

    def test_reused_complete_profiles_do_not_launch_or_install_again(self):
        plan = {"players": [{"name": "a"}, {"name": "b"}]}
        with mock.patch.object(prepare, "preflight"), \
             mock.patch.object(prepare, "_paths", return_value=(Path("release"), Path("run"))), \
             mock.patch.object(prepare, "_cache_ready", return_value=True), \
             mock.patch.object(prepare, "checkpoint"), \
             mock.patch.object(prepare, "launch_signed_desktop") as launch:
            self.assertEqual(prepare.prepare(plan), {"reused": ["a", "b"], "prepared": []})
            launch.assert_not_called()


if __name__ == "__main__":
    unittest.main()
