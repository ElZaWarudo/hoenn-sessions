#!/usr/bin/env python3
"""Two-player group proofs and server save evidence for test-only live legs.

Groups form the way players form them on main: the inviter's pairing code is
issued through the public API while no client is running, then the partner
types it into the signed desktop's "Join a partner by code" box. A group ends
when its members genuinely leave, so membership is proved while both signed
clients are live, with token-only ``GET /v1/group/partner`` before the crossing
and again after arrival, before Stop. After the clients exit, ``collect``
acquires bounded read leases only to download current and dormant-world saves;
it never reads a group after Stop. Credentials come from the environment and
are never logged.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

from live_region_harness import (HarnessFailure, _health_url, _journal_candidates, _leg,
                                 _paths, _read_json, _sanitize_group_evidence, checkpoint)
from live_save_capture import _capture_path


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None  # A redirect is a stopped API boundary, never a new origin.


def _call(server: str, method: str, path: str, *, payload: dict | None = None,
          token: str | None = None, lease: dict | None = None,
          raw: bytes | None = None) -> tuple[int, bytes]:
    headers = {}
    if token:
        headers["Authorization"] = "Bearer " + token
    if lease:
        headers.update({
            "x-coop-session-id": lease["session_id"],
            "x-coop-session-epoch": str(lease["session_epoch"]),
            "x-coop-client-instance-id": lease["client_instance_id"],
        })
    data = raw
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload, separators=(",", ":")).encode()
    request = urllib.request.Request(server + path, data=data, headers=headers, method=method)
    try:
        with urllib.request.build_opener(_NoRedirect()).open(request, timeout=15) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


def _expect(status: int, body: bytes, expected: int, label: str) -> bytes:
    if status != expected:
        try:
            code = json.loads(body).get("error", {}).get("code", "unknown")
        except (UnicodeError, ValueError):
            code = "unknown"
        raise HarnessFailure(f"{label}: HTTP {status} {code}")
    return body


def _acquire_after_release(server: str, request: dict, token: str, wait_seconds: int) -> dict:
    if type(wait_seconds) is not int or not 0 <= wait_seconds <= 30:
        raise HarnessFailure("lease wait must be an integer from 0 to 30 seconds")
    deadline = time.monotonic() + wait_seconds
    while True:
        status, body = _call(server, "POST", "/v1/sessions/acquire-world",
                             payload=request, token=token)
        if status != 409 or time.monotonic() >= deadline:
            return json.loads(_expect(status, body, 200, "evidence acquire"))["lease"]
        # This exact acquire key is reused; no takeover or lease mutation is
        # requested while the signed desktop completes its normal release.
        time.sleep(.25)


def _evidence_output(output_dir: Path, run_dir: Path) -> Path:
    # Resolve symlinks before putting both paths into the same long-path namespace.
    output = _capture_path(output_dir.resolve())
    run = _capture_path(run_dir.resolve())
    system = _capture_path(Path(os.environ.get("SystemDrive", "C:") + "\\")).drive.casefold()
    if (system and (output.drive.casefold() == system or run.drive.casefold() == system)) or not output.is_relative_to(run):
        raise HarnessFailure("server evidence output must be under spare-volume run_dir")
    return output


PAIRING_CODE = re.compile(r"^[A-HJ-NP-Z2-9]{3}-[A-HJ-NP-Z2-9]{3}$")
JOIN_LINK_PREFIX = "hoenn-sessions://join/"
FENCE = ("session_id", "current_revision", "session_epoch", "client_instance_id")
GROUP_BASIS = ("pairing code issued to a while no client ran, redeemed through b's signed desktop "
               "Join box; token-only GET /v1/group/partner showed both members Active with each "
               "other before the crossing and after arrival (before Stop); group_id is the "
               "identical id in both committed crossing journals")


def loopback_server(plan: dict) -> str:
    _, parsed = _health_url(plan)
    if parsed.hostname not in ("127.0.0.1", "localhost", "::1"):
        raise HarnessFailure("test-account credentials may be sent only to loopback")
    return f"{parsed.scheme}://{parsed.netloc}"


def _canonical_username(username: str) -> str:
    # The server validates and lowercases usernames (coop-cloud auth.rs).
    return username.lower()


def _same_username(reported: str, expected: str) -> bool:
    return _canonical_username(reported) == _canonical_username(expected)


def _accounts(plan: dict) -> tuple[dict, str]:
    players = plan.get("players", [])
    if len(players) != 2 or {p.get("name") for p in players} != {"a", "b"}:
        raise HarnessFailure("group proof requires actors a and b")
    usernames = {p["name"]: os.environ.get("COOP_HARNESS_USERNAME_" + p["name"].upper()) for p in players}
    password = os.environ.get("COOP_HARNESS_PASSWORD")
    if (not password or not all(usernames.values())
            or len({_canonical_username(u) for u in usernames.values()}) != 2):
        raise HarnessFailure("group proof requires two distinct environment accounts and password")
    return usernames, password


def _login(server: str, username: str, password: str, character: str, label: str) -> str:
    status, body = _call(server, "POST", "/v1/auth/login",
                         payload={"api_version": 1, "username": username, "password": password})
    login = json.loads(_expect(status, body, 200, label + " login"))
    if login.get("character_id") != character:
        raise HarnessFailure(f"{label}: account belongs to another character")
    return login["access_token"]


def _release(server: str, token: str, lease: dict, character: str, label: str) -> None:
    status, body = _call(server, "POST", "/v1/sessions/release", token=token, lease=lease,
                         payload={"api_version": 1, "character_id": character,
                                  "idempotency_key": str(uuid.uuid4()), **{k: lease[k] for k in FENCE}})
    _expect(status, body, 200, label + " release")


def partner_status(server: str, token: str, label: str) -> dict | None:
    """Token-only partner read; it never presents or needs a lease fence."""
    status, body = _call(server, "GET", "/v1/group/partner", token=token)
    value = json.loads(_expect(status, body, 200, label + " partner status"))
    if value.get("api_version") != 1 or "partner" not in value:
        raise HarnessFailure(f"{label}: partner status shape differs")
    partner = value["partner"]
    if partner is not None and (not isinstance(partner, dict) or not isinstance(partner.get("username"), str)
                                or type(partner.get("group_active")) is not bool
                                or type(partner.get("online")) is not bool
                                or not isinstance(partner.get("world_zone"), dict)):
        raise HarnessFailure(f"{label}: partner status shape differs")
    return partner


def _statuses(plan: dict, label: str) -> dict:
    server = loopback_server(plan)
    usernames, password = _accounts(plan)
    result = {}
    for player in plan["players"]:
        name = player["name"]
        token = _login(server, usernames[name], password, player["character_id"], f"{label}/{name}")
        result[name] = partner_status(server, token, f"{label}/{name}")
    return result


def _other(plan: dict, name: str) -> dict:
    return next(p for p in plan["players"] if p["name"] != name)


def require_ungrouped(plan: dict, label: str) -> dict:
    """Before pairing, neither actor may belong to an Active group."""
    statuses = _statuses(plan, label)
    for name, partner in statuses.items():
        if partner is not None and partner["group_active"]:
            raise HarnessFailure(f"{label}/{name}: an Active group already exists; refusing to pair")
    return {name: {"previous_partner": None if p is None else p["username"], "group_active": False}
            for name, p in statuses.items()}


def partner_active(plan: dict, label: str = "join") -> bool:
    """True once both actors see an Active group with each other; a wrong partner fails closed."""
    usernames, _ = _accounts(plan)
    statuses = _statuses(plan, label)
    for name, partner in statuses.items():
        if partner is None or not partner["group_active"]:
            return False
        if not _same_username(partner["username"], usernames[_other(plan, name)["name"]]):
            raise HarnessFailure(f"{label}/{name}: Active group has an unexpected partner")
    return True


def partner_proof(plan: dict, regions: list, label: str) -> dict:
    """Token-only proof that both actors are Active with each other in ``regions``."""
    if not isinstance(regions, list) or not regions or not all(isinstance(r, str) for r in regions):
        raise HarnessFailure(f"{label}: expected presence regions are missing")
    usernames, _ = _accounts(plan)
    statuses = _statuses(plan, label)
    players = {}
    for player in plan["players"]:
        name, other = player["name"], _other(plan, player["name"])
        partner = statuses[name]
        if partner is None or not _same_username(partner["username"], usernames[other["name"]]):
            raise HarnessFailure(f"{label}/{name}: partner is missing or unexpected")
        if partner["group_active"] is not True:
            raise HarnessFailure(f"{label}/{name}: group is not Active")
        if partner["online"] is not True:
            raise HarnessFailure(f"{label}/{name}: partner is not online")
        live = partner.get("live_world_zone")
        zone = live if isinstance(live, dict) else partner["world_zone"]
        region = zone.get("region")
        if region not in regions:
            raise HarnessFailure(f"{label}/{name}: partner zone {region!r} is outside {regions}")
        players[name] = {"character_id": player["character_id"], "partner_character_id": other["character_id"],
                         "partner_username": partner["username"], "group_active": True, "partner_online": True,
                         "partner_region": region, "zone_source": "live" if isinstance(live, dict) else "saved",
                         "partner_saved_region": partner["world_zone"].get("region")}
    proof = {"label": label, "checked_at_unix_ms": int(time.time() * 1000), "players": players}
    _, run_dir = _paths(plan)
    checkpoint(run_dir, "group-partner-proved", proof)
    return proof


def create_pairing_code(plan: dict, label: str) -> dict:
    """Issue a's pairing code under a short harness lease; a is ungrouped, so release ends nothing."""
    server = loopback_server(plan)
    usernames, password = _accounts(plan)
    inviter = next(p for p in plan["players"] if p["name"] == "a")
    character = inviter["character_id"]
    token = _login(server, usernames["a"], password, character, label + "/a")
    lease = _acquire_after_release(server, {"api_version": 1, "character_id": character,
                                            "client_instance_id": str(uuid.uuid4()),
                                            "idempotency_key": str(uuid.uuid4())}, token, 0)
    try:
        status, body = _call(server, "POST", "/v1/groups/pairing-codes", token=token, lease=lease,
                             payload={"api_version": 1, "character_id": character, **{k: lease[k] for k in FENCE}})
        issued = json.loads(_expect(status, body, 201, label + " pairing code"))
        code = issued.get("code")
        if (issued.get("api_version") != 1 or not isinstance(code, str) or not PAIRING_CODE.fullmatch(code)
                or issued.get("join_link") != JOIN_LINK_PREFIX + code
                or type(issued.get("expires_at")) is not int or issued["expires_at"] <= 0):
            raise HarnessFailure(f"{label}: pairing code response differs")
    finally:
        original = sys.exc_info()[1]
        try:
            _release(server, token, lease, character, label + " pairing lease")
        except Exception as cleanup:
            if original is None:
                raise
            original.add_note(f"Pairing lease release also failed: {cleanup}")
    return {"code": code, "code_sha256": hashlib.sha256(code.encode()).hexdigest(),
            "expires_at_unix_ms": issued["expires_at"], "inviter_character_id": character}


