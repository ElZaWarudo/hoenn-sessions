"""Inventory the pinned Cormoria campaign without changing live game sources.

The script graph is closed at label granularity, including assembly fallthrough.
Native functions are semantic adapter boundaries, not permission to copy a donor
engine. Their definitions and source assets remain evidence for the owning unit.
All proposed identities are logical allocations: U2/U3 must implement routing.
"""
from __future__ import annotations

import argparse
import ast
import hashlib
import json
import re
import subprocess
import sys
from collections import defaultdict, deque
from pathlib import Path
from typing import Any

from tools.cormoria import content_closure

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "data/cormoria"
DONOR_REVISION = "f7997186345885bfa23a170e5f573851fc034b9b"
DONOR_REPOSITORY = "https://github.com/dsmyst/dreamstone-mysteries"
HOST_REVISION = "ea4bbc060b74b6418ac3fcaa0008cd62b1f83dce"
GROUP_SIZES = (28, 32, 40, 31, 30, 4)
GROUP_NAMES = ("gMapGroup_MyMaps", "gMapGroup_Phase2", "gMapGroup_Phase3",
               "gMapGroup_Phase4", "gMapGroup_Phase5", "gMapGroup_Phase6")
RANGES = {"flags": (0x8000, 0x1000), "vars": (0x9100, 0x100),
          "trainers": (0x5000, 0x1000), "heals": (0x300, 0x100)}
TOKEN = re.compile(r"\b[A-Za-z_][A-Za-z_0-9]*\b")
LABEL = re.compile(r"^\s*([A-Za-z_][\w.]*)::?\s*(?:@.*)?$")
STRING = re.compile(r'"(?:\\.|[^"\\])*"')
INCBIN = re.compile(r'INCBIN_[A-Z0-9]+\s*\(([^)]*)\)|\.incbin\s+("[^"]+")')


class ManifestError(ValueError):
    """An owned dependency or allocation could not be proven."""


def git(root: Path, *arguments: str) -> str:
    result = subprocess.run(["git", "-C", str(root), *arguments], capture_output=True,
                            text=True, check=False)
    if result.returncode:
        raise ManifestError(f"git {' '.join(arguments)}: {result.stderr.strip()}")
    return result.stdout.strip()


def verify_donor(donor: Path) -> dict[str, str]:
    revision = git(donor, "rev-parse", "HEAD")
    if revision != DONOR_REVISION:
        raise ManifestError(f"donor revision mismatch: expected {DONOR_REVISION}, got {revision}")
    dirty = git(donor, "status", "--porcelain", "--untracked-files=all")
    if dirty:
        raise ManifestError(f"dirty donor: {dirty[:300]}")
    return {"repository": DONOR_REPOSITORY, "revision": revision,
            "tree": git(donor, "rev-parse", "HEAD^{tree}")}


def read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        raise ManifestError(f"missing source: {relative}")
    return path.read_text(encoding="utf-8-sig")


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: Any) -> str:
    return json.dumps(value, indent=2, ensure_ascii=False) + "\n"


def allocate(names: list[str], start: int, capacity: int, occupied: set[int],
             owner: str) -> dict[str, int]:
    if len(names) != len(set(names)):
        raise ManifestError(f"{owner}: duplicate symbol")
    if len(names) > capacity or start + len(names) > 0x10000:
        raise ManifestError(f"{owner}: capacity exhausted ({len(names)} > {capacity})")
    result = {name: start + index for index, name in enumerate(sorted(names))}
    collision = set(result.values()) & occupied
    if collision:
        raise ManifestError(f"{owner}: identity collision at {min(collision):#x}")
    return result


def resolve_asset(root: Path, relative: str, owner: str) -> tuple[str, str]:
    """Resolve only documented donor make conversions, never a guessed substitute."""
    path = root / relative
    if not path.resolve().is_relative_to(root.resolve()):
        raise ManifestError(f"{owner}: asset outside donor: {relative}")
    if path.is_file():
        return relative, "identity"
    base = relative.removesuffix(".lz")
    compression = "->lz" if relative.endswith(".lz") else ""
    for suffix, source, conversion in ((".4bpp", ".png", "png->4bpp"),
                                       (".8bpp", ".png", "png->8bpp"),
                                       (".1bpp", ".png", "png->1bpp"),
                                       (".gbapal", ".pal", "pal->gbapal"),
                                       (".gbapal", ".png", "png-palette->gbapal")):
        if base.endswith(suffix):
            candidate = base[:-len(suffix)] + source
            if (root / candidate).is_file():
                return candidate, conversion + compression
    if compression and (root / base).is_file():
        return base, "lz"
    if relative.startswith("sound/") and relative.endswith(".bin"):
        candidate = relative[:-4] + ".aif"
        if (root / candidate).is_file():
            return candidate, "aif->compressed-cry" if "/cries/" in relative and "/uncomp_" not in relative else "aif->pcm"
    raise ManifestError(f"{owner}: missing asset {relative}")


