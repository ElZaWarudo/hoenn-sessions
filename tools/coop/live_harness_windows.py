"""Bounded Windows desktop controls for test-only live ROM journeys.

The caller owns fixture verification, process lifetime, and the output volume.
This module never installs an update or changes signed runtime artifacts.
"""

from __future__ import annotations

import ctypes
import os
import struct
import subprocess
import sys
import time
import zlib
from ctypes import wintypes
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol


class WindowControlError(RuntimeError):
    """A live window or input boundary could not be established."""


@dataclass(frozen=True)
class Window:
    handle: int
    pid: int
    title: str
    visible: bool


class WindowAdapter(Protocol):
    def windows(self) -> list[Window]: ...
    def minimize(self, handle: int) -> None: ...
    def restore(self, handle: int) -> None: ...
    def foreground(self) -> int: ...
    def focus(self, handle: int) -> bool: ...
    def key(self, vk: int, down: bool) -> None: ...
    def capture_rgb(self, handle: int) -> tuple[int, int, bytes]: ...


KEYS = {
    "up": 0x26, "down": 0x28, "left": 0x25, "right": 0x27,
    "enter": 0x0D, "backspace": 0x08, "escape": 0x1B,
    "x": 0x58, "z": 0x5A, "a": 0x41, "s": 0x53,
    # The signed mGBA 0.11 fixture maps GBA A to X and GBA B to Z.
    "gba_a": 0x58, "gba_b": 0x5A, "gba_start": 0x0D,
}


def game_window(pid: int, adapter: WindowAdapter) -> Window:
    """Find the titled game window, ignoring Qt's dummy and Scripts windows."""
    if pid <= 0:
        raise ValueError("pid must be positive")
    windows = adapter.windows()
    candidates = [w for w in windows
                  if w.pid == pid and w.visible and w.title.strip()
                  and "mgba" in w.title.casefold()
                  and "scripts" not in w.title.casefold()]
    if len(candidates) != 1:
        titles = [w.title for w in windows if w.pid == pid]
        raise WindowControlError(
            f"expected one titled mGBA game window for PID {pid}; "
            f"found {len(candidates)} (same-process titles: {titles!r})")
    return candidates[0]


def wait_game_window(pid: int, adapter: WindowAdapter, *, timeout: float = 30.0,
                     rom_title: str | None = None) -> Window:
    """Wait for a newly launched mGBA window; ambiguous windows fail at once."""
    if not 0 < timeout <= 120:
        raise ValueError("window timeout must be within (0, 120] seconds")
    deadline = time.monotonic() + timeout
    while True:
        try:
            window = game_window(pid, adapter)
            if rom_title is None or rom_title.casefold() in window.title.casefold():
                return window
            if time.monotonic() >= deadline:
                raise WindowControlError("mGBA did not load the expected ROM title before deadline")
            time.sleep(.1)
        except WindowControlError as exc:
            if "found 0" not in str(exc) or time.monotonic() >= deadline:
                raise
            time.sleep(0.1)


def focus_game(pid: int, adapter: WindowAdapter, *, timeout: float = 3.0) -> Window:
    if not 0 < timeout <= 30:
        raise ValueError("focus timeout must be within (0, 30] seconds")
    window = game_window(pid, adapter)
    for other in adapter.windows():
        if other.pid == pid and other.handle != window.handle and "scripts" in other.title.casefold():
            adapter.minimize(other.handle)
    adapter.restore(window.handle)
    deadline = time.monotonic() + timeout
    while True:
        adapter.focus(window.handle)
        if adapter.foreground() == window.handle:
            return window
        if time.monotonic() >= deadline:
            # Windows may deny programmatic activation after another app takes
            # the foreground. A native adapter can activate only a verified
            # owned caption; it cannot send gameplay input to that other app.
            activate = getattr(adapter, "activate_caption", None)
            if activate is not None and activate(window.handle) and adapter.foreground() == window.handle:
                return window
            reason = getattr(adapter, "last_caption_failure", None)
            detail = f" (caption activation: {reason})" if reason else ""
            raise WindowControlError(f"mGBA game window {window.handle} did not become foreground{detail}")
        time.sleep(0.05)