def pair_desktops(plan: dict, desktops: dict, code: dict, regions: list, label: str) -> dict:
    """b types a's code into its live signed desktop; proof precedes any crossing input."""
    from live_harness_windows import Win32Adapter, capture_desktop
    from live_prepare_clients import enter_join_code

    _, run_dir = _paths(plan)
    adapter = Win32Adapter()
    try:
        join = enter_join_code(desktops["b"], code["code"], adapter, lambda: partner_active(plan, label))
    except HarnessFailure as error:
        try:
            capture_desktop(desktops["b"], run_dir / "screenshots", label + "-join-failed-b", adapter)
        except Exception as capture_error:
            error.add_note("Join status capture failed: " + type(capture_error).__name__)
        raise
    proof = partner_proof(plan, regions, label + " pre-crossing")
    return dict(proof, join=join, code_sha256=code["code_sha256"])


def journal_group_id(plan: dict, leg: dict) -> str:
    """Both actors' newest committed crossing journals must name one group."""
    ids = set()
    for player in plan["players"]:
        journals = _journal_candidates(Path(player["profile_localappdata"]), leg, player["character_id"])
        if not journals:
            raise HarnessFailure(f"{leg['name']}/{player['name']}: no committed crossing journal")
        ids.add(journals[0][2].get("intent", {}).get("request", {}).get("group_id"))
    if len(ids) != 1 or not isinstance(next(iter(ids)), str):
        raise HarnessFailure(f"{leg['name']}: crossing journals name different or missing groups")
    return str(uuid.UUID(next(iter(ids))))


