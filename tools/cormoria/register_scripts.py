"""Prepare an authenticated, isolated Cormoria script include preview.

The preview is deliberately not linked. Donor event_scripts.s supplies include
order only: its command table, shared globals, and Hoenn includes stay in the
host assembly unit. Every selected definition receives a Cormoria namespace so
the donor's copied common and Hoenn-map scripts cannot redefine host labels.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path

from tools.cormoria import berry_plots, import_world

ROOT = Path(__file__).resolve().parents[2]
SCRIPT_COUNTS = {"maps": 168, "scripts": 12, "text": 7}
INCLUDE = re.compile(r'^\s*\.include\s+"([^"]+)"\s*(?:@.*)?$', re.MULTILINE)
LABEL = re.compile(r'^\s*([A-Za-z_][A-Za-z_0-9]*)\s*:{1,2}(?!:)', re.MULTILINE)
LOCAL_SET = re.compile(r'^\s*\.set\s+(LOCALID_[A-Za-z_0-9]+)\s*,', re.MULTILINE)
TOKEN = re.compile(r'\b[A-Za-z_][A-Za-z_0-9]*\b')
QUOTED = re.compile(r'("(?:\\.|[^"\\])*")')
GIVEMON_SHINY = re.compile(r'(?m)(^\s*givemon\b[^\n]*?)\bisShiny\s*=\s*(TRUE|FALSE)\b')
GACHA_TOKEN_SETTLEMENT = "data/maps/GalecrestCity_GameCorner/scripts.inc"


class ScriptRegistrationError(ValueError):
    """The staged scripts cannot safely be registered."""


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _stage_bytes(stage: Path, relative: str, records: dict[str, dict]) -> bytes:
    if relative not in records:
        raise ScriptRegistrationError(f"unlisted staged source: {relative}")
    path = stage / import_world.safe_relative(relative)
    if not path.is_file() or not path.resolve().is_relative_to(stage):
        raise ScriptRegistrationError(f"missing or escaping staged source: {relative}")
    data = path.read_bytes()
    record = records[relative]
    if len(data) != record["bytes"] or _sha(data) != record["sha256"]:
        raise ScriptRegistrationError(f"staged source drift: {relative}")
    return data


def _definitions(text: str, path: str) -> list[str]:
    # Assembly directives and preprocessor line markers cannot define labels.
    labels = LABEL.findall(text)
    if len(labels) != len(set(labels)):
        raise ScriptRegistrationError(f"duplicate label within {path}")
    return labels


def _host_labels(root: Path) -> set[str]:
    host = root / "data/event_scripts.s"
    text = host.read_text(encoding="utf-8-sig")
    labels = set(LABEL.findall(text))
    for relative in INCLUDE.findall(text):
        if not relative.startswith("data/"):
            continue
        path = root / import_world.safe_relative(relative)
        if not path.is_file() or not path.resolve().is_relative_to(root.resolve()):
            raise ScriptRegistrationError(f"missing host include: {relative}")
        labels.update(LABEL.findall(path.read_text(encoding="utf-8-sig")))
    return labels


def _rename(text: str, labels: dict[str, str]) -> str:
    chunks = QUOTED.split(text)
    return "".join(chunk if index % 2 else TOKEN.sub(
        lambda match: labels.get(match.group(), match.group()), chunk)
        for index, chunk in enumerate(chunks))


def _adapt_givemon_shininess(text: str) -> tuple[str, int]:
    """Keep the donor's forced shiny/non-shiny gifts under the host enum."""
    return GIVEMON_SHINY.subn(
        lambda match: match[1] + "shinyMode=" + (
            "SHINY_MODE_ALWAYS" if match[2] == "TRUE" else "SHINY_MODE_NEVER"),
        text,
    )


def _adapt_gacha_token_settlement(text: str) -> tuple[str, int]:
    """Remove donor script spends; the C minigame owns token settlement.

    ``StartGacha`` removes one token only after the Pokémon reaches party or
    PC. The donor script's post-``waitstate`` removal would charge twice on
    success and charge once on cancellation or failed delivery.
    """
    lines = text.splitlines(keepends=True)
    transformed = 0
    for index, line in enumerate(lines):
        if line.strip() != "removeitem ITEM_GACHA_TOKEN":
            continue

        previous = index - 1
        while previous >= 0 and lines[previous].lstrip().startswith("#"):
            previous -= 1
        if previous < 0 or lines[previous].strip() != "waitstate":
            raise ScriptRegistrationError(
                "Gacha token removal is not immediately after waitstate"
            )

        following = index + 1
        while following < len(lines) and lines[following].lstrip().startswith("#"):
            following += 1
        if following >= len(lines) or not lines[following].strip().startswith("goto "):
            raise ScriptRegistrationError(
                "Gacha token removal does not have its authenticated return label"
            )
        lines[index] = ""
        transformed += 1

    return "".join(lines), transformed


