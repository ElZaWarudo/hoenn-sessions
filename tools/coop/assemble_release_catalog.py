#!/usr/bin/env python3
"""Assemble the multi-world release catalogs from freshly built world outputs.

`assemble` stages every registered world's ROM and manifests, binds each
first-arrival save to the exact ROM it was attested against, writes the
canonical client region catalog (release_catalog.json), validates it with
tools.rom_release_catalog (including the server's V2 arrival-save parser), and
derives the server build catalog (server-build-catalog.json, schema 3).

`signing-artifacts` copies the region catalog and the signed per-world files
into a Windows or game bundle and prints the dynamic `ID=PATH` artifact list
in the canonical order coop-release-tool expects.

Arrival saves are never regenerated here. If a built ROM differs from the ROM
an arrival save was attested against, assembly stops with a recertification
error; tools/coop/recert_arrival.py is the manual helper for that step.

object_catalog_sha256: shared object contract fingerprint, version 1
--------------------------------------------------------------------
Every world entry's `object_catalog_sha256` is derived here from the built
manifests; no digest is configured. It is SHA-256 over the canonical JSON
(sorted keys, `,`/`:` separators, ASCII, one trailing LF) of

    {"schema": "hoenn-sessions/shared-object-contract-fingerprint",
     "version": 1,
     "contracts": {"experience_table": ..., "object_scalar": ...,
                   "player_transfer": ...}}

holding only the world-independent content of the three cross-ROM contracts
that `multiworld_build_ci.py verify` already requires to agree:

* experience_table (experience_table_manifest.py, schema 1): symbol, size and
  the SHA-256 of the gExperienceTables bytes (re-hashed here from the ROM).
* object_scalar (object_contract_manifest.py, schema 4): scope; descriptor
  size and digest; count-probe size; for each shared-ID table (items, species,
  moves, abilities, TM/HM move IDs) the size, record count and stride, pointer
  and text offsets, pointer-free scalar digest, pointer-presence digest and
  bounded display-text digest, plus the move AdditionalEffect layout and
  digest. rom_release_catalog re-verifies this manifest against the ROM.
* player_transfer (player_transfer_manifest.py, schema 3): symbol, schema
  version, descriptor size and digest, field count, fields, saved spans,
  re-key and Day Care custody field IDs.

Per-world fields are excluded: ROM digests, link addresses, raw table digests
(they embed ROM pointers), build IDs and world names. Each manifest must name
the built ROM. Every manifest field is classified as included or excluded; an
unknown field or another generator schema version is refused.

NOT covered: graphics, palettes, icons, cries, callbacks and other function
pointers, field and battle scripts, menus and UI, bag/PC capacity beyond the
saved spans, and full object semantics. Equal fingerprints mean the covered
bytes agree, not that every shared object behaves identically. Adding or
removing coverage, or reclassifying a field, changes what the digest means and
requires bumping FINGERPRINT_VERSION together with the version declared in
data/rom_world_release.json.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path

if not __package__:
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools import rom_release_catalog
from tools.rom_world_registry import load_registry
from tools.coop import generate_server_build_catalog

DIGEST_LENGTH = 64
MAX_SERVER_CATALOG_BYTES = 64 * 1024
MAX_REGION_CATALOG_BYTES = 256 * 1024
ROM_BASE = 0x08000000
FINGERPRINT_SCHEMA = "hoenn-sessions/shared-object-contract-fingerprint"
FINGERPRINT_SOURCE = "shared-object-contract-fingerprint"
FINGERPRINT_VERSION = 1
# Version 1 field classification: (contract, build output, generator
# schema_version, included keys, excluded per-world keys). Generator schema
# versions are pinned on purpose; a generator bump is a fingerprint bump.
_EXPERIENCE = ("experience_table", "experience_table_manifest.json", 1,
               ("schema_version", "symbol", "size", "sha256"), ("rom_sha256", "address"))
_OBJECT = ("object_scalar", "object_scalar_manifest.json", 4,
           ("schema_version", "scope", "descriptor", "count_probe", "tables"), ("rom_sha256",))
_TRANSFER = ("player_transfer", "player_transfer_manifest.json", 3,
             ("schema_version", "symbol", "size", "sha256", "descriptor_size", "field_count",
              "fields", "spans", "rekey_field_ids", "daycare_custody_field_ids"),
             ("rom_sha256", "address"))
FINGERPRINT_CONTRACTS = (_EXPERIENCE, _OBJECT, _TRANSFER)
_OBJECT_TABLES = ("gItemsInfo", "gSpeciesInfo", "gMovesInfo", "gAbilitiesInfo", "gTMHMItemMoveIds")
_OBJECT_TABLE_KEYS = (("size", "record_count", "record_stride", "pointer_offsets", "text_offsets",
                       "scalar_sha256", "pointer_presence_sha256", "display_text_sha256"),
                      ("address", "raw_sha256"))
_OBJECT_MOVE_KEYS = ("additional_effect_stride", "additional_effect_count_offset",
                     "additional_effect_count_mask", "additional_effect_sha256")
_OBJECT_DESCRIPTOR_KEYS = (("size", "sha256"), ("address",))
_OBJECT_PROBE_KEYS = (("size",), ("address",))
# Per-world build outputs (multiworld_build_ci names) -> release names.
WORLD_FILES = (
    ("game.gba", "game.gba"),
    ("bridge_manifest.json", "bridge_manifest.json"),
    ("player_transfer_manifest.json", "player_transfer.json"),
    ("experience_table_manifest.json", "experience_table_manifest.json"),
    ("map_binding_manifest.json", "map_binding_manifest.json"),
    ("object_scalar_manifest.json", "object_scalar_manifest.json"),
    ("generated_addresses.lua", "generated_addresses.lua"),
)
SIGNED_WORLD_FILES = (("rom", "game.gba"), ("compatibility", "bridge_manifest.json"),
                      ("player-transfer", "player_transfer.json"))


class AssemblyError(ValueError):
    """Release inputs are inconsistent; nothing may be signed."""


class RecertifyError(AssemblyError):
    """A built ROM differs from the ROM an arrival save was attested against."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_json(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
            + "\n").encode("ascii")


