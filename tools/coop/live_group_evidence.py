#!/usr/bin/env python3
"""Collect two-player server evidence after the signed game clients have exited.

This test-only probe takes credentials from the environment, acquires bounded
read leases, downloads current and dormant-world ROM saves to the spare run
volume, and releases each lease in a finally block. It never logs credentials.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

from live_region_harness import (HarnessFailure, _health_url, _leg, _paths,
                                 _read_json, _sanitize_group_evidence)
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


def collect(plan: dict, leg_name: str, group_id: str, output_dir: Path,
            *, lease_wait_seconds: int = 0) -> dict:
    leg = _leg(plan, leg_name)
    _, run_dir = _paths(plan)
    output_dir = _evidence_output(output_dir, run_dir)
    _, parsed = _health_url(plan)
    if parsed.hostname not in ("127.0.0.1", "localhost", "::1"):
        raise HarnessFailure("test-account credentials may be sent only to loopback")
    server = f"{parsed.scheme}://{parsed.netloc}"
    catalog = _read_json(Path(plan["release_dir"]) / "release_catalog.json", "catalog")
    destination = next(world for world in catalog["worlds"]
                       if world["world_id"] == leg["destination_world_id"])
    output_dir.mkdir(parents=True, exist_ok=True)
    groups = []
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
            status, body = _call(server, "GET", "/v1/groups/" + group_id,
                                 token=token, lease=lease)
            groups.append(_sanitize_group_evidence(json.loads(
                _expect(status, body, 200, f"{name} group"))))
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
    if groups[0] != groups[1]:
        raise HarnessFailure("two characters received different group records")
    expected = {player["character_id"] for player in plan["players"]}
    if ({member["character_id"] for member in groups[0]["members"]} != expected
            or groups[0]["world_zone"]["region"] not in destination["presence_regions"]):
        raise HarnessFailure("group membership or destination region differs")
    group_path = output_dir / "group.json"
    group_path.write_text(json.dumps(groups[0], indent=2) + "\n", encoding="utf-8")
    return {"group": str(group_path), "players": players}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("--leg", required=True)
    parser.add_argument("--group-id", required=True)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    result = collect(_read_json(args.plan, "plan"), args.leg, args.group_id,
                     args.output_dir)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
