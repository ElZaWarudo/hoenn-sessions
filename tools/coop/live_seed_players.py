#!/usr/bin/env python3
"""TEST ONLY: seed two local accounts through real snapshot admission.

Copies pinned ROM-written lineage bytes; never patches or fabricates save data.
Can resume a matching partially seeded account, but refuses a played/changed
head. Registration is explicit; credentials and invitation stay in environment.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import uuid
from pathlib import Path

from live_group_evidence import _call, _expect
from live_harness_oracles import OracleFailure, logical_field, read_flash
from live_fixture_validation import validate_player_fixture
from live_region_harness import (HarnessFailure, _health_url, _paths, _read_json, check_c_space,
                                 checkpoint, require_hash)


def validate_lineages(plan: dict) -> dict:
    release, _ = _paths(plan)
    catalog_path = release / "server-build-catalog.json"
    require_hash(catalog_path, plan["server_catalog_sha256"], "seed signed catalog")
    catalog = _read_json(catalog_path, "seed catalog")
    world_id = plan["seed_world_id"]
    world = next((w for w in catalog["worlds"] if w["world_id"] == world_id), None)
    if world is None:
        raise HarnessFailure("seed world is not in the pinned catalog")
    players = plan["players"]
    if len(players) != 2 or len({p["name"] for p in players}) != 2:
        raise HarnessFailure("seed needs two distinct named players")
    saves = {}
    for player in players:
        lineage = player.get("seed_lineage", [])
        if not 1 <= len(lineage) <= 64:
            raise HarnessFailure("seed lineage must contain 1..64 ROM saves")
        parsed = []
        for generation, item in enumerate(lineage, start=1):
            path = Path(item["path"])
            require_hash(path, item["sha256"], "seed ROM-written image")
            save = read_flash(path)
            if save.generation != generation:
                raise HarnessFailure("seed lineage has missing or reordered generations")
            if parsed and save.lineage != parsed[0].lineage:
                raise HarnessFailure("seed lineage changes trainer")
            parsed.append(save)
        if parsed[-1].sha256 != player["source_sha256"]:
            raise HarnessFailure("seed final image is not the pinned player source")
        saves[player["name"]] = parsed
    descriptor = bytes.fromhex(catalog["shared_player_descriptor_hex"])
    final = [saves[p["name"]][-1] for p in players]
    for player, save in zip(players, final):
        validate_player_fixture(save, descriptor, player)
    if (final[0].sha256 == final[1].sha256
            or logical_field(final[0], descriptor, 0x0102)
            == logical_field(final[1], descriptor, 0x0102)):
        raise HarnessFailure("seed players lack distinct logical money sentinels")
    return {"world": world, "saves": saves}


def seed(plan: dict, *, register: bool = False) -> dict:
    check_c_space()
    validated = validate_lineages(plan)  # Validate EVERYTHING before mutation.
    _, parsed = _health_url(plan)
    if parsed.hostname not in ("localhost", "127.0.0.1", "::1"):
        raise HarnessFailure("test seeds may only be uploaded to loopback")
    server = f"{parsed.scheme}://{parsed.netloc}"
    password = os.environ.get("COOP_HARNESS_PASSWORD")
    usernames = [os.environ.get(f"COOP_HARNESS_USERNAME_{p['name'].upper()}")
                 for p in plan["players"]]
    if not password or any(not u for u in usernames) or usernames[0] == usernames[1]:
        raise HarnessFailure("seed requires two distinct environment accounts and password")
    invitation = os.environ.get("COOP_HARNESS_INVITATION")
    if register and not invitation:
        raise HarnessFailure("fresh registration requires COOP_HARNESS_INVITATION")
    result = {"players": {}}
    for player, username in zip(plan["players"], usernames):
        name = player["name"]
        if register:
            status, body = _call(server, "POST", "/v1/auth/register", payload={
                "api_version": 1, "username": username, "password": password,
                "invitation_code": invitation})
            _expect(status, body, 201, "seed register")
        status, body = _call(server, "POST", "/v1/auth/login", payload={
            "api_version": 1, "username": username, "password": password})
        account = json.loads(_expect(status, body, 200, "seed login"))
        character, token = account["character_id"], account["access_token"]
        status, body = _call(server, "POST", "/v1/sessions/acquire", payload={
            "api_version": 1, "character_id": character,
            "client_instance_id": str(uuid.uuid4()), "idempotency_key": str(uuid.uuid4())}, token=token)
        lease = json.loads(_expect(status, body, 200, "seed acquire"))
        try:
            revision = lease["current_revision"]
            saves = validated["saves"][name]
            if revision > len(saves):
                raise HarnessFailure("seed refuses a player already advanced beyond fixture")
            runtime = {"session": {k: lease[k] for k in
                       ("session_id", "character_id", "session_epoch", "client_instance_id")},
                       "build": validated["world"]["build"]}
            status, body = _call(server, "POST", "/v1/realtime/tickets", payload={
                "realtime_version": 1, "runtime": runtime}, token=token)
            _expect(status, body, 200, "seed build admission")
            if revision:
                path = f"/v1/characters/{character}/resume-package/artifacts/character.sav?revision={revision}"
                status, body = _call(server, "GET", path, token=token, lease=lease)
                head = _expect(status, body, 200, "seed existing head")
                if hashlib.sha256(head).hexdigest() != saves[revision - 1].sha256:
                    raise HarnessFailure("seed refuses changed player head")
            pending = b"[]"
            pending_sha = hashlib.sha256(pending).hexdigest()
            for save in saves[revision:]:
                check_c_space()
                data = save.path.read_bytes()
                if hashlib.sha256(data).hexdigest() != save.sha256:
                    raise HarnessFailure("seed source changed after validation")
                fence = {"api_version": 1, "snapshot_id": str(uuid.uuid4()),
                         "rom_world_id": plan["seed_world_id"],
                         "expected_parent_revision": lease["current_revision"],
                         "idempotency_key": str(uuid.uuid4()),
                         "pending_commits_sha256": pending_sha,
                         **{k: lease[k] for k in ("session_id", "character_id", "session_epoch", "client_instance_id")},
                         "files": [{"artifact": "character.sav", "sha256": save.sha256,
                                    "size_bytes": len(data)},
                                   {"artifact": "pending_commits.json", "sha256": pending_sha,
                                    "size_bytes": len(pending)}]}
                prefix = f"/v1/characters/{character}/snapshots/"
                status, body = _call(server, "POST", prefix + "prepare", payload=fence,
                                     token=token, lease=lease)
                prepared = json.loads(_expect(status, body, 200, "seed prepare"))
                for target in prepared["upload_targets"]:
                    url = target["url"]
                    # Never follow a server-controlled credential/byte upload off loopback.
                    if not url.startswith(server + "/"):
                        raise HarnessFailure("seed upload target left pinned loopback server")
                    content = {"character.sav": data, "pending_commits.json": pending}[target["artifact"]]
                    status, body = _call(server, "PUT", url[len(server):], raw=content)
                    _expect(status, body, 204, "seed upload")
                finalize = dict(fence)
                finalize.pop("rom_world_id")
                finalize.update(revision=lease["current_revision"] + 1,
                                idempotency_key=prepared["idempotency_key"], last_applied_commit=None)
                status, body = _call(server, "POST", prefix + "finalize", payload=finalize,
                                     token=token, lease=lease)
                record = json.loads(_expect(status, body, 200, "seed finalize"))
                if record["revision"] != finalize["revision"]:
                    raise HarnessFailure("seed finalize revision mismatch")
                lease["current_revision"] = record["revision"]
            result["players"][name] = {"character_id": character,
                                        "source_sha256": saves[-1].sha256,
                                        "revision": lease["current_revision"]}
        finally:
            primary_error = sys.exc_info()[1]
            try:
                status, body = _call(server, "POST", "/v1/sessions/release", token=token, lease=lease,
                                 payload={"api_version": 1, "idempotency_key": str(uuid.uuid4()),
                                          **{k: lease[k] for k in ("session_id", "character_id", "current_revision",
                                                                  "session_epoch", "client_instance_id")}})
                _expect(status, body, 200, "seed release")
            except Exception as cleanup_error:
                if primary_error is None:
                    raise
                primary_error.add_note(f"Lease release also failed: {cleanup_error}")
        if register and player is plan["players"][0]:
            status, body = _call(server, "POST", "/v1/auth/invitations", payload={}, token=token)
            invitation = json.loads(_expect(status, body, 201, "seed second invitation"))["invitation_code"]
    if len({p["character_id"] for p in result["players"].values()}) != 2:
        raise HarnessFailure("seed server returned the same character for both accounts")
    _, run_dir = _paths(plan)
    checkpoint(run_dir, "rom-written-players-seeded", result)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("--register", action="store_true")
    args = parser.parse_args()
    print(json.dumps(seed(_read_json(args.plan, "seed plan"), register=args.register), indent=2))
