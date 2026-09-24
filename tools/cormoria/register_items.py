"""Render Cormoria's new shared ItemInfo entries from the pinned donor file."""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from pathlib import Path

from tools import shared_item_registry

ROOT = Path(__file__).resolve().parents[2]
DONOR_ITEMS = Path("src/data/items.h")
DONOR_ITEMS_SHA256 = "f35d7724b9426ef8dec1a5de0833358e595dd39b3f6980ed44ce2ca9a69fa66a"
OUTPUT = Path("src/data/items_cormoria.h")
ENTRY = re.compile(r"^    \[(ITEM_[A-Z0-9_]+)\]\s*=\s*\{.*?^    \},",
                   re.MULTILINE | re.DOTALL)
ICON = re.compile(r"\bgItemIcon(?:Palette)?_[A-Za-z0-9_]+\b")


class ItemImportError(ValueError):
    """The donor item table differs from the reviewed shared-item binding."""


def render(donor: Path, root: Path = ROOT) -> bytes:
    rows = [row for row in shared_item_registry.validate(root)["items"]
            if row["introduced_by_world"] == "cormoria"]
    data = (donor / DONOR_ITEMS).read_bytes()
    if hashlib.sha256(data).hexdigest() != DONOR_ITEMS_SHA256:
        raise ItemImportError("donor item table hash differs from pinned revision")
    source = data.decode("utf-8")
    entries = {match[1]: match[0] for match in ENTRY.finditer(source)}
    if any(row["symbol"] not in entries for row in rows):
        raise ItemImportError("missing Cormoria item definition")
    blocks = []
    for row in rows:
        block = entries[row["symbol"]]
        block = block.replace(".name = _(", ".name = ITEM_NAME(")
        block = block.replace(".pluralName = _(", ".pluralName = ITEM_PLURAL_NAME(")
        block = re.sub(r"\bgItemIcon_(?:DouseDrive|BurnDrive|ChillDrive)\b",
                       "gItemIcon_Drive", block)
        if row["symbol"] == "ITEM_HM_SPLASH":
            if '.name = ITEM_NAME("HM01")' not in block:
                raise ItemImportError("donor Splash HM name drifted")
            block = block.replace('ITEM_NAME("HM01")', 'ITEM_NAME("HM10")')
        blocks.append(block)
    result = ("/* Cormoria ItemInfo data from pinned Dreamstone Mysteries revision\n"
              " * f7997186345885bfa23a170e5f573851fc034b9b. Shared item IDs are\n"
              " * allocated by data/shared_item_ids.json, not donor numbers. */\n\n"
              + "\n\n".join(blocks) + "\n").encode("utf-8")
    text = result.decode("utf-8")
    pockets = {name: text.count(f".pocket = {name},") for name in
               ("POCKET_ITEMS", "POCKET_KEY_ITEMS", "POCKET_TM_HM")}
    if pockets != {"POCKET_ITEMS": 2, "POCKET_KEY_ITEMS": 27, "POCKET_TM_HM": 1}:
        raise ItemImportError("Cormoria item pockets drifted")
    graphics = (root / "src/data/graphics/items.h").read_text(encoding="utf-8")
    if any(icon not in graphics for icon in ICON.findall(text)):
        raise ItemImportError("Cormoria item icon is absent from the shared catalog")
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        output = args.output.resolve()
        root = ROOT.resolve()
        donor = args.donor.resolve(strict=True)
        if output == root or output.is_relative_to(root) or output.is_relative_to(donor) or output.exists():
            raise ItemImportError("output must be new and outside source trees")
        content = render(donor)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(content)
    except (OSError, ValueError) as exc:
        print(f"Cormoria item import: {exc}", file=sys.stderr)
        return 1
    print(f"Prepared {len(content)} bytes of shared Cormoria item data at {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
