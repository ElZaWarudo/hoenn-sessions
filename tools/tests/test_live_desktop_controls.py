"""Mocked signed desktop Play/Stop presses; never touches real windows or processes."""
from contextlib import ExitStack
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_desktop_controls as controls
from live_harness_windows import Window, WindowControlError

WIDTH, HEIGHT = 320, 240


def capture(play: bool, stop: bool) -> tuple[int, int, bytes]:
    """Light window with dark label ink only inside enabled action buttons."""
    rgb = bytearray([239]) * (WIDTH * HEIGHT * 3)
    for enabled, (cx, cy) in ((play, controls.PLAY), (stop, controls.STOP)):
        shade = 60 if enabled else 160  # Disabled egui labels stay light grey.
        for y in range(cy - 5, cy + 6):
            for x in range(cx - 12, cx + 13, 2):
                i = (y * WIDTH + x) * 3
                rgb[i:i + 3] = bytes((shade, shade, shade))
    return WIDTH, HEIGHT, bytes(rgb)


class Clock:
    def __init__(self):
        self.now = 0.0

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class Desktop:
    """One signed window whose controller reacts only to registered presses."""

    def __init__(self, *, play=True, stop=False, drops=0, foreground=True):
        self.play, self.stop, self.drops = play, stop, drops
        self.handle, self.pid = 77, 7
        self.is_foreground = foreground
        self.clicks, self.nudges, self.events = [], [], []

    def windows(self):
        return [Window(self.handle, self.pid, "Hoenn Sessions", True)]

    def restore(self, handle):
        pass

    def focus(self, handle):
        return self.is_foreground

    def foreground(self):
        return self.handle if self.is_foreground else 1

    def capture_rgb(self, handle):
        return capture(self.play, self.stop)

    def click(self, pid, x, y, adapter, *, settle=0.0):
        self.clicks.append((pid, x, y, settle))
        self.events.append("click")
        if self.drops:
            self.drops -= 1
            return
        if (x, y) == controls.PLAY and self.play:
            self.play, self.stop = False, True
        elif (x, y) == controls.STOP and self.stop:
            self.stop = False

    def nudge(self, adapter, handle, x, y):
        self.nudges.append((handle, x, y))
        self.events.append("nudge")


