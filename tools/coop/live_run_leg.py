#!/usr/bin/env python3
"""Test-only guarded execution of one pending leg after a verified crossing.

Checks parked-world proof and current leased server heads before starting any
signed desktop. The preceding session's group ended when its members left
(Stop), so this session forms a fresh group the way main does: a's pairing code
is issued before launch and b types it into its live signed desktop's Join box.
Never prepares profiles, seeds accounts, or replays prior legs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import uuid

import live_desktop_controls as controls
import live_group_evidence as groups
import live_region_harness as harness
from live_harness_windows import KEYS
from live_save_capture import capture_directory
from live_signed_presence import bind_games, presence_prefix


def validate_inputs(plan: dict, name: str) -> dict:
    leg = harness._leg(plan, name)
    if (not isinstance(name, str) or not 1 <= len(name) <= 48 or not name.isascii()
            or not all(c.isalnum() or c in "-_" for c in name)):
        raise harness.HarnessFailure("selected leg name is not a bounded ASCII stem")
    if {p.get("name") for p in plan.get("players", [])} != {"a", "b"} or len(plan["players"]) != 2:
        raise harness.HarnessFailure("selected leg requires actors a and b")
    allowed = {"player", "key", "hold_ms", "release_ms", "wait_ms", "screenshot",
               "expect_presence_published", "expect_travel", "timeout_ms"}
    for section in ("inputs", "arrival_inputs"):
        actions = leg.get(section)
        if not isinstance(actions, list) or not 1 <= len(actions) <= 120:
            raise harness.HarnessFailure(f"selected {section} must contain 1..120 bounded inputs")
        budget = 0
        for action in actions:
            if (not isinstance(action, dict) or set(action) - allowed
                    or action.get("player") not in {"a", "b"}
                    or action.get("key") not in KEYS):
                raise harness.HarnessFailure(f"selected {section} has invalid actor/key/action")
            for field, default, low, high in (("hold_ms", 80, 10, 2000), ("release_ms", 80, 0, 2000),
                                             ("wait_ms", 0, 0, 30000), ("timeout_ms", 60000, 1000, 120000)):
                value = action.get(field, default)
                if type(value) is not int or not low <= value <= high:
                    raise harness.HarnessFailure(f"selected {field} outside bounds")
                if field != "timeout_ms" or action.get("expect_travel"):
                    budget += value
            for field in ("screenshot", "expect_presence_published"):
                if field in action and type(action[field]) is not bool:
                    raise harness.HarnessFailure(f"selected {field} must be boolean")
            if "expect_travel" in action:
                value = action["expect_travel"]
                if value is not True and (not isinstance(value, list) or not value
                                          or len(value) != len(set(value)) or any(n not in {"a", "b"} for n in value)):
                    raise harness.HarnessFailure("selected travel expectation is invalid")
            if section == "arrival_inputs" and ("expect_travel" in action or "timeout_ms" in action):
                raise harness.HarnessFailure("arrival inputs cannot initiate another travel")
        if budget > 600000 or {a["player"] for a in actions} != {"a", "b"}:
            raise harness.HarnessFailure("selected input budget/actors differ")
    presence_prefix(plan, name)
    arrival_plan = dict(plan, legs=[dict(leg, inputs=leg["arrival_inputs"])])
    _, prefix = presence_prefix(arrival_plan, name)
    if len(prefix) != len(leg["arrival_inputs"]):
        raise harness.HarnessFailure("arrival script must end at both presence publications")
    if not any(a.get("expect_travel") is True or set(a.get("expect_travel", [])) == {"a", "b"}
               for a in leg["inputs"]):
        raise harness.HarnessFailure("selected leg lacks both-player travel boundary")
    return leg


def preceding_proof(plan: dict, leg: dict) -> dict:
    """Only retained, verified prior destination may become this source head."""
    release, run_dir = harness._paths(plan)
    previous = harness._leg(plan, leg.get("destination_base_leg"))
    if plan["legs"].index(previous) >= plan["legs"].index(leg):
        raise harness.HarnessFailure("selected leg must follow its parked-base leg")
    events = harness._read_json(run_dir / "checkpoint.json", "selected checkpoints").get("events", [])
    if any(e.get("boundary") == leg["name"] + "-verified" for e in events):
        raise harness.HarnessFailure("selected leg is already verified; refusing replay")
    if (capture_directory(run_dir, leg["name"], "evidence") / "evidence.json").exists():
        raise harness.HarnessFailure("selected leg has retained arrival evidence; refusing replay")
    prior = harness._read_json(capture_directory(run_dir, previous["name"], "evidence") / "evidence.json", "preceding evidence")
    attest = plan.get("departure_attestation", {})
    report = attest.get("verification", {})
    verified = [event for event in events if event.get("boundary") == previous["name"] + "-verified"]
    if (attest.get("version") != 1 or attest.get("leg") != previous["name"]
            or report.get("fixture_key") != harness._fixture_key(plan, release)
            or any(report.get(k) != previous.get(k) for k in ("source_world_id", "destination_world_id", "portal_id"))
            or not verified or any(verified[-1].get(k) != value for k, value in report.items())):
        raise harness.HarnessFailure("preceding departure attestation differs")
    proof = {"players": {}, "group": report.get("group")}
    catalog = harness._read_json(release / "release_catalog.json", "selected catalog")
    source = next(w for w in catalog["worlds"] if w["world_id"] == leg["source_world_id"])
    proof["presence_regions"] = source["presence_regions"]
    for player in plan["players"]:
        name = player["name"]
        if harness._journal_candidates(Path(player["profile_localappdata"]), leg, player["character_id"]):
            raise harness.HarnessFailure(f"{name}: selected leg has already committed; refusing replay")
        harness._destination_base(plan, leg, player, Path("unused-signed-template"))
        item = prior["players"][name]
        if not isinstance(item.get("journal_sha256"), str) or len(item["journal_sha256"]) != 64:
            raise harness.HarnessFailure("preceding journal must be pinned")
        journal = harness._read_evidence_journal(item, "preceding journal")
        commit = journal.get("terminal", {}).get("committed", {})
        stage, intent = journal.get("stage", {}), journal.get("intent", {})
        actors = [p for p in report.get("players", []) if p.get("name") == name]
        receipts = [p for p in attest.get("players", []) if p.get("player") == name]
        if (len(actors) != 1 or len(receipts) != 1 or journal.get("character_id") != player["character_id"]
                or journal.get("phase") not in ("committed", "adopted")
                or commit.get("own_world_id") != leg["source_world_id"]
                or type(commit.get("own_revision")) is not int or commit["own_revision"] < 1
                or not isinstance(commit.get("own_snapshot_id"), str) or not commit["own_snapshot_id"]
                or stage.get("destination_world_id") != leg["source_world_id"]
                or intent.get("source_world_id") != previous["source_world_id"]
                or intent.get("request", {}).get("portal_id") != previous["portal_id"]
                or actors[0].get("character_id") != player["character_id"]
                or actors[0].get("destination_sha256") != stage.get("destination_save_sha256")
                or actors[0].get("exact_journal_source_inspected") is not True
                or actors[0].get("source_sha256") != item.get("journal_source_sha256")
                or receipts[0].get("source_sha256") != item.get("journal_source_sha256")
                or receipts[0].get("staged_sha256") != stage.get("destination_save_sha256")
                or receipts[0].get("journal_sha256") != item["journal_sha256"]):
            raise harness.HarnessFailure(f"{name}: preceding actor/source/stage/commit proof differs")
        harness.require_hash(Path(item["source"]), item["journal_source_sha256"], "preceding source")
        harness.require_hash(Path(item["staged"]), stage["destination_save_sha256"], "preceding stage")
        proof["players"][name] = {"revision": commit["own_revision"], "snapshot_id": commit["own_snapshot_id"],
                                  "sha256": stage["destination_save_sha256"]}
    expected = {p["character_id"] for p in plan["players"]}
    group = proof["group"]
    if (not isinstance(group, dict) or {m["character_id"] for m in group.get("members", [])} != expected
            or group.get("world_zone", {}).get("region") not in proof["presence_regions"]):
        raise harness.HarnessFailure("preceding group proof differs")
    return proof


def guard_heads(plan: dict, leg: dict, proof: dict) -> dict:
    """Bounded leased reads, every release completes before returning to launch."""
    _, parsed = harness._health_url(plan)
    if parsed.hostname not in ("localhost", "127.0.0.1", "::1"):
        raise harness.HarnessFailure("selected head guard is loopback only")
    server = f"{parsed.scheme}://{parsed.netloc}"
    password = os.environ.get("COOP_HARNESS_PASSWORD")
    usernames = {p["name"]: os.environ.get("COOP_HARNESS_USERNAME_" + p["name"].upper()) for p in plan["players"]}
    if not password or not all(usernames.values()) or len(set(usernames.values())) != 2:
        raise harness.HarnessFailure("selected head guard requires distinct environment accounts")
    checked = {}
    for player in plan["players"]:
        harness.check_c_space()
        name, character = player["name"], player["character_id"]
        status, body = groups._call(server, "POST", "/v1/auth/login", payload={"api_version": 1, "username": usernames[name], "password": password})
        login = json.loads(groups._expect(status, body, 200, name + " head login"))
        if login.get("character_id") != character:
            raise harness.HarnessFailure(f"{name}: head account belongs to another actor")
        token = login["access_token"]
        lease = groups._acquire_after_release(server, {"api_version": 1, "character_id": character,
                "client_instance_id": str(uuid.uuid4()), "idempotency_key": str(uuid.uuid4())}, token, 0)
        try:
            expected = proof["players"][name]
            if lease.get("current_revision") != expected["revision"]:
                raise harness.HarnessFailure(f"{name}: current head revision differs")
            status, body = groups._call(server, "GET", f"/v1/characters/{character}/snapshots", token=token, lease=lease)
            snapshots = json.loads(groups._expect(status, body, 200, name + " head snapshots"))["snapshots"]
            current = [s for s in snapshots if s.get("revision") == expected["revision"]]
            if len(current) != 1 or current[0].get("snapshot_id") != expected["snapshot_id"] or current[0].get("rom_world_id") != leg["source_world_id"]:
                raise harness.HarnessFailure(f"{name}: current manifest snapshot/world differs")
            files = [f for f in current[0].get("files", []) if f.get("artifact") == "character.sav"]
            if len(files) != 1 or files[0].get("sha256") != expected["sha256"]:
                raise harness.HarnessFailure(f"{name}: current manifest save differs")
            status, body = groups._call(server, "GET", f"/v1/characters/{character}/resume-package/artifacts/character.sav?revision={expected['revision']}", token=token, lease=lease)
            groups._expect(status, body, 200, name + " head raw save")
            if hashlib.sha256(body).hexdigest() != expected["sha256"]:
                raise harness.HarnessFailure(f"{name}: downloaded current save differs")
            checked[name] = {"character_id": character, "revision": expected["revision"],
                             "snapshot_id": expected["snapshot_id"], "world_id": leg["source_world_id"],
                             "sha256": expected["sha256"]}
        finally:
            original = sys.exc_info()[1]
            try:
                status, body = groups._call(server, "POST", "/v1/sessions/release", token=token, lease=lease,
                    payload={"api_version": 1, "character_id": character, "session_id": lease["session_id"],
                             "current_revision": lease["current_revision"], "session_epoch": lease["session_epoch"],
                             "client_instance_id": lease["client_instance_id"], "idempotency_key": str(uuid.uuid4())})
                groups._expect(status, body, 200, name + " head release")
            except Exception as cleanup:
                if original is None:
                    raise
                original.add_note(f"Head lease release also failed: {cleanup}")
    return checked


def bind_loaded_sources(plan: dict, pids: dict, desktops: dict, proof: dict) -> dict:
    """Close the leased-read/startup gap before any gameplay input."""
    result = {}
    for player in plan["players"]:
        name, pid = player["name"], pids[player["name"]]
        process = harness.psutil.Process(pid)
        if pid not in {p.pid for p in harness.psutil.Process(desktops[name]).children(recursive=True)}:
            raise harness.HarnessFailure(f"{name}: game is outside the owned signed desktop")
        roms = [arg for arg in process.cmdline() if arg.casefold().endswith(".gba")]
        if len(roms) != 1:
            raise harness.HarnessFailure(f"{name}: missing unique loaded ROM path")
        raw = roms[0]
        if raw.startswith("\\\\?\\"):
            raw = raw[4:]  # Ordinary Windows drive paths; UNC remains outside the local profile.
        folder = Path(raw).resolve().parent
        sessions = (Path(player["profile_localappdata"]) / "Hoenn Sessions/sessions").resolve()
        if not folder.is_relative_to(sessions) or folder == sessions:
            raise harness.HarnessFailure(f"{name}: loaded save folder is outside the isolated profile session")
        save = folder / "character.sav"
        harness.require_hash(save, proof["players"][name]["sha256"], name + " loaded source")
        result[name] = {"path": str(save), "sha256": proof["players"][name]["sha256"]}
    return result


def presence_regions(plan: dict, world_id: int) -> list:
    release, _ = harness._paths(plan)
    catalog = harness._read_json(release / "release_catalog.json", "selected group catalog")
    return next(w for w in catalog["worlds"] if w["world_id"] == world_id)["presence_regions"]


def run_leg(plan: dict, name: str) -> dict:
    leg = validate_inputs(plan, name)
    harness.preflight(plan)
    proof = preceding_proof(plan, leg)
    heads = guard_heads(plan, leg, proof)
    _, run_dir = harness._paths(plan)
    harness.checkpoint(run_dir, "selected-heads-guarded", {"leg": name, "players": heads})
    # Neither actor may still be grouped: the preceding group must have ended
    # with its session, so the group proved below is fresh for this session.
    ungrouped = groups.require_ungrouped(plan, name + " pre-pairing")
    code = groups.create_pairing_code(plan, name)
    harness.checkpoint(run_dir, "selected-pairing-code-issued", {"leg": name, "ungrouped": ungrouped,
                       **{k: code[k] for k in ("code_sha256", "expires_at_unix_ms", "inviter_character_id")}})
    desktops = harness.launch(plan)
    boundary = "selected-start"
    try:
        pids = controls.start_games(plan, desktops)
        bindings = bind_games(plan, leg, pids)
        loaded = bind_loaded_sources(plan, pids, desktops, proof)
        harness.checkpoint(run_dir, "selected-sources-bound", {"leg": name, "roms": bindings, "saves": loaded})
        boundary = "selected-pairing"
        before = groups.pair_desktops(plan, desktops, code, presence_regions(plan, leg["source_world_id"]), name)
        boundary = "selected-drive"
        harness.drive(plan, name, pids)
        arrived = harness.wait_arrival_games(plan, leg, pids)
        harness.continue_arrivals(plan, leg, arrived)
        group_id = groups.journal_group_id(plan, leg)
        if group_id == str(proof["group"]["group_id"]):
            raise harness.HarnessFailure("selected travel reused the preceding session's group; a fresh group is required")
        after = groups.partner_proof(plan, presence_regions(plan, leg["destination_world_id"]), name + " post-arrival")
        record = groups.group_record(plan, leg, group_id, before, after)
        harness.checkpoint(run_dir, "selected-group-proved", {"leg": name, "group_id": record["group_id"],
                           "region": record["world_zone"]["region"]})
        controls.stop_runtime(plan, desktops)
        boundary = "selected-evidence"
        collected = groups.collect(plan, name, record, capture_directory(run_dir, name, "server-evidence"), lease_wait_seconds=15)
        evidence_plan = dict(plan, group_evidence=collected["group"], evidence_roots=[str(capture_directory(run_dir, name, "server-evidence")), *plan.get("evidence_roots", [])])
        evidence_plan["legs"] = [dict(l, group_evidence=collected["group"]) if l["name"] == name else l for l in plan["legs"]]
        evidence = harness.discover_evidence(evidence_plan, name)
        result = harness.verify_leg(evidence_plan, name, evidence)
        harness.checkpoint(run_dir, "selected-leg-complete", {"leg": name})
        return result
    except Exception as error:
        try:
            harness.checkpoint(run_dir, "selected-leg-failed", {"leg": name, "boundary": boundary, "error": str(error)})
        except Exception as diagnostic:
            error.add_note(f"Selected checkpoint also failed: {diagnostic}")
        raise
    finally:
        original, errors = sys.exc_info()[1], []
        for cleanup in (lambda: controls.stop_runtime(plan, desktops), lambda: harness.close_desktops(desktops)):
            try:
                cleanup()
            except Exception as error:
                errors.append(error)
        if errors:
            if original is None:
                original = errors.pop(0)
                for error in errors:
                    original.add_note(f"Selected cleanup also failed: {error}")
                raise original
            for error in errors:
                original.add_note(f"Selected cleanup also failed: {error}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--leg", required=True)
    args = parser.parse_args()
    try:
        result = run_leg(harness._read_json(args.plan, "selected plan"), args.leg)
    except Exception as error:
        print(json.dumps({"ok": False, "error": str(error), "notes": getattr(error, "__notes__", [])}))
        return 1
    print(json.dumps({"ok": True, "result": result}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
