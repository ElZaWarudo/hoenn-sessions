"""One-time test-only signed desktop authentication/install via its real UI.

Existing account profiles and complete fixture caches are retained. No account,
accepted-generation marker or save is fabricated. Form credentials come only
from environment and are never printed or checkpointed.
"""

import ctypes
import math
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


# Logical (96-DPI) client rectangles of the signed sign-in form's 360x44
# TextEdits (coop-desktop renderer.rs draw_auth), measured at 100% and 125%.
USERNAME_FIELD = (8, 124, 368, 168)
PASSWORD_FIELD = (8, 171, 368, 215)
# A hover frame before each press; egui drops a jump+press+release that lands
# in one idle frame (observed live: the username click after a blur).
CLICK_SETTLE = .15
FOCUS_ATTEMPTS = 3


def _signed_handle(pid, adapter):
    matches = [w for w in adapter.windows() if w.pid == pid and w.visible
               and w.title == "Hoenn Sessions"]
    if len(matches) != 1:
        raise HarnessFailure("prepare: signed desktop window is ambiguous")
    return matches[0].handle


def form_geometry(handle, adapter):
    """Return (scale, client-x, client-y) in the window-rect pixel space.

    Clicks (click_window_pixel) and captures (capture_rgb) both use physical
    window-rect pixels because Win32Adapter is DPI aware; the client layout is
    scaled by the target window's own DPI, not the system DPI.
    """
    adapter.user32.GetDpiForWindow.argtypes = [wintypes.HWND]
    adapter.user32.GetDpiForWindow.restype = wintypes.UINT
    adapter.user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
    adapter.user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    dpi = adapter.user32.GetDpiForWindow(handle)
    if not 96 <= dpi <= 480:
        raise HarnessFailure("prepare: signed form DPI unavailable")
    point = wintypes.POINT(0, 0)
    rect = wintypes.RECT()
    if not adapter.user32.ClientToScreen(handle, ctypes.byref(point)) or not adapter.user32.GetWindowRect(handle, ctypes.byref(rect)):
        raise HarnessFailure("prepare: signed form coordinates unavailable")
    return dpi / 96, point.x - rect.left, point.y - rect.top


def to_window(geometry, x, y):
    scale, left, top = geometry
    return left + round(x * scale), top + round(y * scale)


def field_box(geometry, field):
    x0, y0 = to_window(geometry, field[0], field[1])
    x1, y1 = to_window(geometry, field[2], field[3])
    return x0, y0, x1, y1


def click_form(pid, x, y, adapter, *, settle=CLICK_SETTLE):
    handle = _signed_handle(pid, adapter)
    px, py = to_window(form_geometry(handle, adapter), x, y)
    click_desktop(pid, px, py, adapter, settle=settle)
    return handle


def _capture(handle, adapter):
    if adapter.foreground() != handle:
        raise HarnessFailure("prepare: signed form lost focus; input stopped")
    width, height, rgb = adapter.capture_rgb(handle)
    if len(rgb) != width * height * 3:
        raise HarnessFailure("prepare: signed form capture is invalid")
    return width, height, rgb