def group_record(plan: dict, leg: dict, group_id: str, before: dict, after: dict) -> dict:
    """Combine the live pre/post proofs with the journals' group id (no post-Stop read)."""
    expected = {p["character_id"] for p in plan["players"]}
    for proof, label in ((before, "pre-crossing"), (after, "post-arrival")):
        players = proof.get("players", {}) if isinstance(proof, dict) else {}
        if (set(players) != {"a", "b"} or {p.get("character_id") for p in players.values()} != expected
                or any(p.get("group_active") is not True or p.get("partner_online") is not True
                       or {p["character_id"], p.get("partner_character_id")} != expected
                       for p in players.values())):
            raise HarnessFailure(f"{leg['name']}: {label} group proof differs")
    if before["checked_at_unix_ms"] > after["checked_at_unix_ms"]:
        raise HarnessFailure(f"{leg['name']}: group proofs are out of order")
    regions = {p["partner_region"] for p in after["players"].values()}
    if len(regions) != 1:
        raise HarnessFailure(f"{leg['name']}: members arrived in different regions")
    return {"group_id": str(uuid.UUID(group_id)),
            "members": [{"character_id": p["character_id"]} for p in plan["players"]],
            "world_zone": {"region": next(iter(regions))}, "basis": GROUP_BASIS,
            "pre_crossing": before, "post_arrival": after}


