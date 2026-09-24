"""Authenticate and import Dreamstone's selected trainer table without substitution.

The region inventory owns selection and Cormoria's numeric IDs.  The donor's
generated C table owns every battle field.  No presentation or party values are
guessed from the human-readable .party file.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.cormoria.import_world import PINNED_SHA256

ROOT = Path(__file__).resolve().parents[2]
SOURCE = "src/data/trainers.h"
REGION_ID_START = 0x5000
RECORD_COUNT = 195
DIFFICULTY_RECORD_COUNT = 388
RECORD_START = re.compile(r"\[(DIFFICULTY_\w+)\]\[(TRAINER_\w+)\]\s*=")
LINE_DIRECTIVE = re.compile(r"^#line [^\n]*\n?", re.MULTILINE)
FIELD = re.compile(r"\.([A-Za-z][A-Za-z0-9_]*)\s*=")
TRAINER_FIELDS = {
    "trainerName", "trainerClass", "trainerPic", "encounterMusic_gender",
    "doubleBattle", "aiFlags", "mugshotColor", "partySize", "party",
    "items", "startingStatus",
}
MON_FIELDS = {
    "species", "gender", "heldItem", "ev", "iv", "ability", "lvl",
    "nature", "dynamaxLevel", "moves", "isShiny",
}
STARTING_STATUS_FIELDS = {
    "STARTING_STATUS_ELECTRIC_TERRAIN": "electricTerrain",
    "STARTING_STATUS_TRICK_ROOM": "trickRoom",
    "STARTING_STATUS_RAINBOW_OPPONENT": "rainbowOpponent",
}


class ImportErrorStrict(ValueError):
    pass


def _canonical_manifest(root: Path, name: str) -> dict:
    data = (root / "data/cormoria" / name).read_bytes()
    if b"\r" in data.replace(b"\r\n", b""):
        raise ImportErrorStrict(f"invalid manifest line endings: {name}")
    canonical = data.replace(b"\r\n", b"\n")
    if hashlib.sha256(canonical).hexdigest() != PINNED_SHA256[name]:
        raise ImportErrorStrict(f"pinned manifest hash changed: {name}")
    return json.loads(canonical)


def _braced(text: str, opening: int) -> int:
    if opening >= len(text) or text[opening] != "{":
        raise ImportErrorStrict("missing trainer initializer")
    depth = 0
    in_string = False
    escaped = False
    for offset in range(opening, len(text)):
        char = text[offset]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
        elif char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return offset
    raise ImportErrorStrict("unterminated trainer initializer")


def _record_body(text: str, match: re.Match[str]) -> str:
    opening = text.find("{", match.end())
    if opening < 0 or text[match.end():opening].strip():
        raise ImportErrorStrict(f"invalid initializer for {match.group(2)}")
    end = _braced(text, opening)
    return text[opening:end + 1]


def _top_level_fields(body: str) -> list[str]:
    fields = []
    depth = 0
    in_string = False
    escaped = False
    index = 0
    while index < len(body):
        char = body[index]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
        elif char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
        elif char == "." and depth == 1:
            match = FIELD.match(body, index)
            if match:
                fields.append(match.group(1))
                index = match.end() - 1
        index += 1
    return fields


def _field_value(body: str, name: str) -> str:
    match = re.search(rf"\.{name}\s*=\s*([^,]+),", body)
    if match is None:
        raise ImportErrorStrict(f"missing {name}")
    return match.group(1).strip()


def _transform(body: str, symbol: str) -> str:
    body = LINE_DIRECTIVE.sub("", body)
    fields = _top_level_fields(body)
    if len(fields) != len(set(fields)) or not set(fields) <= TRAINER_FIELDS:
        raise ImportErrorStrict(f"unsupported trainer fields in {symbol}: {fields}")
    required = {"trainerClass", "trainerPic", "encounterMusic_gender", "doubleBattle", "partySize", "party"}
    if not required <= set(fields):
        raise ImportErrorStrict(f"missing battle fields in {symbol}: {sorted(required - set(fields))}")
    music_gender = _field_value(body, "encounterMusic_gender")
    tokens = [token.strip() for token in music_gender.split("|")]
    female = "F_TRAINER_FEMALE" in tokens
    tokens = [token for token in tokens if token != "F_TRAINER_FEMALE"]
    if len(tokens) != 1 or not re.fullmatch(r"TRAINER_ENCOUNTER_MUSIC_[A-Z0-9_]+", tokens[0]):
        raise ImportErrorStrict(f"unsupported encounter music in {symbol}: {music_gender}")
    double = _field_value(body, "doubleBattle")
    if double not in {"TRUE", "FALSE"}:
        raise ImportErrorStrict(f"unsupported battle type in {symbol}: {double}")
    body = re.sub(
        r"\.encounterMusic_gender\s*=\s*[^,]+,",
        f".encounterMusic = {tokens[0]},\n        .gender = TRAINER_GENDER_{'FEMALE' if female else 'MALE'},",
        body,
        count=1,
    )
    body = re.sub(
        r"\.doubleBattle\s*=\s*(?:TRUE|FALSE),",
        f".battleType = TRAINER_BATTLE_TYPE_{'DOUBLES' if double == 'TRUE' else 'SINGLES'},",
        body,
        count=1,
    )
    if "startingStatus" in fields:
        status = _field_value(body, "startingStatus")
        if status not in STARTING_STATUS_FIELDS:
            raise ImportErrorStrict(f"unsupported starting status in {symbol}: {status}")
        # Dreamstone stores one enum.  The shared engine stores independent
        # status bits; set the corresponding bit rather than truncating the
        # enum into the first one-bit field of the host struct.
        body = re.sub(
            r"\.startingStatus\s*=\s*[^,]+,",
            f".startingStatus = {{ .{STARTING_STATUS_FIELDS[status]} = TRUE }},",
            body,
            count=1,
        )
    # The donor stores party literals as file-scope compound literals, which
    # have static storage duration in C. Preserve all mon fields verbatim.
    for name in FIELD.findall(body):
        if name not in TRAINER_FIELDS - {"encounterMusic_gender", "doubleBattle"} | MON_FIELDS | {"encounterMusic", "battleType"} | set(STARTING_STATUS_FIELDS.values()):
            raise ImportErrorStrict(f"unsupported member {name} in {symbol}")
    return body


def load_records(source_root: Path, repo_root: Path = ROOT) -> tuple[tuple[int, str, dict[str, str]], ...]:
    region = _canonical_manifest(repo_root, "region_manifest.json")
    sources = _canonical_manifest(repo_root, "source_manifest.json")
    if region["provenance"] != sources["provenance"] or region["provenance"]["revision"] != "f7997186345885bfa23a170e5f573851fc034b9b":
        raise ImportErrorStrict("donor provenance mismatch")
    source_entry = next((row for row in sources["files"] if row["path"] == SOURCE), None)
    if source_entry is None:
        raise ImportErrorStrict("trainer source absent from pinned inventory")
    raw = (source_root / SOURCE).read_bytes()
    if len(raw) != source_entry["bytes"] or hashlib.sha256(raw).hexdigest() != source_entry["sha256"]:
        raise ImportErrorStrict("trainer source differs from authenticated donor bytes")
    text = raw.decode("utf-8-sig")
    starts = list(RECORD_START.finditer(text))
    inventory = region["content"]["trainers"]
    ledger = _canonical_manifest(repo_root, "symbol_ledger.json")
    allocation = {entry["source_symbol"]: entry["target_id"] for entry in ledger["trainers"]}
    if len(inventory) != RECORD_COUNT or len(allocation) != RECORD_COUNT:
        raise ImportErrorStrict("trainer identity count drift")
    if sorted(allocation.values()) != list(range(REGION_ID_START, REGION_ID_START + RECORD_COUNT)):
        raise ImportErrorStrict("trainer runtime IDs are not contiguous")
    selected = {entry["source_symbol"] for entry in inventory}
    if selected != set(allocation):
        raise ImportErrorStrict("trainer inventory and ID allocation differ")
    found: dict[tuple[str, str], tuple[str, str]] = {}
    for index, match in enumerate(starts):
        symbol = match.group(2)
        if symbol not in selected:
            continue
        end = starts[index + 1].start() if index + 1 < len(starts) else len(text)
        original = text[match.start():end].strip()
        key = (match.group(1), symbol)
        if key in found:
            raise ImportErrorStrict(f"duplicate trainer record: {key}")
        found[key] = (original, _record_body(text, match))
    records = []
    for entry in inventory:
        symbol = entry["source_symbol"]
        by_difficulty = {}
        for expected in entry["records"]:
            key = expected["difficulty"], symbol
            if key not in found:
                raise ImportErrorStrict(f"missing trainer record: {key}")
            original, body = found[key]
            if hashlib.sha256(original.encode()).hexdigest() != expected["record_sha256"]:
                raise ImportErrorStrict(f"trainer record inventory hash drift: {key}")
            by_difficulty[key[0]] = _transform(body, symbol)
        records.append((allocation[symbol] - REGION_ID_START, symbol, by_difficulty))
    if len(found) != DIFFICULTY_RECORD_COUNT or sum(len(row[2]) for row in records) != DIFFICULTY_RECORD_COUNT:
        raise ImportErrorStrict("trainer difficulty record count drift")
    if any(set(row[2]) - {"DIFFICULTY_EASY", "DIFFICULTY_NORMAL"} for row in records):
        raise ImportErrorStrict("unreviewed trainer difficulty")
    return tuple(sorted(records))


def render(records: tuple[tuple[int, str, dict[str, str]], ...]) -> str:
    lines = [
        "/* Generated by tools/cormoria/import_trainers.py from pinned Dreamstone data. */",
        "#ifndef GUARD_DATA_CORMORIA_TRAINERS_H",
        "#define GUARD_DATA_CORMORIA_TRAINERS_H",
        "",
        "const struct Trainer gCormoriaTrainers[DIFFICULTY_COUNT][CORMORIA_TRAINER_RECORD_COUNT] =",
        "{",
    ]
    for difficulty in ("DIFFICULTY_EASY", "DIFFICULTY_NORMAL"):
        lines.append(f"    [{difficulty}] = {{")
        for ordinal, symbol, by_difficulty in records:
            if difficulty in by_difficulty:
                lines.extend((f"        [{ordinal}] = /* {symbol} */", by_difficulty[difficulty] + ","))
        lines.append("    },")
    lines.extend(("};", "", "#endif /* GUARD_DATA_CORMORIA_TRAINERS_H */", ""))
    return "\n".join(lines)


def run(source_root: Path, repo_root: Path = ROOT, check: bool = False) -> int:
    output = repo_root / "src/data/cormoria/trainers.h"
    rendered = render(load_records(source_root, repo_root))
    if check:
        if not output.is_file() or output.read_text(encoding="utf-8") != rendered:
            raise ImportErrorStrict("generated Cormoria roster drift")
    else:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered, encoding="utf-8", newline="\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", required=True, type=Path)
    parser.add_argument("--repo-root", default=ROOT, type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        return run(args.source_root, args.repo_root, args.check)
    except (OSError, ValueError, KeyError) as exc:
        print(f"Cormoria trainer import rejected: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