def definitions(root: Path) -> tuple[dict[str, str], dict[str, str]]:
    expressions: dict[str, str] = {}
    owners: dict[str, str] = {}
    for folder in ("include", "constants"):
        for path in sorted((root / folder).rglob("*.h")):
            relative = path.relative_to(root).as_posix()
            for number, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines(), 1):
                match = re.match(r"\s*#define\s+([A-Za-z_]\w*)(?:\([^)]*\))?(\s+.*|$)", line)
                if match:
                    name, value = match.groups()
                    expressions.setdefault(name, value.split("//")[0].strip())
                    owners.setdefault(name, f"{relative}:{number}")
    for folder in ("constants", "asm/macros"):
        for path in sorted((root / folder).rglob("*.inc")):
            relative = path.relative_to(root).as_posix()
            for number, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines(), 1):
                match = re.match(r"\s*(?:\.set\s+)?([A-Z_][A-Z_0-9]*)\s*(?:=|,)\s*([^@]+)", line)
                if match:
                    name, value = match.groups()
                    expressions.setdefault(name, value.strip())
                    owners.setdefault(name, f"{relative}:{number}")
    charmap = root / "charmap.txt"
    if charmap.is_file():
        for number, line in enumerate(charmap.read_text(encoding="utf-8-sig").splitlines(), 1):
            match = re.match(r"([A-Z_][A-Z_0-9]*)\s*=\s*(.*)", line)
            if match:
                expressions.setdefault(match.group(1), "charmap:" + match.group(2))
                owners.setdefault(match.group(1), f"charmap.txt:{number}")
    tm_path = root / "include/constants/tms_hms.h"
    if tm_path.is_file():
        tm_text = tm_path.read_text(encoding="utf-8-sig")
        for kind in ("TM", "HM"):
            match = re.search(rf"#define FOREACH_{kind}\(F\)(.*?)(?=\n\s*\n|\n#|\Z)", tm_text, re.S)
            if match:
                for index, move in enumerate(re.findall(r"F\((\w+)\)", match.group(1))):
                    symbol = f"ITEM_{kind}_{move}"
                    expressions[symbol] = f"ITEM_{kind}01 + {index}"
                    owners[symbol] = "include/constants/tms_hms.h:1"
    return expressions, owners


def numeric_constants(expressions: dict[str, str]) -> dict[str, int]:
    """Evaluate integer constant arithmetic without eval or executing donor code."""
    values: dict[str, int] = {}
    operators = {ast.Add: lambda a, b: a + b, ast.Sub: lambda a, b: a - b,
                 ast.Mult: lambda a, b: a * b, ast.FloorDiv: lambda a, b: a // b,
                 ast.Div: lambda a, b: a // b, ast.Mod: lambda a, b: a % b,
                 ast.LShift: lambda a, b: a << b, ast.RShift: lambda a, b: a >> b,
                 ast.BitOr: lambda a, b: a | b, ast.BitAnd: lambda a, b: a & b}

    def visit(node: ast.AST) -> int:
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.Name):
            return values[node.id]
        if isinstance(node, ast.BinOp) and type(node.op) in operators:
            return operators[type(node.op)](visit(node.left), visit(node.right))
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.USub, ast.UAdd, ast.Invert)):
            value = visit(node.operand)
            return -value if isinstance(node.op, ast.USub) else ~value if isinstance(node.op, ast.Invert) else value
        raise ValueError("not integer arithmetic")

    pending = dict(expressions)
    while pending:
        progress = False
        for name, value in list(pending.items()):
            try:
                cleaned = re.sub(r"(?<=\d)[uUlL]+\b", "", value)
                values[name] = visit(ast.parse(cleaned, mode="eval").body)
            except (ValueError, SyntaxError, KeyError, ZeroDivisionError, OverflowError):
                continue
            del pending[name]
            progress = True
        if not progress:
            break
    return values


def c_functions(root: Path) -> dict[str, list[dict[str, Any]]]:
    result: dict[str, list[dict[str, Any]]] = defaultdict(list)
    pattern = re.compile(r"(?m)^[A-Za-z_][\w \t*]*?\b([A-Za-z_]\w*)[ \t]*\([^;{}]*?\)\s*\{")
    for path in sorted((root / "src").rglob("*.c")):
        text = path.read_text(encoding="utf-8-sig")
        code = re.sub(r'/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'',
                      lambda match: re.sub(r"[^\n]", " ", match.group()), text, flags=re.S)
        for match in pattern.finditer(code):
            name = match.group(1)
            if name in {"if", "while", "switch"}:
                continue
            depth, end = 1, match.end()
            while end < len(code) and depth:
                depth += (code[end] == "{") - (code[end] == "}")
                end += 1
            if depth:
                raise ManifestError(f"{path.relative_to(root)}: unterminated function {name}")
            result[name].append({"path": path.relative_to(root).as_posix(),
                                 "line": text.count("\n", 0, match.start()) + 1,
                                 "_body": code[match.end():end - 1]})
    return dict(result)


