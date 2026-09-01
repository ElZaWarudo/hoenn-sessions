"""Smoke test for the two-player Kanto coop vertical slice."""

from __future__ import annotations

import argparse
import json
import random
import sys
import urllib.error
import urllib.request
import uuid
from typing import Any, Dict


def fail(message: str) -> None:
    """Abort with an actionable error."""

    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def request_json(
    base_url: str,
    method: str,
    path: str,
    body: Dict[str, Any] | None = None,
) -> Dict[str, Any]:
    """Perform an HTTP JSON request and return parsed response JSON."""

    payload = None
    if body is not None:
        payload = json.dumps(body).encode("utf-8")

    request = urllib.request.Request(
        url=f"{base_url}{path}",
        data=payload,
        method=method,
        headers={"Content-Type": "application/json", "Accept": "application/json"},
    )

    try:
        with urllib.request.urlopen(request, timeout=8) as response:
            raw = response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8") if error.fp is not None else "<empty>"
        fail(
            f"{method} {path} failed with HTTP {error.code}: {raw}"
            " (expected body to be valid JSON)"
        )
    except urllib.error.URLError as error:
        fail(f"Request to {path} failed: {error}")

    if not raw:
        return {}

    try:
        return json.loads(raw)
    except json.JSONDecodeError as error:
        fail(f"{method} {path} response was not JSON: {error}: {raw}")


def request_text(base_url: str, path: str) -> str:
    """Perform a text GET and return plain response text."""

    request = urllib.request.Request(url=f"{base_url}{path}", method="GET")
    try:
        with urllib.request.urlopen(request, timeout=8) as response:
            return response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8") if error.fp is not None else "<empty>"
        fail(f"GET {path} failed with HTTP {error.code}: {raw}")
    except urllib.error.URLError as error:
        fail(f"Request to {path} failed: {error}")


def assert_value(condition: bool, message: str) -> None:
    """Fail with an actionable message when a required condition fails."""

    if not condition:
        fail(message)


def assert_json_value(payload: Dict[str, Any], field: str, expected: Any) -> None:
    """Assert a top-level JSON field value."""

    assert_value(
        payload.get(field) == expected,
        f"Expected {field}={expected!r} but got {payload.get(field)!r}",
    )


def progress_with_regions(
    hoenn_badges: int,
    kanto_badges: int,
    kanto_defeats: list[str],
) -> Dict[str, Any]:
    """Build two-region participant progress for the smoke fixture."""

    return {
        "character_id": 0,
        "regional_progress": [
            {
                "region": "HOENN",
                "badge_mask": hoenn_badges,
                "story_checkpoint": 0,
                "defeated_trainers": [],
                "unlocked_fly_points": [],
            },
            {
                "region": "KANTO",
                "badge_mask": kanto_badges,
                "story_checkpoint": 0,
                "defeated_trainers": kanto_defeats,
                "unlocked_fly_points": [],
            },
        ],
    }


def pick_progress_for_character(character_id: int) -> Dict[str, Any]:
    """Create the requested two-region progress for the initiator."""

    progress = progress_with_regions(0x00FF, 0x0007, [])
    progress["character_id"] = character_id
    return progress


def pick_progress_for_helper(character_id: int) -> Dict[str, Any]:
    """Create the requested two-region progress for the helper."""

    progress = progress_with_regions(0x001F, 0x0001, ["KANTO:TRAINER_BROCK"])
    progress["character_id"] = character_id
    return progress


def find_regional_progress(participant: Dict[str, Any], region: str) -> Dict[str, Any]:
    """Return participant progress entry for a given region."""

    for regional_progress in participant["progress"]["regional_progress"]:
        if regional_progress["region"] == region:
            return regional_progress
    fail(f"Participant {participant['character_id']} missing {region} progress")