def _digest(value: object, label: str) -> str:
    if (not isinstance(value, str) or len(value) != DIGEST_LENGTH
            or any(c not in "0123456789abcdef" for c in value)):
        raise AssemblyError(f"{label} must be a lowercase SHA-256 hex digest")
    return value


def _load_json(path: Path, label: str) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, ValueError) as exc:
        raise AssemblyError(f"cannot read {label} {path}: {exc}") from exc
    if not isinstance(data, dict) or data.get("schema_version") != 1:
        raise AssemblyError(f"unsupported {label} schema: {path}")
    return data


def object_catalog_config(config: dict) -> None:
    """The tracked config names the derivation; it never carries a digest."""
    if config.get("object_catalog") != {"source": FINGERPRINT_SOURCE,
                                        "version": FINGERPRINT_VERSION}:
        raise AssemblyError(
            f"release config object_catalog must be exactly {{\"source\": \"{FINGERPRINT_SOURCE}\", "
            f"\"version\": {FINGERPRINT_VERSION}}}; configured digests are not accepted")


def _project(value: object, included: tuple[str, ...], excluded: tuple[str, ...],
             label: str) -> dict:
    if not isinstance(value, dict):
        raise AssemblyError(f"{label} must be an object")
    unknown = set(value) - set(included) - set(excluded)
    missing = set(included) - set(value)
    if unknown or missing:
        raise AssemblyError(
            f"{label} fields do not match fingerprint v{FINGERPRINT_VERSION} "
            f"(unknown {sorted(unknown)}, missing {sorted(missing)}); "
            "classifying a field requires a fingerprint version bump")
    return {key: value[key] for key in included}


