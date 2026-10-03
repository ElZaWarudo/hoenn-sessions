"""Bounded test-only capture of immutable ROM-written save images."""

from __future__ import annotations

import hashlib
import os
import threading
from pathlib import Path

from live_harness_oracles import OracleFailure, read_flash_bytes


def _capture_path(path: Path) -> Path:
    # Nested run/actor/digest names exceed MAX_PATH. Return the extended path
    # to readers too: changing only the temporary write leaves the .sav unreadable.
    if os.name != "nt":
        return path
    absolute = os.path.abspath(path)
    if absolute.startswith("\\\\?\\"):
        return Path(absolute)
    if absolute.startswith("\\\\"):
        return Path("\\\\?\\UNC\\" + absolute[2:])
    return Path("\\\\?\\" + absolute)


def capture_directory(run_dir: Path, leg: str, player: str) -> Path:
    # Plan labels never become path components (nor collide on Windows casing).
    key = hashlib.sha256((leg + "\0" + player).encode()).hexdigest()
    return _capture_path(run_dir / "captured-saves" / key)


class SaveCapture:
    """Read during input waits; never write back to a signed client's session.

    Invalid/in-progress images are retried. A valid image is hashed, validated
    and persisted from the SAME read. Missing exact receipts still fail later;
    polling cannot guarantee observation of an arbitrarily short-lived file.
    """

    def __init__(self, sessions: Path, output: Path, *, interval: float = .05,
                 limit: int = 256):
        self.sessions, self.output = sessions, _capture_path(output)
        self.interval, self.limit = interval, limit
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._error: Exception | None = None

    def scan(self) -> None:
        self.output.mkdir(parents=True, exist_ok=True)
        paths = list(self.sessions.glob("coop-session-*/*.sav"))
        if len(paths) > self.limit:
            raise RuntimeError("save capture: too many live save paths")
        for path in paths:
            try:
                data = path.read_bytes()
            except (FileNotFoundError, PermissionError):
                continue  # Atomic replacement/retirement can race the read.
            try:
                save = read_flash_bytes(data, path)
            except OracleFailure:
                continue
            target = self.output / (save.sha256 + ".sav")
            if target.exists():
                if target.read_bytes() != data:
                    raise RuntimeError("save capture: existing digest file changed")
                continue
            if len(list(self.output.glob("*.sav"))) >= self.limit:
                raise RuntimeError("save capture: image limit reached")
            temporary = target.with_suffix(".tmp")
            temporary.write_bytes(data)
            temporary.replace(target)

    def _run(self) -> None:
        try:
            while not self._stop.wait(self.interval):
                self.scan()
        except Exception as exc:
            self._error = exc
            self._stop.set()

    def check(self) -> None:
        if self._error is not None:
            raise RuntimeError("save capture stopped: " + str(self._error)) from self._error

    def __enter__(self) -> SaveCapture:
        self.scan()  # Capture initial head BEFORE the first input.
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()
        return self

    def __exit__(self, kind, value, traceback) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=5)
            if self._thread.is_alive() and kind is None:
                raise RuntimeError("save capture did not stop within five seconds")
        if kind is None:
            self.check()
            self.scan()
