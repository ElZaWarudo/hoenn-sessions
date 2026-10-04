"""Test-only signed desktop Play/Stop presses that must be seen to register.

egui repaints only on input, so a pointer jump plus press plus release that
lands in one idle frame can be dropped, and SetCursorPos to the position the
cursor already holds sends no WM_MOUSEMOVE: blind retries at the same pixel
fall into the same dropped state (live: a return-leg desktop stayed "Ready to
play." through 120 one-per-second Play clicks). Every press here hovers first
(``SETTLE``), nudges the pointer one pixel off target before each retry so the
hover is real movement, and counts a press only once the window shows the
controller left that state. Exhausting the budget fails closed.

Kept apart from live_region_harness.py and live_harness_windows.py: those
files key the fixture-authoring caches, and this fix changes no authored byte.
"""
from __future__ import annotations

import ctypes
from ctypes import wintypes
from pathlib import Path
import time
from typing import Callable

import live_region_harness as harness
from live_harness_windows import (WindowControlError, capture_desktop, click_desktop,
                                  wait_desktop_window, wait_game_window)

# Window-rect pixels of the signed desktop's primary actions (renderer.rs
# draw_actions). Play is enabled only in Ready; Stop only in Starting/Running.
PLAY = (88, 178)
STOP = (235, 178)
SETTLE = .15
NUDGE_PX = 1
CONFIRM_SECONDS = 3.0
MAX_UNREGISTERED = 8
# A label box well inside the 140x55 button; enabled labels are dark ink
# (measured 111 pixels below 128), disabled labels are light grey (min 154).
LABEL_HALF = (30, 9)
INK_LUMA = 110
MIN_INK = 20


def _desktop_handle(pid: int, adapter) -> int:
    """The single signed window, restored and foreground; otherwise stop."""
    matches = [w for w in adapter.windows() if w.pid == pid and w.visible
               and w.title == "Hoenn Sessions"]
    if len(matches) != 1:
        raise WindowControlError(f"expected one signed desktop window for PID {pid}")
    handle = matches[0].handle
    adapter.restore(handle)
    deadline = time.monotonic() + 3
    while adapter.foreground() != handle:
        adapter.focus(handle)
        if time.monotonic() >= deadline:
            raise WindowControlError(f"desktop window {handle} did not become foreground")
        time.sleep(.05)
    return handle


def label_enabled(capture: tuple[int, int, bytes], centre: tuple[int, int]) -> bool:
    width, height, rgb = capture
    if len(rgb) != width * height * 3:
        raise harness.HarnessFailure("signed desktop capture is invalid")
    (cx, cy), (hx, hy) = centre, LABEL_HALF
    if not (hx <= cx < width - hx and hy <= cy < height - hy):
        raise harness.HarnessFailure("signed desktop action is outside the captured window")
    ink = 0
    for y in range(cy - hy, cy + hy + 1):
        row = y * width
        for x in range(cx - hx, cx + hx + 1):
            i = (row + x) * 3
            if (rgb[i] * 299 + rgb[i + 1] * 587 + rgb[i + 2] * 114) // 1000 < INK_LUMA:
                ink += 1
    return ink >= MIN_INK


def desktop_actions(pid: int, adapter) -> dict[str, bool]:
    """Which primary actions the signed window currently draws enabled."""
    handle = _desktop_handle(pid, adapter)
    capture = adapter.capture_rgb(handle)
    if adapter.foreground() != handle:
        raise WindowControlError("signed desktop lost foreground during capture")
    return {"play": label_enabled(capture, PLAY), "stop": label_enabled(capture, STOP)}


def nudge_pointer(adapter, handle: int, x: int, y: int) -> None:
    """Park the pointer one pixel beside the target so the next hover moves."""
    rect = wintypes.RECT()
    adapter.user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    adapter.user32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
    if not adapter.user32.GetWindowRect(handle, ctypes.byref(rect)):
        raise WindowControlError("GetWindowRect failed")
    nx = x + NUDGE_PX if x + NUDGE_PX < rect.right - rect.left else x - NUDGE_PX
    if not adapter.user32.SetCursorPos(rect.left + nx, rect.top + y):
        raise WindowControlError("SetCursorPos failed")
    time.sleep(.05)


def press_until(pid: int, target: tuple[int, int], adapter, *, ready: Callable[[dict], bool],
                registered: Callable[[dict], bool], done: Callable[[], bool], deadline: float,
                label: str, attempts: int = MAX_UNREGISTERED, max_accepted: int = 3,
                confirm_seconds: float = CONFIRM_SECONDS) -> dict:
    """Settled presses until ``done`` holds, each one confirmed on screen.

    Presses only while ``ready`` holds for the drawn actions and otherwise
    waits (startup or shutdown in progress). A press whose confirmation window
    ends without ``registered`` or ``done`` counts as unregistered, and the
    next press is preceded by a one-pixel nudge. Raises at the overall
    deadline, after ``attempts`` unregistered presses, or after
    ``max_accepted`` presses the controller accepted without finishing.
    """
    presses = unregistered = accepted = 0

    def summary() -> dict:
        return {"presses": presses, "unregistered": unregistered, "accepted": accepted}

    while not done():
        if time.monotonic() >= deadline:
            raise harness.HarnessFailure(f"{label}: not finished before the deadline "
                                         f"({presses} settled presses, {accepted} accepted, "
                                         f"{unregistered} unregistered)")
        if not ready(desktop_actions(pid, adapter)):
            time.sleep(.5)
            continue
        if unregistered >= attempts:
            raise harness.HarnessFailure(f"{label}: {unregistered} settled presses never changed the "
                                         "desktop status; input is not reaching the window")
        if accepted >= max_accepted:
            raise harness.HarnessFailure(f"{label}: {accepted} accepted presses returned to the same "
                                         "state without finishing")
        handle = _desktop_handle(pid, adapter)
        if presses:
            nudge_pointer(adapter, handle, *target)
        click_desktop(pid, target[0], target[1], adapter, settle=SETTLE)
        presses += 1
        settle_deadline = time.monotonic() + confirm_seconds
        while True:
            if done():
                accepted += 1
                return summary()
            if registered(desktop_actions(pid, adapter)):
                accepted += 1
                break
            if time.monotonic() >= settle_deadline:
                unregistered += 1
                break
            time.sleep(.1)
    return summary()