def contract_projection(manifests: dict[str, dict]) -> dict:
    """World-independent content of one world's cross-ROM contract manifests."""
    if set(manifests) != {contract[0] for contract in FINGERPRINT_CONTRACTS}:
        raise AssemblyError("fingerprint needs exactly the experience, object and transfer manifests")
    contracts = {}
    for contract, _, schema, included, excluded in FINGERPRINT_CONTRACTS:
        manifest = manifests[contract]
        if (not isinstance(manifest, dict) or type(manifest.get("schema_version")) is not int
                or manifest["schema_version"] != schema):
            raise AssemblyError(f"{contract} manifest schema is not the one fingerprint "
                                f"v{FINGERPRINT_VERSION} covers (expected {schema})")
        contracts[contract] = _project(manifest, included, excluded, contract)
    obj = contracts["object_scalar"]
    obj["descriptor"] = _project(obj["descriptor"], *_OBJECT_DESCRIPTOR_KEYS, "object_scalar descriptor")
    obj["count_probe"] = _project(obj["count_probe"], *_OBJECT_PROBE_KEYS, "object_scalar count_probe")
    tables = obj["tables"]
    if not isinstance(tables, dict) or set(tables) != set(_OBJECT_TABLES):
        raise AssemblyError(f"object_scalar tables do not match fingerprint v{FINGERPRINT_VERSION}")
    obj["tables"] = {
        name: _project(tables[name],
                       _OBJECT_TABLE_KEYS[0] + (_OBJECT_MOVE_KEYS if name == "gMovesInfo" else ()),
                       _OBJECT_TABLE_KEYS[1], f"object_scalar {name}")
        for name in _OBJECT_TABLES}
    _digest(contracts["experience_table"]["sha256"], "experience_table sha256")
    _digest(contracts["player_transfer"]["sha256"], "player_transfer sha256")
    _digest(obj["descriptor"]["sha256"], "object_scalar descriptor sha256")
    for name, table in obj["tables"].items():
        for key, value in table.items():
            if key.endswith("_sha256"):
                _digest(value, f"object_scalar {name} {key}")
    return contracts


def fingerprint_document(contracts: dict) -> bytes:
    return canonical_json({"schema": FINGERPRINT_SCHEMA, "version": FINGERPRINT_VERSION,
                           "contracts": contracts})


def world_contract_manifests(source: Path, rom_digest: str) -> dict[str, dict]:
    """Load one world's contract manifests and bind each to its built ROM."""
    manifests = {}
    for contract, filename, *_ in FINGERPRINT_CONTRACTS:
        path = source / filename
        try:
            manifest = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, ValueError) as exc:
            raise AssemblyError(f"cannot read {contract} manifest {path}: {exc}") from exc
        if not isinstance(manifest, dict) or manifest.get("rom_sha256") != rom_digest:
            raise AssemblyError(f"{path} does not describe the built ROM")
        manifests[contract] = manifest
    experience = manifests["experience_table"]
    address, size = experience.get("address"), experience.get("size")
    if type(address) is not int or type(size) is not int or size <= 0 or address < ROM_BASE:
        raise AssemblyError(f"{source / _EXPERIENCE[1]} has an invalid location")
    with (source / "game.gba").open("rb") as rom:
        rom.seek(address - ROM_BASE)
        payload = rom.read(size)
    if len(payload) != size or hashlib.sha256(payload).hexdigest() != experience.get("sha256"):
        raise AssemblyError(f"{source / _EXPERIENCE[1]} does not match the built ROM bytes")
    return manifests


def object_catalog_fingerprint(dist: Path, rom_digests: dict[str, str]) -> tuple[str, bytes]:
    """Derive each world's fingerprint document and require them byte-identical."""
    reference = None
    for name, rom_digest in rom_digests.items():
        document = fingerprint_document(
            contract_projection(world_contract_manifests(dist / name, rom_digest)))
        if reference is None:
            reference = (name, document)
        elif document != reference[1]:
            mine = json.loads(document)["contracts"]
            theirs = json.loads(reference[1])["contracts"]
            differing = sorted(key for key in mine if mine[key] != theirs.get(key))
            raise AssemblyError(
                f"world '{name}' shared object contract differs from '{reference[0]}' "
                f"({', '.join(differing)}); a world whose shared objects cannot be "
                "represented identically is not travel-compatible")
    if reference is None:
        raise AssemblyError("no worlds to fingerprint")
    return hashlib.sha256(reference[1]).hexdigest(), reference[1]


def _smoke_arrivals(path: Path) -> dict[str, set[tuple[int, int, int]]]:
    data = _load_json(path, "ROM world arrivals")
    return {name: {(entry["map_group"], entry["map_number"], entry["map_layout_id"])
                   for entry in entries}
            for name, entries in data.get("worlds", {}).items()}


