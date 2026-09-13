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

# Engine-owned persisted identities occupy the top of each reserved window.
# They are ledgered separately because the pinned donor does not reference
# them, while still receiving generated-header freshness and collision checks.
RUNTIME_ALLOCATIONS = {
    "flags": [{"symbol": "FLAG_KANTO_LATER_INITIALIZED", "qualified": "JOHTO_FLAG_KANTO_LATER_INITIALIZED", "ordinal": 767, "runtime_id": FLAG_START + 767, "purpose": "marks completion of the real Later Kanto initializer"}],
    "vars": [
        {"symbol": "VAR_PENDING_KANTO_DESTINATION", "qualified": "JOHTO_VAR_PENDING_KANTO_DESTINATION", "ordinal": 92, "runtime_id": VAR_START + 92, "purpose": "selected world for an in-progress cross-region transition"},
        {"symbol": "VAR_LAST_HEAL_JOHTO", "qualified": "JOHTO_VAR_LAST_HEAL_JOHTO", "ordinal": 93, "runtime_id": VAR_START + 93, "purpose": "last validated Johto heal location"},
        {"symbol": "VAR_LAST_HEAL_KANTO_ORIGINAL", "qualified": "JOHTO_VAR_LAST_HEAL_KANTO_ORIGINAL", "ordinal": 94, "runtime_id": VAR_START + 94, "purpose": "last validated original Kanto heal location"},
        {"symbol": "VAR_LAST_HEAL_KANTO_LATER", "qualified": "JOHTO_VAR_LAST_HEAL_KANTO_LATER", "ordinal": 95, "runtime_id": VAR_START + 95, "purpose": "last validated Later Kanto heal location"},
    ],
    "trainers": [],
}
RUNTIME_ALLOCATION_BINDINGS_SHA256 = "f88ba36dd2675a28930a0e241193f4d4e264613cab4db3c888ba65580f96c4d2"

# Frozen initial allocation, independent of future lexical discovery order.
INITIAL_COUNTS = {"flags": 539, "vars": 63, "trainers": 284}
INITIAL_BINDINGS_SHA256 = "7f570138eb10da1801d23ef12d950b38f9d65fcb80af533bd83ab147cd3f1dc4"
SEALED_COUNTS = {"flags": 614, "vars": 69, "trainers": 412}
SEALED_BINDINGS_SHA256 = "41d3bfc0ab8ea0e5986690f36236fcf6e6451bfcf3ac2eb0f147cfd031a33aff"
INITIAL_SELECTED_MAP_COUNT = 239
INITIAL_SELECTED_SOURCE_COUNT = INITIAL_SELECTED_MAP_COUNT * 2
# This is the manifest fingerprint recorded by the bootstrapped 239-map
# ledger.  It is deliberately separate from the current expanded manifest:
# changing the selected source set is accepted only through the explicit
# predecessor transition below.
INITIAL_MANIFEST_SHA256 = "af52f3596252f0ffd0fa14862cb13fafc6fc316a4baeeb5c2f29806b199e91f6"
EXPANDED_SELECTED_MAP_COUNT = 407
EXPANDED_LATER_MAP_COUNT = EXPANDED_SELECTED_MAP_COUNT - INITIAL_SELECTED_MAP_COUNT
RAW_MANIFEST_PROVENANCE_LEDGER_SHA256 = "2ab74da0f90bab1ff0fc7234368f93f0e88272c71342e0457f161608083a04ea"

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


def _json_identity(value: Any) -> str:
    return _sha256_bytes(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())


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


