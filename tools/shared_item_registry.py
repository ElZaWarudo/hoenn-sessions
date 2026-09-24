"""Validate the append-only item IDs shared by every ROM in a release family."""

from __future__ import annotations

import argparse
import json
import hashlib
import re
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = Path("data/shared_item_ids.json")
SYMBOL = re.compile(r"ITEM_[A-Z0-9_]+\Z")
HOST_PREFIX_SHA256 = "7969b41ea032e3cbc1f5f44b820f835dd06399c7e9b8f847c8d06d922fb908a3"


class ItemRegistryError(ValueError):
    """A shared item identity or donor binding would change meaning."""


def _machine_rosters(header: str) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    for kind in ("TM", "HM"):
        match = re.search(rf"#define FOREACH_{kind}\(F\)(.*?)(?=\n#define|\Z)",
                          header, re.DOTALL)
        if match is None:
            raise ItemRegistryError(f"missing shared {kind} roster")
        result[kind] = re.findall(r"F\(([A-Z0-9_]+)\)", match[1])
    return result


def _installed_item_rows(root: Path) -> set[str]:
    table = root / "src/data/items.h"
    pending = [table]
    seen: set[Path] = set()
    items: set[str] = set()
    while pending:
        path = pending.pop()
        if path in seen:
            continue
        seen.add(path)
        source = path.read_text(encoding="utf-8")
        items.update(re.findall(r"^\s*\[(ITEM_[A-Z0-9_]+)\]\s*=", source,
                                re.MULTILINE))
        for included in re.findall(r'^\s*#include\s+"(items_[A-Za-z0-9_]+\.h)"',
                                   source, re.MULTILINE):
            pending.append(path.parent / included)
    return items


