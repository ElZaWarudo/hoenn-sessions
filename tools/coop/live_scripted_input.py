"""TEST ONLY: bounded mGBA button input and screenshots, without desktop focus.

Use only in an isolated standalone emulator after signed-fixture preflight.
This is not loaded into signed clients or their authenticated bridge scripts.
It never edits emulated memory or save bytes; saves must use normal ROM menus.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import time
from contextlib import contextmanager
from pathlib import Path

from live_harness_oracles import OracleFailure

KEY_MASKS = {"a": 1, "b": 2, "select": 4, "start": 8, "right": 16,
             "left": 32, "up": 64, "down": 128, "r": 256, "l": 512}

DRIVER = r'''local frame, sequence, keys, active = 0, 0, 0, nil
local function log(status, label)
 local f = assert(io.open("script-input.jsonl", "a"))
 f:write(string.format('{"status":"%s","sequence":%d,"label":"%s","frame":%d}\n', status, sequence, label, frame))
 f:close()
end
local function update()
 frame = frame + 1
 if frame > 72000 then error("script frame budget expired") end
 if active then
  if frame >= active.release then keys = 0 end
  if frame >= active.finish then
   emu:setKeys(0)
   emu:screenshot(active.label .. ".png")
   log("complete", active.label)
   active = nil
  end
 elseif frame % 6 == 0 then
  local path = string.format("request-%04d.txt", sequence + 1)
  local f = io.open(path, "r")
  if f then
   local data = f:read("*a"); f:close()
   local seq, mask, hold, wait, label = data:match("^(%d+),(%d+),(%d+),(%d+),([A-Za-z0-9_-]+)\n$")
   seq, mask, hold, wait = tonumber(seq), tonumber(mask), tonumber(hold), tonumber(wait)
   if not seq or seq ~= sequence + 1 or seq > 1000 or mask > 1023
      or hold < 1 or hold > 120 or wait > 7200 or #label > 48 then
    error("invalid input request")
   end
   sequence = seq; keys = mask
   active = {release = frame + hold, finish = frame + hold + wait, label = label}
   log("started", label)
  end
 end
 emu:setKeys(keys)
end
callbacks:add("keysRead", function() emu:setKeys(keys) end)
local frameCallback
frameCallback = callbacks:add("frame", function()
 local ok = pcall(update)
 if not ok then
  keys = 0; active = nil; emu:setKeys(0)
  log("failed", "driver-error")
  callbacks:remove(frameCallback)
 end
end)
'''


def request_line(sequence: int, label: str, mask: int, hold: int, wait: int) -> str:
    if (type(sequence) is not int or not 1 <= sequence <= 1000
            or not isinstance(label, str) or re.fullmatch(r"[A-Za-z0-9_-]{1,48}", label) is None
            or type(mask) is not int or not 0 <= mask <= 1023
            or type(hold) is not int or not 1 <= hold <= 120
            or type(wait) is not int or not 0 <= wait <= 7200):
        raise OracleFailure("invalid bounded scripted input request")
    return f"{sequence},{mask},{hold},{wait},{label}\n"


class ScriptedInput:
    def __init__(self, root: Path, process: subprocess.Popen):
        self.root, self.process = root, process
        self.sequence = 0
        self.labels: set[str] = set()

    def act(self, label: str, mask: int = 0, *, hold: int = 8,
            wait: int = 20, timeout: float = 60) -> Path:
        if not 0 < timeout <= 180 or label in self.labels:
            raise OracleFailure("invalid scripted input deadline or repeated label")
        line = request_line(self.sequence + 1, label, mask, hold, wait)
        if (self.root / (label + ".png")).exists():
            raise OracleFailure("scripted input screenshot already exists")
        self.sequence += 1
        self.labels.add(label)
        pending = self.root / f"request-{self.sequence:04d}.tmp"
        request = pending.with_suffix(".txt")
        with pending.open("x", encoding="ascii", newline="\n") as file:
            file.write(line)
            file.flush()
            os.fsync(file.fileno())
        if request.exists():
            raise OracleFailure("scripted input request already exists")
        pending.rename(request)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise OracleFailure("owned scripted emulator exited before input completion")
            log = self.root / "script-input.jsonl"
            if log.exists():
                for entry in log.read_text(encoding="ascii").splitlines(keepends=True):
                    if not entry.endswith("\n"):
                        continue
                    receipt = json.loads(entry)
                    if receipt["status"] == "failed":
                        raise OracleFailure("scripted emulator driver failed")
                    if receipt["sequence"] == self.sequence and receipt["status"] == "complete":
                        if receipt["label"] != label:
                            raise OracleFailure("scripted input receipt label differs")
                        screenshot = self.root / (label + ".png")
                        if not screenshot.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"):
                            raise OracleFailure("scripted input screenshot missing or invalid")
                        return screenshot
            time.sleep(.05)
        raise OracleFailure(f"scripted input timed out at {label}")


@contextmanager
def owned_scripted_emulator(exe: Path, root: Path, env: dict):
    """Own one standalone process; never send system-wide keyboard/mouse input."""
    if (root / "script-input.jsonl").exists() or any(root.glob("request-*")):
        raise OracleFailure("scripted emulator needs an unused protocol directory")
    script = root / "script-input.lua"
    with script.open("x", encoding="ascii", newline="\n") as file:
        file.write(DRIVER)
    with (root / "script-stdout.log").open("xb") as out, (root / "script-stderr.log").open("xb") as err:
        process = subprocess.Popen([str(exe), "--script", str(script), str(root / "game.gba")],
                                   cwd=root, env=env, stdout=out, stderr=err)
        primary = None
        try:
            yield ScriptedInput(root, process)
        except BaseException as exc:
            primary = exc
            raise
        finally:
            try:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
            except Exception as cleanup:
                if primary is None:
                    raise
                primary.add_note("owned scripted emulator cleanup failed: " + str(cleanup))