def collect(plan: dict, leg_name: str, group_proof: dict, output_dir: Path,
            *, lease_wait_seconds: int = 0) -> dict:
    """Download server saves after Stop; group membership comes only from the pre-Stop proof."""
    leg = _leg(plan, leg_name)
    _, run_dir = _paths(plan)
    output_dir = _evidence_output(output_dir, run_dir)
    if not isinstance(group_proof, dict) or not isinstance(group_proof.get("post_arrival"), dict):
        raise HarnessFailure("post-Stop group reads were removed; supply the pre-Stop partner proof")
    server = loopback_server(plan)
    catalog = _read_json(Path(plan["release_dir"]) / "release_catalog.json", "catalog")
    destination = next(world for world in catalog["worlds"]
                       if world["world_id"] == leg["destination_world_id"])
    group = _sanitize_group_evidence(group_proof)
    expected = {player["character_id"] for player in plan["players"]}
    if ({member["character_id"] for member in group["members"]} != expected
            or len(group["members"]) != 2
            or group["world_zone"]["region"] not in destination["presence_regions"]):
        raise HarnessFailure("group membership or destination region differs")
    output_dir.mkdir(parents=True, exist_ok=True)
    players = {}
    for player in plan["players"]:
        name = player["name"]
        username = os.environ.get(f"COOP_HARNESS_USERNAME_{name.upper()}")
        password = os.environ.get("COOP_HARNESS_PASSWORD")
        if not username or not password:
            raise HarnessFailure(f"{name}: missing test account credentials in environment")
        login = {"api_version": 1, "username": username, "password": password}
        status, body = _call(server, "POST", "/v1/auth/login", payload=login)
        token = json.loads(_expect(status, body, 200, f"{name} login"))["access_token"]
        character = player["character_id"]
        request = {"api_version": 1, "character_id": character,
                   "client_instance_id": str(uuid.uuid4()),
                   "idempotency_key": str(uuid.uuid4())}
        lease = _acquire_after_release(server, request, token, lease_wait_seconds)
        try:
            status, body = _call(server, "GET", f"/v1/characters/{character}/snapshots",
                                 token=token, lease=lease)
            snapshots = json.loads(_expect(status, body, 200, f"{name} snapshots"))["snapshots"]
            current = next(s for s in snapshots if s["revision"] == lease["current_revision"])
            if current["rom_world_id"] != leg["destination_world_id"]:
                raise HarnessFailure(f"{name}: active save is in the wrong world")
            older = [s for s in snapshots if s["rom_world_id"] == leg["destination_world_id"]
                     and s["revision"] < current["revision"]]
            dormant = max(older, key=lambda s: s["revision"]) if older else None
            saved = {}
            for label, snapshot in (("current", current), ("dormant", dormant)):
                if snapshot is None:
                    continue
                expected = next(f["sha256"] for f in snapshot["files"]
                                if f["artifact"] == "character.sav")
                path = (f"/v1/characters/{character}/resume-package/artifacts/"
                        f"character.sav?revision={snapshot['revision']}")
                status, body = _call(server, "GET", path, token=token, lease=lease)
                _expect(status, body, 200, f"{name} {label} save")
                if hashlib.sha256(body).hexdigest() != expected:
                    raise HarnessFailure(f"{name}: {label} save differs from snapshot manifest")
                output = output_dir / f"{name}-{label}.sav"
                output.write_bytes(body)
                saved[label] = {"path": str(output), "sha256": expected,
                                "revision": snapshot["revision"]}
            players[name] = saved
        finally:
            primary_error = sys.exc_info()[1]
            release = {"api_version": 1, "session_id": lease["session_id"],
                       "character_id": character,
                       "current_revision": lease["current_revision"],
                       "session_epoch": lease["session_epoch"],
                       "client_instance_id": lease["client_instance_id"],
                       "idempotency_key": str(uuid.uuid4())}
            try:
                status, body = _call(server, "POST", "/v1/sessions/release",
                                     payload=release, token=token, lease=lease)
                _expect(status, body, 200, f"{name} release")
            except Exception as cleanup_error:
                if primary_error is None:
                    raise
                primary_error.add_note(f"Lease release also failed: {cleanup_error}")
    # The retained record is the live pre-Stop proof; discovery sanitizes it to
    # group_id/members/region for the unchanged verify oracle.
    group_path = output_dir / "group.json"
    group_path.write_text(json.dumps(group_proof, indent=2) + "\n", encoding="utf-8")
    return {"group": str(group_path), "players": players}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("--leg", required=True)
    parser.add_argument("--group-proof", required=True, type=Path,
                        help="group_record JSON captured before Stop")
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    result = collect(_read_json(args.plan, "plan"), args.leg,
                     _read_json(args.group_proof, "group proof"), args.output_dir)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
