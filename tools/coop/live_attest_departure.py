#!/usr/bin/env python3
"""Read-only test attestation of LoadPlayerParty's empty sixth-slot hpLost reset.

Never alters a save or grants a general Party exception. Strict retained travel
projection and custody checks must pass before an immutable evidence plan exists.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path

import live_region_harness as harness
from live_fixture_population import check_population
from live_fixture_validation import validate_player_fixture
from live_harness_oracles import OracleFailure, logical_field, read_flash
from live_save_capture import capture_directory
from live_seed_players import validate_lineages


def _comparison_path(path: Path) -> Path:
    """Compare Windows namespace aliases without shortening paths used for I/O."""
    resolved = str(path.resolve())
    if os.name == "nt":
        if resolved[:8].casefold() == "\\\\?\\unc\\":
            resolved = "\\\\" + resolved[8:]
        elif resolved[:4] == "\\\\?\\":
            resolved = resolved[4:]
        resolved = resolved.casefold()
    return Path(resolved)


def attest_party(baseline: bytes, source: bytes) -> dict:
    """Explicit hoenn-box80-v1 ABI; empty-tail normalization only, no padding waiver.

    The fixed ZeroMonData (9840305105) leaves the emptied sixth slot all zero
    apart from mail=MAIL_NONE (offset 85). LoadPlayerParty then recomputes
    box.hpLost (offset 534) as 0 - 0, so the Party must be byte-identical.
    The pre-fix leftover (previous occupant's maxHP in hpLost) is rejected.
    """
    if len(baseline) != 604 or len(source) != 604 or baseline[:4] != b"\x05\0\0\0":
        raise OracleFailure("departure requires Party604 with five occupied slots")
    if baseline[504:604] != bytes(85) + b"\xff" + bytes(14):
        raise OracleFailure("departure sixth slot is not the exact empty ZeroMonData/mail sentinel")
    if source != baseline:
        raise OracleFailure("departure Party differs from the authored baseline")
    return {"field_id": 0x0101, "offset": 534, "old": 0, "new": 0,
            "reason": "empty sixth slot all zero (fixed ZeroMonData); LoadPlayerParty changed nothing"}


def derive_plan(base: dict, leg_name: str, evidence: dict) -> dict:
    for player in base.get("players", []):
        if not isinstance(player.get("custody_recipe"), dict) or not isinstance(player.get("population_recipe"), dict):
            raise OracleFailure("departure requires custody and population recipes")
        witnesses = player.get("shared_witnesses")
        if not isinstance(witnesses, list):
            raise OracleFailure("departure requires authored custody/Bag/PC witnesses")
        for fid in (0x0101, 0x010B, 0x010D, 0x0106, 0x0301):
            if sum(isinstance(w, dict) and w.get("field_id") == fid for w in witnesses) != 1:
                raise OracleFailure(f"departure requires exactly one witness for 0x{fid:04X}")
    harness.preflight(base, require_live_space=False)
    validate_lineages(base)
    leg = harness._leg(base, leg_name)
    if leg.get("runtime_field_exceptions") or leg.get("source_is_baseline"):
        raise OracleFailure("departure requires exact journal source and no projection exceptions")
    if evidence.get("leg") != leg_name or set(evidence.get("players", {})) != {"a", "b"}:
        raise OracleFailure("departure evidence needs the selected leg and both actors")
    release, _ = harness._paths(base)
    catalog = harness._read_json(release / "server-build-catalog.json", "departure catalog")
    descriptor = bytes.fromhex(catalog["shared_player_descriptor_hex"])
    result = copy.deepcopy(base)
    receipts = []
    for player in result["players"]:
        name = player["name"]
        item = evidence["players"][name]
        if not isinstance(item.get("journal_sha256"), str) or len(item["journal_sha256"]) != 64:
            raise OracleFailure("departure requires a pinned retained journal")
        journal = harness._read_evidence_journal(item, f"departure {name} journal")
        intent, stage = journal.get("intent", {}), journal.get("stage", {})
        if (journal.get("character_id") != player["character_id"]
                or journal.get("phase") not in ("committed", "adopted")
                or intent.get("source_world_id") != leg["source_world_id"]
                or intent.get("request", {}).get("portal_id") != leg["portal_id"]
                or stage.get("destination_world_id") != leg["destination_world_id"]
                or journal.get("terminal", {}).get("committed", {}).get("own_world_id") != leg["destination_world_id"]
                or intent.get("source_save_sha256") != item.get("journal_source_sha256")
                or item.get("source_is_baseline")):
            raise OracleFailure(f"departure {name} journal boundary differs")
        baseline = read_flash(Path(player["source_save"]))
        source, staged = (read_flash(Path(item[key])) for key in ("source", "staged"))
        if (baseline.sha256 != player["source_sha256"]
                or source.sha256 != intent.get("source_save_sha256")
                or staged.sha256 != stage.get("destination_save_sha256")):
            raise OracleFailure(f"departure {name} source/stage hash differs")
        chain = player.get("seed_lineage")
        if (not isinstance(chain, list) or len(chain) != 6 or baseline.generation != 6
                or source.generation != 7 or staged.generation != 8
                or baseline.lineage != source.lineage or source.lineage[:9] != staged.lineage[:9]
                or source.lineage[10:] != staged.lineage[10:]):
            raise OracleFailure(f"departure {name} generation/trainer lineage differs")
        validate_player_fixture(baseline, descriptor, player)
        normalization = attest_party(logical_field(baseline, descriptor, 0x0101),
                                     logical_field(source, descriptor, 0x0101))
        party = [w for w in player["shared_witnesses"] if w["field_id"] == 0x0101]
        if len(party) != 1 or party[0]["offset"] != 0 or party[0]["size"] != 604:
            raise OracleFailure("departure requires one full original Party witness")
        party[0]["sha256"] = hashlib.sha256(logical_field(source, descriptor, 0x0101)).hexdigest()
        # All other authored witnesses stay exact, including full Mail/Daycare.
        validate_player_fixture(source, descriptor, player)
        check_population(source, descriptor, player["population_recipe"])
        player["source_save"] = str(source.path.resolve())
        player["source_sha256"] = source.sha256
        player["seed_lineage"].append({"path": str(source.path.resolve()), "sha256": source.sha256})
        receipts.append({"player": name, "baseline_sha256": baseline.sha256,
                         "source_sha256": source.sha256, "staged_sha256": staged.sha256,
                         "journal_sha256": item["journal_sha256"], "normalization": normalization})
    harness.preflight(result, require_live_space=False)
    validate_lineages(result)
    verified = harness.verify_leg(result, leg_name, evidence, require_live_space=False)
    result["departure_attestation"] = {"version": 1, "leg": leg_name,
                                       "players": receipts, "verification": verified}
    return result


def publish_plan(plan_path: Path, leg_name: str, output: Path) -> Path:
    original = plan_path.read_bytes()
    base = json.loads(original.decode("utf-8-sig"))
    release, run_dir = harness._paths(base)
    evidence_path = capture_directory(run_dir, leg_name, "evidence") / "evidence.json"
    evidence_bytes = evidence_path.read_bytes()
    evidence = json.loads(evidence_bytes.decode("utf-8-sig"))
    target = output.resolve()
    protected = [_comparison_path(plan_path), _comparison_path(evidence_path)]
    protected += [_comparison_path(Path(p["source_save"])) for p in base["players"]]
    protected += [_comparison_path(Path(s["path"])) for p in base["players"] for s in p.get("seed_lineage", [])]
    protected += [_comparison_path(Path(item[key])) for item in evidence["players"].values()
                  for key in ("journal", "source", "staged", "template")]
    pending = target.with_suffix(target.suffix + ".tmp")
    target_identity, pending_identity = _comparison_path(target), _comparison_path(pending)
    if (target_identity in protected or pending_identity in protected
            or target_identity.is_relative_to(_comparison_path(release))
            or target_identity.drive == "c:" or not target.parent.is_dir()):
        raise OracleFailure("departure output must be an existing spare directory, outside protected inputs")
    result = derive_plan(base, leg_name, evidence)
    result["departure_attestation"].update(base_plan_sha256=hashlib.sha256(original).hexdigest(),
                                         evidence_sha256=hashlib.sha256(evidence_bytes).hexdigest())
    if plan_path.read_bytes() != original or evidence_path.read_bytes() != evidence_bytes:
        raise OracleFailure("departure input changed during attestation")
    encoded = (json.dumps(result, indent=2, sort_keys=True) + "\n").encode()
    if target.exists():
        if target.read_bytes() != encoded:
            raise OracleFailure("departure immutable output differs")
        return target
    owned = False
    try:
        with pending.open("xb") as file:
            owned = True
            file.write(encoded); file.flush(); os.fsync(file.fileno())
        if os.name == "nt":
            pending.rename(target)
        else:
            os.link(pending, target); pending.unlink()
        owned = False
    finally:
        if owned:
            pending.unlink(missing_ok=True)
    return target


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--leg", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    print(publish_plan(args.plan, args.leg, args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