def native_module_references(root: Path, calls: list[dict[str, str]],
                             functions: dict[str, list[dict[str, Any]]]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """Follow a native entry's same-module helpers and callback tables for audio.

    Cross-module engine calls remain the explicit semantic adapter boundary.
    This never pulls every function or every asset of the donor engine.
    """
    records_by_path: dict[str, dict[str, str]] = {}
    queue = deque((call["symbol"], definition["path"])
                  for call in calls for definition in functions[call["symbol"]])
    visited = set()
    references = []
    evidence = []
    while queue:
        name, path = queue.popleft()
        if (name, path) in visited:
            continue
        visited.add((name, path))
        if path not in records_by_path:
            records_by_path[path] = record_blocks(read(root, path))
        records = records_by_path[path]
        definitions_for_name = [definition for definition in functions.get(name, []) if definition["path"] == path]
        bodies = [(definition["_body"], f"{path}:{definition['line']}") for definition in definitions_for_name]
        if name in records:
            bodies.append((records[name], f"{path}:{name}"))
        for body, owner in bodies:
            body = re.sub(r"/\*.*?\*/|//[^\n]*", "", STRING.sub("", body), flags=re.S)
            tokens = set(TOKEN.findall(body))
            references.extend({"symbol": token, "kind": "constant", "owner": owner}
                              for token in sorted(tokens) if token.startswith(("MUS_", "SE_")))
            for token in sorted(tokens):
                if token in records or any(definition["path"] == path for definition in functions.get(token, [])):
                    queue.append((token, path))
        evidence.append({"symbol": name, "path": path, "kind": "function" if definitions_for_name else "callback_or_data_record"})
    return references, sorted(evidence, key=lambda item: (item["path"], item["symbol"]))


class ScriptGraph:
    """Exact assembly label closure with owner-bearing references."""

    def __init__(self, root: Path, constants: dict[str, str],
                 natives: dict[str, list[dict[str, Any]]] | None = None):
        self.root = root
        self.constants = constants
        self.natives = natives if natives is not None else c_functions(root)
        self.blocks: dict[str, dict[str, Any]] = {}
        self.duplicates: dict[str, list[str]] = defaultdict(list)
        self.files: dict[str, list[str]] = defaultdict(list)
        self.selected: dict[str, dict[str, Any]] = {}
        self.references: list[dict[str, Any]] = []
        self.native_calls: list[dict[str, str]] = []
        self.commands: dict[str, list[str]] = defaultdict(list)
        self.numeric_references: list[dict[str, Any]] = []
        self.local_constants: dict[str, dict[str, str]] = defaultdict(dict)
        self.includes: set[str] = set()
        paths = sorted(set((root / "data").rglob("*.inc")) | set((root / "data").rglob("*.s")))
        for path in paths:
            relative = path.relative_to(root).as_posix()
            previous = None
            current = None
            for number, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines(), 1):
                local = re.match(r"\s*\.set\s+(\w+),\s*(.*)", line)
                if local:
                    self.local_constants[relative][local.group(1)] = local.group(2).split("@")[0].strip()
                match = LABEL.match(line)
                if match:
                    name = match.group(1)
                    if name in self.blocks:
                        self.duplicates[name].append(f"{relative}:{number}")
                    current = {"owner": f"{relative}:{number}", "path": relative,
                               "line": number, "lines": [], "next": None}
                    self.blocks[name] = current
                    self.files[relative].append(name)
                    if previous:
                        previous["next"] = name
                    previous = current
                elif current is not None:
                    current["lines"].append((number, line))

    def close(self, files: list[str], labels: list[str] | None = None) -> None:
        queue = deque(labels or [])
        for path in files:
            read(self.root, path)
            queue.extend(self.files[path])
            self.includes.add(path)
        while queue:
            name = queue.popleft()
            if name in self.selected:
                continue
            if name not in self.blocks:
                raise ManifestError(f"map event: missing label {name}")
            if name in self.duplicates:
                raise ManifestError(f"duplicate reachable label {name}: {self.duplicates[name]}")
            block = self.blocks[name]
            self.selected[name] = block
            self.includes.add(block["path"])
            last = ""
            for number, raw in block["lines"]:
                line = raw.split("@", 1)[0].strip()
                if not line or line.startswith(("#", "//")):
                    continue
                include = re.match(r'\.include\s+"([^"]+)"', line)
                if include:
                    included = include.group(1)
                    read(self.root, included)
                    self.includes.add(included)
                    queue.extend(self.files[included])
                    continue
                parts = line.split(None, 1)
                command = parts[0].lstrip(".")
                operand_text = parts[1] if len(parts) > 1 else ""
                token_line = re.sub(r"\b[A-Za-z_]\w*\s*=", "", STRING.sub("", operand_text))
                tokens = [command] + TOKEN.findall(token_line)
                if command not in {"align", "balign", "set", "global", "type", "size"}:
                    last = command
                owner = f"{block['path']}:{number}"
                self.commands[command].append(owner)
                operands = line.split(None, 1)[1] if len(line.split(None, 1)) > 1 else ""
                args = [arg.strip() for arg in operands.split(",")]
                numeric_kind = None
                positions = [0]
                if command in {"setflag", "clearflag", "checkflag", "goto_if_set", "goto_if_unset", "call_if_set", "call_if_unset"}:
                    numeric_kind = "FLAG_"
                elif command in {"setvar", "addvar", "subvar", "compare", "copyvar", "copyvarifnotzero", "map_script_2"}:
                    numeric_kind = "VAR_"
                    if command == "copyvar":
                        positions = [0, 1]
                elif command.startswith("trainerbattle"):
                    numeric_kind = "TRAINER_"
                    positions = [1] if command == "trainerbattle" else [0]
                elif command in {"setrespawn", "setheallocation"}:
                    numeric_kind = "HEAL_LOCATION_"
                if numeric_kind:
                    for position in positions:
                        if position < len(args) and re.fullmatch(r"(?:0x[0-9A-Fa-f]+|\d+)", args[position]):
                            self.numeric_references.append({"prefix": numeric_kind, "value": int(args[position], 0), "owner": owner})
                if command in {"special", "specialvar", "callnative", "gotonative"}:
                    native = args[1] if command == "specialvar" else args[0]
                    if native not in self.natives:
                        raise ManifestError(f"{owner}: missing native {native}")
                    self.native_calls.append({"symbol": native, "command": command, "owner": owner})
                    # specialvar's first operand remains a normal variable reference.
                    tokens = [token for token in tokens if token != native]
                if command.startswith(("warp", "setwarp", "setdynamicwarp", "setescapewarp", "setdivewarp", "setholewarp")):
                    first = operands.split(",")[0].strip()
                    if re.fullmatch(r"(?:0x[0-9a-fA-F]+|\d+)", first):
                        raise ManifestError(f"{owner}: unallocated numeric map destination {first}")
                for token in tokens[1:]:
                    if token in self.blocks:
                        queue.append(token)
                        kind = "label"
                    elif token in self.constants or token.isupper() or token.startswith("MAP_"):
                        kind = "constant"
                    elif token in {"byte", "hword", "word", "string", "align", "incbin", "null"}:
                        continue
                    else:
                        raise ManifestError(f"{owner}: missing label or constant {token}")
                    self.references.append({"symbol": token, "kind": kind, "owner": owner})
            # Text/arrays end at their label; event control falls through unless terminated.
            if block["next"] and last not in {"end", "endram", "return", "goto", "gotonative", "returnram", "string", "byte", "2byte", "4byte", "incbin", "step_end"}:
                queue.append(block["next"])


