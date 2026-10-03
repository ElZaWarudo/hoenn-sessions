#!/usr/bin/env python3
"""Test-only signed cold Continue through presence publication, never travel.

Screens and transport publication are evidence, not a gameplay-state oracle.
Uses installed signed clients; never prepares profiles or registers accounts.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import time

import live_region_harness as harness


def presence_prefix(plan: dict, leg_name: str | None = None) -> tuple[dict, list[dict]]:
    """Validate the entire bounded prefix before any client can be launched."""
    players = plan.get("players")
    if (not isinstance(players, list) or len(players) != 2
            or any(not isinstance(p, dict) for p in players)
            or {p.get("name") for p in players} != {"a", "b"}):
        raise harness.HarnessFailure("presence requires exactly players a and b")
    legs = plan.get("legs")
    if not isinstance(legs, list) or not legs or not isinstance(legs[0], dict):
        raise harness.HarnessFailure("presence requires a recorded leg")
    leg = harness._leg(plan, leg_name if leg_name is not None else legs[0].get("name"))
    name = leg.get("name")
    # Leaves ample room for the bounded presence-{leg}-{index}-{actor} stem.
    if (not isinstance(name, str) or not 1 <= len(name) <= 48
            or not name.isascii() or not all(c.isalnum() or c in "-_" for c in name)):
        raise harness.HarnessFailure("presence leg name must be an ASCII filename stem of 1..48 characters")
    if type(leg.get("source_world_id")) is not int or leg["source_world_id"] <= 0:
        raise harness.HarnessFailure("presence requires a source world")
    inputs = leg.get("inputs")
    if not isinstance(inputs, list):
        raise harness.HarnessFailure("presence requires recorded inputs")
    allowed = {"player", "key", "hold_ms", "release_ms", "wait_ms",
               "screenshot", "expect_presence_published"}
    ready, prefix, budget = set(), [], 0
    for action in inputs:
        if len(prefix) >= 32 or not isinstance(action, dict) or set(action) - allowed:
            raise harness.HarnessFailure("presence prefix has unsupported actions or exceeds 32 inputs")
        actor = action.get("player")
        if actor not in {"a", "b"} or actor in ready:
            raise harness.HarnessFailure("presence input targets an unknown or already ready player")
        if action.get("key") not in {"gba_b", "gba_start", "gba_a"}:
            raise harness.HarnessFailure("presence accepts only cold Continue keys")
        for key, default, low, high in (("hold_ms", 80, 10, 2000),
                                       ("release_ms", 80, 0, 2000),
                                       ("wait_ms", 0, 0, 30000)):
            value = action.get(key, default)
            if type(value) is not int or not low <= value <= high:
                raise harness.HarnessFailure(f"presence {key} outside bounded range")
            budget += value
        for key in ("screenshot", "expect_presence_published"):
            if key in action and type(action[key]) is not bool:
                raise harness.HarnessFailure(f"presence {key} must be boolean")
        if budget > 180000:
            raise harness.HarnessFailure("presence prefix exceeds 180 seconds")
        prefix.append(dict(action))
        if action.get("expect_presence_published"):
            if action["key"] != "gba_a":
                raise harness.HarnessFailure("presence checkpoint must follow Continue A")
            ready.add(actor)
        if ready == {"a", "b"}:
            return leg, prefix
    raise harness.HarnessFailure("presence prefix lacks both player checkpoints")


def bind_games(plan: dict, leg: dict, pids: dict) -> dict:
    """Hash the actual process ROM and emulator before sending any key."""
    if (set(pids) != {"a", "b"} or any(type(p) is not int or p <= 0 for p in pids.values())
            or len(set(pids.values())) != 2):
        raise harness.HarnessFailure("presence requires both signed game PIDs")
    release, _ = harness._paths(plan)
    catalog = harness._read_json(release / "release_catalog.json", "presence catalog")
    worlds = [w for w in catalog["worlds"] if w["world_id"] == leg["source_world_id"]]
    if len(worlds) != 1:
        raise harness.HarnessFailure("presence source world is absent or ambiguous")
    world = worlds[0]
    emulator_sha = harness.digest(release / "runtime/mgba.exe")
    result = {}
    for name, pid in pids.items():
        process = harness.psutil.Process(pid)
        roms = [Path(arg) for arg in process.cmdline() if arg.casefold().endswith(".gba")]
        if len(roms) != 1:
            raise harness.HarnessFailure(f"{name}: expected one actual ROM")
        rom, exe = roms[0], Path(process.exe())
        harness.require_hash(rom, world["rom_sha256"], f"{name}: live source ROM")
        harness.require_hash(exe, emulator_sha, f"{name}: live signed emulator")
        result[name] = {"pid": pid, "world_id": leg["source_world_id"],
                        "rom_path": str(rom), "rom_sha256": world["rom_sha256"],
                        "rom_header": harness._rom_header_title(rom),
                        "emulator_sha256": emulator_sha}
    return result


def check_presence(plan: dict, leg_name: str | None = None) -> dict:
    leg, inputs = presence_prefix(plan, leg_name)
    harness.preflight(plan)  # Includes installed account/character and signed-family checks.
    release, run_dir = harness._paths(plan)
    desktops = harness.launch(plan)
    boundary = "start-games"
    try:
        pids = harness.start_games(plan, desktops)
        boundary = "bind-source-roms"
        bindings = bind_games(plan, leg, pids)
        harness.checkpoint(run_dir, "presence-roms-bound", bindings)
        adapter = harness.Win32Adapter()
        screenshots, published = [], []
        for index, action in enumerate(inputs):
            name = action["player"]
            boundary = f"presence-{leg['name']}-{index}-{name}"
            harness.check_c_space()
            if not harness.psutil.pid_exists(pids[name]):
                raise harness.HarnessFailure(f"{boundary}: game exited before input")
            harness.tap(pids[name], action["key"], adapter,
                        hold=action.get("hold_ms", 80) / 1000,
                        release=action.get("release_ms", 80) / 1000)
            time.sleep(action.get("wait_ms", 0) / 1000)
            image = harness.capture_game(pids[name], run_dir / "screenshots", boundary, adapter)
            screenshots.append({"boundary": boundary, "path": str(image),
                                "sha256": harness.digest(Path(image))})
            if action.get("expect_presence_published"):
                harness.require_presence_published(pids[name], release, leg["source_world_id"],
                                                   run_dir, boundary)
                published.append(name)
        result = {"leg": leg["name"], "players_published": published,
                  "input_count": len(inputs), "bindings": bindings, "screenshots": screenshots,
                  "scope": "cold Continue and transport presence; gameplay and travel unverified"}
        harness.checkpoint(run_dir, "signed-presence-complete", result)
        return result
    except Exception as error:
        try:
            harness.checkpoint(run_dir, "signed-presence-failed",
                               {"boundary": boundary, "error": str(error), "type": type(error).__name__})
        except Exception as diagnostic_error:
            error.add_note(f"Presence checkpoint also failed: {diagnostic_error}")
        raise
    finally:
        original = sys.exc_info()[1]
        errors = []
        for cleanup in (lambda: harness.stop_runtime(plan, desktops),
                        lambda: harness.close_desktops(desktops)):
            try:
                cleanup()
            except Exception as error:
                errors.append(error)
        if errors:
            if original is None:
                original = errors.pop(0)
                for error in errors:
                    original.add_note(f"Signed presence cleanup also failed: {error}")
                raise original
            for error in errors:
                original.add_note(f"Signed presence cleanup also failed: {error}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--leg")
    args = parser.parse_args()
    try:
        result = check_presence(harness._read_json(args.plan, "presence plan"), args.leg)
    except Exception as error:
        print(json.dumps({"ok": False, "error": str(error), "type": type(error).__name__,
                          "notes": getattr(error, "__notes__", [])}))
        return 1
    print(json.dumps({"ok": True, **result}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
