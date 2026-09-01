#!/usr/bin/env python3
"""Validate and generate the append-only regional identity registry.

The JSON source is the authority for persisted bit ordinals.  Ordinals are
written explicitly and must be contiguous within each identity kind; this
generator never derives them from alphabetical order.  Removing an identity
therefore requires retaining its ledger entry instead of shifting later bits.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Final


ROOT = Path(__file__).resolve().parents[2]
SOURCE_PATH = ROOT / "data" / "coop" / "regional_identities.json"
RUST_OUTPUT_PATH = (
    ROOT
    / "coop"
    / "crates"
    / "coop-protocol"
    / "src"
    / "generated_identity_registry.rs"
)
C_HEADER_OUTPUT_PATH = ROOT / "include" / "coop" / "generated_regional_identities.h"
C_SOURCE_OUTPUT_PATH = ROOT / "src" / "coop" / "generated_regional_identities.c"
LOCK_PATH = ROOT / "data" / "coop" / "regional_identities.lock.json"
OPPONENTS_PATH = ROOT / "include" / "constants" / "opponents.h"

SCHEMA_VERSION: Final = 1
LOCK_SCHEMA_VERSION: Final = 1
DIGEST_SIZE: Final = 16
REGIONS: Final = {
    "HOENN": "Hoenn",
    "KANTO": "Kanto",
    "JOHTO": "Johto",
    "SEVII": "Sevii",
}
REGION_WIRE_ORDINALS: Final = {
    "HOENN": 1,
    "KANTO": 2,
    "JOHTO": 3,
    "SEVII": 4,
}
KIND_ORDER: Final = ("trainer", "gym", "badge", "fly", "event")
KIND_PREFIX: Final = {
    "trainer": "TRAINER_",
    "gym": "GYM_",
    "badge": "BADGE_",
    "fly": "FLY_",
    "event": "EVENT_",
}
CAPACITY_KEYS: Final = {
    "trainer": "trainers",
    "gym": "gyms",
    "fly": "fly_points",
    "event": "events",
}
EXPECTED_CAPACITIES: Final = {
    "trainers": 2048,
    "events": 2048,
    "fly_points": 128,
    "gyms": 64,
}
BADGES_PER_REGION: Final = 8
LOCAL_KEY_PATTERN: Final = re.compile(r"[A-Z][A-Z0-9_]*\Z")
HOENN_TRAINER_FIRST: Final = 1
HOENN_TRAINER_LAST: Final = 854
HOENN_TRAINER_PRODUCT_ALIASES: Final = {
    "TRAINER_WALLY_VR_1": "TRAINER_WALLY_1",
}


class RegistryError(ValueError):
    """A registry inconsistency that must block code generation."""


@dataclass(frozen=True)
class RegistryEntry:
    kind: str
    ordinal: int | None
    qualified_id: str
    region: str
    local_key: str
    legacy_value: int | None
    legacy_symbol: str | None
    badge_bit: int | None


@dataclass(frozen=True)
class Registry:
    version: int
    capacities: dict[str, int]
    entries: tuple[RegistryEntry, ...]
    digest: bytes


def registry_entries_by_kind(registry: Registry, kind: str) -> tuple[RegistryEntry, ...]:
    return tuple(entry for entry in registry.entries if entry.kind == kind)


def validate_append_only_transition(previous: Registry, current: Registry) -> None:
    if current.version != previous.version + 1:
        raise RegistryError(
            "registry_version must increment by exactly one when updating the lock"
        )
    if current.capacities != previous.capacities:
        raise RegistryError("persisted identity capacities cannot change")
    for kind in KIND_ORDER:
        previous_entries = registry_entries_by_kind(previous, kind)
        current_entries = registry_entries_by_kind(current, kind)
        if len(current_entries) < len(previous_entries):
            raise RegistryError(f"{kind} identities cannot be removed from the append-only ledger")
        if current_entries[: len(previous_entries)] != previous_entries:
            raise RegistryError(
                f"existing {kind} identity assignments are immutable; only append new entries"
            )


def validate_lock_history(value: Any) -> tuple[Any, ...]:
    if not isinstance(value, dict):
        raise RegistryError("registry lock root must be an object")
    require_exact_keys(value, {"lock_schema_version", "snapshots"}, "registry lock")
    version = require_plain_int(
        value["lock_schema_version"], "lock_schema_version", 1, 0xFFFF
    )
    if version != LOCK_SCHEMA_VERSION:
        raise RegistryError(
            f"unsupported lock_schema_version {version}; expected {LOCK_SCHEMA_VERSION}"
        )
    snapshots = value["snapshots"]
    if not isinstance(snapshots, list) or not snapshots:
        raise RegistryError("registry lock snapshots must be a non-empty array")
    registries = [validate_registry(snapshot) for snapshot in snapshots]
    if registries[0].version != 1:
        raise RegistryError("registry lock history must start at version 1")
    for previous, current in zip(registries, registries[1:], strict=False):
        validate_append_only_transition(previous, current)
    return tuple(snapshots)


def updated_lock_history(lock_value: Any | None, source_value: Any) -> dict[str, Any]:
    source_registry = validate_registry(source_value)
    if lock_value is None:
        if source_registry.version != 1:
            raise RegistryError("a new registry lock can only bootstrap version 1")
        return {
            "lock_schema_version": LOCK_SCHEMA_VERSION,
            "snapshots": [source_value],
        }
    snapshots = list(validate_lock_history(lock_value))
    previous = validate_registry(snapshots[-1])
    validate_append_only_transition(previous, source_registry)
    snapshots.append(source_value)
    return {
        "lock_schema_version": LOCK_SCHEMA_VERSION,
        "snapshots": snapshots,
    }


def ensure_source_matches_lock(lock_value: Any, source_value: Any) -> None:
    snapshots = validate_lock_history(lock_value)
    if snapshots[-1] != source_value:
        raise RegistryError(
            "registry source differs from its immutable lock; review the append-only "
            "change, increment registry_version, then run with --update-lock"
        )


def load_json(path: Path) -> Any:
    try:
        with path.open(encoding="utf-8") as source:
            return json.load(source)
    except (OSError, json.JSONDecodeError) as error:
        raise RegistryError(f"cannot read {path}: {error}") from error


def canonical_registry_bytes(value: Any) -> bytes:
    """Return the exact bytes fingerprinted by every implementation."""
    try:
        rendered = json.dumps(
            value,
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        )
    except (TypeError, ValueError) as error:
        raise RegistryError(f"registry is not canonical JSON: {error}") from error
    return rendered.encode("ascii")


def require_exact_keys(value: dict[str, Any], expected: set[str], context: str) -> None:
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        unknown = sorted(actual - expected)
        details: list[str] = []
        if missing:
            details.append("missing " + ", ".join(missing))
        if unknown:
            details.append("unknown " + ", ".join(unknown))
        raise RegistryError(f"{context} has invalid fields ({'; '.join(details)})")


def require_plain_int(value: Any, context: str, minimum: int, maximum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise RegistryError(f"{context} must be an integer")
    if not minimum <= value <= maximum:
        raise RegistryError(f"{context} must be in {minimum}..{maximum}")
    return value


def parse_qualified_id(value: Any, kind: str, context: str) -> tuple[str, str]:
    if not isinstance(value, str) or value.count(":") != 1:
        raise RegistryError(f"{context}.id must be REGION:LOCAL_KEY")
    region, local_key = value.split(":", 1)
    if region not in REGIONS:
        raise RegistryError(f"{context}.id has unsupported region {region!r}")
    if not LOCAL_KEY_PATTERN.fullmatch(local_key):
        raise RegistryError(f"{context}.id has non-canonical local key {local_key!r}")
    if not local_key.startswith(KIND_PREFIX[kind]) or local_key == KIND_PREFIX[kind]:
        raise RegistryError(
            f"{context}.id must use the {KIND_PREFIX[kind]} prefix"
        )
    return region, local_key


def load_hoenn_trainer_definitions(path: Path = OPPONENTS_PATH) -> list[tuple[str, int]]:
    """Read only direct numeric Hoenn definitions from opponents.h itself."""
    try:
        source = path.read_text(encoding="utf-8")
    except OSError as error:
        raise RegistryError(f"cannot read {path}: {error}") from error
    definitions = [
        (match.group(1), int(match.group(2)))
        for match in re.finditer(
            r"^#define\s+(TRAINER_[A-Z0-9_]+)\s+(\d+)\s*$", source, re.MULTILINE
        )
        if HOENN_TRAINER_FIRST <= int(match.group(2)) <= HOENN_TRAINER_LAST
    ]
    values = [value for _, value in definitions]
    expected = list(range(HOENN_TRAINER_FIRST, HOENN_TRAINER_LAST + 1))
    if values != expected:
        raise RegistryError(
            "opponents.h direct Hoenn trainer definitions must cover 1..854 in order"
        )
    return definitions


def validate_hoenn_trainer_coverage(entries: list[RegistryEntry]) -> None:
    hoenn = [
        entry
        for entry in entries
        if entry.kind == "trainer" and entry.region == "HOENN"
    ]
    definitions = load_hoenn_trainer_definitions()
    if len(hoenn) != len(definitions):
        raise RegistryError(
            f"Hoenn trainer ledger must contain {len(definitions)} entries, found {len(hoenn)}"
        )
    for entry, (symbol, legacy_value) in zip(hoenn, definitions, strict=True):
        expected_key = HOENN_TRAINER_PRODUCT_ALIASES.get(symbol, symbol)
        if (
            entry.ordinal != legacy_value - 1
            or entry.local_key != expected_key
            or entry.legacy_value != legacy_value
            or entry.legacy_symbol != symbol
        ):
            raise RegistryError(
                "Hoenn trainer ledger drift at legacy value "
                f"{legacy_value}: expected ordinal={legacy_value - 1}, "
                f"id=HOENN:{expected_key}, symbol={symbol}"
            )


def validate_registry(value: Any) -> Registry:
    if not isinstance(value, dict):
        raise RegistryError("registry root must be an object")
    require_exact_keys(
        value,
        {
            "schema_version",
            "registry_version",
            "regions",
            "capacities",
            "badges_per_region",
            "identities",
        },
        "registry",
    )
    schema_version = require_plain_int(
        value["schema_version"], "schema_version", 1, 0xFFFF
    )
    if schema_version != SCHEMA_VERSION:
        raise RegistryError(
            f"unsupported schema_version {schema_version}; expected {SCHEMA_VERSION}"
        )
    version = require_plain_int(
        value["registry_version"], "registry_version", 1, 0xFFFF_FFFF
    )

    regions_value = value["regions"]
    if not isinstance(regions_value, list):
        raise RegistryError("regions must be an array")
    expected_regions = [
        {"id": token, "wire": wire} for token, wire in REGION_WIRE_ORDINALS.items()
    ]
    if regions_value != expected_regions:
        raise RegistryError(
            "regions must preserve canonical order and wire ordinals: "
            + json.dumps(expected_regions, separators=(",", ":"))
        )

    capacities_value = value["capacities"]
    if not isinstance(capacities_value, dict):
        raise RegistryError("capacities must be an object")
    require_exact_keys(capacities_value, set(EXPECTED_CAPACITIES), "capacities")
    capacities: dict[str, int] = {}
    for name, expected in EXPECTED_CAPACITIES.items():
        actual = require_plain_int(capacities_value[name], f"capacities.{name}", 1, 0xFFFF)
        if actual != expected:
            raise RegistryError(
                f"capacities.{name} is persisted ABI {expected}, received {actual}"
            )
        capacities[name] = actual

    badges_per_region = require_plain_int(
        value["badges_per_region"], "badges_per_region", 1, 16
    )
    if badges_per_region != BADGES_PER_REGION:
        raise RegistryError(
            f"badges_per_region is persisted ABI {BADGES_PER_REGION}, "
            f"received {badges_per_region}"
        )

    identities = value["identities"]
    if not isinstance(identities, list) or not identities:
        raise RegistryError("identities must be a non-empty array")

    entries: list[RegistryEntry] = []
    seen_ids: set[str] = set()
    seen_ordinals: dict[str, set[int]] = {kind: set() for kind in KIND_ORDER}
    seen_badge_bits: set[tuple[str, int]] = set()
    seen_flag_values: set[tuple[str, int]] = set()
    seen_trainer_values: set[tuple[str, int]] = set()
    kind_position = 0

    for index, raw_entry in enumerate(identities):
        context = f"identities[{index}]"
        if not isinstance(raw_entry, dict):
            raise RegistryError(f"{context} must be an object")
        required = {"kind", "id", "legacy_value"}
        if raw_entry.get("kind") == "badge":
            required.add("badge_bit")
        else:
            required.add("ordinal")
        if raw_entry.get("legacy_value") is not None:
            required.add("legacy_symbol")
        require_exact_keys(raw_entry, required, context)

        kind = raw_entry["kind"]
        if kind not in KIND_ORDER:
            raise RegistryError(f"{context}.kind is unsupported: {kind!r}")
        current_position = KIND_ORDER.index(kind)
        if current_position < kind_position:
            raise RegistryError("identities must be grouped in canonical kind order")
        kind_position = current_position

        ordinal: int | None = None
        if kind != "badge":
            ordinal = require_plain_int(
                raw_entry["ordinal"], f"{context}.ordinal", 0, 0xFFFF
            )
        region, local_key = parse_qualified_id(raw_entry["id"], kind, context)
        qualified_id = raw_entry["id"]
        if qualified_id in seen_ids:
            raise RegistryError(f"duplicate qualified identity {qualified_id}")
        seen_ids.add(qualified_id)
        if ordinal is not None:
            if ordinal in seen_ordinals[kind]:
                raise RegistryError(f"duplicate {kind} ordinal {ordinal}")
            seen_ordinals[kind].add(ordinal)

        capacity_key = CAPACITY_KEYS.get(kind)
        if capacity_key is not None and ordinal is not None and ordinal >= capacities[capacity_key]:
            raise RegistryError(
                f"{kind} ordinal {ordinal} exceeds {capacity_key} capacity"
            )

        legacy_raw = raw_entry["legacy_value"]
        legacy_value: int | None
        legacy_symbol: str | None = None
        if legacy_raw is None:
            legacy_value = None
        else:
            legacy_value = require_plain_int(
                legacy_raw, f"{context}.legacy_value", 0, 0xFFFE
            )
            legacy_symbol_raw = raw_entry["legacy_symbol"]
            expected_prefix = "TRAINER_" if kind == "trainer" else "FLAG_"
            if (
                not isinstance(legacy_symbol_raw, str)
                or not LOCAL_KEY_PATTERN.fullmatch(legacy_symbol_raw)
                or not legacy_symbol_raw.startswith(expected_prefix)
            ):
                raise RegistryError(
                    f"{context}.legacy_symbol must use the {expected_prefix} prefix"
                )
            legacy_symbol = legacy_symbol_raw
            # Trainer legacy values are opponent IDs, whereas all other
            # persisted kinds are engine flag IDs.  A flag may be reused in a
            # different region, but never for two meanings in one region.
            legacy_key = (region, legacy_value)
            if kind == "trainer":
                # Keep trainer opponent IDs in their own legacy namespace.
                trainer_legacy_key = (region, legacy_value)
                if trainer_legacy_key in seen_trainer_values:
                    raise RegistryError(
                        f"ambiguous legacy trainer id {legacy_value:#x} in {region}"
                    )
                seen_trainer_values.add(trainer_legacy_key)
            else:
                if legacy_key in seen_flag_values:
                    raise RegistryError(
                        f"ambiguous legacy flag {legacy_value:#x} in {region}"
                    )
                seen_flag_values.add(legacy_key)

        badge_bit: int | None = None
        if kind == "badge":
            badge_bit = require_plain_int(
                raw_entry["badge_bit"], f"{context}.badge_bit", 0, BADGES_PER_REGION - 1
            )
            badge_key = (region, badge_bit)
            if badge_key in seen_badge_bits:
                raise RegistryError(f"duplicate badge bit {badge_bit} in {region}")
            seen_badge_bits.add(badge_key)

        entries.append(
            RegistryEntry(
                kind=kind,
                ordinal=ordinal,
                qualified_id=qualified_id,
                region=region,
                local_key=local_key,
                legacy_value=legacy_value,
                legacy_symbol=legacy_symbol,
                badge_bit=badge_bit,
            )
        )

    for kind, ordinals in seen_ordinals.items():
        if kind == "badge":
            continue
        if not ordinals:
            raise RegistryError(f"registry has no {kind} identities")
        expected = set(range(max(ordinals) + 1))
        if ordinals != expected:
            missing = ", ".join(str(value) for value in sorted(expected - ordinals))
            raise RegistryError(
                f"{kind} ordinals must be append-only and contiguous; missing {missing}"
            )
        encountered = [entry.ordinal for entry in entries if entry.kind == kind]
        if encountered != sorted(encountered):
            raise RegistryError(f"{kind} entries must appear in ordinal order")

    validate_hoenn_trainer_coverage(entries)

    digest = hashlib.sha256(canonical_registry_bytes(value)).digest()[:DIGEST_SIZE]
    return Registry(
        version=version,
        capacities=capacities,
        entries=tuple(entries),
        digest=digest,
    )


def rust_option(value: int | None, type_name: str) -> str:
    if value is None:
        return "None"
    return f"Some({value}{type_name})"


def render_rust(registry: Registry) -> str:
    digest = ", ".join(f"0x{byte:02x}" for byte in registry.digest)
    lines = [
        "// This file is generated by tools/coop/generate_regional_identities.py.",
        "// Do not edit it by hand; update data/coop/regional_identities.json.",
        "",
        f"pub const IDENTITY_REGISTRY_VERSION: u32 = {registry.version};",
        f"pub const IDENTITY_REGISTRY_DIGEST: [u8; {DIGEST_SIZE}] = [{digest}];",
        f"pub const TRAINER_IDENTITY_CAPACITY: usize = {registry.capacities['trainers']};",
        f"pub const EVENT_IDENTITY_CAPACITY: usize = {registry.capacities['events']};",
        f"pub const FLY_POINT_IDENTITY_CAPACITY: usize = {registry.capacities['fly_points']};",
        f"pub const GYM_IDENTITY_CAPACITY: usize = {registry.capacities['gyms']};",
        f"pub const BADGES_PER_REGION: usize = {BADGES_PER_REGION};",
        "",
        "pub const GENERATED_IDENTITY_REGISTRY: &[IdentityCatalogEntry] = &[",
    ]
    for entry in registry.entries:
        lines.extend(
            [
                "    IdentityCatalogEntry {",
                f"        kind: IdentityKind::{entry.kind.title().replace('Fly', 'FlyPoint')},",
                f"        region: RegionId::{REGIONS[entry.region]},",
                f'        qualified_id: "{entry.qualified_id}",',
                f"        ordinal: {rust_option(entry.ordinal, '')},",
                f"        legacy_value: {rust_option(entry.legacy_value, '')},",
                (
                    f'        legacy_symbol: Some("{entry.legacy_symbol}"),'
                    if entry.legacy_symbol is not None
                    else "        legacy_symbol: None,"
                ),
                f"        badge_bit: {rust_option(entry.badge_bit, '')},",
                "    },",
            ]
        )
    lines.extend(["];"])
    return "\n".join(lines) + "\n"


def macro_stem(entry: RegistryEntry) -> str:
    return f"COOP_{entry.kind.upper()}_{entry.region}_{entry.local_key}"


def c_initializer_bytes(data: bytes) -> str:
    return "{ " + ", ".join(f"0x{byte:02X}" for byte in data) + " }"


def render_c_header(registry: Registry) -> str:
    lines = [
        "/* Generated by tools/coop/generate_regional_identities.py. */",
        "/* Do not edit; update data/coop/regional_identities.json. */",
        "#ifndef GUARD_COOP_GENERATED_REGIONAL_IDENTITIES_H",
        "#define GUARD_COOP_GENERATED_REGIONAL_IDENTITIES_H",
        "",
        '#include "gba/types.h"',
        '#include "coop/region.h"',
        "",
        f"#define COOP_IDENTITY_REGISTRY_VERSION {registry.version}u",
        f"#define COOP_IDENTITY_REGISTRY_DIGEST_SIZE {DIGEST_SIZE}",
        f"#define COOP_IDENTITY_REGISTRY_DIGEST_BYTES {c_initializer_bytes(registry.digest)}",
        f"#define COOP_TRAINER_IDENTITY_CAPACITY {registry.capacities['trainers']}",
        f"#define COOP_EVENT_IDENTITY_CAPACITY {registry.capacities['events']}",
        f"#define COOP_FLY_POINT_IDENTITY_CAPACITY {registry.capacities['fly_points']}",
        f"#define COOP_GYM_IDENTITY_CAPACITY {registry.capacities['gyms']}",
        f"#define COOP_BADGES_PER_REGION {BADGES_PER_REGION}",
        "#define COOP_IDENTITY_LEGACY_NONE 0xFFFFu",
        "#define COOP_IDENTITY_ORDINAL_NONE 0xFFFFu",
        "#define COOP_IDENTITY_BADGE_BIT_NONE 0xFFu",
        "",
    ]
    for entry in registry.entries:
        if entry.ordinal is not None:
            lines.append(f"#define {macro_stem(entry)}_ORDINAL {entry.ordinal}u")
        if entry.badge_bit is not None:
            lines.append(f"#define {macro_stem(entry)}_BIT {entry.badge_bit}u")
    lines.extend(
        [
            "",
            "struct CoopIdentityRegistryEntry",
            "{",
            "    u16 ordinal;",
            "    u16 legacy_value;",
            "    u8 region;",
            "    u8 badge_bit;",
            "};",
            "",
            "extern const u8 gCoopIdentityRegistryDigest[COOP_IDENTITY_REGISTRY_DIGEST_SIZE];",
        ]
    )
    for kind in KIND_ORDER:
        count = sum(entry.kind == kind for entry in registry.entries)
        symbol = "FlyPoint" if kind == "fly" else kind.title()
        lines.append(f"#define COOP_{kind.upper()}_IDENTITY_COUNT {count}")
        lines.append(
            f"extern const struct CoopIdentityRegistryEntry "
            f"gCoop{symbol}IdentityRegistry[COOP_{kind.upper()}_IDENTITY_COUNT];"
        )
    lines.extend(["", "#endif /* GUARD_COOP_GENERATED_REGIONAL_IDENTITIES_H */"])
    return "\n".join(lines) + "\n"


def render_c_source(registry: Registry) -> str:
    lines = [
        "/* Generated by tools/coop/generate_regional_identities.py. */",
        "/* Do not edit; update data/coop/regional_identities.json. */",
        '#include "global.h"',
        '#include "constants/flags.h"',
        '#include "constants/opponents.h"',
        '#include "coop/generated_regional_identities.h"',
        "",
        "const u8 gCoopIdentityRegistryDigest[COOP_IDENTITY_REGISTRY_DIGEST_SIZE] =",
        "    COOP_IDENTITY_REGISTRY_DIGEST_BYTES;",
    ]
    asserted_symbols: set[str] = set()
    for entry in registry.entries:
        if entry.legacy_symbol is None or entry.legacy_symbol in asserted_symbols:
            continue
        asserted_symbols.add(entry.legacy_symbol)
        lines.append(
            f'_Static_assert({entry.legacy_symbol} == {entry.legacy_value}, '
            f'"regional identity legacy value drift: {entry.legacy_symbol}");'
        )
    for kind in KIND_ORDER:
        symbol = "FlyPoint" if kind == "fly" else kind.title()
        lines.extend(
            [
                "",
                f"const struct CoopIdentityRegistryEntry gCoop{symbol}IdentityRegistry[] =",
                "{",
            ]
        )
        for entry in (entry for entry in registry.entries if entry.kind == kind):
            ordinal = (
                "COOP_IDENTITY_ORDINAL_NONE"
                if entry.ordinal is None
                else f"{entry.ordinal}u"
            )
            legacy = (
                "COOP_IDENTITY_LEGACY_NONE"
                if entry.legacy_value is None
                else f"{entry.legacy_value}u"
            )
            badge_bit = (
                "COOP_IDENTITY_BADGE_BIT_NONE"
                if entry.badge_bit is None
                else f"{entry.badge_bit}u"
            )
            lines.append(
                f"    {{ {ordinal}, {legacy}, "
                f"COOP_REGION_{entry.region}, {badge_bit} }},"
            )
        lines.append("};")
    return "\n".join(lines) + "\n"


def atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        dir=path.parent, prefix=f".{path.name}.", suffix=".tmp"
    )
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    except BaseException:
        temporary.unlink(missing_ok=True)
        raise


def build_outputs(source_path: Path) -> tuple[Registry, dict[Path, str]]:
    raw_registry = load_json(source_path)
    registry = validate_registry(raw_registry)
    return registry, {
        RUST_OUTPUT_PATH: render_rust(registry),
        C_HEADER_OUTPUT_PATH: render_c_header(registry),
        C_SOURCE_OUTPUT_PATH: render_c_source(registry),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--check",
        action="store_true",
        help="verify every checked-in generated file without writing it",
    )
    mode.add_argument(
        "--update-lock",
        action="store_true",
        help="accept one reviewed append-only version and retain its immutable snapshot",
    )
    args = parser.parse_args()

    try:
        source_value = load_json(SOURCE_PATH)
        lock_value = load_json(LOCK_PATH) if LOCK_PATH.exists() else None
        if args.update_lock:
            lock_value = updated_lock_history(lock_value, source_value)
            atomic_write(
                LOCK_PATH,
                json.dumps(lock_value, indent=2, ensure_ascii=True) + "\n",
            )
        elif lock_value is None:
            raise RegistryError(
                "registry lock is missing; bootstrap reviewed version 1 with --update-lock"
            )
        else:
            ensure_source_matches_lock(lock_value, source_value)

        registry = validate_registry(source_value)
        outputs = {
            RUST_OUTPUT_PATH: render_rust(registry),
            C_HEADER_OUTPUT_PATH: render_c_header(registry),
            C_SOURCE_OUTPUT_PATH: render_c_source(registry),
        }
        if args.check:
            stale = []
            for path, expected in outputs.items():
                actual = path.read_text(encoding="utf-8") if path.exists() else None
                if actual != expected:
                    stale.append(path)
            if stale:
                for path in stale:
                    print(f"generated identity registry is out of date: {path}", file=sys.stderr)
                return 1
        else:
            for path, expected in outputs.items():
                atomic_write(path, expected)
        print(
            f"regional identity registry v{registry.version} "
            f"digest={registry.digest.hex()} entries={len(registry.entries)}"
        )
        return 0
    except (OSError, RegistryError) as error:
        print(f"regional identity generation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