class PressTests(unittest.TestCase):
    def setUp(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        self.clock = Clock()
        stack.enter_context(mock.patch.object(controls, "time", SimpleNamespace(
            monotonic=self.clock.monotonic, sleep=self.clock.sleep)))
        self.stack = stack

    def wire(self, desktop):
        self.stack.enter_context(mock.patch.object(controls, "click_desktop", side_effect=desktop.click))
        self.stack.enter_context(mock.patch.object(controls, "nudge_pointer", side_effect=desktop.nudge))
        return desktop

    def play(self, desktop, **kwargs):
        def finished():  # Stand-in for the mGBA PID appearing once Starting.
            return not desktop.play and desktop.stop
        return controls.press_until(desktop.pid, controls.PLAY, desktop,
                                    ready=lambda a: a["play"] and not a["stop"],
                                    registered=lambda a: a["stop"] or not a["play"],
                                    done=kwargs.pop("finished", finished),
                                    deadline=kwargs.pop("deadline", 120), label="a: Play", **kwargs)

    def test_label_classifier_reads_enabled_and_disabled_actions(self):
        self.assertEqual(controls.label_enabled(capture(True, False), controls.PLAY), True)
        self.assertEqual(controls.label_enabled(capture(True, False), controls.STOP), False)
        self.assertEqual(controls.desktop_actions(7, Desktop(play=False, stop=True)), {"play": False, "stop": True})
        with self.assertRaises(controls.harness.HarnessFailure):
            controls.label_enabled((10, 10, bytes(300)), controls.PLAY)

    def test_play_press_hovers_with_settle_and_counts_on_status_change(self):
        desktop = self.wire(Desktop())
        result = self.play(desktop)
        self.assertEqual(desktop.clicks, [(7, 88, 178, controls.SETTLE)])
        self.assertEqual(controls.SETTLE, .15)
        self.assertEqual(desktop.nudges, [])
        self.assertEqual(result, {"presses": 1, "unregistered": 0, "accepted": 1})

    def test_dropped_press_is_retried_after_a_one_pixel_nudge(self):
        desktop = self.wire(Desktop(drops=2))
        result = self.play(desktop)
        self.assertEqual(desktop.events, ["click", "nudge", "click", "nudge", "click"])
        self.assertEqual(desktop.nudges, [(77, 88, 178)] * 2)
        self.assertTrue(all(click[3] == controls.SETTLE for click in desktop.clicks))
        self.assertEqual(result, {"presses": 3, "unregistered": 2, "accepted": 1})

    def test_no_press_until_play_is_drawn_enabled(self):
        desktop = self.wire(Desktop(play=False))
        def refresh(seconds):
            self.clock.now += seconds
            if self.clock.now >= 5:
                desktop.play = True  # Cached-account refresh finished: Ready.
        self.stack.enter_context(mock.patch.object(controls.time, "sleep", side_effect=refresh))
        self.play(desktop)
        self.assertEqual(len(desktop.clicks), 1)
        self.assertGreaterEqual(self.clock.now, 5)

    def test_unchanged_status_is_never_counted_and_budget_fails_closed(self):
        desktop = self.wire(Desktop(drops=99))
        with self.assertRaisesRegex(controls.harness.HarnessFailure, "never changed the desktop status"):
            self.play(desktop)
        self.assertEqual(len(desktop.clicks), controls.MAX_UNREGISTERED)
        self.assertEqual(len(desktop.nudges), controls.MAX_UNREGISTERED - 1)

    def test_overall_deadline_fails_closed_while_waiting(self):
        desktop = self.wire(Desktop(play=False))
        with self.assertRaisesRegex(controls.harness.HarnessFailure, "not finished before the deadline"):
            self.play(desktop, deadline=10)
        self.assertEqual(desktop.clicks, [])

    def test_lost_foreground_stops_before_any_press(self):
        desktop = self.wire(Desktop(foreground=False))
        with self.assertRaises(WindowControlError):
            self.play(desktop)
        self.assertEqual(desktop.clicks, [])

    def test_stop_press_is_settled_and_confirmed_by_stop_disabling(self):
        desktop = self.wire(Desktop(play=False, stop=True, drops=1))
        result = controls.press_until(7, controls.STOP, desktop, ready=lambda a: a["stop"],
                                      registered=lambda a: not a["stop"], done=lambda: not desktop.stop,
                                      deadline=45, label="a: Stop")
        self.assertEqual(desktop.events, ["click", "nudge", "click"])
        self.assertEqual(desktop.clicks[-1], (7, 235, 178, controls.SETTLE))
        self.assertEqual(result["accepted"], 1)


class StartStopTests(unittest.TestCase):
    def setUp(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        self.clock = Clock()
        stack.enter_context(mock.patch.object(controls, "time", SimpleNamespace(
            monotonic=self.clock.monotonic, sleep=self.clock.sleep)))
        self.desktop = Desktop()
        self.game = {"pid": None}
        h = controls.harness
        stack.enter_context(mock.patch.object(h, "_paths", return_value=(Path("release"), Path("run"))))
        stack.enter_context(mock.patch.object(h, "Win32Adapter", return_value=self.desktop))
        stack.enter_context(mock.patch.object(h, "_mgba_pid_for", side_effect=lambda profile: self.game["pid"]))
        stack.enter_context(mock.patch.object(h, "_rom_header_title", return_value="TITLE"))
        stack.enter_context(mock.patch.object(h.psutil, "pid_exists", return_value=True))
        self.checkpoint = stack.enter_context(mock.patch.object(h, "checkpoint"))
        stack.enter_context(mock.patch.object(controls, "wait_desktop_window"))
        stack.enter_context(mock.patch.object(controls, "wait_game_window"))
        self.capture = stack.enter_context(mock.patch.object(controls, "capture_desktop", return_value=Path("failed.png")))
        stack.enter_context(mock.patch.object(controls, "nudge_pointer", side_effect=self.desktop.nudge))
        process = mock.Mock(); process.cmdline.return_value = ["mgba.exe", "--script", "x.lua", "game.gba"]
        process.children.return_value = [SimpleNamespace(pid=20)]
        stack.enter_context(mock.patch.object(h.psutil, "Process", return_value=process))
        self.stack = stack
        self.plan = {"players": [{"name": "a", "profile_localappdata": "profile-a"}], "start_timeout_seconds": 60}

    def test_start_games_records_registered_play_and_binds_mgba(self):
        def click(*args, **kwargs):
            self.desktop.click(*args, **kwargs)
            if self.desktop.stop:
                self.game["pid"] = 20
        self.stack.enter_context(mock.patch.object(controls, "click_desktop", side_effect=click))
        self.assertEqual(controls.start_games(self.plan, {"a": 7}), {"a": 20})
        self.assertEqual(self.desktop.clicks, [(7, 88, 178, controls.SETTLE)])
        boundary, payload = self.checkpoint.call_args.args[1:]
        self.assertEqual(boundary, "signed-games-started")
        self.assertEqual(payload["play_presses"]["a"]["accepted"], 1)

    def test_start_games_fails_closed_with_status_image_when_presses_never_register(self):
        self.desktop.drops = 99
        self.stack.enter_context(mock.patch.object(controls, "click_desktop", side_effect=self.desktop.click))
        with self.assertRaisesRegex(controls.harness.HarnessFailure, "a: signed desktop did not start mGBA") as caught:
            controls.start_games(self.plan, {"a": 7})
        self.capture.assert_called_once()
        self.assertEqual(self.capture.call_args.args[2], "startup-failed-a")
        self.assertIn("failed.png", " ".join(caught.exception.__cause__.__notes__))
        self.checkpoint.assert_not_called()

    def test_stop_runtime_uses_settled_confirmed_stop_and_skips_absent_games(self):
        self.desktop.play, self.desktop.stop = False, True
        self.game["pid"] = 20
        def click(*args, **kwargs):
            self.desktop.click(*args, **kwargs)
            self.game["pid"] = None
        self.stack.enter_context(mock.patch.object(controls, "click_desktop", side_effect=click))
        self.stack.enter_context(mock.patch.object(controls, "_runtime_children", return_value=[]))
        controls.stop_runtime(self.plan, {"a": 7})
        self.assertEqual(self.desktop.clicks, [(7, 235, 178, controls.SETTLE)])
        controls.stop_runtime(self.plan, {"a": 7})  # Already drained: no further press.
        self.assertEqual(len(self.desktop.clicks), 1)


if __name__ == "__main__":
    unittest.main()