def tap(pid: int, key: str, adapter: WindowAdapter, *, hold: float = 0.08,
        release: float = 0.08) -> None:
    """Send one bounded key tap after establishing the exact game window."""
    if not 0.01 <= hold <= 2.0 or not 0 <= release <= 2.0:
        raise ValueError("key hold/release outside bounded range")
    try:
        vk = KEYS[key.casefold()]
    except KeyError as exc:
        raise ValueError(f"unsupported key: {key}") from exc
    focus_game(pid, adapter)
    pressed = False
    try:
        adapter.key(vk, True)
        pressed = True
        time.sleep(hold)
    finally:
        if pressed:
            adapter.key(vk, False)
    if release:
        time.sleep(release)


def _png(width: int, height: int, rgb: bytes) -> bytes:
    if not 0 < width <= 8192 or not 0 < height <= 8192 or len(rgb) != width * height * 3:
        raise WindowControlError("invalid game window capture")
    scanlines = b"".join(b"\x00" + rgb[y * width * 3:(y + 1) * width * 3]
                         for y in range(height))

    def chunk(name: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + name + data + struct.pack(
            ">I", zlib.crc32(name + data) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(scanlines)) + chunk(b"IEND", b""))


def capture_game(pid: int, output_dir: Path, name: str, adapter: WindowAdapter) -> Path:
    """Capture the foreground game window to an explicitly selected directory."""
    if not name or not all(c.isalnum() or c in "-_" for c in name):
        raise ValueError("screenshot name must be a simple filename stem")
    window = focus_game(pid, adapter)
    width, height, rgb = adapter.capture_rgb(window.handle)
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    path = output_dir / f"{name}.png"
    path.write_bytes(_png(width, height, rgb))
    return path


def capture_desktop(pid: int, output_dir: Path, name: str, adapter: WindowAdapter) -> Path:
    """Keep a focused signed-client status image at a failed startup boundary."""
    if not name or not all(c.isalnum() or c in "-_" for c in name):
        raise ValueError("screenshot name must be a simple filename stem")
    matches = [w for w in adapter.windows() if w.pid == pid and w.visible
               and w.title == "Hoenn Sessions"]
    if len(matches) != 1:
        raise WindowControlError(f"expected one signed desktop window for PID {pid}")
    target = matches[0]
    adapter.restore(target.handle)
    deadline = time.monotonic() + 3
    while adapter.foreground() != target.handle:
        adapter.focus(target.handle)
        if time.monotonic() >= deadline:
            raise WindowControlError(f"desktop window {target.handle} did not become foreground")
        time.sleep(0.05)
    width, height, rgb = adapter.capture_rgb(target.handle)
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    path = output_dir / f"{name}.png"
    path.write_bytes(_png(width, height, rgb))
    return path


def launch_signed_desktop(executable: Path, profile_localappdata: Path, *,
                          extra_env: dict[str, str] | None = None,
                          output_log: Path | None = None) -> subprocess.Popen[bytes]:
    """Launch an already verified signed fixture without copying or installing it."""
    executable = Path(executable).resolve(strict=True)
    if not executable.is_file() or executable.suffix.casefold() != ".exe":
        raise ValueError("signed desktop path must be an existing .exe")
    profile_localappdata = Path(profile_localappdata).resolve()
    profile_localappdata.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["LOCALAPPDATA"] = str(profile_localappdata)
    if extra_env:
        if "LOCALAPPDATA" in {key.upper() for key in extra_env}:
            raise ValueError("extra_env cannot override isolated LOCALAPPDATA")
        env.update(extra_env)
    log = None
    try:
        if output_log is not None:
            output_log.parent.mkdir(parents=True, exist_ok=True)
            log = output_log.open("xb")
        process = subprocess.Popen([str(executable)], cwd=executable.parent, env=env,
                                   stdout=log, stderr=subprocess.STDOUT if log else None)
    except OSError as exc:
        raise WindowControlError(f"signed desktop could not start: {type(exc).__name__}") from exc
    finally:
        if log is not None:
            log.close()
    # A signed client that exits synchronously is a deterministic startup
    # failure.  Do not return a dead PID that would make the next command
    # click an unrelated window or create a second client.
    status = process.poll()
    if isinstance(status, int):
        raise WindowControlError(f"signed desktop exited during startup (code {status})")
    return process