def plan_worlds(registry: Path, config: dict, attested: dict, smoke_path: Path) -> list[dict]:
    """Join the registry, release config and attestations into one world plan."""
    _, builds = load_registry(registry)
    worlds_config = config.get("worlds")
    attested_worlds = attested.get("worlds")
    if not isinstance(worlds_config, dict) or set(worlds_config) != set(builds):
        raise AssemblyError("release config must declare exactly the registered worlds")
    if not isinstance(attested_worlds, dict) or set(attested_worlds) != set(builds):
        raise AssemblyError("release arrivals must declare exactly the registered worlds")
    smoke = _smoke_arrivals(smoke_path)
    ids = {name: entry["world_id"] for name, entry in builds.items()}
    plan = []
    for name, entry in sorted(builds.items(), key=lambda item: item[1]["world_id"]):
        world = worlds_config[name]
        arrivals = world.get("arrivals")
        proofs = attested_worlds[name].get("arrivals") if isinstance(attested_worlds[name], dict) else None
        if not isinstance(arrivals, dict) or not arrivals:
            raise AssemblyError(f"{name} needs at least one arrival")
        if not isinstance(proofs, dict) or set(proofs) != set(arrivals):
            raise AssemblyError(f"{name} arrival attestations must match its release arrivals")
        for portal_id, arrival in arrivals.items():
            key = (arrival.get("map_group"), arrival.get("map_number"), arrival.get("map_layout_id"))
            if key not in smoke.get(name, set()):
                raise AssemblyError(f"{name} {portal_id} arrival is not covered by "
                                    "data/rom_world_arrivals.json build checks")
            proof = proofs[portal_id]
            if (not isinstance(proof, dict) or not isinstance(proof.get("sav_path"), str)
                    or not isinstance(proof.get("receipt"), (dict, str)) or not proof["receipt"]):
                raise AssemblyError(f"{name} {portal_id} attestation is incomplete")
            _digest(proof.get("rom_sha256"), f"{name} {portal_id} attested rom_sha256")
            _digest(proof.get("sav_sha256"), f"{name} {portal_id} attested sav_sha256")
        portals = []
        for portal in world.get("portals", []):
            destination = portal.get("destination_world")
            if destination not in ids:
                raise AssemblyError(f"{name} portal names unknown world {destination!r}")
            portals.append({"id": portal["id"], "destination_world_id": ids[destination],
                            "arrival_portal_id": portal["arrival_portal_id"],
                            "return_portal_id": portal["return_portal_id"]})
        plan.append({"name": name, "world_id": entry["world_id"], "config": world,
                     "arrivals": arrivals, "proofs": proofs, "portals": portals})
    return plan


def _copy(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)


