"""Import the pinned Johto region-map assets and generate its section layout."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DONOR_REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
PNG_SHA256 = "8cf72765d1132137e0046ec6e4546e3f49abf3f7393c58f64cc50b57fc1a2384"
BIN_SHA256 = "ce624f29b03ab9edac01f3b2c4321094609b75878b98b826de8eb96a02477948"
LAYOUT_SHA256 = "b1530031cca347c8b5d9abeee14e45aab846901d4e3eb9df6a66df604b21fe19"
DONOR_SECTIONS_SHA256 = "38dea2564b1ca0c49731a18a12d80c7757794cca561c518cdf7f41d90a6bd1cc"

MANIFEST_PATH = Path("data/johto/region_manifest.json")
HOST_SECTIONS_PATH = Path("src/data/region_map/region_map_sections.json")
OUTPUT_LAYOUT_PATH = Path("src/data/region_map/region_map_layout_johto.h")
ASSET_DIRECTORY = Path("graphics/pokenav/region_map")
SOURCE_LAYOUT_PATH = Path("src/data/region_map/region_map_layout_johto.h")
SOURCE_SECTIONS_PATH = Path("src/data/region_map/region_map_sections.json")
SOURCE_PNG_PATH = Path("graphics/pokenav/region_map/johtomap.png")
SOURCE_BIN_PATH = Path("graphics/pokenav/region_map/johtomap.bin")
MAP_WIDTH = 28
MAP_HEIGHT = 15
TOKEN_RE = re.compile(r"MAPSEC_[A-Z0-9_]+")


class RegionMapImportError(ValueError):
    """A pinned source, identity, geometry, or generated-output mismatch."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise RegionMapImportError(f"cannot read JSON {path}: {exc}") from exc


def _git(donor: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(donor), *args], text=True, stderr=subprocess.STDOUT
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        detail = getattr(exc, "output", "") or str(exc)
        raise RegionMapImportError(f"donor git verification failed: {detail.strip()}") from exc


def verify_donor(donor: Path) -> None:
    if not donor.is_dir():
        raise RegionMapImportError(f"donor root does not exist: {donor}")
    if (donor / ".git").exists():
        revision = _git(donor, "rev-parse", "HEAD")
        tree = _git(donor, "rev-parse", "HEAD^{tree}")
        if (revision, tree) != (DONOR_REVISION, DONOR_TREE):
            raise RegionMapImportError(
                f"donor pin mismatch: {revision}/{tree}; expected {DONOR_REVISION}/{DONOR_TREE}"
            )
        status = _git(
            donor, "status", "--porcelain=v1", "--", str(SOURCE_LAYOUT_PATH),
            str(SOURCE_SECTIONS_PATH), str(SOURCE_PNG_PATH), str(SOURCE_BIN_PATH)
        )
        if status:
            raise RegionMapImportError("donor region-map inputs are dirty")


def _read_pinned(path: Path, expected_sha: str) -> bytes:
    try:
        data = path.read_bytes()
    except OSError as exc:
        raise RegionMapImportError(f"cannot read pinned input {path}: {exc}") from exc
    actual = sha256(data)
    if actual != expected_sha:
        raise RegionMapImportError(
            f"pinned input hash mismatch for {path}: {actual}; expected {expected_sha}"
        )
    return data


def validate_png(data: bytes) -> None:
    if len(data) < 33 or data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise RegionMapImportError("Johto map PNG has an invalid header")
    width, height, bit_depth, color_type = struct.unpack(">IIBB", data[16:26])
    if (width, height, bit_depth, color_type) != (128, 128, 8, 3):
        raise RegionMapImportError(
            "Johto map PNG must be a 128x128 indexed 8-bit image"
        )
    if b"PLTE" not in data:
        raise RegionMapImportError("Johto map PNG has no indexed palette")


