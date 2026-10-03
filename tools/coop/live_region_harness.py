#!/usr/bin/env python3
"""Test-only, resumable two-player journey over an installed signed ROM family.

The JSON plan names regions and portals; no Cormoria rule is embedded here.
`preflight` performs only reads, `launch` reuses installed profiles, `drive`
sends a bounded scripted input leg, and `verify` checks the resulting saves.
Every boundary writes a small checkpoint under the chosen spare-volume run dir.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import socket
import shutil
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from contextlib import ExitStack
from pathlib import Path
import psutil

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

from live_harness_oracles import (OracleFailure, check_projection,
                                 check_travel_witnesses, logical_field, read_flash)
from live_fixture_validation import validate_player_fixture
from live_save_capture import SaveCapture, capture_directory
from live_harness_windows import (Win32Adapter, capture_desktop, capture_game,
                                  click_desktop, launch_signed_desktop, tap,
                                  wait_game_window,
                                  WindowControlError)
from live_harness_windows import game_window, wait_desktop_window
from player_transfer_manifest import parse_schema_payload


class HarnessFailure(RuntimeError):
    pass


def _leg(plan: dict, leg_name: str) -> dict:
    """Return a named leg with a short, stable error for malformed plans."""
    legs = plan.get("legs")
    if not isinstance(legs, list):
        raise HarnessFailure("plan: legs must be an array")
    matches = [item for item in legs
               if isinstance(item, dict) and item.get("name") == leg_name]
    if len(matches) != 1:
        raise HarnessFailure(f"unknown leg {leg_name!r}")
    return matches[0]


def _validate_plan_shape(plan: dict) -> None:
    if not isinstance(plan, dict):
        raise HarnessFailure("plan must be a JSON object")
    required = ("release_id", "release_dir", "run_dir", "players", "legs")
    missing = [key for key in required if key not in plan]
    if missing:
        raise HarnessFailure(f"plan missing required fields: {', '.join(missing)}")
    if not isinstance(plan["players"], list) or len(plan["players"]) != 2:
        raise HarnessFailure("plan must contain exactly two players")
    if any(not isinstance(player, dict) or not isinstance(player.get("name"), str)
           or not player["name"] or not all(c.isalnum() or c in "-_" for c in player["name"])
           for player in plan["players"]):
        raise HarnessFailure("plan players must have simple filename-safe names")


def _read_json(path: Path, boundary: str) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise HarnessFailure(f"{boundary}: invalid JSON") from exc
    if not isinstance(value, dict):
        raise HarnessFailure(f"{boundary}: JSON root must be an object")
    return value


def digest(path: Path) -> str:
    sha = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def require_hash(path: Path, expected: str, boundary: str) -> None:
    if not path.is_file() or digest(path) != expected:
        raise HarnessFailure(f"{boundary}: missing or changed {path}")


def checkpoint(run_dir: Path, boundary: str, detail: dict) -> None:
    run_dir.mkdir(parents=True, exist_ok=True)
    path = run_dir / "checkpoint.json"
    try:
        old = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {"events": []}
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise HarnessFailure("checkpoint: existing checkpoint is not valid JSON") from exc
    if not isinstance(old, dict) or not isinstance(old.get("events", []), list):
        raise HarnessFailure("checkpoint: existing checkpoint has invalid shape")
    old["events"].append({"boundary": boundary, "time_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), **detail})
    temp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temp.write_text(json.dumps(old, indent=2) + "\n", encoding="utf-8")
    temp.replace(path)


def _paths(plan: dict) -> tuple[Path, Path]:
    release, run_dir = Path(plan["release_dir"]), Path(plan["run_dir"])
    if release.resolve() == run_dir.resolve() or run_dir.drive.casefold() == os.environ.get("SystemDrive", "C:").casefold():
        raise HarnessFailure("large run output must be on a volume other than C:")
    return release, run_dir


def check_c_space() -> int:
    free = shutil.disk_usage(os.environ.get("SystemDrive", "C:") + "\\").free
    if free < 2 * 1024**3:
        raise HarnessFailure("C: free space below 2 GiB; stopping live harness")
    return free


def _artifact_relative(ident: str) -> str:
    paths = {"desktop-app": "app/coop-launcher.exe", "managed-mgba": "runtime/mgba.exe",
             "rom": "runtime/game.gba", "sidecar": "runtime/coop-sidecar.exe",
             "bridge-main": "bridge/main.lua", "bridge-memory": "bridge/memory.lua",
             "bridge-protocol": "bridge/protocol.lua", "bridge-addresses": "bridge/generated_addresses.lua",
             "compatibility-manifest": "bridge_manifest.json", "trust-bundle": "trust/release-trust.json",
             "notices": "THIRD_PARTY_NOTICES.txt", "region-catalog": "release_catalog.json"}
    if ident.startswith("world-"):
        parts = ident.split("-", 2)
        suffix = {"rom": "game.gba", "compatibility": "bridge_manifest.json", "player-transfer": "player_transfer.json"}
        if len(parts) != 3 or not parts[1].isdigit() or parts[2] not in suffix:
            raise HarnessFailure(f"unknown signed artifact {ident}")
        return f"worlds/{parts[1]}/{suffix[parts[2]]}"
    if ident not in paths:
        raise HarnessFailure(f"unknown signed artifact {ident}")
    return paths[ident]


def require_plain_cache_path(path: Path) -> None:
    """Match the signed updater's rejection of symlink/reparse components."""
    import stat
    for node in (path, *path.parents):
        metadata = node.lstat()
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise HarnessFailure(f"installed cache contains a symlink/reparse component: {node}")
    if os.name == "nt":
        try:
            path.resolve(strict=True)
        except OSError as exc:
            raise HarnessFailure("installed cache cannot be canonicalized by Windows") from exc


def check_installed_family(plan: dict, cache: Path) -> None:
    require_plain_cache_path(cache)
    release, _ = _paths(plan)
    envelope = _read_json(release / "release-envelope.json", "fixture envelope")
    payload = base64.b64decode(envelope["payload"], validate=True)
    signed = json.loads(payload)
    marker = cache / ".complete"
    if not marker.is_file():
        raise HarnessFailure("installed fixture is not complete")
    require_plain_cache_path(marker)
    require_plain_cache_path(cache / ".signed-release")
    expected = {key: str(signed[key]) for key in ("schema", "platform", "release_id", "sequence", "issued_at", "expires_at")}
    expected["payload_sha256"] = hashlib.sha256(payload).hexdigest()
    lines = marker.read_text(encoding="utf-8").splitlines()
    if lines != [f"{key}={value}" for key, value in expected.items()]:
        raise HarnessFailure("installed completion marker differs from signed fixture")
    require_hash(cache / ".signed-release", plan["envelope_sha256"], "installed signed envelope")
    for artifact in signed["artifacts"]:
        require_plain_cache_path(cache / _artifact_relative(artifact["id"]))
        require_hash(cache / _artifact_relative(artifact["id"]), artifact["sha256"],
                     "installed signed artifact " + artifact["id"])