class Sources:
    def __init__(self, root: Path):
        self.root = root
        self.owners: dict[str, set[str]] = defaultdict(set)
        self.assets: dict[str, dict[str, Any]] = {}

    def add(self, path: str, owner: str) -> None:
        if not (self.root / path).is_file():
            raise ManifestError(f"{owner}: missing source {path}")
        self.owners[path].add(owner)

    def asset(self, path: str, owner: str) -> None:
        if path in self.assets:
            item = self.assets[path]
            if owner not in item["owners"]:
                item["owners"].append(owner)
            if "source" in item:
                self.owners[item["source"]].add(owner)
            else:
                for dependency in item["sources"]:
                    self.asset(dependency, owner)
            return
        try:
            source, conversion = resolve_asset(self.root, path, owner)
        except ManifestError:
            # Explicit concatenation recipes in the pinned donor makefile.
            composites = {
                "graphics/roulette/roulette_tilt.4bpp": ["shroomish", "tailow"],
                "graphics/roulette/wheel_icons.4bpp": ["wynaut", "azurill", "skitty", "makuhita"],
            }
            base = path.removesuffix(".lz")
            if base not in composites:
                raise
            dependencies = [f"graphics/roulette/{name}.4bpp" for name in composites[base]]
            for dependency in dependencies:
                self.asset(dependency, owner)
            self.add("graphics_file_rules.mk", path)
            self.assets[path] = {"requested": path, "sources": dependencies,
                                 "conversion": "concatenate->lz" if path.endswith(".lz") else "concatenate",
                                 "rule": "graphics_file_rules.mk", "owners": [owner]}
            return
        self.add(source, owner)
        item = self.assets.setdefault(path, {"requested": path, "source": source,
                                          "conversion": conversion, "owners": []})
        if owner not in item["owners"]:
            item["owners"].append(owner)

    def scan_assets(self, path: str, owner: str, text: str | None = None) -> None:
        self.add(path, owner)
        for match in INCBIN.finditer(text if text is not None else read(self.root, path)):
            for asset in re.findall(r'"([^"]+)"', match.group(1) or match.group(2)):
                self.asset(asset, owner)

    def manifest(self, provenance: dict[str, str]) -> dict[str, Any]:
        return {"schema_version": 1, "provenance": provenance,
                "files": [{"path": path, "bytes": (self.root / path).stat().st_size,
                           "sha256": sha((self.root / path).read_bytes()), "owners": sorted(owners)}
                          for path, owners in sorted(self.owners.items())],
                "assets": [dict(item, owners=sorted(item["owners"]))
                           for _, item in sorted(self.assets.items())]}


def record_blocks(text: str) -> dict[str, str]:
    """Top-level C initializer records used by tilesets and graphics tables."""
    result = {}
    pattern = re.compile(r"(?m)^[ \t]*(?:(?:static|const|struct)[ \t]+)*[A-Za-z_][\w *]*?\b([gs][A-Z]\w*)\s*(?:\[[^;]*?\])?\s*=\s*")
    for match in pattern.finditer(text):
        end = text.find(";", match.end())
        if end >= 0:
            result[match.group(1)] = text[match.start():end + 1]
    return result


