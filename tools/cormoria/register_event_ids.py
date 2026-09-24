"""Prepare authenticated Cormoria event IDs outside the live ROM tree.

This only allocates symbolic IDs. Trainer tables, heal destinations, and the
campaign content must be registered before the Cormoria ROM can use them.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

from tools.cormoria import import_world

ROOT = Path(__file__).resolve().parents[2]
KINDS = {
    "flags": (483, 0x8000, 4096),
    "vars": (43, 0x9100, 256),
    "trainers": (195, 0x5000, 4096),
    "heals": (17, 0x0300, 256),
}
SYMBOL = re.compile(r"Cormoria_[A-Za-z_][A-Za-z_0-9]*\Z")
OUTPUT = Path("include/constants/cormoria_event_ids.h")


class EventIdError(ValueError):
    """The pinned event IDs do not form the expected Cormoria allocation."""


def render_event_ids(root: Path = ROOT) -> bytes:
    _, ledger, _ = import_world.load_manifests(root)
    contract = ledger["allocation_contract"]
    lines = [
        "#ifndef GUARD_CONSTANTS_CORMORIA_EVENT_IDS_H",
        "#define GUARD_CONSTANTS_CORMORIA_EVENT_IDS_H",
        "",
        "// Generated from the pinned Cormoria symbol ledger; not a runtime adapter.",
    ]
    all_symbols: set[str] = set()
    all_ids: dict[str, set[int]] = {}
    for kind, (count, start, capacity) in KINDS.items():
        assigned = [row for row in ledger[kind] if row.get("binding") == "campaign_owned"]
        allocation = contract[kind]
        if (len(assigned) != count or allocation["start"] != start
                or allocation["capacity"] != capacity or allocation["allocated"] != count):
            raise EventIdError(f"{kind} allocation drifted")
        ids = [row["target_id"] for row in assigned]
        symbols = [row["target_symbol"] for row in assigned]
        if (any(type(value) is not int or value < start or value >= start + capacity for value in ids)
                or len(set(ids)) != len(ids) or ids != list(range(start, start + count))
                or any(not isinstance(name, str) or not SYMBOL.fullmatch(name) for name in symbols)
                or len(set(symbols)) != len(symbols)
                or all_symbols.intersection(symbols)):
            raise EventIdError(f"{kind} IDs or names are invalid")
        all_symbols.update(symbols)
        all_ids[kind] = set(ids)
        lines.extend(["", f"// {kind}: {count} IDs starting at 0x{start:04X}."])
        lines.extend(f"#define {row['target_symbol']} 0x{row['target_id']:04X}"
                     for row in assigned)

    if all_ids["flags"] & all_ids["vars"] or all_ids["flags"] & all_ids["trainers"]:
        raise EventIdError("event ID ranges overlap")
    lines.extend(["", "#endif // GUARD_CONSTANTS_CORMORIA_EVENT_IDS_H", ""])
    return "\n".join(lines).encode("utf-8")


def write_preview(output: Path, root: Path = ROOT) -> Path:
    output = output.resolve()
    root = root.resolve()
    if output == root or output.is_relative_to(root) or output.exists():
        raise EventIdError("output must be new and outside the host tree")
    data = render_event_ids(root)
    target = output / OUTPUT
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    return target


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        target = write_preview(args.output)
    except (OSError, KeyError, ValueError) as exc:
        print(f"Cormoria event IDs: {exc}", file=sys.stderr)
        return 1
    print(f"Prepared authenticated event IDs at {target}; runtime adapters pending")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