def build_preview(stage: Path, root: Path = ROOT) -> dict[str, bytes]:
    stage = stage.resolve(strict=True)
    root = root.resolve(strict=True)
    region, symbols, sources = import_world.load_manifests(root)
    source_index = {row["path"]: row for row in sources["files"]}
    stage_manifest = json.loads((stage / "staging_manifest.json").read_text(encoding="utf-8"))
    if (stage_manifest.get("provenance") != region["provenance"]
            or stage_manifest.get("manifest_sha256") != import_world.PINNED_SHA256
            or stage_manifest.get("world_id") != "cormoria"
            or stage_manifest.get("namespaced_script_source_count") != 188
            or stage_manifest.get("runtime_ready") is not False):
        raise ScriptRegistrationError("stage provenance or status drift")
    rows = stage_manifest["files"]
    records = {row["path"]: row for row in rows}
    if len(records) != len(rows):
        raise ScriptRegistrationError("duplicate staged source record")
    source_paths = {row["definition"].rsplit(":", 1)[0] for row in symbols["labels"]}
    root_script = "data/event_scripts.s"
    if root_script not in source_paths or len(source_paths) != 188:
        raise ScriptRegistrationError("pinned script source set drift")
    scripts = source_paths - {root_script}
    counts = {category: sum(path.startswith(f"data/{category}/") for path in scripts)
              for category in SCRIPT_COUNTS}
    if counts != SCRIPT_COUNTS or sum(counts.values()) != len(scripts):
        raise ScriptRegistrationError(f"script category counts drift: {counts}")
    staged_paths = {path.removeprefix("namespaced_scripts/") for path in records
                    if path.startswith("namespaced_scripts/")}
    if staged_paths != source_paths:
        raise ScriptRegistrationError("staged script source set differs from pinned ledger")
    identities = import_world.identities(region, symbols)
    berry_bindings = berry_plots.bindings(root)
    # The donor lets LOCALID_CLEF leak from another map's .set directive.
    # Resolve the two storage-room uses against that map's authenticated
    # object order instead of depending on global assembler include order.
    storage_map = json.loads(_stage_bytes(
        stage, "source/data/maps/SSElegant_Storage/map.json", records))
    storage_objects = storage_map["object_events"]
    clef_ids = [index + 1 for index, obj in enumerate(storage_objects)
                if obj["graphics_id"] == "OBJ_EVENT_GFX_SPECIES(CLEFABLE)"]
    if len(clef_ids) != 1:
        raise ScriptRegistrationError("S.S. Elegant storage Clefable identity drift")
    texts: dict[str, str] = {}
    for relative in sorted(source_paths):
        source = _stage_bytes(stage, f"source/{relative}", records)
        pin = source_index.get(relative)
        if pin is None or len(source) != pin["bytes"] or _sha(source) != pin["sha256"]:
            raise ScriptRegistrationError(f"source differs from pinned donor: {relative}")
        expected = import_world.rewrite_script(source.decode("utf-8-sig"), identities).encode("utf-8")
        actual = _stage_bytes(stage, f"namespaced_scripts/{relative}", records)
        if actual != expected:
            raise ScriptRegistrationError(f"namespaced source drift: {relative}")
        texts[relative] = actual.decode("utf-8")
    ordered = [path for path in INCLUDE.findall(texts[root_script]) if path in scripts]
    if len(ordered) != len(scripts) or set(ordered) != scripts:
        raise ScriptRegistrationError("donor event_scripts.s omits or repeats a campaign include")
    labels: dict[str, str] = {}
    for relative in ordered:
        if INCLUDE.search(texts[relative]):
            raise ScriptRegistrationError(f"nested include requires review: {relative}")
        for original in _definitions(texts[relative], relative):
            if original in labels:
                raise ScriptRegistrationError(f"duplicate campaign label: {original}")
            labels[original] = original if original.startswith("Cormoria_") else f"Cormoria_{original}"
    if len(set(labels.values())) != len(labels):
        raise ScriptRegistrationError("script namespace collision")
    host_labels = _host_labels(root)
    if host_labels.intersection(labels.values()):
        raise ScriptRegistrationError("campaign script duplicates a host global")
    rendered: dict[str, bytes] = {}
    shiny_gifts = 0
    donor_trade_references = 0
    donor_partner_references = 0
    donor_number_input_references = 0
    donor_gacha_token_references = 0
    gacha_token_transformations = 0
    for relative in ordered:
        target = f"data/cormoria/{relative.removeprefix('data/')}"
        local_sets = LOCAL_SET.findall(texts[relative])
        if len(local_sets) != len(set(local_sets)):
            raise ScriptRegistrationError(f"duplicate local-ID definition within {relative}")
        prefix = re.sub(r'[^A-Za-z_0-9]', '_', relative.removeprefix('data/').removesuffix('.inc'))
        scoped_ids = {name: f"Cormoria_{prefix}_{name}" for name in local_sets}
        if relative == "data/maps/SSElegant_Storage/scripts.inc":
            if texts[relative].count("LOCALID_CLEF") != 2:
                raise ScriptRegistrationError("S.S. Elegant storage Clefable script drift")
            scoped_ids["LOCALID_CLEF"] = str(clef_ids[0])
        donor_trade_references += texts[relative].count("INGAME_TRADE_WIMPOD")
        donor_partner_references += texts[relative].count("PARTNER_ROUTE6_GAB")
        donor_number_input_references += texts[relative].count("MULTI_NUMBER_INPUT")
        rewritten = _rename(texts[relative], labels | berry_bindings | scoped_ids
                            | {"INGAME_TRADE_WIMPOD": "INGAME_TRADE_CORMORIA_WIMPOD",
                               "PARTNER_ROUTE6_GAB": "PARTNER_CORMORIA_GABRIELLE",
                               "MULTI_NUMBER_INPUT": "MULTI_CORMORIA_NUMBER_INPUT",
                               "FLAG_VISITED_RIVETSHORE_RANGER":
                                   "Cormoria_FLAG_VISITED_RIVETSHORE_RANGER"})
        rewritten, gift_count = _adapt_givemon_shininess(rewritten)
        donor_gacha_token_references += texts[relative].count("removeitem ITEM_GACHA_TOKEN")
        if relative == GACHA_TOKEN_SETTLEMENT:
            rewritten, transformed = _adapt_gacha_token_settlement(rewritten)
            gacha_token_transformations += transformed
        shiny_gifts += gift_count
        expected_labels = {labels[name] for name in _definitions(texts[relative], relative)}
        if set(_definitions(rewritten, target)) != expected_labels:
            raise ScriptRegistrationError(f"script label rewrite failed: {relative}")
        rendered[target] = rewritten.encode("utf-8")
    if shiny_gifts != 6:
        raise ScriptRegistrationError(f"donor forced-shininess gift count drifted: {shiny_gifts}")
    if donor_trade_references != 1:
        raise ScriptRegistrationError(f"donor in-game trade reference count drifted: {donor_trade_references}")
    if donor_partner_references != 1:
        raise ScriptRegistrationError(f"donor Route 6 partner reference count drifted: {donor_partner_references}")
    if donor_number_input_references != 1:
        raise ScriptRegistrationError(f"donor number-input menu reference count drifted: {donor_number_input_references}")
    if donor_gacha_token_references != 4:
        raise ScriptRegistrationError(
            f"donor Gacha token settlement site count drifted: {donor_gacha_token_references}"
        )
    if gacha_token_transformations != 4:
        raise ScriptRegistrationError(
            f"Gacha token settlement transformations drifted: {gacha_token_transformations}"
        )
    wrapper = "@ Cormoria content preview; include once from data/event_scripts.s.\n"
    wrapper += "".join(f'\t.include "data/cormoria/{path.removeprefix("data/")}"\n'
                       for path in ordered)
    rendered["data/cormoria/scripts.inc"] = wrapper.encode("utf-8")
    metadata = {"schema_version": 1, "world_id": "cormoria", "runtime_ready": False,
                "source_revision": region["provenance"]["revision"], "counts": counts,
                "include_order": ordered,
                "files": [{"path": path, "bytes": len(data), "sha256": _sha(data)}
                          for path, data in sorted(rendered.items())]}
    rendered["registration.json"] = (json.dumps(metadata, indent=2) + "\n").encode("utf-8")
    return rendered


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        output = args.output.resolve()
        stage = args.stage.resolve(strict=True)
        if (output.exists() or output.is_relative_to(ROOT.resolve())
                or output.is_relative_to(stage) or stage.is_relative_to(output)):
            raise ScriptRegistrationError("output must be fresh and external to stage and host")
        rendered = build_preview(stage)
        output.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="cormoria-script-preview-", dir=output.parent) as temporary:
            directory = Path(temporary)
            for relative, data in rendered.items():
                target = directory / import_world.safe_relative(relative)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            directory.rename(output)
        print(f"Prepared {len(rendered) - 2} Cormoria script includes at {output}")
    except (OSError, UnicodeError, ValueError, KeyError, TypeError) as exc:
        print(f"Cormoria script registration: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
