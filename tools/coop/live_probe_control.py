"""TEST ONLY: atomically publish numbered controls for the standalone probe.

Writes control JSON only. Does not launch an emulator or change saves. The
consumer may open a .json as soon as it exists, so publish only after fsync.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

from live_harness_oracles import OracleFailure
from live_scripted_input import request_line


def validate_control(sequence: int, request: dict) -> dict:
    # Validate the control number even for a done request with no button actions.
    request_line(sequence, "control", 0, 1, 0)
    if not isinstance(request, dict):
        raise OracleFailure("probe control must be an object")
    if "done" in request:
        if set(request) != {"done"} or request["done"] is not True:
            raise OracleFailure("probe done must be a separate true-only request")
        return {"done": True}
    if (set(request) - {"label", "actions", "checkpoint"}
            or not {"label", "actions"} <= request.keys()
            or type(request.get("checkpoint", False)) is not bool):
        raise OracleFailure("probe control keys/checkpoint differ")
    actions = request["actions"]
    if not isinstance(actions, list) or not 1 <= len(actions) <= 120:
        raise OracleFailure("probe control needs one to 120 actions")
    label = request["label"]
    request_line(sequence, label, 0, 1, 0)
    normalized = []
    for index, action in enumerate(actions):
        if not isinstance(action, dict) or set(action) - {"mask", "hold", "wait"}:
            raise OracleFailure("probe action keys differ")
        mask, hold, wait = action.get("mask", 0), action.get("hold", 8), action.get("wait", 20)
        # The probe derives intermediate labels exactly this way. Check those
        # too, so a valid final label cannot fail later at the driver boundary.
        derived = label if index == len(actions) - 1 else f"{label}-{index}"
        request_line(sequence, derived, mask, hold, wait)
        normalized.append({"mask": mask, "hold": hold, "wait": wait})
    return {"label": label, "actions": normalized, "checkpoint": request.get("checkpoint", False)}


def publish_control(directory: Path, sequence: int, request: dict) -> Path:
    """Publish once; reject malformed/duplicate controls before creating files."""
    payload = validate_control(sequence, request)
    directory = Path(directory)
    if not directory.is_dir():
        raise OracleFailure("probe control directory must already exist")
    destination = directory / f"control-{sequence:04d}.json"
    pending = destination.with_suffix(".tmp")
    if destination.exists() or pending.exists():
        raise OracleFailure("probe control number already exists")
    encoded = (json.dumps(payload, ensure_ascii=True, separators=(",", ":")) + "\n").encode("ascii")
    owned = False
    try:
        with pending.open("xb") as file:
            owned = True
            file.write(encoded)
            file.flush()
            os.fsync(file.fileno())
        # Windows rename refuses an existing destination, including a race
        # after the precheck. POSIX hard-link publication has the same property.
        if os.name == "nt":
            pending.rename(destination)
        else:
            os.link(pending, destination)
            pending.unlink()
        owned = False
    finally:
        if owned:
            pending.unlink(missing_ok=True)
    return destination


def publish_done(directory: Path, sequence: int) -> Path:
    return publish_control(directory, sequence, {"done": True})


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--sequence", type=int, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--done", action="store_true")
    mode.add_argument("--label")
    parser.add_argument("--actions-json", help="JSON list of mask/hold/wait action objects")
    parser.add_argument("--checkpoint", action="store_true")
    args = parser.parse_args(argv)
    if args.done:
        if args.actions_json is not None or args.checkpoint:
            parser.error("--done cannot include actions or checkpoint")
        result = publish_done(args.root, args.sequence)
    else:
        if args.actions_json is None:
            parser.error("--label requires --actions-json")
        try:
            actions = json.loads(args.actions_json)
        except json.JSONDecodeError:
            parser.error("--actions-json is not valid JSON")
        result = publish_control(args.root, args.sequence,
                                 {"label": args.label, "actions": actions, "checkpoint": args.checkpoint})
    print(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