def _assert_donor_stable(
    donor: Path,
    revision: str,
    tree: str,
    source_hashes: dict[str, str],
) -> None:
    """Reauthenticate the donor after every selected input has been read."""
    final_revision, final_tree = _git_revision(donor)
    if ((final_revision, final_tree) != (revision, tree)
            or final_revision != DONOR_REVISION
            or final_tree != DONOR_TREE):
        raise ContentSymbolError("donor revision changed while selected inputs were read")
    _assert_donor_clean(donor)
    for relative, expected_hash in source_hashes.items():
        path = donor / relative
        if not path.is_file() or _sha256(path) != expected_hash:
            raise ContentSymbolError(
                f"donor source changed while selected inputs were read: {relative}"
            )


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
    selected_count = selection.get("selected_count")
    later_count = selection.get("later_selected_count")
    if (selected_count != EXPANDED_SELECTED_MAP_COUNT
            or selection.get("original_selected_count") != INITIAL_SELECTED_MAP_COUNT
            or later_count != EXPANDED_LATER_MAP_COUNT
            or not isinstance(maps, list)
            or len(maps) != EXPANDED_SELECTED_MAP_COUNT):
        raise ContentSymbolError(
            "manifest does not contain the approved 239+168 selected maps"
        )
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


SOURCE_FILE_FIELDS = ("map", "map_name", "kind", "path", "sha256")


def _normalized_source_inventory(source_files: Any) -> list[dict[str, Any]]:
    """Return the exact ordered source inventory in canonical field order."""
    if not isinstance(source_files, list):
        raise ContentSymbolError("selected source inventory must be a list")
    normalized: list[dict[str, Any]] = []
    for entry in source_files:
        if not isinstance(entry, dict) or set(entry) != set(SOURCE_FILE_FIELDS):
            raise ContentSymbolError("selected source inventory entry is malformed")
        if (not isinstance(entry["map"], str)
                or not isinstance(entry["map_name"], str)
                or entry["kind"] not in ("map_json", "script")
                or not isinstance(entry["path"], str)
                or not isinstance(entry["sha256"], str)
                or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])):
            raise ContentSymbolError("selected source inventory entry is malformed")
        normalized.append({field: entry[field] for field in SOURCE_FILE_FIELDS})
    return normalized


def _source_inventory_digest(source_files: Any) -> str:
    normalized = _normalized_source_inventory(source_files)
    return _sha256_bytes(json.dumps(
        normalized, sort_keys=True, separators=(",", ":")
    ).encode())


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


def _identity_membership(identities: dict[str, list[dict[str, Any]]]) -> dict[str, list[str]]:
    """Record the donor-backed identity names independently of saved ordinals."""
    return {
        kind: sorted(entry["symbol"] for entry in identities[kind])
        for kind in ("flags", "vars", "trainers")
    }


def _identity_semantics_digest(identities: dict[str, list[dict[str, Any]]]) -> str:
    """Attest each identity's donor definition, record, and source references."""
    semantics = {
        kind: {
            entry["symbol"]: {
                "qualified": entry.get("qualified"),
                "definition": entry.get("definition"),
                "record_definition": entry.get("record_definition"),
                "references": entry.get("references"),
            }
            for entry in sorted(identities[kind], key=lambda item: item["symbol"])
        }
        for kind in ("flags", "vars", "trainers")
    }
    return _sha256_bytes(json.dumps(
        semantics, sort_keys=True, separators=(",", ":")
    ).encode())


def _identity_semantic_record(entry: dict[str, Any]) -> dict[str, Any]:
    """Return the donor-derived fields that define an identity's meaning."""
    return {
        "qualified": entry.get("qualified"),
        "definition": entry.get("definition"),
        "record_definition": entry.get("record_definition"),
        "references": entry.get("references"),
    }


LEXICAL_COUNT_KEYS = ("flags", "vars", "trainers")


def _validate_lexical_counts(value: Any) -> None:
    if not isinstance(value, dict) or set(value) != set(LEXICAL_COUNT_KEYS):
        raise ContentSymbolError("lexical counts shape drifted")
    if any(
        isinstance(value[key], bool)
        or not isinstance(value[key], int)
        or value[key] < 0
        for key in LEXICAL_COUNT_KEYS
    ):
        raise ContentSymbolError("lexical counts values are malformed")