def assemble(dist: Path, out: Path, *, registry: Path, release_config: Path,
             release_arrivals: Path, smoke_arrivals: Path, repo_root: Path,
             arrival_verifier: Path | None = None) -> dict:
    config = _load_json(release_config, "release config")
    attested = _load_json(release_arrivals, "release arrivals")
    object_catalog_config(config)
    plan = plan_worlds(registry, config, attested, smoke_arrivals)
    if out.exists():
        raise AssemblyError(f"refusing to reuse existing output directory {out}")
    rom_digests = {}
    for world in plan:
        rom = dist / world["name"] / "game.gba"
        if not rom.is_file():
            raise AssemblyError(f"{world['name']} build output is missing game.gba")
        name = world["name"]
        rom_digest = rom_digests[name] = sha256_file(rom)
        for portal_id, proof in sorted(world["proofs"].items()):
            if proof["rom_sha256"] != rom_digest:
                raise RecertifyError(
                    f"recertify arrival saves: world '{name}' built ROM sha256 {rom_digest} "
                    f"differs from {proof['rom_sha256']} attested for arrival '{portal_id}' in "
                    f"{release_arrivals}. Re-save the arrival under this exact ROM with "
                    "tools/coop/recert_arrival.py, review it, and commit the new .sav and "
                    "attestation before releasing.")
            sav = repo_root / proof["sav_path"]
            if not sav.is_file() or sha256_file(sav) != proof["sav_sha256"]:
                raise AssemblyError(f"{name} {portal_id} attested save is missing or altered: {sav}")
    object_digest, object_document = object_catalog_fingerprint(dist, rom_digests)
    stage = out / "catalog"
    entries = []
    for world in plan:
        name, world_id = world["name"], world["world_id"]
        source = dist / name
        rom_digest = rom_digests[name]
        prefix = f"worlds/{world_id}"
        for source_name, release_name in WORLD_FILES:
            if not (source / source_name).is_file():
                raise AssemblyError(f"{name} build output is missing {source_name}")
            _copy(source / source_name, stage / prefix / release_name)
        single = len(world["arrivals"]) == 1
        arrivals = {}
        for portal_id, arrival in sorted(world["arrivals"].items()):
            relative = f"{prefix}/arrival.sav" if single else f"{prefix}/arrival-{portal_id}.sav"
            _copy(repo_root / world["proofs"][portal_id]["sav_path"], stage / relative)
            arrivals[portal_id] = {
                "map_group": arrival["map_group"], "map_number": arrival["map_number"],
                "warp_id": arrival["warp_id"], "map_layout_id": arrival["map_layout_id"],
                "template_sav_path": relative,
                "template_sav_sha256": sha256_file(stage / relative)}
        bridge = json.loads((stage / prefix / "bridge_manifest.json").read_text(encoding="utf-8"))
        transfer = json.loads((stage / prefix / "player_transfer.json").read_text(encoding="utf-8"))
        if bridge["game_build"]["rom_sha256"] != rom_digest or transfer["rom_sha256"] != rom_digest:
            raise AssemblyError(f"{name} manifests do not describe the built ROM")
        cfg = world["config"]
        entries.append({
            "arrivals": arrivals,
            "bridge_path": f"{prefix}/bridge_manifest.json",
            "bridge_sha256": sha256_file(stage / prefix / "bridge_manifest.json"),
            "experience_table_path": f"{prefix}/experience_table_manifest.json",
            "experience_table_sha256": sha256_file(stage / prefix / "experience_table_manifest.json"),
            "location_codec": config["location_codec"],
            "map_binding_path": f"{prefix}/map_binding_manifest.json",
            "map_binding_sha256": sha256_file(stage / prefix / "map_binding_manifest.json"),
            "name": name,
            "object_catalog_sha256": object_digest,
            "object_scalar_path": f"{prefix}/object_scalar_manifest.json",
            "object_scalar_sha256": sha256_file(stage / prefix / "object_scalar_manifest.json"),
            "owned_location_sections": cfg["owned_location_sections"],
            "player_transfer_path": f"{prefix}/player_transfer.json",
            "player_transfer_sha256": sha256_file(stage / prefix / "player_transfer.json"),
            "portals": world["portals"],
            "presence_regions": cfg["presence_regions"],
            "regional_save_schema": config["regional_save_schema"],
            "rom_path": f"{prefix}/game.gba",
            "rom_sha256": rom_digest,
            "save_namespace": cfg["save_namespace"],
            "shared_player_schema": bridge["save"]["schema_version"],
            "world_id": world_id})
    region_bytes = canonical_json({"schema_version": 1, "worlds": entries})
    if len(region_bytes) > MAX_REGION_CATALOG_BYTES:
        raise AssemblyError("region catalog exceeds the launcher's 256 KiB bound")
    region_path = stage / "release_catalog.json"
    region_path.write_bytes(region_bytes)
    region_digest = hashlib.sha256(region_bytes).hexdigest()
    if arrival_verifier is not None:
        verifier = Path(arrival_verifier)
        rom_release_catalog._arrival_verifier = lambda: verifier  # noqa: SLF001 - prebuilt same binary
    try:
        validated = rom_release_catalog.validate_catalog(registry, region_path, region_digest)
        server_bytes, server_digest = generate_server_build_catalog.generate(
            registry, region_path, region_digest)
    except (OSError, ValueError) as exc:
        raise AssemblyError(f"release catalog rejected: {exc}") from exc
    if len(server_bytes) > MAX_SERVER_CATALOG_BYTES:
        raise AssemblyError("server build catalog exceeds the server's 64 KiB bound")
    (stage / "server-build-catalog.json").write_bytes(server_bytes)
    server_root = out / "server-catalog" / server_digest
    _copy(stage / "server-build-catalog.json", server_root / "server-build-catalog.json")
    for entry in entries:
        for arrival in entry["arrivals"].values():
            _copy(stage / arrival["template_sav_path"], server_root / arrival["template_sav_path"])
    summary = {
        "schema": 1,
        "release_catalog_sha256": region_digest,
        "server_build_catalog_sha256": server_digest,
        "object_catalog_sha256": object_digest,
        "object_catalog_source": FINGERPRINT_SOURCE,
        "object_catalog_version": FINGERPRINT_VERSION,
        "validated_worlds": sorted(validated),
        "worlds": [{"world_id": e["world_id"], "name": e["name"], "rom_sha256": e["rom_sha256"],
                    "arrivals": {k: v["template_sav_sha256"] for k, v in e["arrivals"].items()}}
                   for e in entries],
    }
    (out / "object-catalog-fingerprint.json").write_bytes(object_document)
    (out / "assembly.json").write_bytes(canonical_json(summary))
    return summary