def accepted_family_ready(cache: Path) -> bool:
    """Require the signed client's own durable acceptance, never fabricate it.

    Call after check_installed_family: copied .complete bytes alone do not make
    a fresh profile accepted. The first signed client writes these receipts and
    exits with restart code10; the next launch can start the ROM.
    """
    import re
    expected_bytes = (cache / ".complete").read_bytes()
    expected = dict(line.split("=", 1) for line in expected_bytes.decode().splitlines())
    # Production also heals orphan markers before selecting the highest
    # sequence. A newer orphan must not let an older valid head pass here.
    markers = list(cache.parent.glob(".accepted-generation-*"))
    if len(markers) > 64:
        raise HarnessFailure("too many accepted-generation records for bounded harness")
    for marker in markers:
        require_plain_cache_path(marker)
        if not re.fullmatch(r"\.accepted-generation-[0-9a-f]{32}", marker.name) or marker.stat().st_size > 512:
            raise HarnessFailure("invalid accepted-generation path or size")
        data = marker.read_bytes()
        pairs = [line.split("=", 1) for line in data.decode().splitlines()]
        if any(len(pair) != 2 for pair in pairs):
            raise HarnessFailure("malformed accepted-generation record")
        body = dict(pairs)
        if (len(body) != len(pairs) or set(body) != set(expected)
                or not re.fullmatch(r"[0-9]{1,20}", body.get("sequence", ""))
                or int(body["sequence"]) > 0xFFFFFFFFFFFFFFFF):
            raise HarnessFailure("malformed accepted-generation fields")
        # CompletionMarker::validate_structure and its bounded integer/hash
        # parser, including older unreferenced records which production reads.
        release_id = body.get("release_id", "")
        if (body.get("schema") != "1" or body.get("platform") != "windows-x86_64"
                or not re.fullmatch(r"[A-Za-z0-9._-]{1,128}", release_id) or release_id in (".", "..")
                or not re.fullmatch(r"[0-9a-fA-F]{64}", body.get("payload_sha256", ""))
                or any(not re.fullmatch(r"-?[0-9]{1,19}", body.get(k, "")) for k in ("issued_at", "expires_at"))):
            raise HarnessFailure("malformed accepted-generation structure")
        issued, expires = int(body["issued_at"]), int(body["expires_at"])
        if (not -(1 << 63) <= issued < (1 << 63) or not -(1 << 63) <= expires < (1 << 63)
                or not 0 < expires - issued <= 90 * 24 * 60 * 60):
            raise HarnessFailure("malformed accepted-generation validity window")
        if int(body["sequence"]) >= int(expected["sequence"]) and (body != expected or data != expected_bytes):
            raise HarnessFailure("accepted marker selects another signed family, possibly orphaned")
        canonical = "".join(f"{k}={body[k]}\n" for k in (
            "schema", "platform", "release_id", "sequence", "issued_at", "expires_at", "payload_sha256")).encode()
        generation = cache.parent / release_id
        complete = generation / ".complete"
        try:
            require_plain_cache_path(generation)
            require_plain_cache_path(complete)
            valid_complete = complete.stat().st_size <= 512 and complete.read_bytes() == canonical
        except OSError as exc:
            raise HarnessFailure("accepted marker's complete generation is missing") from exc
        if data != canonical or not valid_complete:
            raise HarnessFailure("accepted marker differs from its complete generation")
    heads = [p for p in cache.parent.glob(".accepted-head-*")
             if not p.name.startswith(".accepted-head-staging-")]
    if len(heads) > 64:
        raise HarnessFailure("too many accepted-head records for bounded harness")
    matches = False
    for head in heads:
        require_plain_cache_path(head)
        if not re.fullmatch(r"\.accepted-head-[0-9a-f]{32}", head.name) or head.stat().st_size > 1024:
            raise HarnessFailure("invalid accepted-head path or size")
        pairs = [line.split("=", 1) for line in head.read_text().splitlines()]
        if any(len(pair) != 2 for pair in pairs):
            raise HarnessFailure("malformed accepted-head record")
        values = dict(pairs)
        if len(values) != len(pairs) or set(values) != set(expected) | {"marker_file", "marker_sha256"}:
            raise HarnessFailure("ambiguous accepted-head fields")
        if not re.fullmatch(r"\.accepted-generation-[0-9a-f]{32}", values["marker_file"]):
            raise HarnessFailure("accepted-head marker left generation root")
        marker = cache.parent / values["marker_file"]
        require_plain_cache_path(marker)
        data = marker.read_bytes()
        if len(data) > 512 or hashlib.sha256(data).hexdigest() != values["marker_sha256"]:
            raise HarnessFailure("accepted-generation marker hash changed")
        body = {k: v for k, v in values.items() if k in expected}
        if data.decode().splitlines() != [f"{k}={v}" for k, v in body.items()]:
            raise HarnessFailure("accepted-head and marker disagree")
        if not values["sequence"].isdigit():
            raise HarnessFailure("accepted-head sequence is invalid")
        if int(values["sequence"]) >= int(expected["sequence"]):
            if body != expected or data != expected_bytes:
                raise HarnessFailure("accepted head selects another signed family")
            matches = True
    return matches


def _fixture_key(plan: dict, release: Path) -> str:
    return hashlib.sha256((digest(release / "release-envelope.json")
                           + digest(release / "server-build-catalog.json")
                           + "".join(p["source_sha256"] for p in plan["players"])).encode()).hexdigest()


