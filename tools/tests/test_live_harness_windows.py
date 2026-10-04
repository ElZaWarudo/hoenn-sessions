"""Focused tests for the external Windows live-test control boundary."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.coop import live_harness_windows as harness


class FakeAdapter:
    def __init__(self, windows: list[harness.Window], foreground: int = 0):
        self.list = windows
        self.active = foreground
        self.events: list[tuple] = []
        self.allow_focus = True

    def windows(self):
        return self.list

    def minimize(self, handle):
        self.events.append(("minimize", handle))

    def restore(self, handle):
        self.events.append(("restore", handle))

    def foreground(self):
        return self.active

    def focus(self, handle):
        self.events.append(("focus", handle))
        if self.allow_focus:
            self.active = handle
        return self.allow_focus

    def key(self, vk, down):
        self.events.append(("key", vk, down))

    def capture_rgb(self, handle):
        self.events.append(("capture", handle))
        return 1, 1, b"\xff\x00\x00"


class LiveHarnessWindowsTests(unittest.TestCase):
    def caption_adapter(self):
        adapter = harness.Win32Adapter.__new__(harness.Win32Adapter)
        adapter.user32 = mock.Mock()
        def rect(handle, pointer):
            pointer._obj.left, pointer._obj.top = 10, 20
            pointer._obj.right, pointer._obj.bottom = 800, 600
            return True
        adapter.user32.GetWindowRect.side_effect = rect
        adapter.user32.GetSystemMetrics.side_effect = lambda metric: 23 if metric == 4 else 8
        adapter.user32.SetWindowPos.return_value = True
        adapter.user32.WindowFromPoint.return_value = 99
        adapter.user32.GetAncestor.return_value = 99
        def hit(handle, message, wparam, lparam, flags, timeout, pointer):
            pointer._obj.value = 2
            return True
        adapter.user32.SendMessageTimeoutW.side_effect = hit
        adapter.user32.SetCursorPos.return_value = True
        adapter.user32.GetForegroundWindow.return_value = 99
        return adapter

    def test_settled_click_hovers_then_presses_only_on_unmoved_foreground(self):
        adapter = self.caption_adapter()
        with mock.patch.object(harness.time, "sleep") as sleep:
            adapter.click_window_pixel(99, 30, 40, settle=.15)
        sleep.assert_any_call(.15)
        self.assertEqual(adapter.user32.SetCursorPos.call_args_list,
                         [mock.call(40, 60), mock.call(40, 60)])
        self.assertEqual(adapter.user32.mouse_event.call_args_list,
                         [mock.call(0x0002, 0, 0, 0, None), mock.call(0x0004, 0, 0, 0, None)])

    def test_settled_click_sends_no_press_after_focus_loss_or_move(self):
        for change in ("foreground", "moved"):
            adapter = self.caption_adapter()
            if change == "foreground":
                adapter.user32.GetForegroundWindow.return_value = 100
                expected = "lost foreground"
            else:
                rects = iter([(10, 20), (11, 20)])
                def rect(handle, pointer):
                    pointer._obj.left, pointer._obj.top = next(rects)
                    pointer._obj.right, pointer._obj.bottom = 800, 600
                    return True
                adapter.user32.GetWindowRect.side_effect = rect
                expected = "moved"
            with mock.patch.object(harness.time, "sleep"):
                with self.assertRaisesRegex(harness.WindowControlError, expected):
                    adapter.click_window_pixel(99, 30, 40, settle=.15)
            adapter.user32.mouse_event.assert_not_called()

    def test_click_desktop_passes_settle_only_when_requested(self):
        window = harness.Window(99, 7, "Hoenn Sessions", True)
        adapter = FakeAdapter([window], foreground=99)
        adapter.click_window_pixel = mock.Mock()
        harness.click_desktop(7, 5, 6, adapter)
        harness.click_desktop(7, 5, 6, adapter, settle=.15)
        self.assertEqual(adapter.click_window_pixel.call_args_list,
                         [mock.call(99, 5, 6), mock.call(99, 5, 6, settle=.15)])

    def test_native_caption_activation_checks_target_and_nonclient_hit(self):
        adapter = self.caption_adapter()
        with mock.patch.object(harness.time, "sleep"):
            self.assertTrue(adapter.activate_caption(99))
        self.assertEqual(adapter.user32.mouse_event.call_args_list,
                         [mock.call(0x0002, 0, 0, 0, None), mock.call(0x0004, 0, 0, 0, None)])
        self.assertEqual(adapter.user32.WindowFromPoint.call_count, 2)
        adapter.user32.keybd_event.assert_not_called()

    def test_caption_occlusion_client_pixel_and_failed_move_send_no_click(self):
        for change in ("occluded", "client", "move", "changed_after_move", "timeout"):
            adapter = self.caption_adapter()
            if change == "occluded":
                adapter.user32.GetAncestor.return_value = 100
            elif change == "client":
                def client_hit(*args):
                    args[-1]._obj.value = 1
                    return True
                adapter.user32.SendMessageTimeoutW.side_effect = client_hit
            elif change == "move":
                adapter.user32.SetCursorPos.return_value = False
            elif change == "changed_after_move":
                adapter.user32.GetAncestor.side_effect = [99, 100]
            else:
                adapter.user32.SendMessageTimeoutW.side_effect = lambda *args: False
            self.assertFalse(adapter.activate_caption(99))
            adapter.user32.mouse_event.assert_not_called()
            adapter.user32.keybd_event.assert_not_called()

    def test_caption_button_release_occurs_if_click_raises(self):
        adapter = self.caption_adapter()
        adapter.user32.mouse_event.side_effect = [RuntimeError("click failed"), None]
        with self.assertRaisesRegex(RuntimeError, "click failed"):
            adapter.activate_caption(99)
        self.assertEqual(adapter.user32.mouse_event.call_args_list[-1], mock.call(0x0004, 0, 0, 0, None))

    def test_caption_waits_for_async_raise_without_clicking_occluding_window(self):
        adapter = self.caption_adapter()
        adapter.user32.GetAncestor.side_effect = [100, 99, 99]
        adapter.user32.GetForegroundWindow.side_effect = [100, 99]
        with mock.patch.object(harness.time, "sleep"):
            self.assertTrue(adapter.activate_caption(99))
        self.assertEqual(adapter.user32.WindowFromPoint.call_count, 3)
        self.assertEqual(adapter.user32.SendMessageTimeoutW.call_count, 2)
        self.assertEqual(adapter.user32.mouse_event.call_count, 2)
        adapter.user32.keybd_event.assert_not_called()

    def test_caption_fallback_still_requires_verified_foreground(self):
        adapter = FakeAdapter([harness.Window(99, 1, "mGBA - POKEMON EMER", True)])
        adapter.allow_focus = False
        adapter.activate_caption = mock.Mock(return_value=True)
        with self.assertRaises(harness.WindowControlError):
            harness.focus_game(1, adapter, timeout=.01)
        adapter.activate_caption.assert_called_once_with(99)
        self.assertFalse(any(event[0] == "key" for event in adapter.events))

    def test_caption_click_without_foreground_times_out_and_releases_mouse(self):
        adapter = self.caption_adapter()
        adapter.user32.GetForegroundWindow.return_value = 100
        with mock.patch.object(harness.time, "monotonic", side_effect=[0, 0, .26]), \
                mock.patch.object(harness.time, "sleep"):
            self.assertFalse(adapter.activate_caption(99))
        self.assertEqual(adapter.last_caption_failure, "foreground_timeout")
        self.assertEqual(adapter.user32.mouse_event.call_args_list,
                         [mock.call(0x0002, 0, 0, 0, None), mock.call(0x0004, 0, 0, 0, None)])
        adapter.user32.keybd_event.assert_not_called()

    def test_focus_failure_reports_caption_boundary_without_gameplay_input(self):
        adapter = FakeAdapter([harness.Window(99, 1, "mGBA - POKEMON EMER", True)])
        adapter.allow_focus = False
        adapter.activate_caption = mock.Mock(return_value=False)
        adapter.last_caption_failure = "cursor_move_error_0"
        with self.assertRaisesRegex(harness.WindowControlError, "caption activation: cursor_move_error_0"):
            harness.focus_game(1, adapter, timeout=.01)
        self.assertFalse(any(event[0] == "key" for event in adapter.events))

    def test_native_focus_attaches_only_target_and_always_detaches(self):
        adapter = harness.Win32Adapter.__new__(harness.Win32Adapter)
        adapter.user32, adapter.kernel32 = mock.Mock(), mock.Mock()
        adapter.kernel32.GetCurrentThreadId.return_value = 10
        adapter.user32.GetWindowThreadProcessId.return_value = 20
        adapter.user32.SetForegroundWindow.side_effect = [False, True]
        adapter.user32.AttachThreadInput.return_value = True
        self.assertTrue(adapter.focus(99))
        self.assertEqual(adapter.user32.AttachThreadInput.call_args_list,
                         [mock.call(10, 20, True), mock.call(10, 20, False)])
        adapter.user32.keybd_event.assert_not_called()
        adapter.user32.AttachThreadInput.reset_mock()
        adapter.user32.SetForegroundWindow.side_effect = [False, RuntimeError("focus failed")]
        with self.assertRaisesRegex(RuntimeError, "focus failed"):
            adapter.focus(99)
        self.assertEqual(adapter.user32.AttachThreadInput.call_args_list[-1], mock.call(10, 20, False))

    def test_native_focus_missing_target_or_rejected_attachment_sends_no_input(self):
        for target in (0, 20):
            with self.subTest(target=target):
                adapter = harness.Win32Adapter.__new__(harness.Win32Adapter)
                adapter.user32, adapter.kernel32 = mock.Mock(), mock.Mock()
                adapter.kernel32.GetCurrentThreadId.return_value = 10
                adapter.user32.GetWindowThreadProcessId.return_value = target
                adapter.user32.SetForegroundWindow.return_value = False
                adapter.user32.AttachThreadInput.return_value = False
                self.assertFalse(adapter.focus(99))
                adapter.user32.BringWindowToTop.assert_not_called()
                adapter.user32.keybd_event.assert_not_called()

    def setUp(self):
        self.adapter = FakeAdapter([
            harness.Window(10, 100, "", False),
            harness.Window(11, 100, "Scripts - mGBA", True),
            harness.Window(12, 100, "Pokemon Emerald - mGBA", True),
            harness.Window(13, 200, "Other - mGBA", True),
        ])

    def test_selects_game_by_pid_and_title_not_dummy_or_scripts(self):
        self.assertEqual(harness.game_window(100, self.adapter).handle, 12)
        self.assertEqual(harness.focus_game(100, self.adapter).handle, 12)
        self.assertEqual(self.adapter.events[:3],
                         [("minimize", 11), ("restore", 12), ("focus", 12)])

    def test_missing_or_ambiguous_game_fails_closed(self):
        with self.assertRaisesRegex(harness.WindowControlError, "expected one"):
            harness.game_window(300, self.adapter)
        self.adapter.list.append(harness.Window(14, 100, "Second - mGBA", True))
        with self.assertRaisesRegex(harness.WindowControlError, "found 2"):
            harness.game_window(100, self.adapter)

    def test_wait_fails_immediately_for_ambiguous_game(self):
        self.adapter.list.append(harness.Window(14, 100, "Second - mGBA", True))
        with self.assertRaisesRegex(harness.WindowControlError, "found 2"):
            harness.wait_game_window(100, self.adapter)

    @mock.patch.object(harness.time, "sleep")
    def test_rom_wait_rejects_empty_emulator_window_until_header_title(self, sleep):
        loaded = harness.Window(12, 100, "mGBA - POKEMON EMER (60 fps)", True)
        self.adapter.windows = mock.Mock(side_effect=[
            [harness.Window(12, 100, "mGBA - 0.11", True)], [loaded]])
        self.assertEqual(harness.wait_game_window(100, self.adapter,
                                                rom_title="POKEMON EMER"), loaded)
        sleep.assert_called_once()

    @mock.patch.object(harness.time, "sleep")
    def test_desktop_wait_ignores_other_windows_until_owned_window_exists(self, sleep):
        target = harness.Window(20, 300, "Hoenn Sessions", True)
        self.adapter.windows = mock.Mock(side_effect=[
            [harness.Window(21, 301, "Hoenn Sessions", True)],
            [harness.Window(20, 300, "Hoenn Sessions", False)],
            [target],
        ])
        self.assertEqual(harness.wait_desktop_window(300, self.adapter), target)
        self.assertEqual(sleep.call_count, 2)
        self.assertEqual(self.adapter.events, [])

    def test_desktop_wait_rejects_ambiguity_without_polling(self):
        self.adapter.list = [harness.Window(20, 300, "Hoenn Sessions", True),
                             harness.Window(21, 300, "Hoenn Sessions", True)]
        with mock.patch.object(harness.time, "sleep") as sleep:
            with self.assertRaisesRegex(harness.WindowControlError, "multiple"):
                harness.wait_desktop_window(300, self.adapter)
            sleep.assert_not_called()

    @mock.patch.object(harness.time, "monotonic", side_effect=[0, 20])
    def test_desktop_wait_has_bounded_missing_window_deadline(self, _clock):
        with self.assertRaisesRegex(harness.WindowControlError, "deadline"):
            harness.wait_desktop_window(300, self.adapter)

    def test_refuses_input_when_foreground_cannot_be_verified(self):
        self.adapter.allow_focus = False
        with self.assertRaisesRegex(harness.WindowControlError, "foreground"):
            harness.tap(100, "x", self.adapter, hold=0.01)
        self.assertFalse(any(event[0] == "key" for event in self.adapter.events))

    @mock.patch.object(harness.time, "sleep")
    def test_tap_always_releases_key(self, _sleep):
        self.adapter.key = mock.Mock(side_effect=[None, None])
        harness.tap(100, "x", self.adapter)
        self.assertEqual(self.adapter.key.call_args_list,
                         [mock.call(0x58, True), mock.call(0x58, False)])

    @mock.patch.object(harness.time, "sleep", side_effect=[RuntimeError("interrupted")])
    def test_tap_releases_key_on_interruption(self, _sleep):
        with self.assertRaisesRegex(RuntimeError, "interrupted"):
            harness.tap(100, "x", self.adapter)
        self.assertEqual(self.adapter.events[-2:],
                         [("key", 0x58, True), ("key", 0x58, False)])

    def test_capture_writes_png_to_supplied_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            output = harness.capture_game(100, Path(directory), "main-harbor", self.adapter)
            self.assertEqual(output.parent, Path(directory))
            self.assertTrue(output.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"))
            with self.assertRaises(ValueError):
                harness.capture_game(100, Path(directory), "../escape", self.adapter)

    def test_failed_startup_capture_requires_exact_signed_window(self):
        self.adapter.list.append(harness.Window(20, 300, "Hoenn Sessions", True))
        with tempfile.TemporaryDirectory() as directory:
            output = harness.capture_desktop(300, Path(directory), "startup-a", self.adapter)
            self.assertTrue(output.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"))
            self.assertIn(("capture", 20), self.adapter.events)
        self.adapter.list.append(harness.Window(21, 300, "Hoenn Sessions", True))
        with self.assertRaisesRegex(harness.WindowControlError, "expected one"):
            harness.capture_desktop(300, Path(directory), "startup-a", self.adapter)

    def test_launch_uses_existing_executable_and_isolated_localappdata(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "signed" / "desktop.exe"
            executable.parent.mkdir()
            executable.write_bytes(b"fixture")
            with mock.patch.object(harness.subprocess, "Popen") as popen:
                harness.launch_signed_desktop(executable, root / "profile")
            args, kwargs = popen.call_args
            self.assertEqual(args[0], [str(executable)])
            self.assertEqual(kwargs["cwd"], executable.parent)
            self.assertEqual(kwargs["env"]["LOCALAPPDATA"], str(root / "profile"))
            with self.assertRaises(ValueError):
                harness.launch_signed_desktop(executable, root / "profile",
                                              extra_env={"LOCALAPPDATA": "override"})

    def test_launch_rejects_client_that_exits_immediately(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "signed" / "desktop.exe"
            executable.parent.mkdir()
            executable.write_bytes(b"fixture")
            dead = mock.Mock()
            dead.poll.return_value = 17
            with mock.patch.object(harness.subprocess, "Popen", return_value=dead):
                with self.assertRaisesRegex(harness.WindowControlError, "exited during startup"):
                    harness.launch_signed_desktop(executable, root / "profile")

    def test_signed_client_logs_share_one_stream_and_parent_closes_handle(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe = root / "desktop.exe"
            exe.write_bytes(b"fixture")
            log = root / "logs/client.log"
            with mock.patch.object(harness.subprocess, "Popen") as popen:
                harness.launch_signed_desktop(exe, root / "profile", output_log=log)
            stream = popen.call_args.kwargs["stdout"]
            self.assertEqual(Path(stream.name), log)
            self.assertTrue(stream.closed)
            self.assertEqual(popen.call_args.kwargs["stderr"], harness.subprocess.STDOUT)
            self.assertTrue(log.is_file())


if __name__ == "__main__":
    unittest.main()