def field_state(capture, box, scale):
    """Non-secret summary of one TextEdit: presence, focus, ink and glyph runs."""
    width, height, rgb = capture
    x0, y0, x1, y1 = box
    edge = max(2, round(2 * scale))
    if x0 - edge < 0 or y0 - edge < 0 or x1 + 8 * scale >= width or y1 + edge >= height:
        return {"present": False, "focused": False, "ink": 0, "runs": 0, "empty": True}

    def px(x, y):
        i = (y * width + x) * 3
        return rgb[i], rgb[i + 1], rgb[i + 2]

    bw, bh = x1 - x0, y1 - y0
    # The right/bottom of an egui TextEdit stays white (text is top-left) and
    # the panel just right of it is the light background, not white.
    blank = all(min(px(x, y)) >= 250
                for y in range(y0 + int(bh * .7), y1 - int(bh * .15), 2)
                for x in range(x0 + int(bw * .75), x1 - int(bw * .05), 3))
    outside = px(min(width - 1, x1 + round(6 * scale)), (y0 + y1) // 2)
    present = blank and 225 <= min(outside) <= 252
    # Focused egui TextEdits draw a blue selection stroke on their frame.
    ring = [(x, y) for x in range(x0 - edge, x1 + edge)
            for y in list(range(y0 - edge, y0 + 1)) + list(range(y1 - 1, y1 + edge))]
    blue = sum(1 for x, y in ring if (lambda p: p[2] - p[0] >= 20 and min(p) < 245)(px(x, y)))
    focused = blue >= bw // 2
    inset = edge + 1
    ink, runs = 0, 0
    for y in range(y0 + inset, y1 - inset):
        row_runs, dark_before = 0, False
        for x in range(x0 + inset, x1 - inset):
            dark = min(px(x, y)) < 128  # typed text/dots; the hint is >= 160
            ink += dark
            row_runs += dark and not dark_before
            dark_before = dark
        runs = max(runs, row_runs)
    # A blinking caret alone (about 2px by one text line) is not content.
    return {"present": present, "focused": focused, "ink": ink, "runs": runs,
            "empty": ink <= 3 * bh and runs <= 1}


def form_states(handle, adapter):
    geometry = form_geometry(handle, adapter)
    capture = _capture(handle, adapter)
    scale = geometry[0]
    return (field_state(capture, field_box(geometry, USERNAME_FIELD), scale),
            field_state(capture, field_box(geometry, PASSWORD_FIELD), scale))


def focus_field(pid, field, adapter):
    """Click one form field until only it shows focus; type nothing otherwise."""
    index = 0 if field is USERNAME_FIELD else 1
    label = "username" if index == 0 else "password"
    center = ((field[0] + field[2]) // 2 - 20, (field[1] + field[3]) // 2)
    for _attempt in range(FOCUS_ATTEMPTS):
        handle = click_form(pid, center[0], center[1], adapter)
        time.sleep(.25)
        states = form_states(handle, adapter)
        if not all(state["present"] for state in states):
            raise HarnessFailure("prepare: signed sign-in form layout not recognized at this display scale")
        if states[index]["focused"] and not states[1 - index]["focused"]:
            return handle, states
    raise HarnessFailure("prepare: " + label + " field did not take focus; no text typed")


def enter_credentials(pid, username, password, adapter):
    """Type both credentials with non-secret visual checks; raise before submit.

    Each field must be the only focused one before typing; the username must be
    visibly entered before any password key is pressed; the password field must
    show one glyph per character (plus an optional caret) afterwards.
    """
    # Reject an unsupported character in either value before any key press.
    keyboard_codes(username, adapter)
    keyboard_codes(password, adapter)
    handle = _signed_handle(pid, adapter)
    states = form_states(handle, adapter)
    if not all(state["present"] for state in states):
        raise HarnessFailure("prepare: signed sign-in form layout not recognized at this display scale")
    if not all(state["empty"] for state in states):
        raise HarnessFailure("prepare: signed sign-in form is not empty; refusing to type")
    handle, _ = focus_field(pid, USERNAME_FIELD, adapter)
    type_text(username, adapter, handle)
    time.sleep(.3)
    user, secret = form_states(handle, adapter)
    if user["empty"] or not secret["empty"]:
        raise HarnessFailure("prepare: username was not entered; refusing to type password or submit")
    handle, (user_before, _) = focus_field(pid, PASSWORD_FIELD, adapter)
    try:
        type_text(password, adapter, handle)
        time.sleep(.3)
        user, secret = form_states(handle, adapter)
        if user["empty"] or abs(user["ink"] - user_before["ink"]) > user_before["ink"] // 10:
            raise HarnessFailure("prepare: username changed while typing password; refusing to submit")
        if secret["runs"] not in (len(password), len(password) + 1):
            raise HarnessFailure("prepare: password field did not show the expected input; refusing to submit")
    except HarnessFailure as exc:
        exc.password_typed = True  # callers must not keep a form capture
        raise
    return handle


def keyboard_codes(value, adapter):
    adapter.user32.VkKeyScanW.argtypes = [ctypes.c_wchar]
    adapter.user32.VkKeyScanW.restype = ctypes.c_short
    codes = [adapter.user32.VkKeyScanW(char) for char in value]
    if any(code == -1 for code in codes):
        raise HarnessFailure("prepare: form text is unsupported by the keyboard layout")
    return codes


def type_text(value, adapter, handle):
    # Check the entire string before pressing any key; never type a prefix of
    # an unsupported password and then accidentally submit it.
    codes = keyboard_codes(value, adapter)
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


# "Join a partner by code" row (coop-desktop renderer.rs draw_join): a 96-px
# TextEdit and a Join button enabled while the runtime plays. Its height moves
# with the partner panel, so the box is located visually and fails closed. All
# values are logical (96-DPI) client pixels.
JOIN_SCAN_ROWS = (140, 540)
# First (pre-typing) scan only. The live 125% capture put the grey "ABC-234"
# hint's ink at logical ~61 and a typed 7-character code plus caret at ~65, so
# the band starts well right of both. Every later scan uses join_band(box).
JOIN_PROBE_COLUMNS = (72, 92)
# egui TextEdit text is left-aligned: the right quarter of the measured box
# holds no hint, glyph or caret (field_state's blank test relies on the same).
JOIN_TEXT_FREE = .75
JOIN_HEIGHT = (14, 36)
JOIN_LEFT = (2, 20)
JOIN_WIDTH = (80, 130)
JOIN_BUTTON_WIDTH = (18, 80)
JOIN_CLICKS = 3
JOIN_WAIT = 4.0
PAIRING_ALPHABET = frozenset("ABCDEFGHJKLMNPQRSTUVWXYZ23456789")


def valid_pairing_code(code):
    return (isinstance(code, str) and len(code) == 7 and code[3] == "-"
            and all(c in PAIRING_ALPHABET for c in code[:3] + code[4:]))


def _pixel(capture, x, y):
    width, _height, rgb = capture
    i = (y * width + x) * 3
    return rgb[i], rgb[i + 1], rgb[i + 2]


def join_band(box, scale):
    """Probe columns (first, last) derived from a measured join-box interior.

    The glyph-free right quarter of the box, ending one pixel inside the
    deepest interior a focus stroke may leave (``join_inset_limit``), so the
    same box is found before typing, while focused and with a code typed.
    """
    x0, _y0, x1, _y1 = box
    return x0 + math.ceil(JOIN_TEXT_FREE * (x1 - x0)), x1 - join_inset_limit(scale) - 2


def find_join_box(capture, geometry, band=None):
    """Return the white interior (x0, y0, x1, y1) of the unique join TextEdit.

    ``band`` (first, last physical column) replaces the fixed logical probe
    columns once a box has been measured; see ``join_band``.
    """
    width, height, _rgb = capture
    scale, left, top = geometry
    white = lambda x, y: min(_pixel(capture, x, y)) >= 250
    if band is None:
        band = (left + round(JOIN_PROBE_COLUMNS[0] * scale), left + round(JOIN_PROBE_COLUMNS[1] * scale))
    columns = range(band[0], band[1] + 1)
    if not columns:
        raise HarnessFailure("join: pairing-code box geometry not recognized; no input sent")
    first, last = top + round(JOIN_SCAN_ROWS[0] * scale), top + round(JOIN_SCAN_ROWS[1] * scale)
    if columns.stop >= width or first >= height:
        raise HarnessFailure("join: signed desktop is too small for the partner panel")
    last = min(last, height - 1)
    runs, start = [], None
    for y in range(first, last):
        if all(white(x, y) for x in columns):
            start = y if start is None else start
        elif start is not None:
            runs.append((start, y))
            start = None
    if start is not None:
        raise HarnessFailure("join: pairing-code box is clipped by the window")
    runs = [run for run in runs if run[1] - run[0] >= 3]
    if (len(runs) != 1
            or not JOIN_HEIGHT[0] * scale <= runs[0][1] - runs[0][0] <= JOIN_HEIGHT[1] * scale):
        raise HarnessFailure("join: pairing-code box not uniquely recognized; no input sent")
    y0, y1 = runs[0]
    # Measure the width on the last white row: the TextEdit's inner bottom
    # margin, below typed glyphs and below a focused box's text caret (which
    # spans the text row and would otherwise cut the scan short).
    row = y1 - 1
    x0 = columns.start
    while x0 - 1 > left and white(x0 - 1, row):
        x0 -= 1
    x1 = columns.stop
    while x1 < width - 1 and white(x1, row):
        x1 += 1
    if (not JOIN_LEFT[0] * scale <= x0 - left <= JOIN_LEFT[1] * scale
            or not JOIN_WIDTH[0] * scale <= x1 - x0 <= JOIN_WIDTH[1] * scale):
        raise HarnessFailure("join: pairing-code box geometry not recognized; no input sent")
    return x0, y0, x1, y1


def find_join_button(capture, box, scale):
    """Centre of the first widget right of the box on its middle row."""
    width = capture[0]
    x0, y0, x1, y1 = box
    y = (y0 + y1) // 2
    gap_x = x1 + max(2, round(3 * scale))
    background = _pixel(capture, min(width - 1, gap_x), y)
    differs = lambda x: max(abs(a - b) for a, b in zip(_pixel(capture, x, y), background)) > 8
    limit = min(width - 1, x1 + round(90 * scale))
    start = next((x for x in range(gap_x, limit) if differs(x)), None)
    if start is None:
        raise HarnessFailure("join: Join button not recognized; code not submitted")
    end, gap = start, 0
    for x in range(start, limit):
        if differs(x):
            end, gap = x, 0
        else:
            gap += 1
            if gap > 3 * scale:
                break
    if not JOIN_BUTTON_WIDTH[0] * scale <= end - start + 1 <= JOIN_BUTTON_WIDTH[1] * scale:
        raise HarnessFailure("join: Join button geometry not recognized; code not submitted")
    return (start + end) // 2, y


def _focus_signed(pid, adapter, timeout=3.0):
    handle = _signed_handle(pid, adapter)
    adapter.restore(handle)
    deadline = time.monotonic() + timeout
    while adapter.foreground() != handle:
        adapter.focus(handle)
        if time.monotonic() >= deadline:
            # Windows may deny programmatic activation; activate only the
            # verified owned caption (never client pixels), as focus_game does.
            activate = getattr(adapter, "activate_caption", None)
            if activate is not None and activate(handle) and adapter.foreground() == handle:
                return handle
            raise HarnessFailure("join: signed desktop did not become foreground; no input sent")
        time.sleep(.05)
    return handle


def _ring_kind(pixel):
    """Classify one frame pixel: egui's selection stroke or its hover stroke."""
    r, _g, b = pixel
    if b - r >= 50 and max(pixel) <= 210:      # selection stroke rgb(0, 83, 125) and its AA edge
        return "focus"
    if max(pixel) - min(pixel) <= 8 and max(pixel) <= 180:  # hovered bg_stroke gray(105)
        return "hover"
    return None


def join_frame(capture, interior, scale):
    """Return "focus"/"hover" if that stroke closes all four sides of a white interior.

    egui 0.31 paints a focused (or hovered) TextEdit with a 1-point stroke
    inside a frame expanded by 1 point; the unfocused, unhovered box has no
    stroke at all. Each side passes only if nearly every position along it
    meets the stroke colour within a few pixels outward.
    """
    width, height, _rgb = capture
    x0, y0, x1, y1 = interior
    reach = join_inset_limit(scale) + max(2, math.ceil(scale))
    if x0 - reach < 0 or y0 - reach < 0 or x1 + reach > width or y1 + reach > height:
        return None
    sides = (
        [[(x0 - d, y) for d in range(1, reach + 1)] for y in range(y0 + 1, y1 - 1)],
        [[(x1 - 1 + d, y) for d in range(1, reach + 1)] for y in range(y0 + 1, y1 - 1)],
        [[(x, y0 - d) for d in range(1, reach + 1)] for x in range(x0 + 1, x1 - 1)],
        [[(x, y1 - 1 + d) for d in range(1, reach + 1)] for x in range(x0 + 1, x1 - 1)],
    )
    for kind in ("focus", "hover"):
        if all(sum(any(_ring_kind(_pixel(capture, x, y)) == kind for x, y in scan) for scan in side)
               >= .9 * len(side) for side in sides):
            return kind
    return None


def join_inset_limit(scale):
    """Largest per-side shrink of the white interior the egui frame can cause."""
    return max(1, math.ceil(scale))


def focus_inset(capture, box, found, scale):
    """Classify a changed join-box interior as the same box under egui's stroke.

    ``box`` is the interior recognized before the focus click. A focused (or
    hovered) box keeps its place but its stroke and anti-aliasing cover up to
    ``join_inset_limit`` pixels of the white interior on each side. The change
    is accepted only when every side moved inwards within that limit, the
    insets are balanced (no shift), and the matching stroke colour actually
    closes the frame around the new interior. Anything else is layout movement.
    """
    insets = (found[0] - box[0], found[1] - box[1], box[2] - found[2], box[3] - found[3])
    limit = join_inset_limit(scale)
    if not all(0 <= inset <= limit for inset in insets):
        return None
    if abs(insets[0] - insets[2]) > 1 or abs(insets[1] - insets[3]) > 1:
        return None
    return join_frame(capture, found, scale)


def _join_state(handle, adapter, geometry, box, *, require_focus=False):
    """Re-identify the box measured before typing and summarize its content.

    The box is re-detected only through the glyph-free columns of ``box``
    itself (never through a typed code or caret), and must be that same
    rectangle or the same rectangle under egui's focus stroke (focus_inset,
    which checks the stroke colour on all four sides). Duplicates and clipping
    still fail in find_join_box. ``require_focus`` (after typing) also demands
    the selection-blue focus border, so the code went into this box.
    """
    capture = _capture(handle, adapter)
    found = find_join_box(capture, geometry, join_band(box, geometry[0]))
    state = field_state(capture, box, geometry[0])
    if found != box:
        frame = focus_inset(capture, box, found, geometry[0])
        if frame is None:
            raise HarnessFailure("join: signed desktop layout moved; input stopped")
        # The shrink is accepted only together with its stroke; focus then
        # needs both the selection-blue frame and field_state's blue ring.
        state = dict(state, focused=state["focused"] and frame == "focus")
    if require_focus and not state["focused"]:
        raise HarnessFailure("join: pairing-code box lost focus while typing; Join not pressed")
    return capture, state


def _await_join(confirm, seconds):
    deadline = time.monotonic() + seconds
    while True:
        if confirm():
            return True
        if time.monotonic() >= deadline:
            return False
        time.sleep(.5)


def enter_join_code(pid, code, adapter, confirm, *, settle=CLICK_SETTLE):
    """Type a pairing code into b's live desktop Join box and press Join.

    Fails closed: the box must be uniquely recognized, empty and focused before
    typing, and must visibly hold the code before Join. Success is only
    ``confirm()`` (a token-only server read) turning true. Join is re-pressed at
    most ``JOIN_CLICKS`` times and only while the code is still in the box (an
    activation click or a runtime that has not finished starting ignores it).
    """
    if not valid_pairing_code(code):
        raise HarnessFailure("join: refusing a malformed pairing code")
    keyboard_codes(code, adapter)
    handle = _focus_signed(pid, adapter)
    geometry = form_geometry(handle, adapter)
    capture = _capture(handle, adapter)
    box = find_join_box(capture, geometry)
    # Confirm through the box's own glyph-free columns, the band every later
    # check uses, so a hint reaching the fixed band cannot pick another box.
    if find_join_box(capture, geometry, join_band(box, geometry[0])) != box:
        raise HarnessFailure("join: pairing-code box not uniquely recognized; no input sent")
    state = field_state(capture, box, geometry[0])
    if not state["present"] or not state["empty"]:
        raise HarnessFailure("join: pairing-code box is not an empty TextEdit; refusing to type")
    centre = ((box[0] + box[2]) // 2, (box[1] + box[3]) // 2)
    for _attempt in range(FOCUS_ATTEMPTS):
        click_desktop(pid, centre[0], centre[1], adapter, settle=settle)
        time.sleep(.25)
        capture, state = _join_state(handle, adapter, geometry, box)
        if state["focused"]:
            break
    else:
        raise HarnessFailure("join: pairing-code box did not take focus; no text typed")
    type_text(code, adapter, handle)
    time.sleep(.3)
    capture, state = _join_state(handle, adapter, geometry, box, require_focus=True)
    if state["empty"] or state["runs"] < 5:
        raise HarnessFailure("join: pairing code was not visibly entered; Join not pressed")
    button = find_join_button(capture, box, geometry[0])
    receipt = {"box": list(box), "button": list(button), "scale": geometry[0]}
    for clicks in range(1, JOIN_CLICKS + 1):
        click_desktop(pid, button[0], button[1], adapter, settle=settle)
        if _await_join(confirm, JOIN_WAIT):
            return dict(receipt, join_clicks=clicks)
        _focus_signed(pid, adapter)
        capture, state = _join_state(handle, adapter, geometry, box)
        if state["empty"]:
            # The desktop clears the box only after its redeem succeeded.
            if _await_join(confirm, 2 * JOIN_WAIT):
                return dict(receipt, join_clicks=clicks)
            raise HarnessFailure("join: desktop cleared the code but the server reports no Active group")
    raise HarnessFailure("join: server did not report the paired group after Join")


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
                try:
                    enter_credentials(process.pid, username, password, adapter)
                except HarnessFailure as exc:
                    # Only reached before submit. A capture before any password
                    # key holds no secret; never keep one after password keys.
                    if not getattr(exc, "password_typed", False):
                        try:
                            capture_desktop(process.pid, run_dir / "screenshots",
                                            "prepare-form-rejected-" + name, adapter)
                        except Exception as capture_error:
                            exc.add_note("Form capture failed: " + type(capture_error).__name__)
                    raise
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