def run_smoke(base_url: str) -> Dict[str, Any]:
    """Execute the full two-player Kanto vertical slice against a running server."""

    health = request_text(base_url, "/health").strip()
    assert_json_value({"health": health}, "health", "ok")

    kanto_zone = {"region": "KANTO", "map": "PALLET_TOWN", "channel": 1}

    first_character_id = random.randint(10_000_000, 10_000_000_000)
    second_character_id = random.randint(10_000_000_000, 20_000_000_000)
    if second_character_id == first_character_id:
        second_character_id += 1

    first_progress = pick_progress_for_character(first_character_id)
    first_payload = {"participant": first_progress, "zone": kanto_zone}
    first_participant = request_json(base_url, "POST", "/v1/participants", first_payload)

    assert_json_value(first_participant, "character_id", first_character_id)

    second_progress = pick_progress_for_helper(second_character_id)
    second_payload = {"participant": second_progress, "zone": kanto_zone}
    second_participant = request_json(base_url, "POST", "/v1/participants", second_payload)
    assert_json_value(second_participant, "character_id", second_character_id)

    second_kanto_before = find_regional_progress(second_participant, "KANTO")
    assert_value(
        second_kanto_before["defeated_trainers"] == ["KANTO:TRAINER_BROCK"],
        "Helper should start with exactly KANTO:TRAINER_BROCK defeated",
    )

    group_request = {
        "first_character_id": first_character_id,
        "second_character_id": second_character_id,
    }
    group = request_json(base_url, "POST", "/v1/groups", group_request)
    assert_json_value(group, "zone", kanto_zone)
    group_id = group["group_id"]
    participants = group["group"]["members"]
    assert_value(
        participants == sorted(participants),
        "Expected deterministic sorted participant order in group members",
    )

    reservation = request_json(
        base_url,
        "POST",
        f"/v1/groups/{group_id}/battles/reserve",
        {
            "character_id": first_character_id,
            "trainer_id": "KANTO:TRAINER_BROCK",
        },
    )
    assert_json_value(reservation, "state", "PENDING")
    assert_json_value(reservation, "battle_region", "KANTO")
    assert_json_value(reservation, "tier", 1)
    assert_json_value(reservation, "level_cap", 19)
    battle_id = reservation["battle_id"]

    members = reservation["participants"]
    assert_value(len(members) == 2, "Reservation must include both participants")
    role_by_character = {entry["character_id"]: entry["role"] for entry in members}
    assert_value(
        role_by_character.get(first_character_id) == "FIRST_CLEAR_CANDIDATE",
        "Initiator must be first_clear_candidate",
    )
    assert_value(
        role_by_character.get(second_character_id) == "REPEAT_HELPER",
        "Second player should be repeat_helper",
    )

    accepted = request_json(
        base_url,
        "POST",
        f"/v1/battles/{battle_id}/accept",
        {"character_id": second_character_id},
    )
    assert_json_value(accepted, "state", "READY")

    idempotency_key = str(uuid.uuid4())
    commit_body = {
        "idempotency_key": idempotency_key,
        "outcome": "WON",
        "final_state_hash": "ab" * 32,
    }
    commit = request_json(base_url, "POST", f"/v1/battles/{battle_id}/commit", commit_body)
    assert_json_value(commit, "replayed", False)

    first_commit = commit["commit"]
    commit_id = first_commit["commit_id"]
    assert_value(
        first_commit["trainer_id"] == "KANTO:TRAINER_BROCK",
        "Committed trainer must be KANTO:TRAINER_BROCK",
    )

    retry_commit = request_json(base_url, "POST", f"/v1/battles/{battle_id}/commit", commit_body)
    assert_value(
        retry_commit["replayed"],
        "Retrying idempotent commit should return replayed=True",
    )
    assert_json_value(retry_commit["commit"], "commit_id", commit_id)

    first_state = request_json(base_url, "GET", f"/v1/participants/{first_character_id}", None)
    second_state = request_json(base_url, "GET", f"/v1/participants/{second_character_id}", None)

    first_kanto = find_regional_progress(first_state, "KANTO")
    assert_value(
        "KANTO:TRAINER_BROCK" in first_kanto["defeated_trainers"],
        "First player should defeat TRAINER_BROCK after commit",
    )

    second_kanto = find_regional_progress(second_state, "KANTO")
    assert_value(
        second_kanto["defeated_trainers"].count("KANTO:TRAINER_BROCK") == 1,
        "Helper should still have exactly one Brock defeat entry",
    )

    return {
        "result": "pass",
        "battle_region": reservation["battle_region"],
        "tier": reservation["tier"],
        "level_cap": reservation["level_cap"],
        "roles": {
            "initiator": role_by_character[first_character_id],
            "helper": role_by_character[second_character_id],
        },
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run an end-to-end smoke test for the Kanto vertical slice."
    )
    parser.add_argument(
        "--base-url",
        default="http://127.0.0.1:3000",
        help="Base URL of the running coop-server (default: %(default)s)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    base_url = args.base_url.rstrip("/")
    summary = run_smoke(base_url)
    print(json.dumps(summary, sort_keys=True))


if __name__ == "__main__":
    main()