def macro_closure(donor: Path, graph: ScriptGraph, sources: Sources) -> list[dict[str, Any]]:
    definitions_by_name = {}
    for path in sorted((donor / "asm/macros").rglob("*.inc")):
        text = path.read_text(encoding="utf-8-sig")
        for match in re.finditer(r"(?m)^\s*\.macro\s+(\w+)([^\n]*)\n(.*?)^\s*\.endm", text, re.S | re.M):
            name, arguments, body = match.groups()
            definitions_by_name[name] = (path.relative_to(donor).as_posix(), arguments, body,
                                        text.count("\n", 0, match.start()) + 1)
    queue = deque(graph.commands)
    visited = set()
    result = []
    while queue:
        name = queue.popleft()
        if name in visited or name not in definitions_by_name:
            continue
        visited.add(name)
        path, arguments, body, line = definitions_by_name[name]
        owner = f"{path}:{line}"
        sources.add(path, f"macro {name}")
        opcodes = re.findall(r"(?m)^\s*\.byte\s+(0x[\dA-Fa-f]+)\b", body)
        result.append({"source_symbol": name, "target_symbol": "Cormoria_" + name,
                       "definition": owner, "arguments": arguments.strip(), "opcodes": opcodes,
                       "body_sha256": sha(body.encode()),
                       "status": "requires_semantic_command_translation",
                       "owner_unit": "U5" if "quest" in name else "U8"})
        for raw in body.splitlines():
            stripped = raw.split("@", 1)[0].strip()
            tokens = TOKEN.findall(stripped)
            if not tokens:
                continue
            if tokens[0] in definitions_by_name:
                queue.append(tokens[0])
            match = re.match(r"(?:callnative|gotonative|special)\s+([A-Za-z_]\w*)|specialvar\s+[^,]+,\s*([A-Za-z_]\w*)", stripped)
            if match:
                native = match.group(1) or match.group(2)
                if native not in graph.natives:
                    raise ManifestError(f"{owner}: missing macro native {native}")
                graph.native_calls.append({"symbol": native, "command": f"macro:{name}", "owner": owner})
            for token in tokens[1:]:
                if token in graph.constants:
                    graph.references.append({"symbol": token, "kind": "constant", "owner": owner})
    return sorted(result, key=lambda item: item["source_symbol"])