class Win32Adapter:
    """Small ctypes adapter; instantiate only on Windows."""

    def __init__(self) -> None:
        if sys.platform != "win32":
            raise OSError("Win32Adapter requires Windows")
        self.user32 = ctypes.WinDLL("user32", use_last_error=True)
        self.gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
        self.kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel32.GetCurrentThreadId.restype = wintypes.DWORD
        self.user32.AttachThreadInput.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.BOOL]
        self.user32.AttachThreadInput.restype = wintypes.BOOL
        self.user32.BringWindowToTop.argtypes = [wintypes.HWND]
        self.user32.BringWindowToTop.restype = wintypes.BOOL
        # Window rectangles and GDI screen pixels must use the same physical
        # coordinate space on scaled Windows desktops.
        self.user32.SetProcessDPIAware()
        self.user32.EnumWindows.argtypes = [ctypes.c_void_p, wintypes.LPARAM]
        self.user32.EnumWindows.restype = wintypes.BOOL
        self.user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
        self.user32.GetWindowTextLengthW.restype = ctypes.c_int
        self.user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
        self.user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
        self.user32.IsWindowVisible.argtypes = [wintypes.HWND]
        self.user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
        self.user32.GetForegroundWindow.restype = wintypes.HWND
        self.user32.SetForegroundWindow.argtypes = [wintypes.HWND]
        self.user32.SetForegroundWindow.restype = wintypes.BOOL
        self.user32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
        self.user32.SetCursorPos.restype = wintypes.BOOL
        self.user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
        self.user32.PostMessageW.restype = wintypes.BOOL
        self.user32.keybd_event.argtypes = [ctypes.c_ubyte, ctypes.c_ubyte, wintypes.DWORD, ctypes.c_void_p]
        self.user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
        self.user32.GetDC.argtypes = [wintypes.HWND]
        self.user32.GetDC.restype = wintypes.HDC
        self.user32.ReleaseDC.argtypes = [wintypes.HWND, wintypes.HDC]
        self.gdi32.CreateCompatibleDC.argtypes = [wintypes.HDC]
        self.gdi32.CreateCompatibleDC.restype = wintypes.HDC
        self.gdi32.CreateCompatibleBitmap.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int]
        self.gdi32.CreateCompatibleBitmap.restype = wintypes.HBITMAP
        self.gdi32.SelectObject.argtypes = [wintypes.HDC, wintypes.HGDIOBJ]
        self.gdi32.SelectObject.restype = wintypes.HGDIOBJ
        self.gdi32.BitBlt.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                     ctypes.c_int, wintypes.HDC, ctypes.c_int, ctypes.c_int,
                                     wintypes.DWORD]
        self.gdi32.GetDIBits.argtypes = [wintypes.HDC, wintypes.HBITMAP, wintypes.UINT,
                                        wintypes.UINT, ctypes.c_void_p, ctypes.c_void_p,
                                        wintypes.UINT]
        self.gdi32.DeleteObject.argtypes = [wintypes.HGDIOBJ]
        self.gdi32.DeleteDC.argtypes = [wintypes.HDC]

    def windows(self) -> list[Window]:
        found: list[Window] = []
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

        def visit(hwnd: int, _param: int) -> bool:
            length = self.user32.GetWindowTextLengthW(hwnd)
            title = ctypes.create_unicode_buffer(length + 1)
            self.user32.GetWindowTextW(hwnd, title, len(title))
            pid = wintypes.DWORD()
            self.user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            found.append(Window(int(hwnd), int(pid.value), title.value,
                                bool(self.user32.IsWindowVisible(hwnd))))
            return True

        callback = callback_type(visit)
        if not self.user32.EnumWindows(callback, 0):
            raise WindowControlError("EnumWindows failed")
        return found

    def close(self, handle: int) -> None:
        if not self.user32.PostMessageW(handle, 0x0010, 0, 0):
            raise WindowControlError("could not request graceful window close")

    def minimize(self, handle: int) -> None:
        self.user32.ShowWindow(handle, 6)  # SW_MINIMIZE

    def restore(self, handle: int) -> None:
        self.user32.ShowWindow(handle, 9)  # SW_RESTORE

    def foreground(self) -> int:
        return int(self.user32.GetForegroundWindow() or 0)

    def focus(self, handle: int) -> bool:
        if self.user32.SetForegroundWindow(handle):
            return True
        # Attach only to the selected owned window's thread, never to the
        # unrelated foreground application. Always detach; callers still
        # verify the foreground handle before sending any gameplay input.
        current = self.kernel32.GetCurrentThreadId()
        target = self.user32.GetWindowThreadProcessId(handle, None)
        if not target or current == target or not self.user32.AttachThreadInput(current, target, True):
            return False
        try:
            self.user32.BringWindowToTop(handle)
            return bool(self.user32.SetForegroundWindow(handle))
        finally:
            self.user32.AttachThreadInput(current, target, False)

    def key(self, vk: int, down: bool) -> None:
        self.user32.keybd_event(vk, 0, 0 if down else 2, 0)

    def activate_caption(self, handle: int) -> bool:
        """Activate a verified owned title bar; never click client/game pixels."""
        self.last_caption_failure = None
        rect = wintypes.RECT()
        if not self.user32.GetWindowRect(handle, ctypes.byref(rect)):
            self.last_caption_failure = "window_rect"
            return False
        width = rect.right - rect.left
        if width < 240:
            self.last_caption_failure = "caption_width"
            return False
        # Raise only this target without activating it, then verify the exact
        # caption under the pointer. Occlusion, buttons and menus fail closed.
        self.user32.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND,
                                            ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                            ctypes.c_int, wintypes.UINT]
        if not self.user32.SetWindowPos(handle, 0, 0, 0, 0, 0, 0x4013):
            self.last_caption_failure = "raise_target"
            return False  # NOSIZE | NOMOVE | NOACTIVATE | ASYNCWINDOWPOS
        point = wintypes.POINT(rect.left + min(100, width // 3),
                               rect.top + self.user32.GetSystemMetrics(4) // 2 + self.user32.GetSystemMetrics(33))
        self.user32.WindowFromPoint.argtypes = [wintypes.POINT]
        self.user32.WindowFromPoint.restype = wintypes.HWND
        self.user32.GetAncestor.argtypes = [wintypes.HWND, wintypes.UINT]
        self.user32.GetAncestor.restype = wintypes.HWND
        self.user32.SendMessageTimeoutW.argtypes = [wintypes.HWND, wintypes.UINT,
                                                  wintypes.WPARAM, wintypes.LPARAM,
                                                  wintypes.UINT, wintypes.UINT,
                                                  ctypes.POINTER(ctypes.c_size_t)]
        self.user32.SendMessageTimeoutW.restype = wintypes.LPARAM
        def caption_is_target() -> bool:
            at = self.user32.WindowFromPoint(point)
            if not at or self.user32.GetAncestor(at, 2) != handle:
                self.last_caption_failure = "caption_occluded"
                return False
            packed = (point.x & 0xFFFF) | ((point.y & 0xFFFF) << 16)
            hit = ctypes.c_size_t()
            # SMTO_BLOCK | SMTO_ABORTIFHUNG: an unresponsive target cannot
            # prevent the caller from reaching owned-process cleanup.
            if not self.user32.SendMessageTimeoutW(handle, 0x84, 0, packed, 3, 250, ctypes.byref(hit)):
                self.last_caption_failure = "hit_test_timeout"
                return False
            self.last_caption_failure = None if hit.value == 2 else "not_caption"
            return hit.value == 2  # WM_NCHITTEST / HTCAPTION
        # The asynchronous raise can return before the target reaches the top.
        # Wait only for this owned caption, without clicking an occluding app.
        deadline = time.monotonic() + .25
        while not caption_is_target():
            if time.monotonic() >= deadline:
                return False
            time.sleep(.01)
        ctypes.set_last_error(0)
        if not self.user32.SetCursorPos(point.x, point.y):
            self.last_caption_failure = "cursor_move_error_" + str(ctypes.get_last_error())
            return False
        if not caption_is_target():
            return False
        self.user32.mouse_event.argtypes = [wintypes.DWORD, wintypes.DWORD,
                                           wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p]
        try:
            self.user32.mouse_event(0x0002, 0, 0, 0, None)
            time.sleep(.08)
        finally:
            self.user32.mouse_event(0x0004, 0, 0, 0, None)
        deadline = time.monotonic() + .25
        while self.foreground() != handle:
            if time.monotonic() >= deadline:
                self.last_caption_failure = "foreground_timeout"
                return False
            time.sleep(.01)
        return True

    def click_window_pixel(self, handle: int, x: int, y: int) -> None:
        """Click a known pixel in one focused desktop window for fixture UI."""
        rect = wintypes.RECT()
        if not self.user32.GetWindowRect(handle, ctypes.byref(rect)):
            raise WindowControlError("GetWindowRect failed")
        width, height = rect.right - rect.left, rect.bottom - rect.top
        if not (0 <= x < width and 0 <= y < height):
            raise WindowControlError("desktop click is outside target window")
        self.user32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
        self.user32.mouse_event.argtypes = [wintypes.DWORD, wintypes.DWORD,
                                           wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p]
        if not self.user32.SetCursorPos(rect.left + x, rect.top + y):
            raise WindowControlError("SetCursorPos failed")
        self.user32.mouse_event(0x0002, 0, 0, 0, None)  # left down
        time.sleep(0.08)
        self.user32.mouse_event(0x0004, 0, 0, 0, None)  # left up


    def capture_rgb(self, handle: int) -> tuple[int, int, bytes]:
        rect = wintypes.RECT()
        if not self.user32.GetWindowRect(handle, ctypes.byref(rect)):
            raise WindowControlError("GetWindowRect failed")
        width, height = rect.right - rect.left, rect.bottom - rect.top
        if width <= 0 or height <= 0:
            raise WindowControlError("game window has empty bounds")
        # Qt/OpenGL may leave parts of a BitBlt window-DC capture black after
        # focus changes even though those pixels are visible on screen. Grab
        # the composed desktop pixels so the harness records what the player
        # actually saw during the live test.
        from PIL import ImageGrab

        image = ImageGrab.grab(
            bbox=(rect.left, rect.top, rect.right, rect.bottom), all_screens=True
        ).convert("RGB")
        if image.size != (width, height):
            raise WindowControlError("desktop capture bounds changed")
        return width, height, image.tobytes()

    def _capture_rgb_gdi(self, handle: int) -> tuple[int, int, bytes]:
        """Legacy GDI capture retained for hosts where desktop capture differs."""
        rect = wintypes.RECT()
        if not self.user32.GetWindowRect(handle, ctypes.byref(rect)):
            raise WindowControlError("GetWindowRect failed")
        width, height = rect.right - rect.left, rect.bottom - rect.top
        if width <= 0 or height <= 0:
            raise WindowControlError("game window has empty bounds")
        screen = self.user32.GetDC(0)
        if not screen:
            raise WindowControlError("GetDC failed")
        memory = bitmap = previous = None
        try:
            memory = self.gdi32.CreateCompatibleDC(screen)
            bitmap = self.gdi32.CreateCompatibleBitmap(screen, width, height)
            if not memory or not bitmap:
                raise WindowControlError("GDI capture allocation failed")
            previous = self.gdi32.SelectObject(memory, bitmap)
            if not self.gdi32.BitBlt(memory, 0, 0, width, height, screen,
                                    rect.left, rect.top, 0x00CC0020):
                raise WindowControlError("BitBlt failed")
            self.gdi32.SelectObject(memory, previous)
            previous = None
            header = _BitmapInfo()
            header.header.biSize = ctypes.sizeof(_BitmapInfoHeader)
            header.header.biWidth = width
            header.header.biHeight = -height
            header.header.biPlanes = 1
            header.header.biBitCount = 32
            header.header.biCompression = 0
            raw = ctypes.create_string_buffer(width * height * 4)
            if self.gdi32.GetDIBits(memory, bitmap, 0, height, raw,
                                    ctypes.byref(header), 0) != height:
                raise WindowControlError("GetDIBits failed")
            bgra = raw.raw
            rgb = bytearray(width * height * 3)
            for source in range(0, len(bgra), 4):
                target = source // 4 * 3
                rgb[target:target + 3] = bgra[source + 2:source + 3] + bgra[source + 1:source + 2] + bgra[source:source + 1]
            return width, height, bytes(rgb)
        finally:
            if previous and memory:
                self.gdi32.SelectObject(memory, previous)
            if bitmap:
                self.gdi32.DeleteObject(bitmap)
            if memory:
                self.gdi32.DeleteDC(memory)
            self.user32.ReleaseDC(0, screen)


def click_desktop(pid: int, x: int, y: int, adapter: Win32Adapter) -> None:
    matches = [w for w in adapter.windows() if w.pid == pid and w.visible
               and w.title == "Hoenn Sessions"]
    if len(matches) != 1:
        raise WindowControlError(f"expected one signed desktop window for PID {pid}")
    target = matches[0]
    adapter.restore(target.handle)
    deadline = time.monotonic() + 3
    while adapter.foreground() != target.handle:
        adapter.focus(target.handle)
        if time.monotonic() >= deadline:
            raise WindowControlError(f"desktop window {target.handle} did not become foreground")
        time.sleep(0.05)
    adapter.click_window_pixel(target.handle, x, y)


def wait_desktop_window(pid: int, adapter: WindowAdapter, *, timeout: float = 20) -> Window:
    if not 0 < timeout <= 30:
        raise ValueError("desktop window timeout must be within (0, 30] seconds")
    deadline = time.monotonic() + timeout
    while True:
        matches = [w for w in adapter.windows() if w.pid == pid and w.visible
                   and w.title == "Hoenn Sessions"]
        if len(matches) == 1:
            return matches[0]
        if len(matches) > 1:
            raise WindowControlError("multiple signed desktop windows for one PID")
        if time.monotonic() >= deadline:
            raise WindowControlError("signed desktop did not create its window before deadline")
        time.sleep(.1)


class _BitmapInfoHeader(ctypes.Structure):
    _fields_ = [("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG),
                ("biHeight", wintypes.LONG), ("biPlanes", wintypes.WORD),
                ("biBitCount", wintypes.WORD), ("biCompression", wintypes.DWORD),
                ("biSizeImage", wintypes.DWORD), ("biXPelsPerMeter", wintypes.LONG),
                ("biYPelsPerMeter", wintypes.LONG), ("biClrUsed", wintypes.DWORD),
                ("biClrImportant", wintypes.DWORD)]


class _BitmapInfo(ctypes.Structure):
    _fields_ = [("header", _BitmapInfoHeader), ("colors", wintypes.DWORD * 3)]