def preflight(plan: dict, *, profiles_ready: bool = True, require_live_space: bool = True) -> dict:
    if type(require_live_space) is not bool:
        raise HarnessFailure("require_live_space must be a boolean")
    _validate_plan_shape(plan)
    release, run_dir = _paths(plan)
    if require_live_space:
        check_c_space()
    envelope_path = release / "release-envelope.json"
    envelope = _read_json(envelope_path, "release envelope")
    try:
        payload = base64.b64decode(envelope["payload"], validate=True)
        Ed25519PublicKey.from_public_bytes(bytes.fromhex(plan["release_public_key_hex"])).verify(
            base64.b64decode(envelope["signature"], validate=True), payload)
        signed = json.loads(payload)
    except (KeyError, ValueError, TypeError, json.JSONDecodeError, InvalidSignature) as exc:
        raise HarnessFailure("release envelope: signature or payload is invalid") from exc
    if not isinstance(signed, dict):
        raise HarnessFailure("release envelope: signed payload must be an object")
    if signed["release_id"] != plan["release_id"]:
        raise HarnessFailure("signed release ID changed")
    if digest(envelope_path) != plan["envelope_sha256"]:
        raise HarnessFailure("signed envelope bytes changed")
    for artifact in signed["artifacts"]:
        ident = artifact["id"]
        relative = _artifact_relative(ident)
        path = release / relative
        if not path.is_file():
            raise HarnessFailure(f"signed artifact missing: {ident}")
        if path.stat().st_size != artifact["size"]:
            raise HarnessFailure(f"signed artifact size changed: {ident}")
        require_hash(path, artifact["sha256"], f"signed artifact {ident}")
    catalog_path = release / "release_catalog.json"
    require_hash(catalog_path, plan["catalog_sha256"], "release catalog")
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    server_catalog_path = release / "server-build-catalog.json"
    require_hash(server_catalog_path, plan["server_catalog_sha256"], "server build catalog")
    server_catalog = json.loads(server_catalog_path.read_text(encoding="utf-8"))
    descriptor = bytes.fromhex(server_catalog["shared_player_descriptor_hex"])
    parse_schema_payload(descriptor)
    worlds = {int(world["world_id"]): world for world in catalog["worlds"]}
    for world_id, world in worlds.items():
        require_hash(release / world["rom_path"], world["rom_sha256"], f"world {world_id} ROM")
        for arrival in world["arrivals"].values():
            template = release / arrival["template_sav_path"]
            require_hash(template, arrival["template_sav_sha256"], f"world {world_id} arrival")
            read_flash(template)
    players = plan["players"]
    if len(players) != 2 or players[0]["character_id"] == players[1]["character_id"]:
        raise HarnessFailure("exactly two distinct characters are required")
    source_saves, witness_receipts = [], []
    for player in players:
        source = Path(player["source_save"])
        require_hash(source, player["source_sha256"], f"player {player['name']} source")
        source_saves.append(read_flash(source))
        witness_receipts.append(validate_player_fixture(source_saves[-1], descriptor, player))
        if "destination_base_save" in player:
            base = Path(player["destination_base_save"])
            require_hash(base, player["destination_base_sha256"],
                         f"player {player['name']} dormant destination")
            read_flash(base)
        if profiles_ready:
            profile = Path(player["profile_localappdata"]) / "Hoenn Sessions"
            account = json.loads((profile / "account.json").read_text(encoding="utf-8"))
            if account.get("character_id") != player["character_id"]:
                raise HarnessFailure(f"player {player['name']}: installed profile belongs to another character")
            cache = profile / "runtime/releases/generations" / plan["release_id"]
            check_installed_family(plan, cache)
            if not accepted_family_ready(cache):
                raise HarnessFailure("signed client has not accepted the copied fixture; run prepare")
    # The server binds each save to its account's character ID. A matching
    # in-game trainer name/ID is legal; require an independently observable
    # shared value so a swapped player cannot pass by comparing identical data.
    money_values = [logical_field(s, descriptor, 0x0102) for s in source_saves]
    if source_saves[0].sha256 == source_saves[1].sha256 or money_values[0] == money_values[1]:
        raise HarnessFailure("players need distinct ROM saves and money sentinels")
    exe = release / "app/coop-launcher.exe"
    require_hash(exe, plan["desktop_sha256"], "signed desktop")
    identity = _fixture_key(plan, release)
    result = {"fixture_key": identity, "release_id": plan["release_id"], "profiles_checked": profiles_ready,
              "worlds": {str(i): w["rom_sha256"] for i, w in worlds.items()},
              "players": [{"name": p["name"], "character_id": p["character_id"],
                           "lineage": s.lineage.hex(), "generation": s.generation,
                           "money_sentinel_hex": value.hex(),
                           "source_sha256": s.sha256, "shared_witnesses": witnesses}
                          for p, s, value, witnesses in zip(players, source_saves, money_values, witness_receipts)]}
    checkpoint(run_dir, "preflight", result)
    return result


def _journal_candidates(profile: Path, leg: dict, character_id: str) -> list[tuple[int, Path, dict]]:
    root = profile / "Hoenn Sessions"
    candidates: list[tuple[int, Path, dict]] = []
    for directory in (root / "paired-travel", root / "travel"):
        if not directory.is_dir():
            continue
        for path in directory.glob("*.json"):
            try:
                journal = _read_json(path, "travel journal")
            except HarnessFailure:
                continue
            if journal.get("character_id") != character_id:
                continue
            if journal.get("phase") not in ("adopted", "committed"):
                continue
            intent = journal.get("intent", {})
            request = intent.get("request", {}) if isinstance(intent, dict) else {}
            stage = journal.get("stage", {})
            if (intent.get("source_world_id") != leg.get("source_world_id")
                    or request.get("portal_id") != leg.get("portal_id")
                    or stage.get("destination_world_id") != leg.get("destination_world_id")
                    or not stage.get("destination_save_sha256")):
                continue
            sequence = journal.get("sequence", -1)
            if type(sequence) is not int or sequence < 0:
                sequence = -1
            candidates.append((sequence, path, journal))
    return sorted(candidates, key=lambda item: (item[0], item[1].name), reverse=True)


def _save_candidates(roots: list[Path], expected_sha256: str, *, limit: int = 256) -> list[Path]:
    """Find a staged save by hash without reading server state or credentials."""
    matches: list[Path] = []
    seen: set[str] = set()
    for root in roots:
        if not root.is_dir():
            continue
        try:
            paths = root.rglob("*.sav")
        except OSError:
            continue
        for path in paths:
            if len(seen) >= limit:
                return matches
            try:
                resolved = str(path.resolve()).casefold()
                if resolved in seen or path.is_symlink() or not path.is_file():
                    continue
                seen.add(resolved)
                if path.stat().st_size not in (128 * 1024, 128 * 1024 + 16):
                    continue
                if digest(path) == expected_sha256:
                    read_flash(path)
                    matches.append(path)
            except (OSError, OracleFailure):
                continue
    return matches


def _arrival_template(release: Path, catalog: dict, leg: dict) -> Path:
    worlds = {int(world["world_id"]): world for world in catalog.get("worlds", [])}
    try:
        destination = worlds[int(leg["destination_world_id"])]
        source = worlds[int(leg["source_world_id"])]
    except (KeyError, TypeError, ValueError) as exc:
        raise HarnessFailure("evidence discovery: leg world is not in release catalog") from exc
    arrivals = destination.get("arrivals", {})
    key = f"from_{source.get('name')}"
    arrival = arrivals.get(key)
    if arrival is None and len(arrivals) == 1:
        arrival = next(iter(arrivals.values()))
    if not isinstance(arrival, dict) or not arrival.get("template_sav_path"):
        raise HarnessFailure(
            f"evidence discovery: no arrival template for world {leg['destination_world_id']}")
    path = release / arrival["template_sav_path"]
    require_hash(path, arrival.get("template_sav_sha256", ""), "arrival template")
    return path


def _sanitize_group_evidence(group: dict) -> dict:
    """Keep only fields consumed by the oracle; drop auth/session material."""
    members = group.get("members")
    zone = group.get("world_zone")
    if not isinstance(group.get("group_id"), str) or not isinstance(members, list) or not isinstance(zone, dict):
        raise HarnessFailure("group evidence: missing group ID, members, or world zone")
    safe_members = []
    for member in members:
        if not isinstance(member, dict) or not isinstance(member.get("character_id"), str):
            raise HarnessFailure("group evidence: invalid member")
        safe_members.append({"character_id": member["character_id"]})
    if not isinstance(zone.get("region"), str):
        raise HarnessFailure("group evidence: missing world-zone region")
    return {"group_id": group["group_id"], "members": safe_members,
            "world_zone": {"region": zone["region"]}}


