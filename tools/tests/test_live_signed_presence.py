"""Mocked signed client boundaries; never launches games or calls servers."""
from contextlib import ExitStack
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_signed_presence as presence


def plan():
    inputs = []
    for name in ("a", "b"):
        for key in ("gba_b", "gba_start", "gba_start", "gba_a"):
            inputs.append({"player": name, "key": key, "hold_ms": 80,
                           "release_ms": 600, "wait_ms": 1000,
                           **({"expect_presence_published": True} if key == "gba_a" else {})})
    inputs.append({"player": "a", "key": "gba_up", "expect_travel": True})
    return {"players": [{"name": "a"}, {"name": "b"}],
            "legs": [{"name": "main-cormoria", "source_world_id": 1, "inputs": inputs}]}


class SignedPresenceTests(unittest.TestCase):
    def lifecycle(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        mocks = {}
        returns = {"preflight": {}, "_paths": (Path("release"), Path("run")),
                   "launch": {"a": 10, "b": 11},
                   "Win32Adapter": mock.sentinel.adapter, "capture_game": Path("image.png"),
                   "digest": "sha", "check_c_space": 3 * 1024**3}
        for name in (*returns, "tap", "require_presence_published", "checkpoint",
                     "close_desktops"):
            mocks[name] = stack.enter_context(mock.patch.object(presence.harness, name,
                                                                return_value=returns.get(name)))
        mocks["start_games"] = stack.enter_context(mock.patch.object(presence.controls, "start_games",
                                                                     return_value={"a": 20, "b": 21}))
        mocks["stop_runtime"] = stack.enter_context(mock.patch.object(presence.controls, "stop_runtime"))
        mocks["alive"] = stack.enter_context(mock.patch.object(presence.harness.psutil, "pid_exists", return_value=True))
        mocks["bind"] = stack.enter_context(mock.patch.object(presence, "bind_games", return_value={"a": {}, "b": {}}))
        stack.enter_context(mock.patch.object(presence.time, "sleep"))
        return mocks

    def test_success_stops_exactly_at_second_presence(self):
        m = self.lifecycle()
        result = presence.check_presence(plan())
        self.assertEqual(result["players_published"], ["a", "b"])
        self.assertEqual(result["input_count"], 8)
        self.assertEqual(m["tap"].call_count, 8)
        self.assertEqual(m["capture_game"].call_count, 8)
        self.assertEqual(m["require_presence_published"].call_count, 2)
        m["preflight"].assert_called_once_with(plan())
        m["stop_runtime"].assert_called_once()
        m["close_desktops"].assert_called_once()

    def test_rejects_unsafe_prefix_before_launch(self):
        cases = [{"expect_travel": True}, {"key": "gba_up"},
                 {"group_id": "group"}, {"key": "gba_select"}]
        for changes in cases:
            with self.subTest(changes=changes):
                p = plan(); p["legs"][0]["inputs"][0].update(changes)
                with mock.patch.object(presence.harness, "launch") as launch:
                    with self.assertRaises(presence.harness.HarnessFailure):
                        presence.check_presence(p)
                    launch.assert_not_called()

    def test_missing_second_player_and_duplicate_actor(self):
        for players in ([{"name": "a"}], [{"name": "a"}, {"name": "a"}]):
            p = plan(); p["players"] = players
            with self.assertRaises(presence.harness.HarnessFailure):
                presence.presence_prefix(p)
        p = plan(); del p["legs"][0]["inputs"][4:]
        with self.assertRaises(presence.harness.HarnessFailure):
            presence.presence_prefix(p)

    def test_invalid_leg_name_rejected_before_launch_or_input(self):
        for name in ("main/cormoria", "main\\cormoria", "", None, 3, "a" * 49, "cormoría"):
            p = plan(); p["legs"][0]["name"] = name
            with self.subTest(name=name), mock.patch.object(presence.harness, "launch") as launch, \
                    mock.patch.object(presence.harness, "tap") as tap, \
                    mock.patch.object(presence.harness, "preflight") as preflight:
                with self.assertRaises(presence.harness.HarnessFailure):
                    presence.check_presence(p)
                launch.assert_not_called()
                tap.assert_not_called()
                preflight.assert_not_called()

    def test_timing_and_boolean_bounds(self):
        for key, value in (("hold_ms", 9), ("hold_ms", 2001), ("hold_ms", True),
                           ("release_ms", -1), ("release_ms", 2001),
                           ("wait_ms", 30001), ("wait_ms", None),
                           ("screenshot", 1), ("expect_presence_published", None)):
            p = plan(); p["legs"][0]["inputs"][0][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(presence.harness.HarnessFailure):
                presence.presence_prefix(p)

    def test_already_ready_actor_cannot_receive_more_input(self):
        p = plan(); p["legs"][0]["inputs"].insert(4, {"player": "a", "key": "gba_a"})
        with self.assertRaises(presence.harness.HarnessFailure):
            presence.presence_prefix(p)

    def test_preflight_profile_rejection_has_no_launch(self):
        m = self.lifecycle(); m["preflight"].side_effect = RuntimeError("wrong profile")
        with self.assertRaisesRegex(RuntimeError, "wrong profile"):
            presence.check_presence(plan())
        m["launch"].assert_not_called()
        m["tap"].assert_not_called()

    def test_rom_binding_rejection_has_no_input_and_cleans_up(self):
        m = self.lifecycle(); m["bind"].side_effect = RuntimeError("wrong source ROM")
        with self.assertRaisesRegex(RuntimeError, "wrong source ROM"):
            presence.check_presence(plan())
        m["tap"].assert_not_called()
        m["stop_runtime"].assert_called_once()
        m["close_desktops"].assert_called_once()

    def test_first_presence_failure_blocks_second_actor(self):
        m = self.lifecycle(); m["require_presence_published"].side_effect = RuntimeError("not ready")
        with self.assertRaisesRegex(RuntimeError, "not ready"):
            presence.check_presence(plan())
        self.assertEqual(m["tap"].call_count, 4)
        self.assertTrue(all(call.args[0] == 20 for call in m["tap"].call_args_list))

    def test_focus_failure_preserved_over_both_cleanup_errors(self):
        m = self.lifecycle(); failure = RuntimeError("focus lost")
        m["tap"].side_effect = failure
        m["stop_runtime"].side_effect = RuntimeError("stop failed")
        m["close_desktops"].side_effect = RuntimeError("close failed")
        with self.assertRaises(RuntimeError) as raised:
            presence.check_presence(plan())
        self.assertIs(raised.exception, failure)
        self.assertEqual(len(failure.__notes__), 2)
        self.assertEqual(m["tap"].call_count, 1)
        m["capture_game"].assert_not_called()

    def test_success_cleanup_failure_is_reported(self):
        m = self.lifecycle(); m["stop_runtime"].side_effect = RuntimeError("stop failed")
        with self.assertRaisesRegex(RuntimeError, "stop failed"):
            presence.check_presence(plan())
        m["close_desktops"].assert_called_once()

    def test_duplicate_game_pids_rejected_before_process_reads(self):
        with mock.patch.object(presence.harness.psutil, "Process") as process:
            with self.assertRaises(presence.harness.HarnessFailure):
                presence.bind_games(plan(), plan()["legs"][0], {"a": 20, "b": 20})
            process.assert_not_called()

    def test_screenshot_failure_stops_inputs_and_cleans_up(self):
        m = self.lifecycle(); m["capture_game"].side_effect = RuntimeError("capture failed")
        with self.assertRaisesRegex(RuntimeError, "capture failed"):
            presence.check_presence(plan())
        self.assertEqual(m["tap"].call_count, 1)
        m["require_presence_published"].assert_not_called()
        m["stop_runtime"].assert_called_once()
        m["close_desktops"].assert_called_once()

    def test_actual_process_rom_and_emulator_are_hashed(self):
        process = mock.Mock(); process.cmdline.return_value = ["mgba.exe", "actual.gba", "--script", "bridge.lua"]
        process.exe.return_value = "actual-mgba.exe"
        with ExitStack() as stack:
            stack.enter_context(mock.patch.object(presence.harness, "_paths", return_value=(Path("release"), Path("run"))))
            stack.enter_context(mock.patch.object(presence.harness, "_read_json", return_value={"worlds": [{"world_id": 1, "rom_sha256": "rom"}]}))
            stack.enter_context(mock.patch.object(presence.harness.psutil, "Process", return_value=process))
            stack.enter_context(mock.patch.object(presence.harness, "digest", return_value="exe"))
            stack.enter_context(mock.patch.object(presence.harness, "_rom_header_title", return_value="MAIN"))
            hashes = stack.enter_context(mock.patch.object(presence.harness, "require_hash"))
            result = presence.bind_games(plan(), plan()["legs"][0], {"a": 20, "b": 21})
            self.assertEqual(result["a"]["rom_header"], "MAIN")
            self.assertEqual([call.args[1] for call in hashes.call_args_list], ["rom", "exe", "rom", "exe"])
            process.cmdline.return_value.append("second.gba")
            with self.assertRaisesRegex(presence.harness.HarnessFailure, "one actual ROM"):
                presence.bind_games(plan(), plan()["legs"][0], {"a": 20, "b": 21})


if __name__ == "__main__":
    unittest.main()
