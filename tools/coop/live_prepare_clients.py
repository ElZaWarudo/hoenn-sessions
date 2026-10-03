"""One-time test-only signed desktop authentication/install via its real UI.

Existing account profiles and complete fixture caches are retained. No account,
accepted-generation marker or save is fabricated. Form credentials come only
from environment and are never printed or checkpointed.
"""

import ctypes
import os
import time
from ctypes import wintypes
from pathlib import Path

import psutil

from live_harness_windows import (Win32Adapter, capture_desktop, click_desktop,
                                  launch_signed_desktop, wait_desktop_window)
from live_region_harness import (HarnessFailure, _paths, _read_json, checkpoint,
                                 check_c_space, close_desktops, digest, preflight)
from live_region_harness import check_installed_family, accepted_family_ready


def click_form(pid, x, y, adapter):
    matches = [w for w in adapter.windows() if w.pid == pid and w.visible
               and w.title == "Hoenn Sessions"]
    if len(matches) != 1:
        raise HarnessFailure("prepare: signed desktop window is ambiguous")
    handle = matches[0].handle
    adapter.user32.GetDpiForWindow.argtypes = [wintypes.HWND]
    adapter.user32.GetDpiForWindow.restype = wintypes.UINT
    adapter.user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
    adapter.user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    scale = adapter.user32.GetDpiForWindow(handle) / 96
    point = wintypes.POINT(round(x * scale), round(y * scale))
    rect = wintypes.RECT()
    if not adapter.user32.ClientToScreen(handle, ctypes.byref(point)) or not adapter.user32.GetWindowRect(handle, ctypes.byref(rect)):
        raise HarnessFailure("prepare: signed form coordinates unavailable")
    click_desktop(pid, point.x - rect.left, point.y - rect.top, adapter)
    return handle


def type_text(value, adapter, handle):
    adapter.user32.VkKeyScanW.argtypes = [ctypes.c_wchar]
    adapter.user32.VkKeyScanW.restype = ctypes.c_short
    # Check the entire string before pressing any key; never type a prefix of
    # an unsupported password and then accidentally submit it.
    codes = [adapter.user32.VkKeyScanW(char) for char in value]
    if any(code == -1 for code in codes):
        raise HarnessFailure("prepare: form text is unsupported by the keyboard layout")
    for code in codes:
        modifiers = [vk for bit, vk in ((1, 16), (2, 17), (4, 18)) if (code >> 8) & bit]
        try:
            if adapter.foreground() != handle:
                raise HarnessFailure("prepare: signed form lost focus; input stopped")
            for vk in modifiers:
                if adapter.foreground() != handle:
                    raise HarnessFailure("prepare: signed form lost focus; input stopped")
                adapter.key(vk, True)
            if adapter.foreground() != handle:
                raise HarnessFailure("prepare: signed form lost focus; input stopped")
            adapter.key(code & 255, True)
            time.sleep(.02)
        finally:
            adapter.key(code & 255, False)
            for vk in reversed(modifiers):
                adapter.key(vk, False)
        time.sleep(.02)


def _cache_ready(plan, player):
    root = Path(player["profile_localappdata"]) / "Hoenn Sessions"
    account_path = root / "account.json"
    if not account_path.exists():
        return False
    account = _read_json(account_path, "prepare account")
    if account.get("character_id") != player["character_id"]:
        raise HarnessFailure("prepare: profile belongs to another character; preserve it")
    cache = root / "runtime/releases/generations" / plan["release_id"]
    if not (cache / ".complete").is_file():
        return False
    check_installed_family(plan, cache)
    return accepted_family_ready(cache)


def prepare(plan):
    preflight(plan, profiles_ready=False)
    release, run_dir = _paths(plan)
    owned = {}
    result = {"reused": [], "prepared": []}
    try:
        for player in plan["players"]:
            name = player["name"]
            if _cache_ready(plan, player):
                result["reused"].append(name)
                continue
            username = os.environ.get("COOP_HARNESS_USERNAME_" + name.upper())
            password = os.environ.get("COOP_HARNESS_PASSWORD")
            if not username or not password:
                raise HarnessFailure("prepare: test credentials must be in environment")
            process = launch_signed_desktop(release / "app/coop-launcher.exe",
                                            Path(player["profile_localappdata"]))
            owned[name] = process.pid
            checkpoint(run_dir, "prepare-desktop-launched", {"player": name, "pid": process.pid})
            adapter = Win32Adapter()
            deadline = time.monotonic() + 180
            wait_desktop_window(process.pid, adapter, timeout=20)
            account_path = Path(player["profile_localappdata"]) / "Hoenn Sessions/account.json"
            if not account_path.exists():
                click_form(process.pid, 184, 112, adapter)
                time.sleep(.4)
                # Native activation may consume the first click. On the opened
                # form this position is above the credential inputs.
                click_form(process.pid, 184, 112, adapter)
                time.sleep(.4)
                handle = click_form(process.pid, 160, 145, adapter)
                type_text(username, adapter, handle)
                handle = click_form(process.pid, 160, 191, adapter)
                type_text(password, adapter, handle)
                handle = click_form(process.pid, 64, 249, adapter)
                time.sleep(.3)
                # The first native click may only focus the submit button.
                # A second click submits the form; on the following status
                # screen this point is blank, below the runtime controls.
                handle = click_form(process.pid, 64, 249, adapter)
                time.sleep(.3)
                # egui's focused submit button accepts Enter reliably after
                # an activation click was consumed by native focus changes.
                if adapter.foreground() != handle:
                    raise HarnessFailure("prepare: signed form lost focus before submit")
                try:
                    adapter.key(13, True)
                    time.sleep(.1)
                finally:
                    adapter.key(13, False)
            password = ""
            while not _cache_ready(plan, player):
                check_c_space()
                if process.poll() is not None:
                    raise HarnessFailure("prepare: signed desktop exited before installation completed")
                if time.monotonic() >= deadline:
                    capture_desktop(process.pid, run_dir / "screenshots", "prepare-failed-" + name, adapter)
                    raise HarnessFailure("prepare: signed authentication/install deadline exceeded")
                time.sleep(.5)
            result["prepared"].append(name)
        preflight(plan)  # The regular gate remains strict for every later step.
        checkpoint(run_dir, "signed-profiles-prepared", result)
        return result
    finally:
        import sys
        primary = sys.exc_info()[1]
        if owned:
            try:
                close_desktops(owned)
            except Exception as cleanup:
                if primary is None:
                    raise
                primary.add_note("Prepare client cleanup also failed: " + str(cleanup))