def _destination_base(plan: dict, leg: dict, player: dict, template: Path) -> tuple[Path, str, str]:
    """Resolve a parked world from the exact source of an earlier journey leg."""
    if leg.get("destination_base_leg"):
        previous = _leg(plan, leg["destination_base_leg"])
        if (previous["source_world_id"] != leg["destination_world_id"]
                or previous["destination_world_id"] != leg["source_world_id"]):
            raise HarnessFailure("destination base leg does not reverse this world pair")
        release, run_dir = _paths(plan)
        record = _read_json(capture_directory(run_dir, previous["name"], "evidence")
                            / "evidence.json", "parked-world evidence")
        item = record.get("players", {}).get(player["name"], {})
        # The client retires old journals after another crossing. Its exact
        # source was already checked by the preceding leg's durable oracle.
        events = _read_json(run_dir / "checkpoint.json", "parked-world checkpoint").get("events", [])
        verified = [event for event in events
                    if event.get("boundary") == previous["name"] + "-verified"
                    and event.get("leg") == previous["name"]]
        if not verified:
            raise HarnessFailure("parked world has no verified preceding leg")
        report = verified[-1]
        if (report.get("fixture_key") != _fixture_key(plan, release)
                or any(report.get(key) != previous.get(key)
                       for key in ("source_world_id", "destination_world_id", "portal_id"))):
            raise HarnessFailure("parked-world receipt belongs to another fixture or world pair")
        actors = [actor for actor in report.get("players", []) if actor.get("name") == player["name"]]
        members = {member.get("character_id") for member in report.get("group", {}).get("members", [])}
        if len(actors) != 1 or members != {p["character_id"] for p in plan["players"]}:
            raise HarnessFailure("parked world belongs to another character or world")
        actor = actors[0]
        if actor.get("character_id") != player["character_id"]:
            raise HarnessFailure("parked-world receipt belongs to another character")
        sha = actor.get("journal_source_sha256")
        if (actor.get("exact_journal_source_inspected") is not True
                or actor.get("source_sha256") != sha or item.get("journal_source_sha256") != sha):
            raise HarnessFailure("parked world lacks verified exact source")
        path = Path(item.get("source", ""))
        require_hash(path, sha, "exact parked-world source")
        return path, "dormant", sha
    if "destination_base_save" in player:
        return Path(player["destination_base_save"]), "dormant", player["destination_base_sha256"]
    return template, "signed_template", digest(template)


def _retain_journal(path: Path, journal: dict, captured: Path) -> dict:
    """Retain private evidence before the client retires its live journal."""
    data = path.read_bytes()
    if json.loads(data) != journal:
        raise HarnessFailure("journal changed during discovery")
    sha = hashlib.sha256(data).hexdigest()
    target = captured / (sha + ".journal.json")
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        require_hash(target, sha, "retained travel journal")
    else:
        with target.open("xb") as output:
            output.write(data)
    return {"journal": str(target), "journal_sha256": sha}


def _read_evidence_journal(item: dict, boundary: str) -> dict:
    if item.get("journal_sha256"):
        require_hash(Path(item["journal"]), item["journal_sha256"], boundary)
    return _read_json(Path(item["journal"]), boundary)


def discover_evidence(plan: dict, leg_name: str, *, require_live_space: bool = True) -> dict:
    """Discover local journal/save paths and leave authenticated group proof explicit.

    The scan is read-only. It never calls an authenticated endpoint, advances a
    lease, or emits journal contents (which include session and nonce material).
    """
    preflight(plan, require_live_space=require_live_space)
    release, run_dir = _paths(plan)
    leg = _leg(plan, leg_name)
    catalog = _read_json(release / "release_catalog.json", "release catalog")
    template = _arrival_template(release, catalog, leg)
    roots = [Path(item) for item in plan.get("evidence_roots", []) if isinstance(item, str)]
    roots.append(run_dir)
    evidence: dict = {"leg": leg_name, "players": {}, "group": None}
    for player in plan["players"]:
        name, profile = player["name"], Path(player["profile_localappdata"])
        journals = _journal_candidates(profile, leg, player["character_id"])
        if not journals:
            raise HarnessFailure(f"{leg_name}/{name}: no committed travel journal found")
        _, journal_path, journal = journals[0]
        expected_sha = journal["stage"]["destination_save_sha256"]
        captured = capture_directory(run_dir, leg_name, name)
        search_roots = [captured, profile / "Hoenn Sessions", *roots, release]
        saves = _save_candidates(search_roots, expected_sha)
        if not saves:
            raise HarnessFailure(f"{leg_name}/{name}: no local save matches journal stage digest")
        source = Path(player["source_save"])
        require_hash(source, player["source_sha256"], f"player {name} source")
        journal_source_sha = journal.get("intent", {}).get("source_save_sha256")
        if not isinstance(journal_source_sha, str) or len(journal_source_sha) != 64:
            raise HarnessFailure(f"{leg_name}/{name}: journal has no source digest")
        exact = _save_candidates(search_roots, journal_source_sha)
        if exact:
            source = exact[0]
        elif not leg.get("source_is_baseline"):
            raise HarnessFailure(f"{leg_name}/{name}: exact journal source was not captured")
        destination_base, base_kind, _ = _destination_base(plan, leg, player, template)
        # Keep a private durable copy: the next travel can delete the live
        # journal. Never print it; it contains session and nonce material.
        retained = _retain_journal(journal_path, journal, captured)
        evidence["players"][name] = {
            "source": str(source), "staged": str(saves[0]),
            "template": str(destination_base), **retained,
            "destination_base_kind": base_kind,
            "journal_source_sha256": journal_source_sha,
            "source_is_baseline": bool(leg.get("source_is_baseline")),
        }
    group_path = leg.get("group_evidence", plan.get("group_evidence"))
    if group_path:
        group = _read_json(Path(group_path), "group evidence")
        evidence["group"] = _sanitize_group_evidence(group)
    checkpoint(run_dir, f"{leg_name}-evidence-discovered", {
        "players": sorted(evidence["players"]), "group_evidence": bool(group_path),
    })
    evidence_path = capture_directory(run_dir, leg_name, "evidence") / "evidence.json"
    evidence_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = evidence_path.with_suffix(".tmp")
    temporary.write_text(json.dumps(evidence, indent=2), encoding="utf-8")
    temporary.replace(evidence_path)
    return evidence


def _health_url(plan: dict) -> tuple[str, urllib.parse.ParseResult]:
    base = plan.get("server_url") or os.environ.get("COOP_HARNESS_SERVER_URL")
    if not isinstance(base, str) or not base.strip():
        raise HarnessFailure("server-check: set plan.server_url or COOP_HARNESS_SERVER_URL")
    parsed = urllib.parse.urlparse(base.strip())
    if parsed.scheme not in ("http", "https") or not parsed.hostname or parsed.username or parsed.password:
        raise HarnessFailure("server-check: server URL must be an http(s) URL without credentials")
    if parsed.query or parsed.fragment:
        raise HarnessFailure("server-check: server URL must not contain query or fragment")
    host = parsed.hostname.casefold()
    loopback = host in {"localhost", "127.0.0.1", "::1"}
    if not loopback and not plan.get("allow_remote_server_checks", False):
        raise HarnessFailure("server-check: remote host requires allow_remote_server_checks")
    endpoint = urllib.parse.urlunparse((parsed.scheme, parsed.netloc, "/health/ready", "", "", ""))
    return endpoint, parsed


