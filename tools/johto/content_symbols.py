"""Generate the selected Johto content symbol ledger.

The donor is intentionally treated as a pinned source corpus.  This tool
scans only the selected map.json and scripts.inc files, records every lexical
reference and its source location, and emits a qualified, append-only symbol
catalog.  It does not import scripts or maps into the game.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = ROOT / "data/johto/region_manifest.json"
OUTPUT_PATH = ROOT / "data/johto/content_symbols.json"
HEADER_PATH = ROOT / "include/constants/johto_content.h"

DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
DONOR_REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
MANIFEST_SOURCE_REVISION = "21b8c9f918800a07b74a5ee2a882b1374d9ac4f9"

FLAG_START = 0x6000
VAR_START = 0x7000
TRAINER_START = 0x4000
FLAG_CAPACITY = 768
VAR_CAPACITY = 96
TRAINER_CAPACITY = 512

# Frozen initial allocation, independent of future lexical discovery order.
INITIAL_COUNTS = {"flags": 539, "vars": 63, "trainers": 284}
INITIAL_BINDINGS_SHA256 = "7f570138eb10da1801d23ef12d950b38f9d65fcb80af533bd83ab147cd3f1dc4"

EXCLUDED_TRAINER_TOKENS = (
    "TRAINER_BATTLE_SET_TRAINER_A",
    "TRAINER_BATTLE_SET_TRAINER_B",
    "TRAINER_TYPE_NONE",
    "TRAINER_TYPE_NORMAL",
)

FLAG_ALIAS_SYMBOLS = (
    "FLAG_TEMP_1", "FLAG_TEMP_2", "FLAG_TEMP_3", "FLAG_TEMP_4",
    "FLAG_TEMP_5", "FLAG_TEMP_6", "FLAG_TEMP_7", "FLAG_TEMP_8",
    "FLAG_TEMP_9", "FLAG_TEMP_A", "FLAG_TEMP_10",
    "FLAG_TEMP_HIDE_FOLLOWER",
)
VAR_ALIAS_SYMBOLS = (
    "VAR_FACING", "VAR_RESULT", "VAR_ITEM_ID", "VAR_LAST_TALKED",
    "VAR_TEMP_0", "VAR_TEMP_1", "VAR_TEMP_2", "VAR_TEMP_3",
    "VAR_TEMP_TRANSFERRED_SPECIES",
)

ALIAS_REASONS = {
    "FLAG_TEMP_1": "host temporary flag for one-shot script introductions",
    "FLAG_TEMP_2": "host temporary flag for one-shot script introductions",
    "FLAG_TEMP_3": "host temporary flag for one-shot script introductions",
    "FLAG_TEMP_4": "host temporary flag for one-shot script introductions",
    "FLAG_TEMP_5": "host temporary flag reserved for transient script operands",
    "FLAG_TEMP_6": "host temporary flag reserved for transient script operands",
    "FLAG_TEMP_7": "host temporary flag reserved for transient script operands",
    "FLAG_TEMP_8": "host temporary flag reserved for transient script operands",
    "FLAG_TEMP_9": "host temporary flag reserved for transient script operands",
    "FLAG_TEMP_A": "host temporary flag reserved for transient script operands",
    "FLAG_TEMP_10": "host temporary flag reserved for transient script operands",
    "FLAG_TEMP_HIDE_FOLLOWER": "host follower visibility flag consumed by the engine",
    "VAR_FACING": "host native-command facing operand",
    "VAR_RESULT": "host native-command result operand",
    "VAR_ITEM_ID": "host native-command item operand",
    "VAR_LAST_TALKED": "host native-command last-talked operand",
    "VAR_TEMP_0": "host temporary script scratch operand",
    "VAR_TEMP_1": "host temporary script scratch operand",
    "VAR_TEMP_2": "host temporary script scratch operand",
    "VAR_TEMP_3": "host temporary script scratch operand",
    "VAR_TEMP_TRANSFERRED_SPECIES": "host temporary species-transfer operand",
}

PENDING_CLOSURE = [
    "external_script_includes_and_labels",
    "macro_and_computed_symbol_references",
    "c_specials_and_animation_callbacks",
    "script_fallthrough_and_control_flow_semantics",
    "external_trainer_rematch_and_postgame_closure",
]

TOKEN_RE = re.compile(
    r"(?<![A-Za-z0-9_])(?:FLAG|VAR|TRAINER)_[A-Z][A-Z0-9_]*(?![A-Za-z0-9_])"
)
DEFINE_RE = re.compile(r"^#define[ \t]+([A-Za-z_][A-Za-z0-9_]*)(?:[ \t]+(.*))?$")
TRAINER_RECORD_RE = re.compile(r"\[([A-Za-z_][A-Za-z0-9_]*)\][ \t]*=")


class ContentSymbolError(ValueError):
    """A fail-closed source, identity, or ledger error."""


def _sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _sha256(path: Path) -> str:
    return _sha256_bytes(path.read_bytes())


def _load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ContentSymbolError(f"missing required JSON: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ContentSymbolError(f"invalid JSON in {path}: {exc}") from exc


def _git_revision(donor: Path) -> tuple[str, str]:
    try:
        revision = subprocess.check_output(
            ["git", "-C", str(donor), "rev-parse", "HEAD"],
            text=True, stderr=subprocess.STDOUT,
        ).strip()
        tree = subprocess.check_output(
            ["git", "-C", str(donor), "rev-parse", "HEAD^{tree}"],
            text=True, stderr=subprocess.STDOUT,
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise ContentSymbolError(f"donor is not a readable git checkout: {donor}") from exc
    return revision, tree


def _assert_donor_clean(donor: Path) -> None:
    if not (donor / ".git").exists():
        return
    try:
        status = subprocess.check_output(
            ["git", "-C", str(donor), "status", "--porcelain=v1",
             "--untracked-files=all", "--", "data/maps", "include/constants",
             "src/data/trainers.h"],
            text=True, stderr=subprocess.STDOUT,
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise ContentSymbolError("unable to verify donor cleanliness") from exc
    if status:
        raise ContentSymbolError("donor selected inputs are dirty: " + status)


def _read_definitions(path: Path, prefix: str, relative_path: str | None = None) -> dict[str, list[dict[str, Any]]]:
    result: dict[str, list[dict[str, Any]]] = {}
    display_path = relative_path or path.as_posix()
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        match = DEFINE_RE.match(line.strip())
        if not match:
            continue
        symbol, expression = match.groups()
        if not symbol.startswith(prefix) or "(" in symbol:
            continue
        result.setdefault(symbol, []).append({
            "path": display_path,
            "line": line_number,
            "column": line.find(symbol) + 1,
            "expression": (expression or "").strip(),
        })
    return result


def _resolve_literal(expression: str, definitions: dict[str, list[dict[str, Any]]], seen: set[str] | None = None) -> int | None:
    """Resolve simple constant expressions for useful provenance evidence."""
    expression = expression.split("//", 1)[0].split("/*", 1)[0].strip()
    expression = expression.strip("() ")
    if not expression:
        return None
    if seen is None:
        seen = set()
    names = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", expression)
    for name in names:
        if name not in definitions or name in seen:
            return None
        replacement = _resolve_literal(
            definitions[name][0]["expression"], definitions, seen | {name}
        )
        if replacement is None:
            return None
        expression = re.sub(rf"\b{re.escape(name)}\b", str(replacement), expression)
    if not re.fullmatch(r"[0-9A-Fa-fxX+*/%<>&|()~\- ]+", expression):
        return None
    try:
        tree = ast.parse(expression, mode="eval")
        allowed = (ast.Expression, ast.Constant, ast.BinOp, ast.UnaryOp,
                   ast.Add, ast.Sub, ast.Mult, ast.Div, ast.FloorDiv,
                   ast.Mod, ast.LShift, ast.RShift, ast.BitAnd, ast.BitOr,
                   ast.BitXor, ast.USub, ast.UAdd, ast.Invert)
        if any(not isinstance(node, allowed) for node in ast.walk(tree)):
            return None
        value = eval(compile(tree, "<constant>", "eval"), {"__builtins__": {}}, {})
        return value if isinstance(value, int) else None
    except (SyntaxError, ValueError, ZeroDivisionError, OverflowError):
        return None


def _definition_for(symbol: str, definitions: dict[str, list[dict[str, Any]]], source_label: str) -> dict[str, Any]:
    matches = definitions.get(symbol)
    if not matches:
        raise ContentSymbolError(f"unrecognized {source_label} symbol definition: {symbol}")
    if len(matches) != 1:
        raise ContentSymbolError(f"duplicate {source_label} definition: {symbol}")
    return dict(matches[0])


def _manifest_maps() -> list[dict[str, Any]]:
    manifest = _load_json(MANIFEST_PATH)
    provenance = manifest.get("provenance", {})
    if provenance.get("donor_revision") != DONOR_REVISION:
        raise ContentSymbolError("manifest donor revision differs from pinned corpus")
    if provenance.get("donor_tree") != DONOR_TREE:
        raise ContentSymbolError("manifest donor tree differs from pinned corpus")
    selection = manifest.get("selection", {})
    maps = manifest.get("maps")
    if selection.get("selected_count") != 239 or not isinstance(maps, list) or len(maps) != 239:
        raise ContentSymbolError("manifest does not contain exactly239 selected maps")
    return maps


def _source_files(donor: Path, maps: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], dict[str, list[dict[str, Any]]]]:
    files: list[dict[str, Any]] = []
    references: dict[str, list[dict[str, Any]]] = {}
    seen_maps: set[str] = set()
    for record in maps:
        source_name = record.get("source_name")
        source_map = record.get("source_map")
        if not isinstance(source_name, str) or not isinstance(source_map, str) or source_map in seen_maps:
            raise ContentSymbolError(f"invalid or duplicate manifest map record: {record!r}")
        seen_maps.add(source_map)
        for kind, relative in (
            ("map_json", f"data/maps/{source_name}/map.json"),
            ("script", f"data/maps/{source_name}/scripts.inc"),
        ):
            path = donor / relative
            if not path.exists():
                raise ContentSymbolError(f"missing selected donor source: {relative}")
            raw = path.read_bytes()
            text = raw.decode("utf-8")
            files.append({
                "map": source_map,
                "map_name": source_name,
                "kind": kind,
                "path": relative,
                "sha256": _sha256_bytes(raw),
            })
            for line_number, line in enumerate(text.splitlines(), 1):
                for match in TOKEN_RE.finditer(line):
                    references.setdefault(match.group(), []).append({
                        "path": relative,
                        "kind": kind,
                        "line": line_number,
                        "column": match.start() + 1,
                    })
            if kind == "map_json":
                map_data = _load_json(path)
                if map_data.get("id") != source_map or map_data.get("name") != source_name:
                    raise ContentSymbolError(f"selected map identity mismatch: {relative}")
    return files, references


def _source_hashes(files: Iterable[dict[str, Any]]) -> dict[str, str]:
    return {entry["path"]: entry["sha256"] for entry in files}


def _record_definitions(path: Path, relative_path: str | None = None) -> dict[str, list[dict[str, Any]]]:
    result: dict[str, list[dict[str, Any]]] = {}
    display_path = relative_path or path.as_posix()
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        for match in TRAINER_RECORD_RE.finditer(line):
            symbol = match.group(1)
            if symbol.startswith("TRAINER_"):
                result.setdefault(symbol, []).append({
                    "path": display_path,
                    "line": line_number,
                    "column": match.start(1) + 1,
                })
    return result


def _alias_records(symbols: Iterable[str], kind: str) -> list[dict[str, Any]]:
    expected = FLAG_ALIAS_SYMBOLS if kind == "flag" else VAR_ALIAS_SYMBOLS
    actual = list(symbols)
    if tuple(actual) != tuple(expected):
        raise ContentSymbolError(f"{kind} aliases differ from reviewed transient set")
    return [
        {
            "symbol": symbol,
            "qualified": f"JOHTO_{symbol}",
            "target": symbol,
            "reason": ALIAS_REASONS[symbol],
        }
        for symbol in expected
    ]


def _foundation_macros() -> set[str]:
    found: set[str] = set()
    for path in (
        ROOT / "include/constants/johto_events.h",
        ROOT / "include/constants/johto_trainers.h",
    ):
        for line in path.read_text(encoding="utf-8").splitlines():
            match = DEFINE_RE.match(line.strip())
            if match:
                found.add(match.group(1))
    return found


def _identity_entry(symbol: str, kind: str, ordinal: int, definition: dict[str, Any], references: list[dict[str, Any]], definition_hash: str, record_definition: dict[str, Any] | None = None) -> dict[str, Any]:
    start = {"flag": FLAG_START, "var": VAR_START, "trainer": TRAINER_START}[kind]
    definition["sha256"] = definition_hash
    if record_definition is not None:
        record_definition = dict(record_definition)
    entry = {
        "symbol": symbol,
        "qualified": f"JOHTO_{symbol}",
        "ordinal": ordinal,
        "runtime_id": start + ordinal,
        "definition": definition,
        "references": references,
    }
    if record_definition is not None:
        entry["record_definition"] = record_definition
    return entry


def build_ledger(donor: str | Path) -> dict[str, Any]:
    donor_path = Path(donor).resolve()
    revision, tree = _git_revision(donor_path)
    if revision != DONOR_REVISION or tree != DONOR_TREE:
        raise ContentSymbolError(
            f"donor pin mismatch: expected {DONOR_REVISION}/{DONOR_TREE}, got {revision}/{tree}"
        )
    _assert_donor_clean(donor_path)
    maps = _manifest_maps()
    source_files, references = _source_files(donor_path, maps)
    source_hashes = _source_hashes(source_files)
    definition_paths = {
        "flag": "include/constants/flags.h",
        "var": "include/constants/vars.h",
        "trainer": "include/constants/opponents.h",
    }
    definitions = {
        kind: _read_definitions(donor_path / relative, prefix, relative)
        for kind, (relative, prefix) in {
            "flag": (definition_paths["flag"], "FLAG_"),
            "var": (definition_paths["var"], "VAR_"),
            "trainer": (definition_paths["trainer"], "TRAINER_"),
        }.items()
    }
    trainer_records = _record_definitions(donor_path / "src/data/trainers.h", "src/data/trainers.h")
    selected = {
        kind: sorted(
            symbol for symbol in references if symbol.startswith(prefix)
            and not (kind == "flag" and symbol in FLAG_ALIAS_SYMBOLS)
            and not (kind == "var" and symbol in VAR_ALIAS_SYMBOLS)
            and not (kind == "trainer" and symbol in EXCLUDED_TRAINER_TOKENS)
        )
        for kind, prefix in (("flag", "FLAG_"), ("var", "VAR_"), ("trainer", "TRAINER_"))
    }
    lexical_counts = {
        "flags": sum(symbol.startswith("FLAG_") for symbol in references),
        "vars": sum(symbol.startswith("VAR_") for symbol in references),
        "trainers": sum(symbol.startswith("TRAINER_") for symbol in references),
    }

    source_definition_paths = {
        "flag": donor_path / "include/constants/flags.h",
        "var": donor_path / "include/constants/vars.h",
        "trainer": donor_path / "include/constants/opponents.h",
    }
    identities: dict[str, list[dict[str, Any]]] = {}
    for kind in ("flag", "var", "trainer"):
        symbols = selected[kind]
        if kind == "trainer" and "TRAINER_JOEY" in symbols:
            symbols = ["TRAINER_JOEY"] + [symbol for symbol in symbols if symbol != "TRAINER_JOEY"]
        entries: list[dict[str, Any]] = []
        for ordinal, symbol in enumerate(symbols):
            definition = _definition_for(symbol, definitions[kind], kind)
            definition["value"] = _resolve_literal(definition["expression"], definitions[kind])
            record_definition = None
            if kind == "trainer":
                record_definition = _definition_for(symbol, trainer_records, "trainer record")
            entries.append(_identity_entry(
                symbol, kind, ordinal, definition, references[symbol],
                _sha256(donor_path / definition_paths[kind]), record_definition
            ))
        identities[kind + "s"] = entries

    host_flags = _read_definitions(ROOT / "include/constants/flags.h", "FLAG_")
    host_vars = _read_definitions(ROOT / "include/constants/vars.h", "VAR_")
    for symbol in FLAG_ALIAS_SYMBOLS:
        _definition_for(symbol, host_flags, "host flag alias")
    for symbol in VAR_ALIAS_SYMBOLS:
        _definition_for(symbol, host_vars, "host var alias")

    aliases = {
        "flags": _alias_records(FLAG_ALIAS_SYMBOLS, "flag"),
        "vars": _alias_records(VAR_ALIAS_SYMBOLS, "var"),
    }
    generated_names = {
        entry["qualified"] for entries in identities.values() for entry in entries
    } | {
        alias["qualified"] for entries in aliases.values() for alias in entries
    }
    collisions = generated_names & _foundation_macros()
    collisions.discard("JOHTO_TRAINER_JOEY")
    if collisions:
        raise ContentSymbolError("generated names collide with foundation: " + ", ".join(sorted(collisions)))

    manifest_bytes = MANIFEST_PATH.read_bytes()
    return {
        "schema_version": 1,
        "ledger_version": 1,
        "authority": {
            "model": "explicit_append_only",
            "bootstrap": "--bootstrap",
            "append_update": "--append-update",
            "check": "--check",
            "saved_ordinals_are_authoritative": True,
            "deletion_renumbering_reclassification_alias_or_provenance_drift": "reject",
        },
        "completeness": {
            "scope": "selected-lexical-only",
            "selected_map_count": 239,
            "source_classes": ["map.json", "scripts.inc"],
            "pending_transitive_closure": PENDING_CLOSURE,
        },
        "provenance": {
            "repository": DONOR_REPOSITORY,
            "donor_revision": revision,
            "donor_tree": tree,
            "manifest_path": "data/johto/region_manifest.json",
            "manifest_sha256": _sha256_bytes(manifest_bytes),
            "manifest_source_revision": MANIFEST_SOURCE_REVISION,
            "selected_source_count": len(source_files),
        },
        "capacities": {"flags": FLAG_CAPACITY, "vars": VAR_CAPACITY, "trainers": TRAINER_CAPACITY},
        "lexical_counts": lexical_counts,
        "allocated_counts": {kind: len(entries) for kind, entries in identities.items()},
        "excluded_trainer_tokens": list(EXCLUDED_TRAINER_TOKENS),
        "aliases": aliases,
        "source_files": source_files,
        "identities": identities,
    }


def _canonical_json(value: Any) -> str:
    return json.dumps(value, indent=2, ensure_ascii=True, sort_keys=False) + "\n"


def _validate_entries(entries: Any, kind: str, capacity: int) -> None:
    if not isinstance(entries, list) or not entries:
        raise ContentSymbolError(f"ledger has no {kind} identities")
    expected_start = {"flag": FLAG_START, "var": VAR_START, "trainer": TRAINER_START}[kind]
    symbols: set[str] = set()
    for ordinal, entry in enumerate(entries):
        if not isinstance(entry, dict) or entry.get("ordinal") != ordinal:
            raise ContentSymbolError(f"{kind} ordinals are not contiguous")
        symbol = entry.get("symbol")
        if not isinstance(symbol, str) or symbol in symbols:
            raise ContentSymbolError(f"duplicate {kind} symbol: {symbol}")
        symbols.add(symbol)
        if entry.get("qualified") != "JOHTO_" + symbol:
            raise ContentSymbolError(f"qualified {kind} identity mismatch: {symbol}")
        if entry.get("runtime_id") != expected_start + ordinal or ordinal >= capacity:
            raise ContentSymbolError(f"{kind} runtime identity/capacity mismatch: {symbol}")


def validate_ledger(value: Any, *, allocated: bool = True) -> None:
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        raise ContentSymbolError("unsupported content ledger schema")
    if value.get("completeness", {}).get("scope") != "selected-lexical-only":
        raise ContentSymbolError("ledger must state selected-lexical-only scope")
    if value.get("completeness", {}).get("pending_transitive_closure") != PENDING_CLOSURE:
        raise ContentSymbolError("mandatory transitive closure metadata drifted")
    provenance = value.get("provenance", {})
    if (provenance.get("repository") != DONOR_REPOSITORY
            or provenance.get("donor_revision") != DONOR_REVISION
            or provenance.get("donor_tree") != DONOR_TREE
            or provenance.get("manifest_source_revision") != MANIFEST_SOURCE_REVISION):
        raise ContentSymbolError("pinned donor provenance drifted")
    if value.get("capacities") != {"flags": FLAG_CAPACITY, "vars": VAR_CAPACITY, "trainers": TRAINER_CAPACITY}:
        raise ContentSymbolError("content identity capacities are ABI")
    identities = value.get("identities", {})
    for kind, capacity in (("flag", FLAG_CAPACITY), ("var", VAR_CAPACITY), ("trainer", TRAINER_CAPACITY)):
        _validate_entries(identities.get(kind + "s"), kind, capacity)
    allocated_counts = value.get("allocated_counts")
    expected_counts = {kind: len(identities[kind]) for kind in ("flags", "vars", "trainers")}
    if allocated_counts != expected_counts:
        raise ContentSymbolError("allocated identity count drifted")
    aliases = value.get("aliases")
    if not isinstance(aliases, dict) or [x.get("symbol") for x in aliases.get("flags", [])] != list(FLAG_ALIAS_SYMBOLS) or [x.get("symbol") for x in aliases.get("vars", [])] != list(VAR_ALIAS_SYMBOLS):
        raise ContentSymbolError("transient alias set drifted")
    for kind in ("flags", "vars"):
        for alias in aliases[kind]:
            if alias.get("qualified") != "JOHTO_" + alias.get("symbol", "") or alias.get("target") != alias.get("symbol"):
                raise ContentSymbolError("transient alias target drifted")
            if alias.get("reason") != ALIAS_REASONS.get(alias.get("symbol")):
                raise ContentSymbolError("transient alias reason drifted")
    if value.get("excluded_trainer_tokens") != list(EXCLUDED_TRAINER_TOKENS):
        raise ContentSymbolError("excluded trainer token set drifted")
    joey = next((entry for entry in identities["trainers"] if entry["symbol"] == "TRAINER_JOEY"), None)
    if joey is None or joey["ordinal"] != 0 or joey["runtime_id"] != TRAINER_START:
        raise ContentSymbolError("Joey must remain trainer ordinal zero")
    if allocated:
        bindings = {
            kind: [[entry["symbol"], entry["ordinal"], entry["runtime_id"]]
                   for entry in identities[kind][:count]]
            for kind, count in INITIAL_COUNTS.items()
        }
        digest = _sha256_bytes(json.dumps(bindings, sort_keys=True, separators=(",", ":")).encode())
        if digest != INITIAL_BINDINGS_SHA256:
            raise ContentSymbolError("initial allocated identity bindings changed")


def append_ledger(existing: dict[str, Any], candidate: dict[str, Any]) -> dict[str, Any]:
    """Apply a source candidate while preserving every saved identity."""
    validate_ledger(existing)
    validate_ledger(candidate, allocated=False)
    for field in ("capacities", "aliases", "completeness", "provenance", "excluded_trainer_tokens"):
        if existing.get(field) != candidate.get(field):
            raise ContentSymbolError(f"append-only {field} provenance drift")
    result = json.loads(json.dumps(existing))
    for kind in ("flags", "vars", "trainers"):
        old_entries = existing["identities"][kind]
        new_entries = {entry["symbol"]: entry for entry in candidate["identities"][kind]}
        old_symbols = {entry["symbol"] for entry in old_entries}
        for old in old_entries:
            current = new_entries.get(old["symbol"])
            if current is None:
                raise ContentSymbolError(f"deletion or reclassification of {old['symbol']}")
            # Scanner ordinals describe lexical order, never saved identity.
            immutable = ("qualified", "definition", "record_definition")
            for field in immutable:
                if old.get(field) != current.get(field):
                    raise ContentSymbolError(f"append-only identity drift for {old['symbol']}: {field}")
            result["identities"][kind][old["ordinal"]]["references"] = current["references"]
        next_ordinal = len(old_entries)
        for entry in candidate["identities"][kind]:
            if entry["symbol"] in old_symbols:
                continue
            appended = json.loads(json.dumps(entry))
            appended["ordinal"] = next_ordinal
            start = {"flags": FLAG_START, "vars": VAR_START, "trainers": TRAINER_START}[kind]
            appended["runtime_id"] = start + next_ordinal
            result["identities"][kind].append(appended)
            next_ordinal += 1
        if next_ordinal > result["capacities"][kind]:
            raise ContentSymbolError(f"{kind} append exceeds capacity")
    result["allocated_counts"] = {kind: len(result["identities"][kind]) for kind in ("flags", "vars", "trainers")}
    result["lexical_counts"] = candidate.get("lexical_counts", result.get("lexical_counts"))
    result["source_files"] = candidate.get("source_files", result.get("source_files"))
    validate_ledger(result)
    return result


def render_header(ledger: dict[str, Any]) -> str:
    validate_ledger(ledger)
    lines = [
        "/* Generated by tools/johto/content_symbols.py. */",
        "/* Do not edit; update data/johto/content_symbols.json. */",
        "#ifndef GUARD_CONSTANTS_JOHTO_CONTENT_H",
        "#define GUARD_CONSTANTS_JOHTO_CONTENT_H",
        "",
        '#include "constants/flags.h"',
        '#include "constants/vars.h"',
        '#include "constants/johto_events.h"',
        '#include "constants/johto_trainers.h"',
        "",
        "#define JOHTO_CONTENT_LEDGER_VERSION 1u",
        f"#define JOHTO_CONTENT_FLAG_CAPACITY {FLAG_CAPACITY}u",
        f"#define JOHTO_CONTENT_VAR_CAPACITY {VAR_CAPACITY}u",
        f"#define JOHTO_CONTENT_TRAINER_CAPACITY {TRAINER_CAPACITY}u",
        f"#define JOHTO_CONTENT_FLAG_COUNT {len(ledger['identities']['flags'])}u",
        f"#define JOHTO_CONTENT_VAR_COUNT {len(ledger['identities']['vars'])}u",
        f"#define JOHTO_CONTENT_TRAINER_COUNT {len(ledger['identities']['trainers'])}u",
        "",
        "#if JOHTO_TRAINER_ORDINAL_JOEY != 0",
        '#error "Johto foundation trainer Joey ordinal changed"',
        "#endif",
        "#if JOHTO_TRAINER_JOEY != 0x4000",
        '#error "Johto foundation trainer Joey runtime ID changed"',
        "#endif",
        "",
    ]
    for alias in ledger["aliases"]["flags"] + ledger["aliases"]["vars"]:
        lines.append(f"#define {alias['qualified']} {alias['target']}")
    lines.append("")
    for entry in ledger["identities"]["flags"]:
        lines.append(f"#define {entry['qualified']} (JOHTO_FLAG_START + {entry['ordinal']}u)")
    lines.append("")
    for entry in ledger["identities"]["vars"]:
        lines.append(f"#define {entry['qualified']} (JOHTO_VAR_START + {entry['ordinal']}u)")
    lines.append("")
    for entry in ledger["identities"]["trainers"]:
        if entry["symbol"] == "TRAINER_JOEY":
            continue
        lines.append(f"#define {entry['qualified']} (JOHTO_TRAINER_ID_MIN + {entry['ordinal']}u)")
    lines.extend(["", "#endif /* GUARD_CONSTANTS_JOHTO_CONTENT_H */", ""])
    return "\n".join(lines)


def _write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8", newline="\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor", required=True, help="pinned donor checkout")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--bootstrap", action="store_true")
    mode.add_argument("--append-update", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        candidate = build_ledger(args.donor)
        if args.bootstrap:
            if OUTPUT_PATH.exists() or HEADER_PATH.exists():
                raise ContentSymbolError("bootstrap refuses to replace an existing ledger")
            validate_ledger(candidate)
            candidate_header = render_header(candidate)
            _write(OUTPUT_PATH, _canonical_json(candidate))
            _write(HEADER_PATH, candidate_header)
        elif args.append_update:
            if not OUTPUT_PATH.exists():
                raise ContentSymbolError("append-update requires a bootstrapped ledger")
            existing = _load_json(OUTPUT_PATH)
            updated = append_ledger(existing, candidate)
            _write(OUTPUT_PATH, _canonical_json(updated))
            _write(HEADER_PATH, render_header(updated))
        else:
            if not OUTPUT_PATH.exists() or not HEADER_PATH.exists():
                raise ContentSymbolError("checked-in ledger or generated header is missing")
            existing = _load_json(OUTPUT_PATH)
            expected = append_ledger(existing, candidate)
            if (_canonical_json(existing) != _canonical_json(expected)
                    or HEADER_PATH.read_text(encoding="utf-8") != render_header(expected)):
                raise ContentSymbolError("checked-in content ledger or header is stale")
        print(f"Johto content ledger flags={len(candidate['identities']['flags'])} vars={len(candidate['identities']['vars'])} trainers={len(candidate['identities']['trainers'])}")
        return 0
    except (OSError, ContentSymbolError) as error:
        print(f"Johto content ledger failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
