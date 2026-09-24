#!/usr/bin/env python3
"""Drive the debug-only GamePilot against an explicitly configured staging server.

Credentials travel through stdin to an app-private file. This script never prints
them, the pilot report, or the ROM-bearing screenshots it creates on the device.
"""

import json
import os
import subprocess
import sys
import time
from urllib.parse import urlparse


PACKAGE = "io.hoenn.sessions"
PRODUCTION_HOST = "169-128-190-115.sslip.io"
ADB = os.environ.get("ADB", "adb")


def adb(*args, input_text=None):
    result = subprocess.run(
        [ADB, *args], input=input_text, text=True, capture_output=True, check=False
    )
    if result.returncode:
        raise RuntimeError("ADB operation failed")
    return result.stdout


def private_write(name, value):
    adb("shell", f"run-as {PACKAGE} sh -c 'umask 077; cat > files/{name}.tmp'", input_text=value)
    adb("shell", "run-as", PACKAGE, "mv", f"files/{name}.tmp", f"files/{name}")


def result_for(identifier, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = subprocess.run(
            [ADB, "shell", "run-as", PACKAGE, "cat", "files/pilot-result.json"],
            text=True, capture_output=True, check=False,
        )
        if result.returncode == 0:
            try:
                report = json.loads(result.stdout)
            except json.JSONDecodeError:
                report = {}
            if "error" in report:
                raise RuntimeError(f"GamePilot stopped during {identifier}")
            if report.get("id") == identifier:
                return report
        time.sleep(0.25)
    raise TimeoutError(f"GamePilot timed out during {identifier}")


def main():
    url = urlparse(os.environ.get("HOENN_SERVER_URL", ""))
    if url.scheme != "https" or not url.hostname or url.hostname == PRODUCTION_HOST or url.username or url.password:
        raise RuntimeError("A distinct HTTPS staging server is required")
    username = os.environ.get("ANDROID_STAGING_USERNAME", "")
    password = os.environ.get("ANDROID_STAGING_PASSWORD", "")
    if not username or not password:
        raise RuntimeError("Staging account secrets are required")

    adb("shell", "run-as", PACKAGE, "mkdir", "-p", "files")
    private_write("pilot-credentials.json", json.dumps({"username": username, "password": password}))
    del username, password

    pilot = subprocess.Popen(
        [ADB, "shell", "am", "instrument", "-w", f"{PACKAGE}/.GamePilot"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    next_id = 1

    def command(op, *, wait_ms=250):
        nonlocal next_id
        identifier = next_id
        next_id += 1
        private_write("pilot-command.json", json.dumps({"id": identifier, "op": op, "wait_ms": wait_ms}))
        return result_for(identifier, 45)

    def until(stage, predicate, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            report = command("status")
            if predicate(report):
                print(f"PASS {stage}", flush=True)
                return
            if pilot.poll() is not None:
                raise RuntimeError(f"GamePilot exited during {stage}")
            time.sleep(1)
        raise TimeoutError(f"Timed out waiting for {stage}")

    try:
        result_for(0, 45)
        command("login")
        until("login and game load", lambda r: r.get("active") and r.get("playing"), 180)

        adb("shell", "input", "keyevent", "KEYCODE_HOME")
        until("activity pause", lambda r: r.get("active") and not r.get("focused"), 30)

        adb("shell", "am", "start", "-n", f"{PACKAGE}/.MainActivity", "-f", "0x30000000")
        until("activity resume", lambda r: r.get("active") and r.get("playing") and r.get("focused"), 45)

        report = command("close", wait_ms=1000)
        if report.get("active"):
            until("session close", lambda r: not r.get("active"), 45)
        else:
            print("PASS session close", flush=True)
        private_write("pilot-command.json", json.dumps({"id": next_id, "op": "finish"}))
        pilot.wait(timeout=15)
        if pilot.returncode:
            raise RuntimeError("GamePilot instrument returned an error")
    finally:
        if pilot.poll() is None:
            pilot.terminate()
            try:
                pilot.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pilot.kill()
        adb("shell", "run-as", PACKAGE, "rm", "-f", "files/pilot-credentials.json", "files/pilot-credentials.json.tmp")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, TimeoutError, subprocess.SubprocessError) as error:
        print(f"Staging pilot failed: {error}", file=sys.stderr)
        sys.exit(1)