def validate(root: Path = ROOT, registry: dict[str, Any] | None = None,
             known_worlds: set[str] | None = None,
             previous_bytes: bytes | None = None,
             trusted_previous_sha256: str | None = None,
             require_installed: bool = False) -> dict[str, Any]:
    root = root.resolve()
    if registry is None:
        registry = json.loads((root / REGISTRY).read_text(encoding="utf-8"))
    if registry.get("schema_version") != 1 or registry.get("host_prefix_count") != 890:
        raise ItemRegistryError("shared item registry version or host prefix drifted")
    rows = registry.get("items")
    if not isinstance(rows, list) or len(rows) < 30:
        raise ItemRegistryError("missing Cormoria shared item allocation")
    if len(rows) > 30 or previous_bytes is not None or trusted_previous_sha256 is not None:
        if (not isinstance(previous_bytes, bytes)
                or not isinstance(trusted_previous_sha256, str)
                or not re.fullmatch(r"[0-9a-f]{64}", trusted_previous_sha256)
                or hashlib.sha256(previous_bytes).hexdigest() != trusted_previous_sha256):
            raise ItemRegistryError("appended items require an authenticated prior registry")
        try:
            previous = json.loads(previous_bytes)
        except (UnicodeError, ValueError) as exc:
            raise ItemRegistryError("invalid prior item registry") from exc
        prior_rows = previous.get("items") if isinstance(previous, dict) else None
        if (not isinstance(previous, dict)
                or previous.get("schema_version") != registry["schema_version"]
                or previous.get("host_prefix_count") != registry["host_prefix_count"]
                or not isinstance(prior_rows, list) or len(prior_rows) < 30
                or rows[:len(prior_rows)] != prior_rows):
            raise ItemRegistryError("previously released item identities changed")
    if known_worlds is None:
        worlds = json.loads((root / "data/rom_worlds.json").read_text(encoding="utf-8"))
        known_worlds = {world["name"] for world in worlds["worlds"]}
    if not known_worlds or "cormoria" not in known_worlds:
        raise ItemRegistryError("Cormoria world is not registered")

    seen: set[str] = set()
    for offset, row in enumerate(rows):
        if (not isinstance(row, dict) or type(row.get("id")) is not int
                or row["id"] != 890 + offset or not isinstance(row.get("symbol"), str)
                or not SYMBOL.fullmatch(row["symbol"]) or row["symbol"] in seen
                or row.get("introduced_by_world") not in known_worlds):
            raise ItemRegistryError("item IDs must be unique, ordered and world-owned")
        seen.add(row["symbol"])

    host = (root / "include/constants/items.h").read_text(encoding="utf-8")
    if not re.search(r"^\s*ITEM_JOHTO_SQUIRT_BOTTLE\s*=\s*889\s*,", host, re.MULTILINE):
        raise ItemRegistryError("pre-Cormoria item ABI drifted")
    defined = {match[1]: int(match[2]) for match in re.finditer(
        r"^\s*(ITEM_[A-Z0-9_]+)\s*=\s*(\d+)\s*,", host, re.MULTILINE)}
    rosters = _machine_rosters(
        (root / "include/constants/tms_hms.h").read_text(encoding="utf-8"))
    if len(rosters["TM"]) < 50 or len(rosters["HM"]) < 9:
        raise ItemRegistryError("shared machine prefix was shortened")
    prefix = {"direct": sorted((symbol, value) for symbol, value in defined.items() if value < 890),
              "tm": rosters["TM"][:50], "hm": rosters["HM"][:9]}
    prefix_digest = hashlib.sha256(json.dumps(prefix, sort_keys=True,
                                              separators=(",", ":")).encode()).hexdigest()
    if prefix_digest != HOST_PREFIX_SHA256:
        raise ItemRegistryError("pre-Cormoria item identities or machine order drifted")
    machine_symbols = {f"ITEM_{kind}_{move}" for kind, roster in rosters.items()
                       for move in roster}
    for row in rows:
        installed = defined.get(row["symbol"])
        slot = row.get("machine_slot")
        if slot is not None:
            if (not isinstance(slot, str) or not re.fullmatch(r"ITEM_(?:TM|HM)\d+", slot)
                    or row["symbol"] not in machine_symbols
                    or defined.get(slot) != row["id"]
                    or installed is not None):
                raise ItemRegistryError(f"machine item binding differs from ledger: {row['symbol']}")
        elif row["symbol"] in machine_symbols or (installed is not None and installed != row["id"]):
            raise ItemRegistryError(f"installed item ID differs from ledger: {row['symbol']}")

    symbols = {line.strip().split()[0].rstrip(",") for line in host.splitlines()
               if line.strip().startswith("ITEM_")}
    symbols.difference_update(seen)
    symbols.update(machine_symbols - seen)
    donor = json.loads((root / "data/cormoria/symbol_ledger.json").read_text(encoding="utf-8"))
    source_items = {row["source_symbol"]: row["source_value"]
                    for row in donor["semantic_constants"]
                    if row.get("source_symbol", "").startswith("ITEM_")}
    if len(source_items) != 379:
        raise ItemRegistryError("Cormoria item reference roster drifted")
    required = set(source_items) - symbols
    cormoria = [row for row in rows if row["introduced_by_world"] == "cormoria"]
    if (len(cormoria) != 30 or [row["id"] for row in cormoria] != list(range(890, 920))
            or {row["symbol"] for row in cormoria} != required
            or [row["symbol"] for row in cormoria] != sorted(required)):
        raise ItemRegistryError("Cormoria item identities are incomplete or reordered")
    for row in cormoria:
        if (type(row.get("donor_id")) is not int
                or row["donor_id"] != source_items[row["symbol"]]
                or row["donor_id"] == row["id"]):
            raise ItemRegistryError(f"Cormoria donor item binding drifted: {row['symbol']}")
    if require_installed:
        item_rows = _installed_item_rows(root)
        for row in rows:
            if (row["symbol"] not in item_rows
                    or (row.get("machine_slot") is None
                        and defined.get(row["symbol"]) != row["id"])):
                raise ItemRegistryError(f"shared item is reserved but not installed: {row['symbol']}")
    return registry


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--previous", type=Path,
                        help="prior released item registry; required for later-world additions")
    parser.add_argument("--trusted-previous-sha256",
                        help="prior registry digest from separately trusted release metadata")
    parser.add_argument("--require-installed", action="store_true",
                        help="require shared enum and ItemInfo data before packaging a release")
    args = parser.parse_args()
    try:
        previous = args.previous.read_bytes() if args.previous is not None else None
        validated = validate(previous_bytes=previous,
                             trusted_previous_sha256=args.trusted_previous_sha256,
                             require_installed=args.require_installed)
    except (OSError, ValueError) as exc:
        parser.exit(1, f"Shared item registry: {exc}\n")
    print(f"Validated {len(validated['items'])} shared item IDs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