def _assert_donor_backed_candidate(candidate: dict[str, Any], donor: str | Path) -> None:
    """Check candidate identities against a fresh scan of the pinned donor.

    Candidate provenance is useful as an integrity attestation, but it is not
    an authority: a caller could recalculate it after adding an invented
    identity.  Rebuilding the selected source inventory and identity evidence
    from the pinned donor gives append_ledger an independent membership set.
    """
    authoritative = build_ledger(donor)
    if (candidate.get("completeness", {}).get("selected_map_count")
            != authoritative["completeness"]["selected_map_count"]):
        raise ContentSymbolError("candidate selected map scope is not donor-backed")
    if (_normalized_source_inventory(candidate.get("source_files"))
            != _normalized_source_inventory(authoritative["source_files"])):
        raise ContentSymbolError("candidate source inventory is not donor-backed")
    if candidate.get("lexical_counts") != authoritative.get("lexical_counts"):
        raise ContentSymbolError("candidate lexical counts are not donor-backed")
    for kind in ("flags", "vars", "trainers"):
        allowed = {
            entry["symbol"]: _identity_semantic_record(entry)
            for entry in authoritative["identities"][kind]
        }
        candidate_symbols = {entry["symbol"] for entry in candidate["identities"][kind]}
        allowed_symbols = set(allowed)
        if candidate_symbols != allowed_symbols:
            missing = sorted(allowed_symbols - candidate_symbols)
            extra = sorted(candidate_symbols - allowed_symbols)
            raise ContentSymbolError(
                f"{kind} identity membership is incomplete or invented: "
                f"missing={missing!r} extra={extra!r}"
            )
        for entry in candidate["identities"][kind]:
            expected = allowed.get(entry["symbol"])
            if expected is None:
                raise ContentSymbolError(
                    f"appended {kind} identity lacks pinned donor membership: {entry['symbol']}"
                )
            if _identity_semantic_record(entry) != expected:
                raise ContentSymbolError(
                    f"{kind} identity lacks pinned donor semantic evidence: {entry['symbol']}"
                )


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
    definition_hashes = {
        relative: _sha256(donor_path / relative)
        for relative in definition_paths.values()
    }
    definitions = {
        kind: _read_definitions(donor_path / relative, prefix, relative)
        for kind, (relative, prefix) in {
            "flag": (definition_paths["flag"], "FLAG_"),
            "var": (definition_paths["var"], "VAR_"),
            "trainer": (definition_paths["trainer"], "TRAINER_"),
        }.items()
    }
    trainer_record_path = "src/data/trainers.h"
    trainer_record_hash = _sha256(donor_path / trainer_record_path)
    trainer_records = _record_definitions(donor_path / trainer_record_path, trainer_record_path)
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
                definition_hashes[definition_paths[kind]], record_definition
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
    } | {
        entry["qualified"]
        for entries in RUNTIME_ALLOCATIONS.values()
        for entry in entries
    }
    collisions = generated_names & _foundation_macros()
    collisions.discard("JOHTO_TRAINER_JOEY")
    if collisions:
        raise ContentSymbolError("generated names collide with foundation: " + ", ".join(sorted(collisions)))

    manifest_identity = _json_identity(_load_json(MANIFEST_PATH))
    selected_map_count = len(maps)
    if selected_map_count != EXPANDED_SELECTED_MAP_COUNT:
        raise ContentSymbolError("only the approved expanded selected-map manifest may be emitted")
    ledger = {
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
            "selected_map_count": selected_map_count,
            "original_selected_map_count": INITIAL_SELECTED_MAP_COUNT,
            "later_selected_map_count": EXPANDED_LATER_MAP_COUNT,
            "source_classes": ["map.json", "scripts.inc"],
            "pending_transitive_closure": PENDING_CLOSURE,
        },
        "provenance": {
            "repository": DONOR_REPOSITORY,
            "donor_revision": revision,
            "donor_tree": tree,
            "manifest_path": "data/johto/region_manifest.json",
            "manifest_sha256": manifest_identity,
            "manifest_source_revision": MANIFEST_SOURCE_REVISION,
            "selected_source_count": len(source_files),
            "source_inventory_sha256": _source_inventory_digest(source_files),
            "identity_membership": _identity_membership(identities),
            "identity_semantics_sha256": _identity_semantics_digest(identities),
            "predecessor_transition": {
                "kind": "validated_append_only_manifest_expansion",
                "from": {
                    "selected_map_count": INITIAL_SELECTED_MAP_COUNT,
                    "selected_source_count": INITIAL_SELECTED_SOURCE_COUNT,
                    "manifest_sha256": INITIAL_MANIFEST_SHA256,
                    "initial_bindings_sha256": INITIAL_BINDINGS_SHA256,
                },
                "to": {
                    "selected_map_count": EXPANDED_SELECTED_MAP_COUNT,
                    "selected_source_count": EXPANDED_SELECTED_MAP_COUNT * 2,
                    "added_map_count": EXPANDED_LATER_MAP_COUNT,
                },
            },
        },
        "capacities": {"flags": FLAG_CAPACITY, "vars": VAR_CAPACITY, "trainers": TRAINER_CAPACITY},
        "lexical_counts": lexical_counts,
        "allocated_counts": {kind: len(entries) for kind, entries in identities.items()},
        "excluded_trainer_tokens": list(EXCLUDED_TRAINER_TOKENS),
        "aliases": aliases,
        "runtime_allocations": json.loads(json.dumps(RUNTIME_ALLOCATIONS)),
        "source_files": source_files,
        "identities": identities,
    }
    _assert_donor_stable(
        donor_path,
        revision,
        tree,
        {
            **source_hashes,
            **definition_hashes,
            trainer_record_path: trainer_record_hash,
        },
    )
    return ledger


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