def build_bundle(donor: Path, host: Path = ROOT) -> dict[str, Any]:
    provenance = verify_donor(donor)
    sources = Sources(donor)
    for path in ("CREDITS.md", "README.md", "Makefile", "graphics_file_rules.mk", "audio_rules.mk",
                 "map_data_rules.mk", "charmap.txt", "data/maps/map_groups.json", "data/layouts/layouts.json"):
        sources.add(path, "provenance/build contract")
    groups = json.loads(read(donor, "data/maps/map_groups.json"))
    if tuple(groups["group_order"][:6]) != GROUP_NAMES:
        raise ManifestError("campaign group order differs from pinned contract")
    if tuple(len(groups[name]) for name in GROUP_NAMES) != GROUP_SIZES:
        raise ManifestError("campaign group capacity/count mismatch")
    names = [name for group in GROUP_NAMES for name in groups[group]]
    if len(set(names)) != len(names):
        raise ManifestError("duplicate map registration")
    host_groups = json.loads(read(host, "data/maps/map_groups.json"))
    baseline_groups = json.loads(git(host, "show", f"{HOST_REVISION}:data/maps/map_groups.json"))
    expected_tail = [f"gMapGroup_Cormoria_Phase{index + 1}" for index in range(6)]
    if (len(baseline_groups["group_order"]) != 79
            or sum(len(baseline_groups[g]) for g in baseline_groups["group_order"]) != 1344
            or host_groups["group_order"][:79] != baseline_groups["group_order"]
            or any(host_groups[group] != baseline_groups[group] for group in baseline_groups["group_order"])):
        raise ManifestError("host append-only map baseline changed (expected preserved 79 groups/1344 maps)")
    tail = host_groups["group_order"][79:]
    if tail and (tail != expected_tail or any(host_groups[target] != ["Cormoria_" + name for name in groups[source]]
                                             for target, source in zip(expected_tail, GROUP_NAMES))):
        raise ManifestError("host Cormoria group tail differs from exact allocation")
    host_names = {name for group in baseline_groups["group_order"] for name in baseline_groups[group]}
    all_layouts = json.loads(read(donor, "data/layouts/layouts.json"))["layouts"]
    layout_index = {entry["id"]: entry for entry in all_layouts}
    if len(layout_index) != len(all_layouts):
        raise ManifestError("duplicate layout identity")
    constants, constant_owners = definitions(donor)
    values = numeric_constants(constants)
    host_expressions, _ = definitions(host)
    host_values = numeric_constants(host_expressions)
    natives = c_functions(donor)
    graph = ScriptGraph(donor, constants, natives)
    maps, layouts, sections, tilesets, map_documents = [], {}, {}, set(), {}
    roots, event_labels = [], []
    all_map_references = []
    for group_index, group in enumerate(GROUP_NAMES):
        for index, name in enumerate(groups[group]):
            owner = f"data/maps/{name}/map.json"
            sources.add(owner, name)
            data = json.loads(read(donor, owner))
            map_documents[name] = data
            if f"Cormoria_{name}" in host_names or 79 + group_index >= 128 or index >= 128:
                raise ManifestError(f"{owner}: map collision or signed capacity")
            if data["layout"] not in layout_index:
                raise ManifestError(f"{owner}: missing layout {data['layout']}")
            layout = layout_index[data["layout"]]
            layouts[layout["id"]] = dict(layout, target_id=layout["id"].replace("LAYOUT_", "LAYOUT_CORMORIA_", 1),
                                         target_name="Cormoria_" + layout["name"])
            for path_key in ("border_filepath", "blockdata_filepath"):
                sources.asset(layout[path_key], owner)
            tilesets.update([layout["primary_tileset"], layout["secondary_tileset"]])
            sections.setdefault(data["region_map_section"], 250 + len(sections))
            maps.append({"source_name": name, "source_id": data["id"], "source_group": group_index,
                         "source_index": index, "target_name": "Cormoria_" + name,
                         "target_id": data["id"].replace("MAP_", "MAP_CORMORIA_", 1),
                         "group": 79 + group_index, "index": index, "layout": data["layout"],
                         "section": data["region_map_section"], "music": data["music"]})
            script = f"data/maps/{name}/scripts.inc"
            roots.append(script)
            sources.add(script, name)
            pory = script.removesuffix(".inc") + ".pory"
            if (donor / pory).is_file():
                sources.add(pory, name + ": review evidence only; compiled scripts.inc is authoritative")
            for field in ("object_events", "coord_events", "bg_events"):
                for event in data.get(field, []):
                    label = event.get("script", "0")
                    if label not in ("0", "0x0", "NULL", None):
                        event_labels.append(label)
            all_map_references.extend({"symbol": token, "kind": "constant", "owner": owner}
                                      for token in TOKEN.findall(json.dumps(data))
                                      if token in constants or token.isupper() or token.startswith(("MAP_", "FLAG_", "VAR_")))
    if (len(maps), len(layouts), len(sections), len(tilesets)) != (165, 165, 51, 59):
        raise ManifestError("pinned map/layout/section/tileset closure counts changed")
    graph.close(roots, event_labels)
    macros = macro_closure(donor, graph, sources)
    native_references, native_module_evidence = native_module_references(donor, graph.native_calls, natives)
    for path in sorted(graph.includes):
        sources.add(path, "script label closure")
    references = graph.references + all_map_references + native_references
    by_symbol: dict[str, set[str]] = defaultdict(set)
    for reference in references:
        by_symbol[reference["symbol"]].add(reference["owner"])
    for reference in graph.numeric_references:
        candidates = sorted(name for name, value in values.items()
                            if name.startswith(reference["prefix"]) and value == reference["value"])
        if not candidates:
            raise ManifestError(f"{reference['owner']}: unallocated numeric {reference['prefix']} reference {reference['value']:#x}")
        reference["source_symbol"] = candidates[0]
        by_symbol[candidates[0]].add(reference["owner"])
    generated_symbols = {entry["id"] for entry in all_layouts}
    for group in groups["group_order"]:
        for name in groups[group]:
            generated_symbols.add(json.loads(read(donor, f"data/maps/{name}/map.json"))["id"])
    generated_symbols.add("MAP_DYNAMIC")
    for token, owners in by_symbol.items():
        if token in constants or token in graph.blocks or token in generated_symbols:
            continue
        if "SPECIES_" + token in constants:
            continue  # token-pasting OBJ_EVENT_GFX_SPECIES constructor argument
        if any(token in graph.local_constants.get(path, {}) for path in graph.includes):
            continue
        raise ManifestError(f"{sorted(owners)[0]}: unresolved constant {token}")
    for token in by_symbol:
        if token in constant_owners:
            sources.add(constant_owners[token].rsplit(":", 1)[0], f"constant {token}")
    symbols: dict[str, Any] = {}
    for category, prefix in (("flags", "FLAG_"), ("vars", "VAR_"), ("trainers", "TRAINER_"), ("heals", "HEAL_LOCATION_")):
        selected = sorted(token for token in by_symbol if token.startswith(prefix)
                          and not token.startswith(("TRAINER_TYPE_", "TRAINER_BATTLE_", "TRAINER_CLASS_")))
        # Scratch registers are host script ABI, not persistent campaign state.
        abi = [token for token in selected if category == "vars" and
               (token.startswith("VAR_TEMP_") or 0x8000 <= values.get(token, -1) < 0x8100)]
        persistent = [token for token in selected if token not in abi]
        for token in selected:
            if token not in constants or token not in values:
                raise ManifestError(f"{sorted(by_symbol[token])[0]}: unallocated {category} {token}")
            sources.add(constant_owners[token].split(":")[0], token)
        start, capacity = RANGES[category]
        occupied = {value for token, value in host_values.items()
                    if token.startswith((prefix, "JOHTO_" + prefix, "CORMORIA_" + prefix))
                    and not token.startswith("Cormoria_")}
        canonical_by_id = {value: min(token for token in persistent if values[token] == value)
                           for value in {values[token] for token in persistent}}
        allocation = allocate(list(canonical_by_id.values()), start, capacity, occupied, category)
        symbols[category] = [{"source_symbol": token, "source_id": values[token],
                              "target_symbol": "Cormoria_" + token,
                              "target_id": values[token] if token in abi else allocation[canonical_by_id[values[token]]],
                              "binding": "host_script_scratch_abi" if token in abi else "campaign_owned",
                              "owners": sorted(by_symbol[token])} for token in selected]
    selected_maps = {item["source_id"] for item in maps}
    external = []
    for symbol in sorted(by_symbol):
        if symbol.startswith("MAP_") and symbol not in selected_maps and not symbol.startswith(("MAP_TYPE_", "MAP_BATTLE_SCENE_", "MAP_SCRIPT_", "MAP_CONNECTION_")):
            if symbol == "MAP_DYNAMIC":
                binding = "Cormoria saved dynamic destination: validate map pair, preserve setter/consumer and region ownership"
                unit = "U3/U8"
            elif symbol == "MAP_ROUTE117_POKEMON_DAY_CARE":
                binding = "Cormoria day-care service: preserve deposited party, offspring and return destination; bind donor common day-care script to Cormoria service"
                unit = "U6/U8"
            elif symbol == "MAP_ROUTE112":
                binding = "Replace stale copied Hoenn escape override with MAP_CORMORIA_ROUTE7 (11,7): Route7 warp0 entrance is (11,6), and UpdateEscapeWarp places exterior escape one tile below the entrance; station disallows direct escaping"
                unit = "U6/U8"
            else:
                raise ManifestError(f"{sorted(by_symbol[symbol])[0]}: unresolved external map {symbol}")
            external.append({"symbol": symbol, "owners": sorted(by_symbol[symbol]), "binding": binding,
                             "owner_unit": unit, "status": "planned_adapter_not_implemented"})
            if symbol == "MAP_ROUTE112":
                external[-1]["target"] = {"map": "MAP_CORMORIA_ROUTE7", "x": 11, "y": 7}
                external[-1]["evidence"] = ["data/maps/PellucaCableCarStation/scripts.inc:9", "data/maps/Route7/map.json", "src/overworld.c:UpdateEscapeWarp"]
    tile_records = {}
    for path in ("src/data/tilesets/headers.h", "src/data/tilesets/graphics.h", "src/data/tilesets/metatiles.h"):
        sources.add(path, "tileset declarations")
        tile_records.update({name: (path, body) for name, body in record_blocks(read(donor, path)).items()})
    tile_entries = []
    for name in sorted(tilesets):
        if name not in tile_records:
            raise ManifestError(f"layouts: missing tileset {name}")
        path, body = tile_records[name]
        dependencies = sorted(set(TOKEN.findall(body)) & tile_records.keys() - {name})
        for dependency in dependencies:
            dep_path, dep_body = tile_records[dependency]
            sources.scan_assets(dep_path, name, dep_body)
        callbacks = re.findall(r"\.callback\s*=\s*([A-Za-z_]\w*)", body)
        tile_entries.append({"source_symbol": name, "target_symbol": "Cormoria_" + name,
                             "record": path, "dependencies": dependencies, "callbacks": callbacks})
        for callback in callbacks:
            if callback != "NULL":
                if callback not in natives:
                    raise ManifestError(f"{path}: missing native tileset callback {callback}")
                graph.native_calls.append({"symbol": callback, "command": "tileset_callback", "owner": name})
    native_entries = []
    for name in sorted({call["symbol"] for call in graph.native_calls}):
        definitions_for_name = natives[name]
        if len({entry['path'] for entry in definitions_for_name}) != 1:
            raise ManifestError(f"native adapter {name}: ambiguous definitions {definitions_for_name}")
        definitions_for_name = [{key: value for key, value in item.items() if not key.startswith("_")}
                                for item in definitions_for_name]
        definition = definitions_for_name[0]
        path = definition["path"]
        sources.scan_assets(path, f"native adapter evidence: {name}")
        native_entries.append({"source_symbol": name, "target_symbol": "Cormoria_" + name,
                               "definition": definition, "calls": [call for call in graph.native_calls if call["symbol"] == name],
                               "conditional_definition_locations": definitions_for_name,
                               "status": "planned_adapter_not_implemented", "owner_unit": "U6",
                               "required_semantics": f"Preserve {name} at {path}:{definition['line']}; review inputs, outputs, waits, persistent writes, cancellation and cleanup before binding",
                               "verification": "donor-equivalent normal, cancel/failure, reward/replay and field-restoration cases where applicable"})
        if name == "CableCarWarp":
            native_entries[-1]["required_semantics"] = "Preserve donor VAR_0x8004 dispatch: nonzero -> Cormoria_PellucaCableCarStation (6,4), zero -> Cormoria_MirrohBaseCampCableCarStation (6,4). Host CableCarWarp has Hoenn endpoints and cannot be reused unchanged. Preserve Cormoria station-state variable."
            native_entries[-1]["verification"] = "Both cable-car directions, station arrival state, cancellation, and Route7 (11,7) escape destination"
        if any(call["command"] == "tileset_callback" for call in native_entries[-1]["calls"]):
            native_entries[-1]["owner_unit"] = "U4"
    commands = [{"symbol": name, "owners": sorted(set(owners))} for name, owners in sorted(graph.commands.items())]
    symbols.update({"labels": [{"source_symbol": name, "target_symbol": "Cormoria_" + name,
                                "definition": block["owner"]} for name, block in sorted(graph.selected.items())],
                    "native_bindings": native_entries, "commands": commands, "macros": macros,
                    "numeric_references": graph.numeric_references,
                    "native_module_dependency_evidence": native_module_evidence})
    symbols["assembly_constants"] = [{"path": path, "source_symbol": name,
                                      "target_symbol": "Cormoria_" + name, "expression": expression}
                                     for path in sorted(graph.includes)
                                     for name, expression in sorted(graph.local_constants.get(path, {}).items())]
    symbols["semantic_constants"] = [{"source_symbol": name, "source_value": values.get(name),
                                      "source_definition": constant_owners[name],
                                      "target_symbol": "Cormoria_" + name,
                                      "owners": sorted(by_symbol[name]),
                                      "status": "requires_semantic_translation_or_proven_host_equivalent"}
                                     for name in sorted(by_symbol) if name in constants]
    content = content_closure.collect(donor, host, maps, symbols, by_symbol, sources, sys.modules[__name__], graph.selected, constants)
    resource_audit = content_closure.resource_audit(donor, host, sources, sys.modules[__name__])
    coop = content_closure.coop_audit(host, symbols, content["heal_destinations"], sys.modules[__name__])
    region = {"schema_version": 1, "provenance": provenance,
              "host_baseline": {"revision": HOST_REVISION, "groups": 79, "maps": 1344,
                                "map_groups_identity_sha256": sha(canonical(baseline_groups).encode())},
              "status": "inventory_only_runtime_not_registered",
              "runtime_ready": False,
              "groups": [{"source": group, "target": f"gMapGroup_Cormoria_Phase{index + 1}",
                          "host_group": 79 + index, "maps": groups[group]} for index, group in enumerate(GROUP_NAMES)],
              "maps": maps, "layouts": list(layouts.values()),
              "sections": [{"source_symbol": name, "target_symbol": name.replace("MAPSEC_", "MAPSEC_CORMORIA_", 1),
                            "target_id": number, "status": "requires_U2_wide_section_api"} for name, number in sections.items()],
              "tilesets": tile_entries, "external_edges": external, "content": content,
              "resource_audit": resource_audit,
              "dependency_policy": {"scripts": "all compiled campaign labels plus transitive shared labels and fallthrough",
                                    "native": "explicit semantic boundary; definitions/assets are evidence, never blanket engine import",
                                    "pory": "review evidence; never compiled by host"}}
    ledger = {"schema_version": 1, "provenance": provenance,
              "allocation_contract": {name: {"start": start, "capacity": capacity,
                                            "allocated": len({x['target_id'] for x in symbols[name] if x['binding'] == 'campaign_owned'}),
                                            "runtime_status": "requires_U3_dispatch_and_save_storage"}
                                      for name, (start, capacity) in RANGES.items()}, "coop_capacity_audit": coop, **symbols}
    if verify_donor(donor) != provenance:
        raise ManifestError("donor changed while building the manifest")
    tracked = set(git(donor, "ls-files").splitlines())
    untracked_inputs = set(sources.owners) - tracked
    if untracked_inputs:
        raise ManifestError(f"source closure contains unpinned inputs: {sorted(untracked_inputs)[:5]}")
    return {"region_manifest.json": region, "symbol_ledger.json": ledger,
            "source_manifest.json": sources.manifest(provenance),
            "DONOR_CREDITS.md": f"# Dreamstone Mysteries donor credits\n\nSource: {DONOR_REPOSITORY}\n\nPinned revision: `{DONOR_REVISION}`. The upstream credits below are preserved verbatim.\n\n" + read(donor, "CREDITS.md")}


def write_bundle(bundle: dict[str, Any], output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    for name, content in bundle.items():
        (output / name).write_text(content if isinstance(content, str) else canonical(content), encoding="utf-8", newline="\n")


def check_bundle(bundle: dict[str, Any], output: Path) -> bool:
    return all((output / name).is_file() and (output / name).read_text(encoding="utf-8") == (content if isinstance(content, str) else canonical(content))
               for name, content in bundle.items())


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        bundle = build_bundle(args.donor)
        if args.check:
            if not check_bundle(bundle, args.output):
                print("Cormoria manifests are missing or stale", file=sys.stderr)
                return 2
        else:
            write_bundle(bundle, args.output)
        print("Cormoria inventory: 165 maps / 165 layouts / 51 sections / 59 tilesets; runtime adapters remain planned")
        return 0
    except ManifestError as error:
        print(f"Cormoria manifest error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
