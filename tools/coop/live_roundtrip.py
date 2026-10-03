#!/usr/bin/env python3
"""Test-only cached two-actor ROM authoring and signed roundtrip orchestration.

Region/portal legs come from the plan. Fixture authoring currently requires the
explicit Hoenn debug/Main ABI. Interrupted mutation phases stop for focused
recovery; a completed run rechecks retained proof without API or client launch.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import uuid

import live_author_harbor as harbor
import live_author_population as population
import live_author_mail as mail
import live_author_pc_mail as pc
import live_author_third_mail as third
import live_author_daycare as daycare
import live_custody_plan as publisher
import live_seed_players as seeder
import live_prepare_clients as prepare
import live_attest_departure as departure
import live_run_leg as selected
import live_group_evidence as groups
import live_region_harness as harness
from live_save_capture import capture_directory
from live_signed_presence import bind_games

STAGES = ("harbor", "population", "mail", "pc_mail", "third_mail", "daycare")


def immutable(path: Path, value: dict) -> Path:
    encoded = (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()
    if path.exists():
        if path.read_bytes() != encoded:
            raise harness.HarnessFailure("roundtrip immutable artifact differs: " + path.name)
        return path
    pending = path.with_suffix(path.suffix + ".tmp")
    owned = False
    try:
        with pending.open("xb") as file:
            owned = True; file.write(encoded); file.flush(); os.fsync(file.fileno())
        if os.name == "nt": pending.rename(path)
        else: os.link(pending, path); pending.unlink()
        owned = False
    finally:
        if owned: pending.unlink(missing_ok=True)
    return path


def inputs(plan_path: Path, config: Path, cache_roots: dict, register: bool) -> tuple[dict, dict]:
    base = harness._read_json(plan_path, "roundtrip authoring plan")
    if (cache_roots.get("adapter") != "hoenn-debug-v1" or cache_roots.get("source_world_id") != 1
            or cache_roots.get("group_mode") != "api-preformed"):
        raise harness.HarnessFailure("roundtrip authoring adapter/source world is unsupported")
    outputs = cache_roots.get("outputs", {})
    if set(outputs) != set(STAGES) or any(not Path(p).is_absolute() for p in outputs.values()):
        raise harness.HarnessFailure("roundtrip needs six absolute factory cache directories")
    if len(base.get("legs", [])) != 2:
        raise harness.HarnessFailure("roundtrip requires exactly two configured legs")
    first, back = base["legs"]
    travel = cache_roots.get("preformed_group_inputs")
    if not isinstance(travel, list):
        raise harness.HarnessFailure("roundtrip requires an explicit preformed-group outbound input recipe")
    base = copy.deepcopy(base)
    base["legs"][0]["inputs"] = travel
    first, back = base["legs"]
    selected.validate_inputs(base, first["name"])
    selected.validate_inputs(base, back["name"])
    if (first["source_world_id"] != 1 or back.get("destination_base_leg") != first["name"]
            or first["destination_world_id"] != back["source_world_id"]
            or back["destination_world_id"] != first["source_world_id"]):
        raise harness.HarnessFailure("roundtrip factory requires Main world1 and a reversed parked-base leg")
    if any(p.get("authoring_menu_profile") != "hoenn-debug-v1" for p in base["players"]):
        raise harness.HarnessFailure("roundtrip factory ABI is unsupported")
    release, _ = harness._paths(base)
    source_cache = cache_roots.get("signed_cache_source")
    if source_cache is not None:
        harness.check_installed_family(base, Path(source_cache))
    from live_signed_presence import presence_prefix
    _, prefix = presence_prefix(base, first["name"])
    if any(a["key"] not in {"up", "down", "left", "right", "gba_a", "gba_b"}
           for a in travel[len(prefix):]):
        raise harness.HarnessFailure("preformed-group recipe permits only ferry controls after Continue")
    # Conservative closure: every live helper may participate in admission,
    # Windows control, caching, lineage, projection, or evidence validation.
    folder = Path(__file__).parent
    dependencies = {p.name: harness.digest(p) for p in sorted(folder.glob("live_*.py"))}
    dependencies["player_transfer_manifest.py"] = harness.digest(folder / "player_transfer_manifest.py")
    templates = {name: harness.digest(folder / "fixtures" / f"harbor-{name}.png") for name in harbor.TEMPLATES}
    account_tags = {name: hashlib.sha256(os.environ.get("COOP_HARNESS_USERNAME_" + name.upper(), "").encode()).hexdigest() for name in ("a", "b")}
    value = {"schema": 1, "authoring_plan_sha256": harness.digest(plan_path),
             "config_sha256": harness.digest(config), "adapter": cache_roots["adapter"], "source_world_id": cache_roots["source_world_id"],
             "group_mode": cache_roots["group_mode"],
             "cache_roots": {k: str(Path(v).resolve()) for k, v in outputs.items()},
             "preformed_group_inputs": travel,
             "signed_cache_source": str(Path(source_cache).resolve()) if source_cache is not None else None,
             "dependencies": dependencies, "templates": templates, "pillow_version": harbor.PILLOW_VERSION,
             "signed": {k: base[k] for k in ("release_id", "envelope_sha256", "catalog_sha256", "server_catalog_sha256")},
             "emulator_sha256": harness.digest(release / "runtime/mgba.exe"),
             "account_tags": account_tags, "register": register}
    return base, value


def preload_signed_cache(plan: dict, source: Path | None) -> dict:
    """Stage only immutable signed generation bytes; never clone acceptance/auth."""
    if source is None:
        return {"enabled": False}
    harness.check_installed_family(plan, source)
    release, _ = harness._paths(plan)
    envelope = harness._read_json(release / "release-envelope.json", "roundtrip preload envelope")
    signed = json.loads(harness.base64.b64decode(envelope["payload"], validate=True))
    names = [harness._artifact_relative(item["id"]) for item in signed["artifacts"]] + [".complete", ".signed-release"]
    actions, targets = [], {}
    for player in plan["players"]:
        profile = Path(player["profile_localappdata"])
        if not profile.is_absolute() or ".." in profile.parts:
            raise harness.HarnessFailure("roundtrip preload needs regular absolute profiles")
        target = profile / "Hoenn Sessions/runtime/releases/generations" / plan["release_id"]
        ancestor = target
        while not ancestor.exists(): ancestor = ancestor.parent
        harness.require_plain_cache_path(ancestor)
        if source.stat().st_dev != ancestor.stat().st_dev:
            raise harness.HarnessFailure("roundtrip preload hardlinks require the same volume")
        for name in names:
            original, destination = source / name, target / name
            parent = destination.parent
            while not parent.exists(): parent = parent.parent
            harness.require_plain_cache_path(parent)
            if destination.exists() or destination.is_symlink():
                harness.require_plain_cache_path(destination)
                if harness.digest(destination) != harness.digest(original):
                    raise harness.HarnessFailure("roundtrip preload refuses changed existing bytes")
            else:
                actions.append((original, destination))
        targets[player["name"]] = target
    # Validate every destination before creating any files. Each hardlink is
    # exclusive; an unexpected concurrent creator is a stopped boundary.
    for original, destination in actions:
        harness.check_c_space()
        destination.parent.mkdir(parents=True, exist_ok=True)
        harness.require_plain_cache_path(destination.parent)
        os.link(original, destination)
    for target in targets.values(): harness.check_installed_family(plan, target)
    return {"enabled": True, "source": str(source.resolve()), "new_hardlinks": len(actions),
            "targets": {name: str(path.resolve()) for name, path in targets.items()},
            "acceptance": "fresh signed client must write genuine account and acceptance records"}


def phase(root: Path, key: str, name: str, action) -> dict:
    receipt, attempted = root / (name + ".json"), root / (name + ".attempt.json")
    if receipt.exists():
        value = harness._read_json(receipt, "roundtrip phase")
        if value.get("input_key") != key or value.get("phase") != name:
            raise harness.HarnessFailure("roundtrip stale phase inputs")
        result = value["result"]
        if "plan_path" in result:
            harness.require_hash(Path(result["plan_path"]), result["plan_sha256"], "roundtrip phase plan")
        return result
    if attempted.exists():
        raise harness.HarnessFailure(f"roundtrip {name} was interrupted; ambiguous effects require focused recovery")
    immutable(attempted, {"input_key": key, "phase": name})
    result = action()
    immutable(receipt, {"input_key": key, "phase": name, "result": result})
    return result


def plan_receipt(path: Path, **extra) -> dict:
    return {"plan_path": str(path.resolve()), "plan_sha256": harness.digest(path), **extra}


def author(base_path: Path, base: dict, config: Path, roots: dict, root: Path) -> dict:
    parents, sources = {}, {}
    for name in ("a", "b"):
        harness.check_c_space()
        h = harbor.author_player(base, name.upper(), Path(roots["harbor"]), config)
        hroot = Path(roots["harbor"]) / h["cache_key"]
        p = population.author_player(base, name, hroot, Path(roots["population"]), config)
        proot = Path(roots["population"]) / p["cache_key"]
        sources[name] = (proot, p)
        population_plan = copy.deepcopy(base)
        player = next(p for p in population_plan["players"] if p["name"] == name)
        player.update(source_save=str(proot / "population.sav"), source_sha256=p["validated"]["population"]["save_sha256"],
                      seed_lineage=p["seed_lineage"], shared_witnesses=p["validated"]["population"]["shared_witnesses"])
        m = mail.author_player(population_plan, player, Path(roots["mail"]), config)
        held = Path(roots["mail"]) / m["cache_key"]
        c = pc.author_player(population_plan, name, held, Path(roots["pc_mail"]), config)
        pcroot = Path(roots["pc_mail"]) / c["cache_key"]
        t = third.author_player(population_plan, name, pcroot, held, Path(roots["third_mail"]), config)
        troot = Path(roots["third_mail"]) / t["cache_key"]
        d = daycare.author_player(population_plan, name, troot, pcroot, held, Path(roots["daycare"]), config)
        parents[name] = {"held_mail_root": str(held), "pc_mail_root": str(pcroot),
                         "third_mail_root": str(troot), "daycare_root": str(Path(roots["daycare"]) / d["cache_key"])}
    # Publisher needs each newly authored population source, not a fabricated
    # save. Reuse the exact per-player source and recipe used above.
    authored_base = copy.deepcopy(base)
    for name, (proot, p) in sources.items():
        player = next(p0 for p0 in authored_base["players"] if p0["name"] == name)
        player.update(source_save=str(proot / "population.sav"),
                      source_sha256=p["validated"]["population"]["save_sha256"], seed_lineage=p["seed_lineage"],
                      shared_witnesses=p["validated"]["population"]["shared_witnesses"])
    source_plan = immutable(root / "population-plan.json", authored_base)
    output = publisher.publish_plan(source_plan, parents, config, root / "custody-plan.json")
    return plan_receipt(output, parents=parents)


def seed_plan(plan: dict, root: Path, register: bool) -> dict:
    result = seeder.seed(plan, register=register)
    actors = result.get("players", {})
    if set(actors) != {"a", "b"} or len({a["character_id"] for a in actors.values()}) != 2:
        raise harness.HarnessFailure("roundtrip seed returned missing or duplicate actors")
    actual = copy.deepcopy(plan)
    for player in actual["players"]:
        actor = actors[player["name"]]
        if actor["source_sha256"] != player["source_sha256"]:
            raise harness.HarnessFailure("roundtrip seed source differs")
        player["character_id"] = actor["character_id"]
    actual["custody_plan"]["character_ids"] = "verified against roundtrip seed receipt"
    actual["custody_plan"]["purpose"] = "seeded-journey-plan"
    path = immutable(root / "signed-plan.json", actual)
    return plan_receipt(path, actors=actors)


def initial_heads(plan: dict, actors: dict, expected_group: dict | None = None) -> dict:
    """Validate seeded Main heads under bounded read leases before launch."""
    _, parsed = harness._health_url(plan)
    if parsed.hostname not in ("localhost", "127.0.0.1", "::1"):
        raise harness.HarnessFailure("roundtrip accounts are loopback only")
    server = f"{parsed.scheme}://{parsed.netloc}"
    checked = {}
    for player in plan["players"]:
        harness.check_c_space()
        if harness._mgba_pid_for(Path(player["profile_localappdata"])) is not None:
            raise harness.HarnessFailure("roundtrip requires closed signed runtimes before leased probes")
        name, character = player["name"], player["character_id"]
        username, password = os.environ.get("COOP_HARNESS_USERNAME_" + name.upper()), os.environ.get("COOP_HARNESS_PASSWORD")
        if not username or not password:
            raise harness.HarnessFailure("roundtrip credentials missing")
        status, body = groups._call(server, "POST", "/v1/auth/login", payload={"api_version": 1, "username": username, "password": password})
        login = json.loads(groups._expect(status, body, 200, "roundtrip head login"))
        if login["character_id"] != character: raise harness.HarnessFailure("roundtrip head actor differs")
        token = login["access_token"]
        lease = groups._acquire_after_release(server, {"api_version": 1, "character_id": character,
                    "client_instance_id": str(uuid.uuid4()), "idempotency_key": str(uuid.uuid4())}, token, 0)
        try:
            revision = actors[name]["revision"]
            if lease["current_revision"] != revision: raise harness.HarnessFailure("roundtrip head advanced or changed")
            status, body = groups._call(server, "GET", f"/v1/characters/{character}/snapshots", token=token, lease=lease)
            snapshots = json.loads(groups._expect(status, body, 200, "roundtrip snapshots"))["snapshots"]
            current = [s for s in snapshots if s["revision"] == revision]
            if len(current) != 1 or current[0]["rom_world_id"] != plan["legs"][0]["source_world_id"]:
                raise harness.HarnessFailure("roundtrip head world differs")
            files = [f for f in current[0]["files"] if f["artifact"] == "character.sav"]
            if len(files) != 1 or files[0]["sha256"] != player["source_sha256"]:
                raise harness.HarnessFailure("roundtrip head manifest differs")
            status, body = groups._call(server, "GET", f"/v1/characters/{character}/resume-package/artifacts/character.sav?revision={revision}", token=token, lease=lease)
            groups._expect(status, body, 200, "roundtrip head save")
            if hashlib.sha256(body).hexdigest() != player["source_sha256"]:
                raise harness.HarnessFailure("roundtrip raw head differs")
            checked[name] = {"sha256": player["source_sha256"], "revision": revision, "snapshot_id": current[0]["snapshot_id"]}
            if expected_group is not None:
                status, body = groups._call(server, "GET", "/v1/groups/" + expected_group["group_id"], token=token, lease=lease)
                actual = harness._sanitize_group_evidence(json.loads(groups._expect(status, body, 200, "roundtrip initial group")))
                if actual != expected_group: raise harness.HarnessFailure("roundtrip initial group differs")
        finally:
            original = sys.exc_info()[1]
            try:
                status, body = groups._call(server, "POST", "/v1/sessions/release", token=token, lease=lease,
                    payload={"api_version": 1, "character_id": character, "session_id": lease["session_id"],
                             "current_revision": lease["current_revision"], "session_epoch": lease["session_epoch"],
                             "client_instance_id": lease["client_instance_id"], "idempotency_key": str(uuid.uuid4())})
                groups._expect(status, body, 200, "roundtrip head release")
            except Exception as cleanup:
                if original is None: raise
                original.add_note(f"Roundtrip head release also failed: {cleanup}")
    return checked


def form_group(plan: dict, actors: dict, root: Path) -> dict:
    """Use public fenced invite/accept APIs; uncertain mutation is never replayed."""
    heads = initial_heads(plan, actors)
    _, parsed = harness._health_url(plan)
    server = f"{parsed.scheme}://{parsed.netloc}"
    leases, tokens = {}, {}
    request_ids = {"invite": str(uuid.uuid4()), "accept": str(uuid.uuid4())}
    immutable(root / "group-request-ids.json", request_ids)
    try:
        for player in plan["players"]:
            name, character = player["name"], player["character_id"]
            status, body = groups._call(server, "POST", "/v1/auth/login", payload={"api_version": 1,
                "username": os.environ["COOP_HARNESS_USERNAME_" + name.upper()], "password": os.environ["COOP_HARNESS_PASSWORD"]})
            login = json.loads(groups._expect(status, body, 200, "roundtrip group login"))
            if login["character_id"] != character: raise harness.HarnessFailure("roundtrip group actor differs")
            tokens[name] = login["access_token"]
            leases[name] = groups._acquire_after_release(server, {"api_version": 1, "character_id": character,
                "client_instance_id": str(uuid.uuid4()), "idempotency_key": str(uuid.uuid4())}, tokens[name], 0)
            if leases[name]["current_revision"] != heads[name]["revision"]:
                raise harness.HarnessFailure("roundtrip group head changed")
            # Under the concurrent leases, require the same exact manifest
            # pinned by the preceding released read, not generation heuristics.
            status, body = groups._call(server, "GET", f"/v1/characters/{character}/snapshots", token=tokens[name], lease=leases[name])
            snapshots = json.loads(groups._expect(status, body, 200, "roundtrip group head"))["snapshots"]
            current = [s for s in snapshots if s["revision"] == heads[name]["revision"]]
            if (len(current) != 1 or current[0]["snapshot_id"] != heads[name]["snapshot_id"]
                    or current[0]["rom_world_id"] != plan["legs"][0]["source_world_id"]
                    or [f["sha256"] for f in current[0]["files"] if f["artifact"] == "character.sav"] != [heads[name]["sha256"]]):
                raise harness.HarnessFailure("roundtrip group manifest changed")
        players = {p["name"]: p for p in plan["players"]}
        def fence(name, key):
            return {"api_version": 1, "character_id": players[name]["character_id"], "idempotency_key": key,
                    **{k: leases[name][k] for k in ("session_id", "current_revision", "session_epoch", "client_instance_id")}}
        status, body = groups._call(server, "POST", "/v1/groups/invitations", token=tokens["a"], lease=leases["a"],
            payload={**fence("a", request_ids["invite"]), "invitee_character_id": players["b"]["character_id"]})
        invitation = json.loads(groups._expect(status, body, 201, "roundtrip invite"))
        if (invitation.get("inviter_character_id") != players["a"]["character_id"]
                or invitation.get("invitee_character_id") != players["b"]["character_id"]):
            raise harness.HarnessFailure("roundtrip invitation actors differ")
        invitation_id = str(uuid.UUID(invitation["invitation_id"]))
        immutable(root / "group-invitation.json", {k: invitation[k] for k in
                  ("api_version", "invitation_id", "inviter_character_id", "invitee_character_id", "expires_at") if k in invitation})
        status, body = groups._call(server, "POST", f"/v1/groups/invitations/{invitation_id}/accept", token=tokens["b"], lease=leases["b"],
            payload=fence("b", request_ids["accept"]))
        accepted = json.loads(groups._expect(status, body, 200, "roundtrip accept"))["group"]
        group = harness._sanitize_group_evidence(accepted)
        group_id = str(uuid.UUID(group["group_id"]))
        catalog = harness._read_json(Path(plan["release_dir"]) / "release_catalog.json", "roundtrip group catalog")
        world = next(w for w in catalog["worlds"] if w["world_id"] == plan["legs"][0]["source_world_id"])
        if (len(group["members"]) != 2 or {m["character_id"] for m in group["members"]} != {p["character_id"] for p in plan["players"]}
                or group["world_zone"]["region"] not in world["presence_regions"]):
            raise harness.HarnessFailure("roundtrip accepted group/zone differs")
        for name in ("a", "b"):
            status, body = groups._call(server, "GET", "/v1/groups/" + group_id, token=tokens[name], lease=leases[name])
            inspected = json.loads(groups._expect(status, body, 200, "roundtrip inspect group"))
            if harness._sanitize_group_evidence(inspected) != group or inspected["world_zone"] != accepted["world_zone"]:
                raise harness.HarnessFailure("roundtrip group inspect differs")
        return {"group": group, "world_zone": accepted["world_zone"], "heads": heads,
                "invitation_id": invitation_id, "request_ids": request_ids}
    finally:
        original, errors = sys.exc_info()[1], []
        for name, lease in leases.items():
            try:
                status, body = groups._call(server, "POST", "/v1/sessions/release", token=tokens[name], lease=lease,
                    payload={"api_version": 1, "character_id": next(p["character_id"] for p in plan["players"] if p["name"] == name),
                             "idempotency_key": str(uuid.uuid4()), **{k: lease[k] for k in
                             ("session_id", "current_revision", "session_epoch", "client_instance_id")}})
                groups._expect(status, body, 200, "roundtrip group release")
            except Exception as error: errors.append(error)
        if errors:
            if original is None:
                original = errors.pop(0)
                for error in errors: original.add_note(f"Group lease release also failed: {error}")
                raise original
            for error in errors: original.add_note(f"Group lease release also failed: {error}")


def cleanup(plan: dict, desktops: dict, receipt: Path) -> None:
    original, failures = sys.exc_info()[1], []
    for action in (lambda: harness.stop_runtime(plan, desktops), lambda: harness.close_desktops(desktops)):
        try: action()
        except Exception as error: failures.append(error)
    alive = [pid for pid in desktops.values() if harness.psutil.pid_exists(pid)]
    if alive: failures.append(harness.HarnessFailure("roundtrip owned desktops still alive"))
    try:
        immutable(receipt, {"ok": not failures, "desktop_pids": desktops, "alive_pids": alive,
                            "errors": [str(error) for error in failures],
                            "scope": "owned desktops absent and graceful runtime stop returned; detached orphan inventory not independently attested"})
    except Exception as error: failures.append(error)
    if failures:
        if original is None:
            original = failures.pop(0)
            for error in failures: original.add_note(f"Roundtrip cleanup also failed: {error}")
            raise original
        for error in failures: original.add_note(f"Roundtrip cleanup also failed: {error}")


def outbound(plan: dict, actors: dict, root: Path, group: dict) -> dict:
    leg = plan["legs"][0]
    heads = initial_heads(plan, actors, group)
    desktops = harness.launch(plan)
    try:
        pids = harness.start_games(plan, desktops)
        bind_games(plan, leg, pids)
        selected.bind_loaded_sources(plan, pids, desktops, {"players": heads})
        harness.drive(plan, leg["name"], pids)
        arrived = harness.wait_arrival_games(plan, leg, pids)
        harness.continue_arrivals(plan, leg, arrived)
        group_ids = set()
        for player in plan["players"]:
            journals = harness._journal_candidates(Path(player["profile_localappdata"]), leg, player["character_id"])
            if not journals: raise harness.HarnessFailure("roundtrip partial outbound crossing")
            group_ids.add(journals[0][2]["intent"]["request"]["group_id"])
        if group_ids != {group["group_id"]}:
            raise harness.HarnessFailure("roundtrip outbound group differs")
        harness.stop_runtime(plan, desktops)
        _, run_dir = harness._paths(plan)
        output = capture_directory(run_dir, leg["name"], "server-evidence")
        collected = groups.collect(plan, leg["name"], next(iter(group_ids)), output, lease_wait_seconds=15)
        evidence_plan = dict(plan, group_evidence=collected["group"], evidence_roots=[str(output), *plan.get("evidence_roots", [])])
        evidence_plan["legs"] = [dict(item, group_evidence=collected["group"]) if item["name"] == leg["name"] else item for item in plan["legs"]]
        harness.discover_evidence(evidence_plan, leg["name"])
        source = immutable(root / "outbound-evidence-plan.json", evidence_plan)
        path = departure.publish_plan(source, leg["name"], root / "attested-plan.json")
        return plan_receipt(path)
    finally:
        cleanup(plan, desktops, root / "outbound-cleanup.json")


def recertify_outbound(plan: dict, root: Path) -> None:
    """Reopen every original outbound artifact, including the gen6 baseline."""
    attestation = plan.get("departure_attestation", {})
    baseline_path = root / "outbound-evidence-plan.json"
    harness.require_hash(baseline_path, attestation.get("base_plan_sha256", ""), "completed outbound baseline plan")
    baseline = harness._read_json(baseline_path, "completed outbound baseline")
    first = baseline["legs"][0]["name"]
    _, run_dir = harness._paths(baseline)
    evidence_path = capture_directory(run_dir, first, "evidence") / "evidence.json"
    harness.require_hash(evidence_path, attestation.get("evidence_sha256", ""), "completed outbound evidence")
    evidence = harness._read_json(evidence_path, "completed outbound evidence")
    # Existing derive_plan rechecks the original fixture/lineages, pinned
    # private journal, exact source/stage hashes, sole offset534 normalization,
    # custody/population semantics, and strict 27/16 projection. It publishes
    # nothing; only existing local preflight/verify checkpoints are appended.
    derived = departure.derive_plan(baseline, first, evidence)
    expected = copy.deepcopy(plan)
    expected_attestation = expected["departure_attestation"]
    expected_attestation.pop("base_plan_sha256", None)
    expected_attestation.pop("evidence_sha256", None)
    if derived != expected:
        raise harness.HarnessFailure("completed outbound attestation differs from retained plan")


def complete_check(plan: dict, root: Path, report: dict) -> None:
    for label in ("outbound-cleanup", "return-cleanup"):
        if harness._read_json(root / (label + ".json"), "roundtrip cleanup").get("ok") is not True:
            raise harness.HarnessFailure("roundtrip lacks successful cleanup proof")
    recertify_outbound(plan, root)
    leg = plan["legs"][1]
    _, run_dir = harness._paths(plan)
    evidence = harness._read_json(capture_directory(run_dir, leg["name"], "evidence") / "evidence.json", "roundtrip retained return")
    if harness.verify_leg(plan, leg["name"], evidence, require_live_space=False) != report:
        raise harness.HarnessFailure("roundtrip completed return proof differs")


def return_leg(plan: dict, root: Path) -> dict:
    _, run_dir = harness._paths(plan)
    before = harness._read_json(run_dir / "checkpoint.json", "roundtrip return checkpoints").get("events", [])
    try:
        result = selected.run_leg(plan, plan["legs"][1]["name"])
        return {"report": result}
    finally:
        original = sys.exc_info()[1]
        try:
            events = harness._read_json(run_dir / "checkpoint.json", "roundtrip return checkpoints").get("events", [])[len(before):]
            launches = [e for e in events if e.get("boundary") == "signed-clients-launched"]
            if not launches:
                if original is None: raise harness.HarnessFailure("roundtrip return lacks owned launch receipt")
            else:
                cleanup(plan, launches[-1]["desktop_pids"], root / "return-cleanup.json")
        except Exception as error:
            if original is None: raise
            original.add_note(f"Return cleanup receipt also failed: {error}")


def run(plan_path: Path, config: Path, cache_roots: dict, output: Path, *, register: bool = False) -> dict:
    base, pinned = inputs(plan_path, config, cache_roots, register)
    key = hashlib.sha256(json.dumps(pinned, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if output.resolve().drive.casefold() == "c:" or not output.is_dir():
        raise harness.HarnessFailure("roundtrip output must be an existing spare-volume directory")
    root = output / key
    done = root / "complete.json"
    if done.exists():
        expected_inputs = (json.dumps(pinned, sort_keys=True, indent=2) + "\n").encode()
        if not (root / "inputs.json").is_file() or (root / "inputs.json").read_bytes() != expected_inputs:
            raise harness.HarnessFailure("roundtrip completed inputs are missing or changed")
        receipt = harness._read_json(done, "roundtrip completion")
        if receipt.get("input_key") != key: raise harness.HarnessFailure("roundtrip stale completion")
        harness.require_hash(Path(receipt["plan_path"]), receipt["plan_sha256"], "roundtrip completed plan")
        complete_check(harness._read_json(Path(receipt["plan_path"]), "roundtrip completed plan"), root, receipt["report"])
        return dict(receipt, reused=True)
    harness.check_c_space()
    root.mkdir(exist_ok=True)
    immutable(root / "inputs.json", pinned)
    base = copy.deepcopy(base)
    base["run_dir"] = str(root / "journey")
    # Serialize all non-idempotent phases; a second invocation must not lease
    # accounts or send controls while the first invocation is running.
    lock = root / ".owned-run.lock"
    lock.open("x").close()
    try:
            harness.preflight(base, profiles_ready=False)
            a = phase(root, key, "authored", lambda: author(plan_path, base, config, cache_roots["outputs"], root))
            authored = harness._read_json(Path(a["plan_path"]), "roundtrip authored plan")
            s = phase(root, key, "seeded", lambda: seed_plan(authored, root, register))
            signed = harness._read_json(Path(s["plan_path"]), "roundtrip signed plan")
            g = phase(root, key, "grouped", lambda: form_group(signed, s["actors"], root))
            phase(root, key, "preloaded", lambda: preload_signed_cache(signed, Path(cache_roots["signed_cache_source"]) if cache_roots.get("signed_cache_source") is not None else None))
            phase(root, key, "prepared", lambda: prepare.prepare(signed))
            o = phase(root, key, "outbound", lambda: outbound(signed, s["actors"], root, g["group"]))
            attested = harness._read_json(Path(o["plan_path"]), "roundtrip attested plan")
            r = phase(root, key, "returned", lambda: return_leg(attested, root))
            complete_check(attested, root, r["report"])
            result = dict(plan_receipt(Path(o["plan_path"])), input_key=key, report=r["report"], reused=False)
            immutable(done, result)
            return result
    finally:
        lock.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--authoring-plan", type=Path, required=True)
    parser.add_argument("--mgba-config", type=Path, required=True)
    parser.add_argument("--cache-roots-json", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--register", action="store_true")
    args = parser.parse_args()
    try:
        result = run(args.authoring_plan, args.mgba_config, harness._read_json(args.cache_roots_json, "roundtrip caches"), args.output_root, register=args.register)
    except Exception as error:
        print(json.dumps({"ok": False, "error": str(error), "notes": getattr(error, "__notes__", [])})); return 1
    print(json.dumps({"ok": True, **result})); return 0


if __name__ == "__main__":
    raise SystemExit(main())