def _validate_runtime_allocations(value: Any, identities: dict[str, Any]) -> None:
    if not isinstance(value, dict) or set(value) != {"flags", "vars", "trainers"}:
        raise ContentSymbolError("runtime allocation classes drifted")
    digest = _sha256_bytes(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())
    if digest != RUNTIME_ALLOCATION_BINDINGS_SHA256:
        raise ContentSymbolError("sealed runtime allocation bindings changed")
    starts = {"flags": FLAG_START, "vars": VAR_START, "trainers": TRAINER_START}
    capacities = {"flags": FLAG_CAPACITY, "vars": VAR_CAPACITY, "trainers": TRAINER_CAPACITY}
    for kind in ("flags", "vars", "trainers"):
        donor_ordinals = {entry["ordinal"] for entry in identities[kind]}
        donor_names = {entry["qualified"] for entry in identities[kind]}
        local_ordinals: set[int] = set()
        local_names: set[str] = set()
        for entry in value[kind]:
            ordinal = entry.get("ordinal")
            qualified = entry.get("qualified")
            if (not isinstance(ordinal, int) or ordinal < 0
                    or ordinal >= capacities[kind]
                    or entry.get("runtime_id") != starts[kind] + ordinal):
                raise ContentSymbolError(f"{kind} runtime allocation exceeds capacity")
            if ordinal in donor_ordinals or ordinal in local_ordinals:
                raise ContentSymbolError(f"{kind} runtime allocation ordinal collision")
            if qualified in donor_names or qualified in local_names:
                raise ContentSymbolError(f"{kind} runtime allocation name collision")
            local_ordinals.add(ordinal)
            local_names.add(qualified)