def _manifest_mapping(manifest: dict[str, Any]) -> dict[str, str]:
    provenance = manifest.get("provenance", {})
    if (
        provenance.get("repository") != DONOR_REPOSITORY
        or provenance.get("donor_revision") != DONOR_REVISION
        or provenance.get("donor_tree") != DONOR_TREE
    ):
        raise RegionMapImportError("region manifest donor provenance drifted")
    entries = manifest.get("sections", {}).get("entries")
    if not isinstance(entries, list):
        raise RegionMapImportError("region manifest has no section mapping")
    mapping: dict[str, str] = {"MAPSEC_NONE": "MAPSEC_NONE"}
    for entry in entries:
        if not isinstance(entry, dict):
            raise RegionMapImportError("region manifest contains a malformed section mapping")
        source = entry.get("source_symbol")
        target = entry.get("target_symbol")
        if not isinstance(source, str) or not isinstance(target, str):
            raise RegionMapImportError("region manifest contains a malformed section symbol")
        if source in mapping and mapping[source] != target:
            raise RegionMapImportError(f"ambiguous section mapping for {source}")
        mapping[source] = target
    trainer_hill = [
        entry for entry in manifest.get("registrations", [])
        if isinstance(entry, dict) and entry.get("name") in {
            "Gate_Route40_TrainerHill_Courtyard", "TrainerHill_Courtyard"
        }
    ]
    if len(trainer_hill) != 2 or any(
        entry.get("classification") != "excluded"
        or entry.get("reason") != "Trainer Hill contamination"
        for entry in trainer_hill
    ):
        raise RegionMapImportError("region manifest Trainer Hill exclusion drifted")
    mapping["MAPSEC_TRAINER_HILL"] = "MAPSEC_NONE"
    return mapping


def _section_records(data: Any, key: str) -> dict[str, dict[str, Any]]:
    records = data.get("map_sections") if isinstance(data, dict) else None
    if not isinstance(records, list):
        raise RegionMapImportError(f"{key} section authority has no map_sections")
    result: dict[str, dict[str, Any]] = {}
    for record in records:
        symbol = record.get(key) if isinstance(record, dict) else None
        if not isinstance(symbol, str) or symbol in result:
            raise RegionMapImportError(f"{key} section authority is malformed")
        result[symbol] = record
    return result


def validate_section_authority(
    manifest: dict[str, Any], donor_sections_data: Any, host_sections_data: Any
) -> None:
    donor_sections = _section_records(donor_sections_data, "map_section")
    host_sections = _section_records(host_sections_data, "id")
    host_ids = {
        record["id"]: index
        for index, record in enumerate(host_sections_data["map_sections"])
    }
    entries = manifest["sections"]["entries"]
    for entry in entries:
        classification = entry.get("classification")
        if classification not in {
            "new_johto", "preserved_host", "johto_alias",
            "reviewed_kanto_geography", "required_host_adapter",
        }:
            continue
        source = entry["source_symbol"]
        target = entry["target_symbol"]
        host_record = host_sections.get(target)
        if host_record is None:
            raise RegionMapImportError(f"missing host section authority for {source} -> {target}")
        if entry.get("target_id") != host_ids[target]:
            raise RegionMapImportError(f"host section id drifted for {target}")
        if classification not in {"new_johto", "preserved_host"}:
            continue
        donor_record = donor_sections.get(source)
        if donor_record is None:
            raise RegionMapImportError(f"missing donor section authority for {source}")
        for field in ("name", "x", "y", "width", "height"):
            if donor_record.get(field) != host_record.get(field):
                raise RegionMapImportError(
                    f"host section authority drifted for {target}.{field}"
                )
        x, y = host_record["x"], host_record["y"]
        width, height = host_record["width"], host_record["height"]
        if not all(isinstance(value, int) and not isinstance(value, bool) for value in (x, y, width, height)):
            raise RegionMapImportError(f"invalid host section geometry for {target}")
        if x < 0 or y < 0 or width < 1 or height < 1 or x + width > MAP_WIDTH or y + height > MAP_HEIGHT:
            raise RegionMapImportError(f"out-of-bounds host section geometry for {target}")


def parse_layout(source: str) -> list[list[str]]:
    rows: list[list[str]] = []
    for line in source.splitlines():
        tokens = TOKEN_RE.findall(line)
        if tokens:
            rows.append(tokens)
    if len(rows) != MAP_HEIGHT or any(len(row) != MAP_WIDTH for row in rows):
        raise RegionMapImportError("donor Johto layout is not a 15x28 section grid")
    return rows


