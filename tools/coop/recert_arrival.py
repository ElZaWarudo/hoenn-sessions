#!/usr/bin/env python3
"""Manually recertify one first-arrival save against an exact new ROM.

CI never runs this. When a ROM change alters a world's ROM hash, release
assembly refuses with "recertify arrival saves". An operator then:

1. Builds the exact release ROM (the hash CI will produce).
2. Runs this helper with the previously attested save as input. It launches
   the pinned mGBA executable in an isolated profile directory, drives only
   key inputs through an mGBA Lua frame callback (cold Continue, START menu,
   SAVE, confirm overwrite) and never writes emulated memory or patches the
   save. Screenshots are recorded for review.
3. With --verify-cold-reload, a second run reloads the new save without any
   save input and requires the save bytes to stay unchanged.
4. Reviews the screenshots (SAVE selected, "saved the game" in the arrival
   town), copies the output save to data/release_arrivals/<world>.sav and
   pastes the printed attestation into data/release_arrivals.json.

`--downs` is the number of DOWN presses from the first START-menu entry to
SAVE (4 for Main, 5 for Cormoria in the 2026-10 builds); confirm it on the
shot-2250 screenshot. The helper is written for the Windows mGBA build used by
the private pilot (APPDATA/LOCALAPPDATA are redirected into the run directory).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import time
from pathlib import Path

PINNED_MGBA_SHA256 = "743157a16a1cb478a2b45e6e20e9a482ea397c3820d7e8e27b1e048e85bd5546"
SAVE_SHOTS = (300, 940, 1500, 1900, 2090, 2250, 2500, 2750, 3000, 3300)
COLD_SHOTS = (300, 940, 1500, 1900, 2090, 2250)

LUA = r'''
local root = "%(root)s"
local out = assert(io.open(root .. "/frames.log", "w"))
local frames = 0
local downs = %(downs)d
local save = %(save)s
local shots = {%(shots)s}
local function inwin(a, b, period, width) return frames >= a and frames <= b and ((frames - a) %% period) < width end
callbacks:add("frame", function()
  frames = frames + 1
  local start = inwin(360, 900, 90, 5) or (frames >= 1850 and frames <= 1855)
  local a = inwin(950, 1450, 90, 5)
  local down = downs > 0 and frames >= 2000 and frames <= 2000 + downs * 15 - 1 and ((frames - 2000) %% 15) < 3
  if save then
    a = a or (frames >= 2300 and frames <= 2305) or (frames >= 2550 and frames <= 2555) or (frames >= 2800 and frames <= 2805)
  end
  if start then emu:addKey(C.GBA_KEY.START) else emu:clearKey(C.GBA_KEY.START) end
  if a then emu:addKey(C.GBA_KEY.A) else emu:clearKey(C.GBA_KEY.A) end
  if down then emu:addKey(C.GBA_KEY.DOWN) else emu:clearKey(C.GBA_KEY.DOWN) end
  for _, f in ipairs(shots) do
    if frames == f then
      emu:screenshot(root .. "/shot-" .. f .. ".png")
      out:write("frame=" .. f .. "\n"); out:flush()
    end
  end
  if frames == shots[#shots] + 5 then out:write("done\n"); out:flush() end
end)
'''


def sha256_file(path: Path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run_emulator(mgba: Path, rom: Path, save: Path, root: Path, downs: int, do_save: bool,
                 timeout: float) -> dict:
    root.mkdir(parents=True, exist_ok=False)
    (root / "appdata").mkdir()
    (root / "localappdata").mkdir()
    shutil.copyfile(rom, root / "game.gba")
    shutil.copyfile(save, root / "game.sav")
    emulator = root / mgba.name
    shutil.copyfile(mgba, emulator)
    shots = SAVE_SHOTS if do_save else COLD_SHOTS
    lua = LUA % {"root": str(root.resolve()).replace("\\", "/"), "downs": downs,
                 "save": "true" if do_save else "false", "shots": ",".join(map(str, shots))}
    (root / "drive.lua").write_text(lua, encoding="ascii", newline="\n")
    env = dict(os.environ, APPDATA=str(root / "appdata"), LOCALAPPDATA=str(root / "localappdata"))
    before = sha256_file(root / "game.sav")
    with (root / "stdout.log").open("wb") as out, (root / "stderr.log").open("wb") as err:
        process = subprocess.Popen([str(emulator), "--script", str(root / "drive.lua"),
                                    str(root / "game.gba")], cwd=root, env=env, stdout=out, stderr=err)
        try:
            deadline = time.monotonic() + timeout
            log = root / "frames.log"
            while True:
                if process.poll() is not None:
                    raise SystemExit("emulator exited before the input script finished")
                if log.exists() and "done" in log.read_text(encoding="ascii", errors="replace"):
                    break
                if time.monotonic() > deadline:
                    raise SystemExit("timed out waiting for the input script")
                time.sleep(0.25)
            time.sleep(3)  # let the emulator flush the flash image
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
    after = sha256_file(root / "game.sav")
    result = {"input_save_sha256": before, "output_save_sha256": after, "changed": before != after,
              "did_save_inputs": do_save, "downs": downs,
              "screenshots": [f"shot-{frame}.png" for frame in shots]}
    (root / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mgba", type=Path, required=True, help="pinned mGBA executable")
    parser.add_argument("--mgba-sha256", default=PINNED_MGBA_SHA256)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--rom-sha256", required=True, help="exact release ROM hash")
    parser.add_argument("--save", type=Path, required=True, help="previously attested arrival save")
    parser.add_argument("--save-sha256", required=True)
    parser.add_argument("--out", type=Path, required=True, help="new run directory")
    parser.add_argument("--downs", type=int, required=True)
    parser.add_argument("--world", required=True, help="registry world name, e.g. main")
    parser.add_argument("--portal", required=True, help="arrival portal id, e.g. from_cormoria")
    parser.add_argument("--verify-cold-reload", action="store_true")
    parser.add_argument("--timeout", type=float, default=180.0)
    args = parser.parse_args()
    if sha256_file(args.mgba) != args.mgba_sha256:
        raise SystemExit("mGBA executable does not match the pinned hash")
    if sha256_file(args.rom) != args.rom_sha256 or sha256_file(args.save) != args.save_sha256:
        raise SystemExit("ROM or input save does not match the supplied hash")
    saved = run_emulator(args.mgba, args.rom, args.save, args.out / "save", args.downs, True,
                         args.timeout)
    if not saved["changed"]:
        raise SystemExit("the in-game save did not change the save file; inspect the screenshots")
    output = args.out / "save" / "game.sav"
    if args.verify_cold_reload:
        cold = run_emulator(args.mgba, args.rom, output, args.out / "cold", args.downs, False,
                            args.timeout)
        if cold["changed"]:
            raise SystemExit("cold reload changed the new save; do not attest it")
    attestation = {args.world: {"arrivals": {args.portal: {
        "rom_sha256": args.rom_sha256,
        "sav_path": f"data/release_arrivals/{args.world}.sav",
        "sav_sha256": saved["output_save_sha256"],
        "receipt": {"method": f"tools/coop/recert_arrival.py --downs {args.downs}"
                              + (" --verify-cold-reload" if args.verify_cold_reload else ""),
                    "input_save_sha256": args.save_sha256,
                    "mgba_executable_sha256": args.mgba_sha256,
                    "evidence": "fill in: reviewer, screenshots inspected, live travel run"}}}}}
    print(f"review screenshots under {args.out}, then copy {output} to "
          f"data/release_arrivals/{args.world}.sav and merge this attestation:")
    print(json.dumps(attestation, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
