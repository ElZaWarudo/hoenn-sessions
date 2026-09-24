"""Pinned Cormoria berry-tree translation for map and script registration."""

from __future__ import annotations

import re
from pathlib import Path

from tools.cormoria import import_world

FIRST = 125
LAST = 153
COUNT = LAST - FIRST + 1
UNUSED_SOURCE_NAMES = (
    "BERRY_TREE_PELLUCA_C",
    "BERRY_TREE_RIVETSHORE_A",
    "BERRY_TREE_RIVETSHORE_B",
    "BERRY_TREE_RIVETSHORE_C",
)
UNUSED_SOURCE_IDS = (112, 113, 114, 115)


def bindings(root: Path = import_world.ROOT) -> dict[str, str]:
    """Return donor names to region-local names, rejecting ledger/header drift."""
    _, symbols, _ = import_world.load_manifests(root)
    rows = [row for row in symbols["semantic_constants"]
            if row["source_symbol"].startswith("BERRY_TREE_")]
    by_id = {row["source_value"]: row for row in rows}
    if len(rows) != 25 or len(by_id) != 25 or set(by_id) != set(range(90, 119)) - set(UNUSED_SOURCE_IDS):
        raise ValueError("Cormoria donor berry IDs drifted")
    result = {row["source_symbol"]: row["target_symbol"] for row in rows}
    for number, name in zip(UNUSED_SOURCE_IDS, UNUSED_SOURCE_NAMES):
        result[name] = f"Cormoria_{name}"
        by_id[number] = {"source_symbol": name, "target_symbol": result[name]}
    header = (root / "include/constants/berry.h").read_text(encoding="utf-8")
    definitions = dict((name, int(value)) for name, value in re.findall(
        r"^#define\s+(Cormoria_BERRY_TREE_\w+)\s+(\d+)\s*$", header, re.MULTILINE))
    expected = {row["target_symbol"]: FIRST + number - 90 for number, row in by_id.items()}
    if definitions != expected or len(expected) != COUNT or max(expected.values()) != LAST:
        raise ValueError("Cormoria berry allocation differs from pinned donor order")
    return result