def _validate_scope_metadata(value: dict[str, Any]) -> None:
    """Validate the selected-source scope without treating it as an ID ledger.

    The first ledger was bootstrapped from 239 maps.  The approved 407-map
    candidate is a deliberate append-only expansion, so its changed manifest
    fingerprint is carried in an explicit predecessor transition rather than
    being silently accepted as ordinary provenance drift.
    """
    completeness = value.get("completeness", {})
    provenance = value.get("provenance", {})
    selected_map_count = completeness.get("selected_map_count")
    selected_source_count = provenance.get("selected_source_count")
    if selected_map_count not in (INITIAL_SELECTED_MAP_COUNT, EXPANDED_SELECTED_MAP_COUNT):
        raise ContentSymbolError("unsupported selected map scope")
    if selected_source_count != selected_map_count * 2:
        raise ContentSymbolError("selected source count does not match map scope")
    source_files = value.get("source_files")
    if not isinstance(source_files, list) or len(source_files) != selected_source_count:
        raise ContentSymbolError("selected source inventory does not match map scope")
    if provenance.get("source_inventory_sha256") != _source_inventory_digest(source_files):
        raise ContentSymbolError("selected source inventory attestation drifted")
    if selected_map_count == INITIAL_SELECTED_MAP_COUNT:
        if ("original_selected_map_count" in completeness
                or "later_selected_map_count" in completeness
                or "predecessor_transition" in provenance):
            raise ContentSymbolError("initial ledger cannot carry an expansion transition")
        if provenance.get("manifest_sha256") != INITIAL_MANIFEST_SHA256:
            raise ContentSymbolError("initial ledger manifest provenance drifted")
        return
    if (completeness.get("original_selected_map_count") != INITIAL_SELECTED_MAP_COUNT
            or completeness.get("later_selected_map_count") != EXPANDED_LATER_MAP_COUNT):
        raise ContentSymbolError("expanded ledger map transition metadata drifted")
    expected_transition = {
        "kind": "validated_append_only_manifest_expansion",
        "from": {
            "selected_map_count": INITIAL_SELECTED_MAP_COUNT,
            "selected_source_count": INITIAL_SELECTED_SOURCE_COUNT,
            "manifest_sha256": INITIAL_MANIFEST_SHA256,
            "initial_bindings_sha256": INITIAL_BINDINGS_SHA256,
        },
        "to": {
            "selected_map_count": EXPANDED_SELECTED_MAP_COUNT,
            "selected_source_count": EXPANDED_SELECTED_MAP_COUNT * 2,
            "added_map_count": EXPANDED_LATER_MAP_COUNT,
        },
    }
    if provenance.get("predecessor_transition") != expected_transition:
        raise ContentSymbolError("expanded ledger predecessor transition is not validated")


def validate_ledger(value: Any, *, allocated: bool = True) -> None:
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        raise ContentSymbolError("unsupported content ledger schema")
    if value.get("completeness", {}).get("scope") != "selected-lexical-only":
        raise ContentSymbolError("ledger must state selected-lexical-only scope")
    if value.get("completeness", {}).get("pending_transitive_closure") != PENDING_CLOSURE:
        raise ContentSymbolError("mandatory transitive closure metadata drifted")
    _validate_scope_metadata(value)
    _validate_lexical_counts(value.get("lexical_counts"))
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
    _validate_runtime_allocations(value.get("runtime_allocations"), identities)
    expected_membership = _identity_membership(identities)
    provenance_membership = value.get("provenance", {}).get("identity_membership")
    if provenance_membership != expected_membership:
        raise ContentSymbolError("donor-backed identity membership drifted")
    if value.get("provenance", {}).get("identity_semantics_sha256") != _identity_semantics_digest(identities):
        raise ContentSymbolError("donor-backed identity semantics drifted")
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
        if value["completeness"]["selected_map_count"] == EXPANDED_SELECTED_MAP_COUNT:
            sealed_bindings = {
                kind: [[entry["symbol"], entry["ordinal"], entry["runtime_id"]]
                       for entry in identities[kind][:count]]
                for kind, count in SEALED_COUNTS.items()
            }
            sealed_digest = _sha256_bytes(json.dumps(
                sealed_bindings, sort_keys=True, separators=(",", ":")
            ).encode())
            if sealed_digest != SEALED_BINDINGS_SHA256:
                raise ContentSymbolError("sealed allocated identity bindings changed")