def render_layout(rows: list[list[str]], mapping: dict[str, str]) -> str:
    try:
        mapped = [[mapping[token] for token in row] for row in rows]
    except KeyError as exc:
        raise RegionMapImportError(f"unmapped donor layout symbol: {exc.args[0]}") from exc
    lines = [
        "// Generated by tools/johto/import_region_map.py; do not edit.",
        f"// Source: {DONOR_REPOSITORY}@{DONOR_REVISION} ({DONOR_TREE})",
        f"// Inputs: PNG sha256:{PNG_SHA256}; BIN sha256:{BIN_SHA256};",
        f"// layout sha256:{LAYOUT_SHA256}; manifest-backed section renaming.",
        "static const mapsec_u16_t sRegionMapSections_Johto[MAP_HEIGHT][MAP_WIDTH] = {",
    ]
    lines.extend("    {" + ", ".join(row) + "}," for row in mapped)
    lines.append("};")
    return "\n".join(lines) + "\n"


def build(donor_root: str | Path, repo_root: str | Path = ROOT) -> tuple[bytes, bytes, str]:
    donor = Path(donor_root).resolve()
    repo = Path(repo_root).resolve()
    verify_donor(donor)
    png = _read_pinned(donor / SOURCE_PNG_PATH, PNG_SHA256)
    tilemap = _read_pinned(donor / SOURCE_BIN_PATH, BIN_SHA256)
    layout_bytes = _read_pinned(donor / SOURCE_LAYOUT_PATH, LAYOUT_SHA256)
    donor_sections_bytes = _read_pinned(donor / SOURCE_SECTIONS_PATH, DONOR_SECTIONS_SHA256)
    validate_png(png)
    if len(tilemap) != 4096:
        raise RegionMapImportError("Johto map tilemap must contain 4096 bytes")
    manifest = load_json(repo / MANIFEST_PATH)
    host_sections = load_json(repo / HOST_SECTIONS_PATH)
    donor_sections = json.loads(donor_sections_bytes.decode("utf-8"))
    validate_section_authority(manifest, donor_sections, host_sections)
    mapping = _manifest_mapping(manifest)
    layout = render_layout(parse_layout(layout_bytes.decode("utf-8")), mapping)
    host_symbols = {record["id"] for record in host_sections["map_sections"]}
    generated_symbols = set(TOKEN_RE.findall(layout)) - {"MAPSEC_NONE"}
    unknown = sorted(generated_symbols - host_symbols)
    if unknown:
        raise RegionMapImportError("generated layout uses unknown host sections: " + ", ".join(unknown))
    return png, tilemap, layout


def run(donor_root: str | Path, repo_root: str | Path = ROOT, *, check: bool) -> int:
    repo = Path(repo_root).resolve()
    png, tilemap, layout = build(donor_root, repo)
    expected = {
        repo / ASSET_DIRECTORY / "johtomap.png": png,
        repo / ASSET_DIRECTORY / "johtomap.bin": tilemap,
        repo / OUTPUT_LAYOUT_PATH: layout.encode("utf-8"),
    }
    if check:
        for path, data in expected.items():
            if not path.is_file():
                raise RegionMapImportError(f"generated Johto region-map output drift: {path}")
            actual = path.read_bytes()
            if path == repo / OUTPUT_LAYOUT_PATH:
                # Git may materialize a just-added text file with CRLF during
                # the same Windows `git apply --index` that adds its eol=lf
                # attribute. Permit that checkout representation only; every
                # other byte must still match the deterministic LF artifact.
                actual = actual.replace(b"\r\n", b"\n")
                if b"\r" in actual:
                    raise RegionMapImportError(
                        f"generated Johto region-map output drift: {path}"
                    )
            if actual != data:
                raise RegionMapImportError(f"generated Johto region-map output drift: {path}")
        return 0
    for path, data in expected.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.suffix in {".png", ".bin"} and path.exists() and path.read_bytes() != data:
            raise RegionMapImportError(f"refusing to overwrite mutated imported asset: {path}")
        path.write_bytes(data)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor-root", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        return run(args.donor_root, args.repo_root, check=args.check)
    except (OSError, UnicodeError, json.JSONDecodeError, RegionMapImportError) as exc:
        print(f"Johto region-map import: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