def _pid_alive(pid: int) -> bool:
    try:
        return harness.psutil.pid_exists(pid)
    except (harness.psutil.Error, OSError):
        return False


def start_games(plan: dict, desktop_pids: dict[str, int]) -> dict:
    """Press Play in each installed client until it registers; bind its exact mGBA."""
    _, run_dir = harness._paths(plan)
    adapter = harness.Win32Adapter()
    game_pids, presses = {}, {}
    timeout = plan.get("start_timeout_seconds", 45)
    if type(timeout) is not int or not 1 <= timeout <= 180:
        raise harness.HarnessFailure("start timeout must be an integer within [1, 180] seconds")
    for player in plan["players"]:
        name, profile = player["name"], Path(player["profile_localappdata"])
        desktop_pid = desktop_pids.get(name)
        if type(desktop_pid) is not int or desktop_pid <= 0:
            raise harness.HarnessFailure(f"{name}: invalid signed desktop PID")
        game_pid = harness._mgba_pid_for(profile)
        if game_pid is None:
            wait_desktop_window(desktop_pid, adapter, timeout=20)
            deadline = time.monotonic() + timeout

            def opened() -> bool:
                if not _pid_alive(desktop_pid):
                    raise harness.HarnessFailure(f"{name}: signed desktop exited before mGBA opened")
                return harness._mgba_pid_for(profile) is not None

            try:
                # Play left enabled means still Ready: the press was dropped.
                # Stop enabled (Starting/Running) or Play disabled proves the
                # controller accepted it; the signed controller disables Play
                # once startup begins, so presses cannot open a second game.
                presses[name] = press_until(
                    desktop_pid, PLAY, adapter, ready=lambda a: a["play"] and not a["stop"],
                    registered=lambda a: a["stop"] or not a["play"], done=opened,
                    deadline=deadline, label=f"{name}: Play")
                game_pid = harness._mgba_pid_for(profile)
                if game_pid is None:
                    raise harness.HarnessFailure(f"{name}: signed mGBA disappeared after opening")
            except Exception as error:
                try:
                    image = capture_desktop(desktop_pid, run_dir / "screenshots",
                                            f"startup-failed-{name}", adapter)
                    error.add_note(f"status image {image}")
                except Exception as diagnostic:
                    error.add_note(f"Startup status capture also failed: {diagnostic}")
                raise harness.HarnessFailure(
                    f"{name}: signed desktop did not start mGBA within {timeout} seconds: {error}") from error
        arguments = harness.psutil.Process(game_pid).cmdline()
        roms = [Path(arg) for arg in arguments if arg.casefold().endswith(".gba")]
        if len(roms) != 1:
            raise harness.HarnessFailure(f"{name}: signed mGBA must name exactly one ROM")
        wait_game_window(game_pid, adapter, timeout=30, rom_title=harness._rom_header_title(roms[0]))
        if not harness.psutil.pid_exists(game_pid):
            raise harness.HarnessFailure(f"{name}: mGBA exited after opening its window")
        game_pids[name] = game_pid
    harness.checkpoint(run_dir, "signed-games-started", {"game_pids": game_pids, "play_presses": presses})
    return game_pids


def _runtime_children(desktop_pid: int) -> list[int]:
    alive = []
    try:
        children = harness.psutil.Process(desktop_pid).children(recursive=True)
    except harness.psutil.NoSuchProcess:
        return alive
    for child in children:
        try:
            if child.name().casefold() in ("mgba.exe", "coop-sidecar.exe"):
                alive.append(child.pid)
        except harness.psutil.NoSuchProcess:
            pass
    return alive


def stop_runtime(plan: dict, desktops: dict[str, int]) -> None:
    """Request signed desktop Stop until it registers, then drain games and leases."""
    adapter = harness.Win32Adapter()
    for player in plan["players"]:
        profile, desktop = Path(player["profile_localappdata"]), desktops[player["name"]]
        pid = harness._mgba_pid_for(profile)
        if pid is None:
            continue
        try:
            children = harness.psutil.Process(desktop).children(recursive=True)
        except harness.psutil.NoSuchProcess:
            raise harness.HarnessFailure("signed desktop disappeared while game remained")
        if pid not in {child.pid for child in children}:
            raise harness.HarnessFailure("refusing to close a game outside the launched signed client")
        # Closing mGBA first is a child failure, not an orderly session stop:
        # the desktop then revokes its credential. Use the controller's Stop.
        wait_desktop_window(desktop, adapter)
        press_until(desktop, STOP, adapter, ready=lambda a: a["stop"],
                    registered=lambda a: not a["stop"],
                    done=lambda: harness._mgba_pid_for(profile) is None,
                    deadline=time.monotonic() + 45, label=f"{player['name']}: Stop")
    deadline = time.monotonic() + 45
    while True:
        alive = [pid for desktop in desktops.values() for pid in _runtime_children(desktop)]
        if not alive:
            return
        if time.monotonic() >= deadline:
            raise harness.HarnessFailure("signed runtime did not finish graceful shutdown within 45 seconds")
        harness.check_c_space()
        time.sleep(.25)
