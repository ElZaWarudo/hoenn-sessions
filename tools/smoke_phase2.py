"""Run the two deterministic Phase 2 integration targets without a shell."""

from __future__ import annotations

import argparse
import ctypes
import json
import os
import signal
import subprocess
import sys
from pathlib import Path
from typing import Any

from ctypes import wintypes


ROOT = Path(__file__).resolve().parents[1]
TIMEOUT_SECONDS = 300
REAP_TIMEOUT_SECONDS = 5
TARGETS = (
    (
        "coop-server-http",
        ("cargo", "test", "-p", "coop-server", "--test", "phase2_http", "--all-features", "--locked"),
    ),
    (
        "coop-launcher-sidecar",
        ("cargo", "test", "-p", "coop-launcher", "--test", "phase2_smoke", "--all-features", "--locked"),
    ),
)


class _ProcessContainment:
    """Own a process group/job so timeout cleanup includes descendants."""

    def __init__(self, process: subprocess.Popen[bytes]) -> None:
        self._job_handle: wintypes.HANDLE | None = None
        self._kernel32: Any | None = None
        if os.name == "nt":
            self._attach_windows_job(process)

    def _attach_windows_job(self, process: subprocess.Popen[bytes]) -> None:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        create_job = kernel32.CreateJobObjectW
        create_job.argtypes = [wintypes.LPVOID, wintypes.LPCWSTR]
        create_job.restype = wintypes.HANDLE
        handle = create_job(None, None)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())

        class BasicLimitInformation(ctypes.Structure):
            _fields_ = [
                ("PerProcessUserTime", ctypes.c_longlong),
                ("PerJobUserTime", ctypes.c_longlong),
                ("LimitFlags", wintypes.DWORD),
                ("MinimumWorkingSetSize", ctypes.c_size_t),
                ("MaximumWorkingSetSize", ctypes.c_size_t),
                ("ActiveProcessLimit", wintypes.DWORD),
                ("Affinity", ctypes.c_size_t),
                ("PriorityClass", wintypes.DWORD),
                ("SchedulingClass", wintypes.DWORD),
            ]

        class IoCounters(ctypes.Structure):
            _fields_ = [
                ("ReadOperationCount", ctypes.c_ulonglong),
                ("WriteOperationCount", ctypes.c_ulonglong),
                ("OtherOperationCount", ctypes.c_ulonglong),
                ("ReadTransferCount", ctypes.c_ulonglong),
                ("WriteTransferCount", ctypes.c_ulonglong),
                ("OtherTransferCount", ctypes.c_ulonglong),
            ]

        class ExtendedLimitInformation(ctypes.Structure):
            _fields_ = [
                ("BasicLimitInformation", BasicLimitInformation),
                ("IoInfo", IoCounters),
                ("ProcessMemoryLimit", ctypes.c_size_t),
                ("JobMemoryLimit", ctypes.c_size_t),
                ("PeakProcessMemoryUsed", ctypes.c_size_t),
                ("PeakJobMemoryUsed", ctypes.c_size_t),
            ]

        info = ExtendedLimitInformation()
        info.BasicLimitInformation.LimitFlags = 0x2000  # KILL_ON_JOB_CLOSE
        set_information = kernel32.SetInformationJobObject
        set_information.argtypes = [
            wintypes.HANDLE,
            wintypes.INT,
            ctypes.c_void_p,
            wintypes.DWORD,
        ]
        set_information.restype = wintypes.BOOL
        if not set_information(
            handle,
            9,  # JobObjectExtendedLimitInformation
            ctypes.byref(info),
            ctypes.sizeof(info),
        ):
            kernel32.CloseHandle(handle)
            raise ctypes.WinError(ctypes.get_last_error())

        assign = kernel32.AssignProcessToJobObject
        assign.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
        assign.restype = wintypes.BOOL
        if not assign(handle, wintypes.HANDLE(process._handle)):
            kernel32.CloseHandle(handle)
            raise ctypes.WinError(ctypes.get_last_error())
        self._kernel32 = kernel32
        self._job_handle = handle

    def terminate(self, process: subprocess.Popen[bytes]) -> None:
        if self._job_handle is not None and self._kernel32 is not None:
            if self._kernel32.TerminateJobObject(self._job_handle, 1):
                return
            if os.name == "nt":
                _terminate_windows_tree(process)
            return
        if os.name != "nt":
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        else:
            process.kill()

    def close(self) -> None:
        if self._job_handle is not None and self._kernel32 is not None:
            self._kernel32.CloseHandle(self._job_handle)
            self._job_handle = None


def _trusted_taskkill() -> Path | None:
    """Return only the pinned Windows tree-kill binary."""

    if os.name != "nt":
        return None
    path = Path(r"C:\Windows\System32\taskkill.exe")
    try:
        canonical = path.resolve(strict=True)
    except OSError:
        return None
    if not path.is_file() or not canonical.is_file():
        return None
    canonical_text = os.path.normcase(os.path.normpath(str(canonical))).removeprefix(
        os.path.normcase("\\\\?\\")
    )
    if canonical_text != os.path.normcase(os.path.normpath(str(path))):
        return None
    return path


def _terminate_windows_tree(process: subprocess.Popen[bytes]) -> bool:
    """Terminate and reap a process tree with the trusted system utility."""

    taskkill = _trusted_taskkill()
    if taskkill is None:
        return False
    try:
        killer = subprocess.Popen(
            (str(taskkill), "/PID", str(process.pid), "/T", "/F"),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            shell=False,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
    except OSError:
        return False
    try:
        killer.wait(timeout=REAP_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        killer.kill()
        try:
            killer.wait(timeout=REAP_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            return False
    return killer.returncode == 0


def run_target(name: str, command: tuple[str, ...]) -> dict[str, object]:
    """Execute one fixed target with a finite wall-clock budget."""

    process_options: dict[str, object]
    if os.name == "nt":
        process_options = {
            "creationflags": subprocess.CREATE_NEW_PROCESS_GROUP,
        }
    else:
        process_options = {"start_new_session": True}
    try:
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            shell=False,
            **process_options,
        )
    except OSError:
        return {"name": name, "status": "unavailable"}

    try:
        containment = _ProcessContainment(process)
    except OSError:
        if not _terminate_windows_tree(process):
            try:
                process.kill()
            except OSError:
                pass
        try:
            process.wait(timeout=REAP_TIMEOUT_SECONDS)
        except (OSError, subprocess.TimeoutExpired):
            pass
        return {"name": name, "status": "unavailable"}

    try:
        returncode = process.wait(timeout=TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        containment.terminate(process)
        try:
            process.wait(timeout=REAP_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            process.kill()
            try:
                process.wait(timeout=REAP_TIMEOUT_SECONDS)
            except subprocess.TimeoutExpired:
                return {"name": name, "status": "timed_out"}
        return {"name": name, "status": "timed_out"}
    finally:
        containment.close()

    return {
        "name": name,
        "status": "passed" if returncode == 0 else "failed",
        "returncode": returncode,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--local",
        action="store_true",
        help="document that both targets use only local deterministic adapters",
    )
    parser.parse_args()
    results = [run_target(name, command) for name, command in TARGETS]
    ok = all(result["status"] == "passed" for result in results)
    output = {"runner": "smoke_phase2", "version": 1, "ok": ok, "targets": results}
    json.dump(output, sys.stdout, sort_keys=True, separators=(",", ":"))
    sys.stdout.write("\n")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
