"""Strict, reproducible importer for the selected Johto trainer roster.

The donor is deliberately parsed as data rather than compiled.  Every source
expression which can affect a battle must have an explicit conversion below;
silently accepting a new donor construct would make a future import unsafe.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
DONOR_REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
RECORD_COUNT = 284
MON_COUNT = 687

# RocketHideout_B2F pairs these opponents against Lance and up to three selected
# player Pokemon. The host multi-battle policy reads opponent team sizes only.
HALF_TEAM_OPPONENTS = frozenset({"TRAINER_ARIANA_1", "TRAINER_GRUNT_23"})


class ImportErrorStrict(ValueError):
    pass


@dataclass(frozen=True)
class Mon:
    iv: int
    level: int
    species: str
    held_item: str
    moves: tuple[str, ...]


@dataclass(frozen=True)
class Trainer:
    symbol: str
    ordinal: int
    trainer_class: str
    music: str
    gender: str
    portrait: str
    name_expr: str
    items: tuple[str, ...]
    double_battle: bool
    ai_flags: str
    party: tuple[Mon, ...]
    custom_moves: bool


def _matching_brace(text: str, opening: int) -> int:
    if opening >= len(text) or text[opening] != "{":
        raise ImportErrorStrict(f"expected initializer at offset {opening}")
    depth = 0
    for i in range(opening, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return i
    raise ImportErrorStrict(f"unterminated initializer at offset {opening}")


def _split_top_level(text: str, separator: str = ",") -> list[str]:
    result: list[str] = []
    start = 0
    depth = 0
    quote = False
    escaped = False
    for i, char in enumerate(text):
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quote = False
            continue
        if char == '"':
            quote = True
        elif char in "({[":
            depth += 1
        elif char in ")}]":
            depth -= 1
        elif char == separator and depth == 0:
            if text[start:i].strip():
                result.append(text[start:i].strip())
            start = i + 1
    if text[start:].strip():
        result.append(text[start:].strip())
    return result


def _field(block: str, name: str) -> str:
    match = re.search(rf"\.{name}\s*=\s*([^,\n]+)", block)
    if not match:
        raise ImportErrorStrict(f"missing .{name} in trainer record")
    return match.group(1).strip()


def _braced_field(block: str, name: str) -> str:
    match = re.search(rf"\.{name}\s*=\s*\{{", block)
    if not match:
        raise ImportErrorStrict(f"missing .{name} in trainer record")
    end = _matching_brace(block, match.end() - 1)
    return block[match.end():end]


def _record_blocks(text: str, selected: set[str]) -> dict[str, str]:
    records: dict[str, str] = {}
    pattern = re.compile(r"^\s*\[(TRAINER_[A-Z0-9_]+)\]\s*=\s*\{", re.M)
    for match in pattern.finditer(text):
        symbol = match.group(1)
        if symbol not in selected:
            continue
        end = _matching_brace(text, match.end() - 1)
        block = text[match.start():end + 1]
        fields = set(re.findall(r"\.([A-Za-z_][A-Za-z0-9_]*)\s*=", block))
        if fields != {
            "trainerClass", "encounterMusic_gender", "trainerPic", "trainerName",
            "items", "doubleBattle", "aiFlags", "party",
        }:
            raise ImportErrorStrict(f"unsupported fields in {symbol}: {sorted(fields)}")
        if symbol in records:
            raise ImportErrorStrict(f"duplicate selected trainer {symbol}")
        records[symbol] = block
    return records


def _party_arrays(text: str, names: set[str]) -> dict[str, tuple[str, str]]:
    arrays: dict[str, tuple[str, str]] = {}
    pattern = re.compile(
        r"static\s+const\s+struct\s+(TrainerMon[A-Za-z]+)\s+"
        r"(sParty[A-Za-z0-9_]+)\s*\[\]\s*=\s*\{"
    )
    for match in pattern.finditer(text):
        type_name, name = match.group(1), match.group(2)
        if name not in names:
            continue
        end = _matching_brace(text, match.end() - 1)
        arrays[name] = (type_name, text[match.end():end])
    return arrays


def _parse_party_array(type_name: str, body: str, party_name: str) -> tuple[Mon, ...]:
    entries: list[str] = []
    i = 0
    while i < len(body):
        if body[i] == "{":
            end = _matching_brace(body, i)
            entries.append(body[i + 1:end])
            i = end + 1
        else:
            i += 1
    expected = {
        "TrainerMonNoItemDefaultMoves": (False, False),
        "TrainerMonItemDefaultMoves": (True, False),
        "TrainerMonNoItemCustomMoves": (False, True),
        "TrainerMonItemCustomMoves": (True, True),
    }
    if type_name not in expected:
        raise ImportErrorStrict(f"unsupported party struct {type_name} ({party_name})")
    has_item, has_moves = expected[type_name]
    result: list[Mon] = []
    for entry in entries:
        fields = set(re.findall(r"\.([A-Za-z_][A-Za-z0-9_]*)\s*=", entry))
        allowed = {"iv", "lvl", "species"}
        # The donor's item structs permit omitting heldItem; the zero value is
        # ITEM_NONE. Treat explicit and omitted values identically.
        if has_item:
            allowed.add("heldItem")
        if has_moves:
            allowed.add("moves")
        required = {"iv", "lvl", "species"}
        if has_moves:
            required.add("moves")
        if not fields.issubset(allowed) or not required.issubset(fields):
            raise ImportErrorStrict(f"unsupported fields in {party_name}: {sorted(fields)}")
        iv_text = _field(entry, "iv")
        if not re.fullmatch(r"(?:0|[1-9][0-9]*)", iv_text):
            raise ImportErrorStrict(f"nonliteral IV in {party_name}: {iv_text}")
        iv = int(iv_text)
        if not 0 <= iv <= 255:
            raise ImportErrorStrict(f"IV outside donor range in {party_name}: {iv}")
        level_text = _field(entry, "lvl")
        if not re.fullmatch(r"(?:0|[1-9][0-9]*)", level_text):
            raise ImportErrorStrict(f"nonliteral level in {party_name}: {level_text}")
        level = int(level_text)
        species = _field(entry, "species")
        if not re.fullmatch(r"SPECIES_[A-Z0-9_]+", species):
            raise ImportErrorStrict(f"invalid species in {party_name}: {species}")
        if not has_item and "heldItem" in fields:
            raise ImportErrorStrict(f"no-item party has heldItem in {party_name}")
        held_item = _field(entry, "heldItem") if "heldItem" in fields else "ITEM_NONE"
        if not re.fullmatch(r"ITEM_[A-Z0-9_]+", held_item):
            raise ImportErrorStrict(f"invalid held item in {party_name}: {held_item}")
        if has_moves:
            move_body = _braced_field(entry, "moves")
            moves = tuple(x.strip() for x in _split_top_level(move_body))
            if len(moves) != 4 or any(not re.fullmatch(r"MOVE_[A-Z0-9_]+", x) for x in moves):
                raise ImportErrorStrict(f"invalid moves in {party_name}: {moves}")
        else:
            moves = ("MOVE_NONE",) * 4
        result.append(Mon(iv, level, species, held_item, moves))
    if not result:
        raise ImportErrorStrict(f"empty party {party_name}")
    return tuple(result)


def _resolve_symbol(source: str, aliases: dict[str, str], host_text: str, kind: str) -> str:
    if source in aliases:
        return aliases[source]
    if re.search(rf"\b{re.escape(source)}\b", host_text):
        return source
    raise ImportErrorStrict(f"unmapped {kind} {source}")


def _parse_ai(expr: str) -> str:
    if expr == "0":
        return "0"
    terms = [x.strip() for x in expr.split("|")]
    mapping = {
        "AI_SCRIPT_CHECK_BAD_MOVE": "AI_FLAG_CHECK_BAD_MOVE",
        "AI_SCRIPT_TRY_TO_FAINT": "AI_FLAG_TRY_TO_FAINT",
        "AI_SCRIPT_CHECK_VIABILITY": "AI_FLAG_CHECK_VIABILITY",
    }
    if not terms or any(x not in mapping for x in terms) or len(set(terms)) != len(terms):
        raise ImportErrorStrict(f"unsupported AI expression {expr}")
    return " | ".join(mapping[x] for x in terms)


def _music_and_gender(expr: str, aliases: dict[str, str], host_text: str) -> tuple[str, str]:
    terms = [x.strip() for x in expr.split("|")]
    if any(x == "F_TRAINER_FEMALE" for x in terms):
        terms.remove("F_TRAINER_FEMALE")
        gender = "TRAINER_GENDER_FEMALE"
    else:
        gender = "TRAINER_GENDER_MALE"
    if len(terms) != 1 or not terms[0].startswith("TRAINER_ENCOUNTER_MUSIC_"):
        raise ImportErrorStrict(f"unsupported encounter music expression {expr}")
    return _resolve_symbol(terms[0], aliases, host_text, "music"), gender


def _parse_name(expr: str) -> str:
    if expr == '_("{B_RIVAL_NAME}")':
        return '_("{RIVAL}")'
    if "{" in expr or "}" in expr:
        raise ImportErrorStrict(f"unsupported trainer name placeholder {expr}")
    if not re.fullmatch(r'_\("(?:[^"\\]|\\.)*"\)', expr):
        raise ImportErrorStrict(f"unsupported trainer name expression {expr}")
    return expr


def _verify_pinned_donor(donor: Path) -> None:
    if not donor.is_dir():
        raise ImportErrorStrict(f"donor root does not exist: {donor}")
    def git(*args: str) -> str:
        try:
            return subprocess.check_output(
                ["git", "-C", str(donor), *args], text=True, stderr=subprocess.STDOUT
            ).strip()
        except subprocess.CalledProcessError as exc:
            raise ImportErrorStrict(f"donor git verification failed: {exc.output.strip()}") from exc
    revision = git("rev-parse", "HEAD")
    tree = git("rev-parse", "HEAD^{tree}")
    if revision != DONOR_REVISION or tree != DONOR_TREE:
        raise ImportErrorStrict(
            f"donor pin mismatch: revision={revision}, tree={tree}; "
            f"expected {DONOR_REVISION}/{DONOR_TREE}"
        )
    if git("status", "--porcelain"):
        raise ImportErrorStrict("donor working tree must be clean")


def load_roster(donor_root: Path, repo_root: Path = ROOT) -> tuple[Trainer, ...]:
    _verify_pinned_donor(donor_root)
    ledger = json.loads((repo_root / "data/johto/content_symbols.json").read_text(encoding="utf-8"))
    selected = ledger.get("identities", {}).get("trainers", [])
    if len(selected) != RECORD_COUNT:
        raise ImportErrorStrict(f"ledger trainer count is {len(selected)}, expected {RECORD_COUNT}")
    if [x.get("ordinal") for x in selected] != list(range(RECORD_COUNT)):
        raise ImportErrorStrict("trainer ledger ordinals are not append-only and contiguous")
    if selected[0].get("symbol") != "TRAINER_JOEY" or selected[0].get("ordinal") != 0:
        raise ImportErrorStrict("Joey must remain immutable ordinal zero")
    expected_pin = ledger.get("provenance", {})
    if expected_pin.get("donor_revision") != DONOR_REVISION or expected_pin.get("donor_tree") != DONOR_TREE:
        raise ImportErrorStrict("content ledger donor pin drift")
    presentation = json.loads((repo_root / "data/johto/trainer_presentation.json").read_text(encoding="utf-8"))
    presentation_pin = presentation.get("provenance", {})
    if (presentation_pin.get("donor_revision"), presentation_pin.get("donor_tree")) != (DONOR_REVISION, DONOR_TREE):
        raise ImportErrorStrict("presentation donor pin drift")
    if presentation_pin.get("donor_repository") != DONOR_REPOSITORY:
        raise ImportErrorStrict("presentation donor repository drift")
    class_aliases = presentation.get("class_aliases", {})
    portrait_aliases = presentation.get("portrait_aliases", {})
    music_aliases = presentation.get("music_aliases", {})
    host_constants = (repo_root / "include/constants/trainers.h").read_text(encoding="utf-8")
    trainer_source = (donor_root / "src/data/trainers.h").read_text(encoding="utf-8")
    party_source = (donor_root / "src/data/trainer_parties.h").read_text(encoding="utf-8")
    symbols = {x["symbol"] for x in selected}
    if len(symbols) != RECORD_COUNT:
        raise ImportErrorStrict("duplicate trainer ledger symbols")
    records = _record_blocks(trainer_source, symbols)
    if set(records) != symbols:
        raise ImportErrorStrict(f"selected trainer records missing: {sorted(symbols - set(records))}")
    party_names = {
        re.search(r"\.party\s*=\s*[A-Z_]+\((sParty[A-Za-z0-9_]+)\)", block).group(1)
        for block in records.values()
    }
    arrays = _party_arrays(party_source, party_names)
    if set(arrays) != party_names:
        raise ImportErrorStrict(f"selected party arrays missing: {sorted(party_names - set(arrays))}")
    parsed_arrays = {name: _parse_party_array(*value, name) for name, value in arrays.items()}
    result: list[Trainer] = []
    for identity in selected:
        symbol = identity["symbol"]
        block = records[symbol]
        class_name = _resolve_symbol(_field(block, "trainerClass"), class_aliases, host_constants, "class")
        music, gender = _music_and_gender(_field(block, "encounterMusic_gender"), music_aliases, host_constants)
        portrait = _resolve_symbol(_field(block, "trainerPic"), portrait_aliases, host_constants, "portrait")
        name_expr = _parse_name(_field(block, "trainerName"))
        item_body = _braced_field(block, "items")
        items = tuple(x.strip() for x in _split_top_level(item_body))
        if len(items) > 4 or any(not re.fullmatch(r"ITEM_[A-Z0-9_]+", x) for x in items):
            raise ImportErrorStrict(f"invalid trainer item list for {symbol}")
        items = items + ("ITEM_NONE",) * (4 - len(items))
        double_text = _field(block, "doubleBattle")
        if double_text not in {"FALSE", "TRUE"}:
            raise ImportErrorStrict(f"invalid doubleBattle for {symbol}")
        party_match = re.fullmatch(r"([A-Z_]+)\((sParty[A-Za-z0-9_]+)\)", _field(block, "party"))
        if not party_match:
            raise ImportErrorStrict(f"invalid party expression for {symbol}")
        layout, party_name = party_match.groups()
        expected_layout = {
            "NO_ITEM_DEFAULT_MOVES": (False, False),
            "NO_ITEM_CUSTOM_MOVES": (False, True),
            "ITEM_DEFAULT_MOVES": (True, False),
            "ITEM_CUSTOM_MOVES": (True, True),
        }
        if layout not in expected_layout:
            raise ImportErrorStrict(f"unsupported party layout {layout} for {symbol}")
        has_item, custom_moves = expected_layout[layout]
        party = parsed_arrays[party_name]
        if not has_item and any(mon.held_item != "ITEM_NONE" for mon in party):
            raise ImportErrorStrict(f"no-item party contains held item: {symbol}")
        if has_item and not any(mon.held_item != "ITEM_NONE" for mon in party):
            # The donor's ITEM layout is still semantically meaningful when every
            # held item is ITEM_NONE, so retain it exactly.
            pass
        if not custom_moves and any(mon.moves != ("MOVE_NONE",) * 4 for mon in party):
            raise ImportErrorStrict(f"default-move party contains explicit moves: {symbol}")
        ai = _parse_ai(_field(block, "aiFlags"))
        result.append(Trainer(
            symbol=symbol,
            ordinal=int(identity["ordinal"]),
            trainer_class=class_name,
            music=music,
            gender=gender,
            portrait=portrait,
            name_expr=name_expr,
            items=items,
            double_battle=double_text == "TRUE",
            ai_flags=ai,
            party=party,
            custom_moves=custom_moves,
        ))
    if sum(len(x.party) for x in result) != MON_COUNT:
        raise ImportErrorStrict(f"party member count is {sum(len(x.party) for x in result)}, expected {MON_COUNT}")
    return tuple(result)


def _scaled_iv(iv: int) -> int:
    return iv * 31 // 255


def render_header(roster: tuple[Trainer, ...]) -> str:
    out = [
        "/* Generated by tools/johto/import_trainers.py; do not edit. */",
        "#ifndef GUARD_DATA_JOHTO_TRAINERS_H",
        "#define GUARD_DATA_JOHTO_TRAINERS_H",
        "",
    ]
    for trainer in roster:
        name = f"sJohtoParty_{trainer.ordinal:03d}"
        out.append(f"static const struct TrainerMon {name}[] =")
        out.append("{")
        for mon in trainer.party:
            scaled = _scaled_iv(mon.iv)
            out.extend([
                "    {",
                f"        .iv = TRAINER_PARTY_IVS({scaled}, {scaled}, {scaled}, {scaled}, {scaled}, {scaled}),",
                f"        .moves = {{ {', '.join(mon.moves)} }},",
                f"        .species = {mon.species},",
                f"        .heldItem = {mon.held_item},",
                "        .ability = ABILITY_NONE,",
                f"        .lvl = {mon.level},",
                "        .ball = BALL_POKE,",
                "    },",
            ])
        out.extend(["};", ""])
    out.extend([
        "const struct Trainer gJohtoTrainers[DIFFICULTY_COUNT][JOHTO_TRAINER_RECORD_COUNT] =",
        "{",
        "    [DIFFICULTY_NORMAL] =",
        "    {",
    ])
    for trainer in roster:
        party_name = f"sJohtoParty_{trainer.ordinal:03d}"
        team_size = "MULTI_TEAM_SIZE_HALF" if trainer.symbol in HALF_TEAM_OPPONENTS else "MULTI_TEAM_SIZE_FULL"
        out.extend([
            f"        [{trainer.ordinal}] = /* {trainer.symbol} */",
            "        {",
            f"            .aiFlags = {trainer.ai_flags},",
            f"            .party = {party_name},",
            f"            .items = {{ {', '.join(trainer.items)} }},",
            f"            .trainerClass = {trainer.trainer_class},",
            f"            .encounterMusic = {trainer.music},",
            f"            .multiTeamSize = {team_size},",
            f"            .gender = {trainer.gender},",
            f"            .battleType = {'TRAINER_BATTLE_TYPE_DOUBLES' if trainer.double_battle else 'TRAINER_BATTLE_TYPE_SINGLES'},",
            f"            .partySize = ARRAY_COUNT({party_name}),",
            f"            .trainerPic = {trainer.portrait},",
            f"            .trainerName = {trainer.name_expr},",
            "        },",
        ])
    out.extend(["    },", "};", "", "#endif /* GUARD_DATA_JOHTO_TRAINERS_H */", ""])
    return "\n".join(out)


def generated_path(repo_root: Path = ROOT) -> Path:
    return repo_root / "src/data/johto/trainers.h"


def run(donor_root: Path, repo_root: Path, check: bool) -> int:
    roster = load_roster(donor_root, repo_root)
    rendered = render_header(roster)
    path = generated_path(repo_root)
    if check:
        if not path.is_file():
            raise ImportErrorStrict(f"generated file is missing: {path}")
        current = path.read_text(encoding="utf-8")
        if current != rendered:
            raise ImportErrorStrict(f"generated file drift: {path}")
        return 0
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(rendered, encoding="utf-8", newline="\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor-root", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        return run(args.donor_root.resolve(), args.repo_root.resolve(), args.check)
    except (OSError, ImportErrorStrict, json.JSONDecodeError) as exc:
        print(f"johto trainer import rejected: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