def check_server(plan: dict) -> dict:
    """Perform bounded GET health checks only; never sends lease-bearing requests."""
    _validate_plan_shape(plan)
    _, run_dir = _paths(plan)
    endpoint, parsed = _health_url(plan)
    retries = plan.get("server_check_retries", 3)
    if type(retries) is not int or not 1 <= retries <= 4:
        raise HarnessFailure("server-check: retries must be an integer from 1 to 4")
    last_error = "unreachable"
    for attempt in range(1, retries + 1):
        request = urllib.request.Request(endpoint, method="GET", headers={"Accept": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=3.0) as response:
                status = int(response.status)
                if 200 <= status < 300:
                    result = {"method": "GET", "path": "/health/ready", "host": parsed.hostname,
                              "status": status, "ready": True, "attempts": attempt}
                    checkpoint(run_dir, "server-health-checked", result)
                    return result
                last_error = f"HTTP {status}"
        except urllib.error.HTTPError as exc:
            last_error = f"HTTP {exc.code}"
            if exc.code < 500:
                break
        except (urllib.error.URLError, TimeoutError, socket.timeout, ConnectionError, OSError):
            last_error = "transport error"
        if attempt < retries:
            time.sleep(0.25 * attempt)
    raise HarnessFailure(f"server-check: /health/ready {last_error} after {retries} read-only attempt(s)")


def launch(plan: dict) -> dict:
    preflight(plan)
    release, run_dir = _paths(plan)
    checkpoint_orphans(plan)
    exe = release / "app/coop-launcher.exe"
    pids = {}
    try:
        for player in plan["players"]:
            log = run_dir / "logs" / f"desktop-{player['name']}-{time.time_ns()}.log"
            proc = launch_signed_desktop(exe, Path(player["profile_localappdata"]), output_log=log)
            pids[player["name"]] = proc.pid
            checkpoint(run_dir, "signed-client-launched", {"desktop_pids": dict(pids),
                                                          "output_log": str(log)})
        checkpoint(run_dir, "signed-clients-launched", {"desktop_pids": pids})
    except Exception as primary_error:
        try:
            close_desktops(pids)
        except Exception as cleanup_error:
            primary_error.add_note(f"Partial launch cleanup also failed: {cleanup_error}")
        raise
    return pids


def checkpoint_orphans(plan: dict) -> None:
    """Retire only byte-identical orphan SAVs from a prior interrupted test.

    Rename is reversible. A marked recovery candidate is left to the signed
    client; a changed save stops here because the server head may be stale.
    """
    _, run_dir = _paths(plan)
    for player in plan["players"]:
        sessions = Path(player["profile_localappdata"]) / "Hoenn Sessions/sessions"
        if not sessions.is_dir():
            continue
        for path in sessions.glob("coop-session-*"):
            save = path / "character.sav"
            if not save.exists() or (path / "recovery.marker").exists():
                continue
            if digest(save) != player["source_sha256"]:
                raise HarnessFailure(f"{player['name']}: orphan session differs from pinned server head: {save}")
            if any(str(path).casefold() in " ".join(proc.info["cmdline"] or []).casefold()
                   for proc in psutil.process_iter(["cmdline"])):
                raise HarnessFailure(f"{player['name']}: orphan session is still in use")
            target = path.with_name("checkpoint-" + path.name)
            if target.exists():
                raise HarnessFailure(f"{player['name']}: checkpoint already exists: {target}")
            path.rename(target)
            checkpoint(run_dir, "orphan-session-checkpointed",
                       {"player": player["name"], "path": str(target), "save_sha256": digest(save if save.exists() else target / "character.sav")})


def _mgba_pid_for(profile: Path) -> int | None:
    matches = []
    try:
        processes = psutil.process_iter(["name", "cmdline"])
        for proc in processes:
            try:
                if (proc.info["name"] or "").casefold() != "mgba.exe":
                    continue
                command = " ".join(proc.info["cmdline"] or []).casefold()
                if str(profile).casefold() in command and "--script" in command:
                    matches.append(proc.pid)
            except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
                continue
    except (psutil.Error, OSError) as exc:
        raise HarnessFailure(f"cannot inspect mGBA processes: {type(exc).__name__}") from exc
    if len(matches) > 1:
        raise HarnessFailure(f"multiple signed mGBA sessions for {profile}")
    return matches[0] if matches else None


def start_games(plan: dict, desktop_pids: dict[str, int]) -> dict:
    """Press Play in each already installed client and identify its exact mGBA."""
    _, run_dir = _paths(plan)
    adapter = Win32Adapter()
    game_pids = {}
    timeout = plan.get("start_timeout_seconds", 45)
    if type(timeout) is not int or not 1 <= timeout <= 180:
        raise HarnessFailure("start timeout must be an integer within [1, 180] seconds")
    for player in plan["players"]:
        name, profile = player["name"], Path(player["profile_localappdata"])
        desktop_pid = desktop_pids.get(name)
        if type(desktop_pid) is not int or desktop_pid <= 0:
            raise HarnessFailure(f"{name}: invalid signed desktop PID")
        game_pid = _mgba_pid_for(profile)
        if game_pid is None:
            wait_desktop_window(desktop_pid, adapter, timeout=20)
            click_desktop(desktop_pid, 88, 178, adapter)
            deadline = time.monotonic() + timeout
            next_click = time.monotonic() + 1
            while (game_pid := _mgba_pid_for(profile)) is None:
                try:
                    alive = psutil.pid_exists(desktop_pid)
                except (psutil.Error, OSError):
                    alive = False
                if not alive:
                    raise HarnessFailure(f"{name}: signed desktop exited before mGBA opened")
                if time.monotonic() >= next_click:
                    # Activation can consume a click; cached-account refresh
                    # can also leave Play disabled briefly. The signed
                    # controller disables Play once startup begins, so bounded
                    # repeat clicks cannot create a second game session.
                    click_desktop(desktop_pid, 88, 178, adapter)
                    next_click = time.monotonic() + 1
                if time.monotonic() >= deadline:
                    image = capture_desktop(desktop_pid, run_dir / "screenshots",
                                            f"startup-failed-{name}", adapter)
                    raise HarnessFailure(
                        f"{name}: signed desktop did not start mGBA within {timeout} seconds; status image {image}")
                time.sleep(0.25)
        arguments = psutil.Process(game_pid).cmdline()
        roms = [Path(arg) for arg in arguments if arg.casefold().endswith(".gba")]
        if len(roms) != 1:
            raise HarnessFailure(f"{name}: signed mGBA must name exactly one ROM")
        wait_game_window(game_pid, adapter, timeout=30, rom_title=_rom_header_title(roms[0]))
        if not psutil.pid_exists(game_pid):
            raise HarnessFailure(f"{name}: mGBA exited after opening its window")
        game_pids[name] = game_pid
    checkpoint(run_dir, "signed-games-started", {"game_pids": game_pids})
    return game_pids


def _rom_header_title(path: Path) -> str:
    with path.open("rb") as rom:
        rom.seek(0xA0)
        title = rom.read(12).rstrip(b"\0").decode("ascii")
    if not title:
        raise HarnessFailure("launched ROM has no header title")
    return title


def drive(plan: dict, leg_name: str, pids: dict[str, int]) -> dict:
    """Capture exact checkpoint bytes throughout every blocking input action."""
    preflight(plan)
    _, run_dir = _paths(plan)
    _leg(plan, leg_name)
    with ExitStack() as stack:
        captures = [stack.enter_context(SaveCapture(
            Path(player["profile_localappdata"]) / "Hoenn Sessions/sessions",
            capture_directory(run_dir, leg_name, player["name"]),
        )) for player in plan["players"]]
        result = _drive_inputs(plan, leg_name, pids, captures)
    return result


def _drive_inputs(plan: dict, leg_name: str, pids: dict[str, int], captures: list) -> dict:
    """Run bounded inputs and stop on the first missing live travel receipt."""
    preflight(plan)
    release, run_dir = _paths(plan)
    leg = _leg(plan, leg_name)
    if not leg.get("inputs"):
        raise HarnessFailure(f"{leg_name}: no bounded input script is recorded")
    adapter = Win32Adapter()
    screenshots = run_dir / "screenshots"
    profiles = {player["name"]: player for player in plan["players"]}
    baseline = {
        name: max((item[0] for item in _journal_candidates(
            Path(player["profile_localappdata"]), leg, player["character_id"]
        )), default=-1)
        for name, player in profiles.items()
    }
    receipts: dict[str, int] = {}
    missing_capture: dict[str, str] = {}
    for index, action in enumerate(leg["inputs"]):
        check_c_space()
        for capture in captures:
            capture.check()
        player = action["player"]
        if player not in pids or type(pids[player]) is not int or pids[player] <= 0:
            raise HarnessFailure(f"{leg_name}: missing mGBA PID for {player}")
        if not psutil.pid_exists(pids[player]):
            raise HarnessFailure(f"{leg_name}/{player}: mGBA exited before input {index}")
        tap(pids[player], action["key"], adapter,
            hold=action.get("hold_ms", 80) / 1000,
            release=action.get("release_ms", 80) / 1000)
        if action.get("wait_ms"):
            time.sleep(min(action["wait_ms"], 30000) / 1000)
        if action.get("screenshot"):
            capture_game(pids[player], screenshots, f"{leg_name}-{index}-{player}", adapter)
        if action.get("expect_presence_published"):
            require_presence_published(pids[player], release, leg["source_world_id"],
                                run_dir, f"{leg_name}-{index}-{player}")
        expected = action.get("expect_travel", [])
        if expected is True:
            expected = list(profiles)
        if expected:
            if not isinstance(expected, list) or not expected or any(
                name not in profiles for name in expected
            ):
                raise HarnessFailure(f"{leg_name}: invalid expect_travel at input {index}")
            timeout_ms = action.get("timeout_ms", 60000)
            if type(timeout_ms) is not int or not 1000 <= timeout_ms <= 120000:
                raise HarnessFailure(f"{leg_name}: invalid travel timeout at input {index}")
            deadline = time.monotonic() + timeout_ms / 1000
            while True:
                check_c_space()
                for capture in captures:
                    capture.check()
                for name in expected:
                    journals = _journal_candidates(
                        Path(profiles[name]["profile_localappdata"]), leg,
                        profiles[name]["character_id"],
                    )
                    if journals and journals[0][0] > baseline[name]:
                        journal = journals[0][2]
                        folder = capture_directory(run_dir, leg_name, name)
                        for boundary, sha in (
                            ("source", journal.get("intent", {}).get("source_save_sha256")),
                            ("staged", journal.get("stage", {}).get("destination_save_sha256")),
                        ):
                            if not isinstance(sha, str) or len(sha) != 64:
                                raise HarnessFailure(f"{leg_name}/{name}: missing {boundary} digest")
                            saved = folder / (sha + ".sav")
                            if not saved.is_file():
                                missing_capture[name] = boundary
                                # Arrival file may appear shortly after commit;
                                # deadline below remains bounded and diagnostic.
                                break
                            require_hash(saved, sha, f"{leg_name}/{name} captured {boundary}")
                        else:
                            missing_capture.pop(name, None)
                            receipts[name] = journals[0][0]
                if all(name in receipts for name in expected):
                    checkpoint(run_dir, f"{leg_name}-travel-receipt", {
                        "after_input": index, "players": sorted(expected),
                        "journal_sequences": {name: receipts[name] for name in expected},
                    })
                    break
                if time.monotonic() >= deadline:
                    missing = [name for name in expected if name not in receipts]
                    for name in missing:
                        if psutil.pid_exists(pids[name]):
                            capture_game(pids[name], screenshots,
                                         f"{leg_name}-missing-receipt-{name}", adapter)
                            try:
                                _capture_bridge_diagnostic(
                                    pids[name], release / "worlds" / str(leg["source_world_id"])
                                    / "bridge_manifest.json",
                                    run_dir / "diagnostics" / f"{leg_name}-{name}-bridge.json",
                                )
                            except (OSError, RuntimeError, ValueError, KeyError):
                                # The missing travel receipt is the primary
                                # failure; an optional memory probe must not hide it.
                                pass
                    raise HarnessFailure(
                        f"{leg_name}: no fresh signed-client travel journal for {missing} "
                        f"within {timeout_ms} ms; missing exact captures {missing_capture} "
                        "(live-window diagnostics attempted)")
                time.sleep(0.25)
    if set(receipts) != set(profiles):
        raise HarnessFailure(f"{leg_name}: itinerary did not prove travel for both players")
    checkpoint(run_dir, f"{leg_name}-inputs-sent", {
        "count": len(leg["inputs"]), "travel_receipts": receipts,
    })
    return {"inputs": len(leg["inputs"]), "travel_receipts": receipts}


def _capture_bridge_diagnostic(pid: int, manifest_path: Path, output: Path) -> dict:
    """Record read-only bridge state for a failed live input boundary."""
    import ctypes
    from live_bridge_probe_windows import KERNEL32, find_bridge, snapshot

    manifest = _read_json(manifest_path, "bridge manifest")
    handle = KERNEL32.OpenProcess(0x0410, False, pid)
    if not handle:
        raise OSError(ctypes.get_last_error(), "OpenProcess")
    try:
        states = [snapshot(handle, address, manifest)
                  for address in find_bridge(handle, manifest)]
    finally:
        KERNEL32.CloseHandle(handle)
    output.parent.mkdir(parents=True, exist_ok=True)
    result = {"pid": pid, "bridge_candidates": states}
    output.write_text(json.dumps(result, indent=2), encoding="utf-8")
    return result


def require_presence_published(pid: int, release: Path, world_id: int, run_dir: Path,
                        boundary: str) -> None:
    """Check transport publication after cold Continue; this is not a control-state oracle."""
    if not boundary or not all(c.isalnum() or c in "-_" for c in boundary):
        raise HarnessFailure("world readiness boundary must be a simple filename stem")
    result = _capture_bridge_diagnostic(
        pid, release / "worlds" / str(world_id) / "bridge_manifest.json",
        run_dir / "diagnostics" / (boundary + "-world-ready.json"))
    candidates = result["bridge_candidates"]
    if len(candidates) != 1:
        raise HarnessFailure(f"{boundary}: expected one live ROM bridge")
    status = candidates[0]["status"]
    required = ("initialized", "session_ready", "player_state_sent")
    errors = ("world_not_ready", "queue_error", "checksum_error", "protocol_error",
              "sidecar_heartbeat_stale")
    if any(status.get(key) is not True for key in required) or any(
            status.get(key) is not False for key in errors):
        raise HarnessFailure(f"{boundary}: ROM has not published compatible presence; inspect bridge diagnostic")
    checkpoint(run_dir, "rom-presence-published", {"boundary_name": boundary, "world_id": world_id})


def verify_leg(plan: dict, leg_name: str, evidence: dict, *, require_live_space: bool = True) -> dict:
    """Fail at the first missing journal/save/data/group boundary."""
    preflight(plan, require_live_space=require_live_space)
    release, run_dir = _paths(plan)
    leg = _leg(plan, leg_name)
    if not isinstance(evidence, dict) or not isinstance(evidence.get("players"), dict):
        raise HarnessFailure(f"{leg_name}: evidence must contain a players object")
    server_catalog = _read_json(release / "server-build-catalog.json", "server build catalog")
    descriptor = bytes.fromhex(server_catalog["shared_player_descriptor_hex"])
    report = {"leg": leg_name, "players": [], "fixture_key": _fixture_key(plan, release),
              **{key: leg[key] for key in ("source_world_id", "destination_world_id", "portal_id")}}
    journal_group_ids = set()
    for player in plan["players"]:
        name = player["name"]
        item = evidence["players"].get(name)
        if not isinstance(item, dict):
            raise HarnessFailure(f"{leg_name}/{name}: missing evidence entry")
        for label in ("source", "staged", "template"):
            if not Path(item[label]).is_file():
                raise HarnessFailure(f"{leg_name}/{name}: missing {label} save {item[label]}")
        journal = _read_evidence_journal(item, f"{leg_name}/{name} journal")
        if journal.get("character_id") != player["character_id"]:
            raise HarnessFailure(f"{leg_name}/{name}: journal belongs to another character")
        if journal.get("phase") not in ("adopted", "committed"):
            raise HarnessFailure(f"{leg_name}/{name}: signed client has not committed arrival")
        intent = journal.get("intent", {})
        journal_source_sha = intent.get("source_save_sha256")
        if journal_source_sha != item.get("journal_source_sha256"):
            raise HarnessFailure(f"{leg_name}/{name}: journal source digest changed since discovery")
        if intent.get("source_world_id") != leg["source_world_id"]:
            raise HarnessFailure(f"{leg_name}/{name}: journal source world differs")
        if intent.get("request", {}).get("portal_id") != leg["portal_id"]:
            raise HarnessFailure(f"{leg_name}/{name}: journal portal differs")
        journal_group_ids.add(intent.get("request", {}).get("group_id"))
        stage = journal.get("stage", {})
        if stage.get("destination_world_id") != leg["destination_world_id"]:
            raise HarnessFailure(f"{leg_name}/{name}: journal destination world differs")
        commit = journal.get("terminal", {}).get("committed", {})
        if commit.get("own_world_id") != leg["destination_world_id"]:
            raise HarnessFailure(f"{leg_name}/{name}: no committed destination receipt")
        source, staged, template = (read_flash(Path(item[label])) for label in ("source", "staged", "template"))
        catalog = _read_json(release / "release_catalog.json", "release catalog")
        base_path, expected_base_kind, base_sha = _destination_base(
            plan, leg, player, _arrival_template(release, catalog, leg))
        if item.get("destination_base_kind") != expected_base_kind:
            raise HarnessFailure(f"{leg_name}/{name}: destination base kind changed")
        require_hash(Path(item["template"]), base_sha, f"{leg_name}/{name} destination base")
        if expected_base_kind == "dormant":
            if template.lineage[:9] != source.lineage[:9] or template.lineage[10:] != source.lineage[10:]:
                raise HarnessFailure(f"{leg_name}/{name}: dormant world belongs to another trainer")
        exact_source = source.sha256 == journal_source_sha
        if not exact_source and not (leg.get("source_is_baseline") and item.get("source_is_baseline")):
            raise HarnessFailure(f"{leg_name}/{name}: source differs from journal checkpoint")
        if staged.sha256 != stage.get("destination_save_sha256"):
            raise HarnessFailure(f"{leg_name}/{name}: staged save differs from signed journal")
        witnesses = check_travel_witnesses(source, staged, descriptor,
                                          player.get("shared_witnesses", []), exact_source=exact_source)
        checked = check_projection(
            source, staged, template, descriptor,
            allow_runtime_changes=frozenset(leg.get("runtime_field_exceptions", [])),
            expected_generation_delta=leg.get("expected_generation_delta", 1),
        )
        report["players"].append({"name": name, "character_id": player["character_id"], "journal": item["journal"],
                                  "journal_source_sha256": journal_source_sha,
                                  "exact_journal_source_inspected": exact_source,
                                  "destination_base_kind": expected_base_kind,
                                  "shared_witnesses": witnesses, **checked})
    if len({p["source_sha256"] for p in report["players"]}) != 2:
        raise HarnessFailure(f"{leg_name}: both actors used the same source save")
    group = evidence.get("group")
    if group:
        members = {m["character_id"] for m in group["members"]}
        expected = {p["character_id"] for p in plan["players"]}
        catalog = _read_json(release / "release_catalog.json", "release catalog")
        destination = next(w for w in catalog["worlds"] if w["world_id"] == leg["destination_world_id"])
        region = group.get("world_zone", {}).get("region")
        if (members != expected or region not in destination["presence_regions"]
                or journal_group_ids != {group.get("group_id")}):
            raise HarnessFailure(f"{leg_name}: group membership or zone mismatch")
        report["group"] = group
    else:
        raise HarnessFailure(f"{leg_name}: missing server-confirmed group membership")
    checkpoint(run_dir, f"{leg_name}-verified", report)
    return report


def wait_arrival_games(plan: dict, leg: dict, old_pids: dict[str, int]) -> dict:
    release, run_dir = _paths(plan)
    catalog = _read_json(release / "release_catalog.json", "arrival catalog")
    world = next(w for w in catalog["worlds"] if w["world_id"] == leg["destination_world_id"])
    arrived = {}
    deadline = time.monotonic() + 45
    adapter = Win32Adapter()
    for player in plan["players"]:
        name = player["name"]
        while True:
            check_c_space()
            pid = _mgba_pid_for(Path(player["profile_localappdata"]))
            if pid is not None and pid != old_pids[name]:
                command = psutil.Process(pid).cmdline()
                roms = [Path(arg) for arg in command if arg.casefold().endswith(".gba")]
                if len(roms) != 1:
                    raise HarnessFailure(f"{name}: arrival emulator has ambiguous ROM arguments")
                require_hash(roms[0], world["rom_sha256"], f"{name} live arrival ROM")
                wait_game_window(pid, adapter, timeout=30, rom_title=_rom_header_title(roms[0]))
                arrived[name] = pid
                break
            if time.monotonic() >= deadline:
                raise HarnessFailure(f"{name}: signed client did not launch the arrival ROM")
            time.sleep(.25)
    checkpoint(run_dir, "arrival-roms-opened", {"leg": leg["name"], "pids": arrived,
                                               "rom_sha256": world["rom_sha256"]})
    return arrived


def stop_runtime(plan: dict, desktops: dict[str, int]) -> None:
    """Request signed desktop Stop and wait for its games and leases to drain."""
    adapter = Win32Adapter()
    for player in plan["players"]:
        pid = _mgba_pid_for(Path(player["profile_localappdata"]))
        if pid is not None:
            # Verify parent ownership as well as isolated command line.
            try:
                children = psutil.Process(desktops[player["name"]]).children(recursive=True)
            except psutil.NoSuchProcess:
                raise HarnessFailure("signed desktop disappeared while game remained")
            if pid not in {child.pid for child in children}:
                raise HarnessFailure("refusing to close a game outside the launched signed client")
            # Closing mGBA first is a child failure, not an orderly session
            # stop: the desktop then revokes its credential. Request the
            # controller's Stop path before any child window is closed.
            wait_desktop_window(desktops[player["name"]], adapter)
            click_desktop(desktops[player["name"]], 235, 178, adapter)
    deadline = time.monotonic() + 45
    while True:
        alive = []
        for pid in desktops.values():
            try:
                children = psutil.Process(pid).children(recursive=True)
            except psutil.NoSuchProcess:
                continue
            for child in children:
                try:
                    if child.name().casefold() in ("mgba.exe", "coop-sidecar.exe"):
                        alive.append(child.pid)
                except psutil.NoSuchProcess:
                    pass
        if not alive:
            return
        if time.monotonic() >= deadline:
            raise HarnessFailure("signed runtime did not finish graceful shutdown within 45 seconds")
        check_c_space()
        time.sleep(.25)


def close_desktops(desktops: dict[str, int]) -> None:
    """Request window close even if runtime cleanup failed; never force-kill."""
    adapter = Win32Adapter()
    pending = set(desktops.values())
    requested = set()
    deadline = time.monotonic() + 45
    while pending:
        pending = {pid for pid in pending if psutil.pid_exists(pid)}
        for window in adapter.windows():
            if (window.pid in pending and window.visible and window.title
                    and window.handle not in requested):
                adapter.close(window.handle)
                requested.add(window.handle)
        if pending and time.monotonic() >= deadline:
            raise HarnessFailure(f"signed desktops did not close within 45 seconds: {sorted(pending)}")
        if pending:
            time.sleep(.25)


def continue_arrivals(plan: dict, leg: dict, pids: dict[str, int]) -> None:
    """Run the recorded cold-Continue inputs and retain both destination screens."""
    _, run_dir = _paths(plan)
    adapter = Win32Adapter()
    for index, action in enumerate(leg["arrival_inputs"]):
        check_c_space()
        name = action["player"]
        pid = pids[name]
        if not psutil.pid_exists(pid):
            raise HarnessFailure(f"{name}: arrival ROM exited before Continue")
        tap(pid, action["key"], adapter, hold=action.get("hold_ms", 80) / 1000,
            release=action.get("release_ms", 80) / 1000)
        time.sleep(min(action.get("wait_ms", 0), 30000) / 1000)
        if action.get("expect_presence_published"):
            release, _ = _paths(plan)
            require_presence_published(pid, release, leg["destination_world_id"], run_dir,
                                f"{leg['name']}-arrival-{index}-{name}")
    for name, pid in pids.items():
        capture_game(pid, run_dir / "screenshots", f"{leg['name']}-arrival-{name}", adapter)
    checkpoint(run_dir, "arrival-continue-inputs-sent", {"leg": leg["name"], "players": sorted(pids)})


def journey(plan: dict) -> dict:
    """Run each named leg and its oracle, stopping before the next on failure."""
    from live_group_evidence import collect
    preflight(plan)
    if not plan["legs"] or any(not leg.get("inputs") for leg in plan["legs"]):
        raise HarnessFailure("journey requires a bounded itinerary for every leg")
    names = {p["name"] for p in plan["players"]}
    if any(not leg.get("arrival_inputs") or
           {action.get("player") for action in leg["arrival_inputs"]} != names
           for leg in plan["legs"]):
        raise HarnessFailure("journey requires cold-Continue inputs for both arrivals on every leg")
    for previous, following in zip(plan["legs"], plan["legs"][1:]):
        if previous["destination_world_id"] != following["source_world_id"]:
            raise HarnessFailure("journey world chain is discontinuous")
    _, run_dir = _paths(plan)
    desktops = launch(plan)
    reports = []
    try:
        for leg in plan["legs"]:
            pids = start_games(plan, desktops)
            drive(plan, leg["name"], pids)
            arrived = wait_arrival_games(plan, leg, pids)
            continue_arrivals(plan, leg, arrived)
            group_ids = set()
            for player in plan["players"]:
                journals = _journal_candidates(Path(player["profile_localappdata"]), leg,
                                               player["character_id"])
                group_ids.add(journals[0][2]["intent"]["request"]["group_id"])
            if len(group_ids) != 1 or not next(iter(group_ids)):
                raise HarnessFailure("journey players have different travel groups")
            stop_runtime(plan, desktops)
            output = capture_directory(run_dir, leg["name"], "server-evidence")
            collected = collect(plan, leg["name"], next(iter(group_ids)), output,
                                lease_wait_seconds=15)
            evidence_plan = dict(plan, group_evidence=collected["group"],
                                 evidence_roots=[str(output), *plan.get("evidence_roots", [])])
            # Per-leg live collection supersedes any prior offline group view.
            evidence_plan["legs"] = [dict(item, group_evidence=collected["group"])
                                     if item["name"] == leg["name"] else item
                                     for item in plan["legs"]]
            evidence = discover_evidence(evidence_plan, leg["name"])
            reports.append(verify_leg(evidence_plan, leg["name"], evidence))
            checkpoint(run_dir, "journey-leg-complete", {"leg": leg["name"]})
        return {"legs": reports}
    finally:
        primary_error = sys.exc_info()[1]
        cleanup_errors = []
        for cleanup in (lambda: stop_runtime(plan, desktops), lambda: close_desktops(desktops)):
            try:
                cleanup()
            except Exception as cleanup_error:
                cleanup_errors.append(cleanup_error)
        if cleanup_errors:
            if primary_error is None:
                primary_error = cleanup_errors.pop(0)
                for error in cleanup_errors:
                    primary_error.add_note(f"Signed-client cleanup also failed: {error}")
                raise primary_error
            for error in cleanup_errors:
                primary_error.add_note(f"Signed-client cleanup also failed: {error}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument(
        "command",
        choices=("preflight", "prepare", "server-check", "discover", "launch", "start", "drive", "verify", "journey"),
    )
    parser.add_argument("--leg")
    parser.add_argument("--pids", type=Path, help="JSON name-to-mGBA-PID map for drive")
    parser.add_argument("--evidence", type=Path, help="JSON ROM/client/server evidence for verify")
    parser.add_argument("--output", type=Path, help="write JSON result atomically to this path")
    args = parser.parse_args()
    try:
        plan = _read_json(args.plan, "plan")
    except HarnessFailure as exc:
        print(f"HARNESS FAILED: {exc}", file=sys.stderr)
        return 1
    try:
        _validate_plan_shape(plan)
        if args.command == "preflight": result = preflight(plan)
        elif args.command == "prepare":
            from live_prepare_clients import prepare
            result = prepare(plan)
        elif args.command == "journey": result = journey(plan)
        elif args.command == "server-check": result = check_server(plan)
        elif args.command == "discover":
            if not args.leg:
                raise HarnessFailure("discover requires --leg")
            result = discover_evidence(plan, args.leg)
        elif args.command == "launch": result = launch(plan)
        elif args.command == "start":
            if not args.pids:
                raise HarnessFailure("start requires --pids")
            result = start_games(plan, _read_json(args.pids, "desktop PID map"))
        elif args.command == "drive":
            if not args.leg or not args.pids:
                raise HarnessFailure("drive requires --leg and --pids")
            result = drive(plan, args.leg, _read_json(args.pids, "game PID map"))
        else:
            if not args.leg or not args.evidence:
                raise HarnessFailure("verify requires --leg and --evidence")
            result = verify_leg(plan, args.leg, _read_json(args.evidence, "evidence"))
        rendered = json.dumps(result, indent=2) + "\n"
        if args.output:
            output = args.output.resolve()
            output.parent.mkdir(parents=True, exist_ok=True)
            temporary = output.with_name(f".{output.name}.{os.getpid()}.tmp")
            temporary.write_text(rendered, encoding="utf-8")
            temporary.replace(output)
        print(rendered, end="")
        return 0
    except (RuntimeError, OracleFailure, WindowControlError, OSError,
            KeyError, ValueError, TypeError) as exc:
        try:
            _, run_dir = _paths(plan)
            checkpoint(run_dir, "FAILED", {"command": args.command, "reason": str(exc)})
        except (HarnessFailure, OSError, KeyError, TypeError, ValueError):
            pass
        print(f"HARNESS FAILED: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