def append_ledger(
    existing: dict[str, Any],
    candidate: dict[str, Any],
    donor: str | Path | None = None,
) -> dict[str, Any]:
    """Apply a source candidate while preserving every saved identity."""
    validate_ledger(existing)
    validate_ledger(candidate, allocated=False)
    raw_manifest_provenance_predecessor = (
        _json_identity(existing) == RAW_MANIFEST_PROVENANCE_LEDGER_SHA256
    )
    candidate_symbols = {
        kind: {entry["symbol"] for entry in candidate["identities"][kind]}
        for kind in ("flags", "vars", "trainers")
    }
    existing_symbols = {
        kind: {entry["symbol"] for entry in existing["identities"][kind]}
        for kind in ("flags", "vars", "trainers")
    }
    has_appended_identity = any(
        candidate_symbols[kind] - existing_symbols[kind]
        for kind in ("flags", "vars", "trainers")
    )
    same_scope = (
        existing.get("completeness", {}).get("selected_map_count")
        == candidate.get("completeness", {}).get("selected_map_count")
    )
    semantic_records_changed = (
        _identity_semantics_digest(existing["identities"])
        != _identity_semantics_digest(candidate["identities"])
    )
    semantic_attestation_changed = (
        existing.get("provenance", {}).get("identity_semantics_sha256")
        != candidate.get("provenance", {}).get("identity_semantics_sha256")
    )
    lexical_counts_changed = existing.get("lexical_counts") != candidate.get("lexical_counts")
    if donor is None and not same_scope:
        raise ContentSymbolError(
            "pinned donor authentication is required for selected-map scope transitions"
        )
    if donor is None and has_appended_identity:
        raise ContentSymbolError(
            "pinned donor is required to validate appended identity membership"
        )
    if donor is None and same_scope and (
        semantic_records_changed or semantic_attestation_changed or lexical_counts_changed
    ):
        raise ContentSymbolError(
            "pinned donor authentication is required for same-scope semantic or lexical changes"
        )
    if donor is not None:
        _assert_donor_backed_candidate(candidate, donor)
    for field in ("capacities", "aliases", "excluded_trainer_tokens", "runtime_allocations"):
        if existing.get(field) != candidate.get(field):
            raise ContentSymbolError(f"append-only {field} provenance drift")
    for field in ("repository", "donor_revision", "donor_tree", "manifest_path",
                  "manifest_source_revision"):
        if existing.get("provenance", {}).get(field) != candidate.get("provenance", {}).get(field):
            raise ContentSymbolError(f"append-only provenance drift: {field}")
    existing_scope = existing["completeness"]["selected_map_count"]
    candidate_scope = candidate["completeness"]["selected_map_count"]
    if existing_scope == candidate_scope:
        if (_normalized_source_inventory(existing.get("source_files"))
                != _normalized_source_inventory(candidate.get("source_files"))):
            raise ContentSymbolError("append-only source inventory drift")
        if existing.get("completeness") != candidate.get("completeness"):
            raise ContentSymbolError("append-only completeness provenance drift")
        dynamic_attestations = {"identity_membership", "identity_semantics_sha256"}
        if raw_manifest_provenance_predecessor:
            dynamic_attestations.add("manifest_sha256")
        existing_provenance = {
            key: value for key, value in existing.get("provenance", {}).items()
            if key not in dynamic_attestations
        }
        candidate_provenance = {
            key: value for key, value in candidate.get("provenance", {}).items()
            if key not in dynamic_attestations
        }
        if existing_provenance != candidate_provenance:
            raise ContentSymbolError("append-only provenance drift")
    elif (existing_scope, candidate_scope) == (
            INITIAL_SELECTED_MAP_COUNT, EXPANDED_SELECTED_MAP_COUNT):
        transition = candidate["provenance"]["predecessor_transition"]
        expected_from = transition["from"]
        if (existing["provenance"].get("manifest_sha256") != expected_from["manifest_sha256"]
                or existing["provenance"].get("selected_source_count")
                != expected_from["selected_source_count"]):
            raise ContentSymbolError("append-only predecessor manifest does not match")
        if existing.get("completeness", {}).get("selected_map_count") != expected_from["selected_map_count"]:
            raise ContentSymbolError("append-only predecessor map scope does not match")
        if _json_identity(_load_json(MANIFEST_PATH)) != candidate["provenance"].get("manifest_sha256"):
            raise ContentSymbolError("expanded manifest provenance does not match selected source")
        old_files = existing.get("source_files", [])
        if (_normalized_source_inventory(candidate.get("source_files", []))[:len(old_files)]
                != _normalized_source_inventory(old_files)):
            raise ContentSymbolError("expanded source selection changed the predecessor prefix")
    else:
        raise ContentSymbolError("unsupported content ledger scope transition")
    result = json.loads(json.dumps(existing))
    if existing_scope != candidate_scope:
        # The identity prefix remains owned by the persisted predecessor, but
        # the selected-source metadata advances to the validated candidate.
        result["completeness"] = json.loads(json.dumps(candidate["completeness"]))
        result["provenance"] = json.loads(json.dumps(candidate["provenance"]))
    else:
        # New donor-backed identities may be discovered within an unchanged
        # source scope; refresh only the attestations that cover that set.
        result["provenance"]["identity_membership"] = json.loads(json.dumps(
            candidate["provenance"]["identity_membership"]
        ))
        result["provenance"]["identity_semantics_sha256"] = candidate["provenance"][
            "identity_semantics_sha256"
        ]
        if raw_manifest_provenance_predecessor:
            result["provenance"]["manifest_sha256"] = candidate["provenance"]["manifest_sha256"]
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
            if entry["symbol"] not in candidate["provenance"]["identity_membership"][kind]:
                raise ContentSymbolError(
                    f"appended {kind} identity lacks donor-backed membership: {entry['symbol']}"
                )
            appended = json.loads(json.dumps(entry))
            appended["ordinal"] = next_ordinal
            start = {"flags": FLAG_START, "vars": VAR_START, "trainers": TRAINER_START}[kind]
            appended["runtime_id"] = start + next_ordinal
            result["identities"][kind].append(appended)
            next_ordinal += 1
        local_ordinals = [entry["ordinal"] for entry in result["runtime_allocations"][kind]]
        donor_capacity = min(local_ordinals, default=result["capacities"][kind])
        if next_ordinal > donor_capacity:
            raise ContentSymbolError(f"{kind} append exceeds capacity")
    result["allocated_counts"] = {kind: len(result["identities"][kind]) for kind in ("flags", "vars", "trainers")}
    result["lexical_counts"] = candidate.get("lexical_counts", result.get("lexical_counts"))
    result["source_files"] = candidate.get("source_files", result.get("source_files"))
    validate_ledger(result)
    return result


def render_header(ledger: dict[str, Any]) -> str:
    # build_ledger returns a scanner-ordered candidate; append_ledger is the
    # operation that assigns saved ordinals before a checked-in header is
    # rendered.
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
    starts = {"flags": "JOHTO_FLAG_START", "vars": "JOHTO_VAR_START",
              "trainers": "JOHTO_TRAINER_ID_MIN"}
    for kind in ("flags", "vars", "trainers"):
        for entry in ledger["runtime_allocations"][kind]:
            lines.append(
                f"#define {entry['qualified']} ({starts[kind]} + {entry['ordinal']}u)"
            )
    lines.append("")
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
            updated = append_ledger(existing, candidate, args.donor)
            _write(OUTPUT_PATH, _canonical_json(updated))
            _write(HEADER_PATH, render_header(updated))
        else:
            if not OUTPUT_PATH.exists() or not HEADER_PATH.exists():
                raise ContentSymbolError("checked-in ledger or generated header is missing")
            existing = _load_json(OUTPUT_PATH)
            expected = append_ledger(existing, candidate, args.donor)
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