def signing_artifacts(catalog_dir: Path, bundle: Path, kind: str) -> list[str]:
    """Copy region catalog and signed world files; return canonical ID=PATH lines."""
    region = catalog_dir / "release_catalog.json"
    raw = region.read_bytes()
    if len(raw) > MAX_REGION_CATALOG_BYTES:
        raise AssemblyError("region catalog is oversized")
    catalog = json.loads(raw)
    worlds = sorted(catalog["worlds"], key=lambda world: world["world_id"])
    ids = [world["world_id"] for world in worlds]
    if not ids or len(set(ids)) != len(ids) or 1 not in ids:
        raise AssemblyError("region catalog needs unique world ids including world 1")
    _copy(region, bundle / "release_catalog.json")
    lines = [f"region-catalog={bundle / 'release_catalog.json'}"]
    for world in worlds:
        world_id = world["world_id"]
        expected = {"game.gba": world["rom_sha256"], "bridge_manifest.json": world["bridge_sha256"],
                    "player_transfer.json": world["player_transfer_sha256"]}
        for kind_name, filename in SIGNED_WORLD_FILES:
            source = catalog_dir / "worlds" / str(world_id) / filename
            destination = bundle / "worlds" / str(world_id) / filename
            if sha256_file(source) != expected[filename]:
                raise AssemblyError(f"world {world_id} {filename} does not match the region catalog")
            _copy(source, destination)
            lines.append(f"world-{world_id}-{kind_name}={destination}")
    default = next(world for world in worlds if world["world_id"] == 1)
    base = {"windows": ("runtime/game.gba", "bridge_manifest.json"),
            "game": ("game.gba", "bridge_manifest.json")}[kind]
    for relative, expected in zip(base, (default["rom_sha256"], default["bridge_sha256"])):
        path = bundle / relative
        if not path.is_file() or sha256_file(path) != expected:
            raise AssemblyError(f"{kind} bundle {relative} is not world 1's signed artifact")
    return lines


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("assemble", help="stage, validate and write both catalogs")
    build.add_argument("--dist", type=Path, default=Path("dist/multiworld"))
    build.add_argument("--out", type=Path, required=True)
    build.add_argument("--registry", type=Path, default=Path("data/rom_worlds.json"))
    build.add_argument("--release-config", type=Path, default=Path("data/rom_world_release.json"))
    build.add_argument("--release-arrivals", type=Path, default=Path("data/release_arrivals.json"))
    build.add_argument("--smoke-arrivals", type=Path, default=Path("data/rom_world_arrivals.json"))
    build.add_argument("--repo-root", type=Path, default=Path("."))
    build.add_argument("--arrival-verifier", type=Path,
                       help="prebuilt coop-save verify_arrival_save binary (default: cargo build)")
    sign = commands.add_parser("signing-artifacts", help="stage signed world files into a bundle")
    sign.add_argument("--catalog-dir", type=Path, required=True)
    sign.add_argument("--bundle", type=Path, required=True)
    sign.add_argument("--kind", choices=("windows", "game"), required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "assemble":
            summary = assemble(
                args.dist, args.out, registry=args.registry, release_config=args.release_config,
                release_arrivals=args.release_arrivals, smoke_arrivals=args.smoke_arrivals,
                repo_root=args.repo_root, arrival_verifier=args.arrival_verifier)
            print(json.dumps(summary, indent=2, sort_keys=True))
        else:
            print("\n".join(signing_artifacts(args.catalog_dir, args.bundle, args.kind)))
    except RecertifyError as exc:
        print(f"release assembly: {exc}", file=sys.stderr)
        return 3
    except (AssemblyError, OSError, KeyError, ValueError) as exc:
        print(f"release assembly: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
