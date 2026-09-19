"""Register the pinned donor's Johto region sections in the host source table.

The first 210 host records are immutable. This tool changes only New Bark's
donor geometry and appends the reviewed 40-section Johto tail; generated
headers are produced by the normal mapjson pipeline.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

from tools.johto.region_manifest import (
    EXPECTED_SECTION_ALLOCATION_ORDER,
    EXPECTED_SECTION_TAIL_IDS,
)


ROOT = Path(__file__).resolve().parents[2]
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
DONOR_TREE = "33661709e5368edc01c37ed9bb5e0a7a0cb192c8"
DONOR_REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
HOST_SECTIONS = ROOT / "src/data/region_map/region_map_sections.json"
MANIFEST = ROOT / "data/johto/region_manifest.json"
EXPECTED_HOST_BASELINE_COUNT = 210
EXPECTED_SENTINEL = 250
EXPECTED_SECTION_COUNT = 250
SECTION_TAIL_START = 210
SECTION_TAIL_END = 249


class SectionRegistrationError(ValueError):
    """A donor or append-only section source inconsistency."""


def _load(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SectionRegistrationError(f"cannot read {path}: {exc}") from exc


def _git(donor: Path, *args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(donor), *args],
            text=True,
            stderr=subprocess.STDOUT,
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        detail = getattr(exc, "output", "") or str(exc)
        raise SectionRegistrationError(f"donor git verification failed: {detail.strip()}") from exc


def _verify_donor(donor: Path) -> None:
    if not donor.is_dir():
        raise SectionRegistrationError(f"donor root does not exist: {donor}")
    revision = _git(donor, "rev-parse", "HEAD")
    tree = _git(donor, "rev-parse", "HEAD^{tree}")
    if (revision, tree) != (DONOR_REVISION, DONOR_TREE):
        raise SectionRegistrationError(
            f"donor pin mismatch: revision={revision}, tree={tree}; "
            f"expected {DONOR_REVISION}/{DONOR_TREE}"
        )
    if _git(donor, "status", "--porcelain"):
        raise SectionRegistrationError("donor working tree must be clean")


def _tail_symbols(manifest: dict[str, Any]) -> tuple[str, ...]:
    provenance = manifest.get("provenance", {})
    if (
        provenance.get("repository") != DONOR_REPOSITORY
        or provenance.get("donor_revision") != DONOR_REVISION
        or provenance.get("donor_tree") != DONOR_TREE
    ):
        raise SectionRegistrationError("region manifest donor pin drift")
    sections = manifest.get("sections")
    if not isinstance(sections, dict) or sections.get("johto_count") != 41:
        raise SectionRegistrationError("region manifest does not describe 41 Johto sections")
    order = sections.get("allocation_order")
    if (
        not isinstance(order, list)
        or tuple(order) != EXPECTED_SECTION_ALLOCATION_ORDER
    ):
        raise SectionRegistrationError("region manifest section allocation order drifted")
    tail = tuple(symbol for symbol in order if symbol.startswith("MAPSEC_JOHTO_"))
    if tail != EXPECTED_SECTION_TAIL_IDS:
        raise SectionRegistrationError("region manifest section allocation order drifted")
    entries = sections.get("entries")
    ids = {
        entry.get("target_symbol"): entry.get("target_id")
        for entry in entries or []
        if isinstance(entry, dict)
    }
    expected_ids = {
        "MAPSEC_NEW_BARK_TOWN": 209,
        **{symbol: SECTION_TAIL_START + index for index, symbol in enumerate(tail)},
    }
    for symbol, ident in expected_ids.items():
        if ids.get(symbol) != ident:
            raise SectionRegistrationError(
                f"region manifest section allocation drifted for {symbol}"
            )
    if ids.get("MAPSEC_KANTO_VICTORY_ROAD") != 132:
        raise SectionRegistrationError("region manifest Kanto section allocation drifted")
    if len(ids) < len(expected_ids):
        raise SectionRegistrationError("region manifest section entries are incomplete")
    return tail


def _record_for_donor(
    donor_sections: dict[str, dict[str, Any]], source_id: str
) -> dict[str, Any]:
    record = donor_sections.get(source_id)
    if record is None:
        raise SectionRegistrationError(f"donor section is missing: {source_id}")
    if set(record) != {"map_section", "name", "x", "y", "width", "height"}:
        raise SectionRegistrationError(f"unsupported donor section fields: {source_id}")
    if not isinstance(record["name"], str):
        raise SectionRegistrationError(f"invalid donor section name: {source_id}")
    for key in ("x", "y", "width", "height"):
        if not isinstance(record[key], int) or isinstance(record[key], bool):
            raise SectionRegistrationError(f"invalid donor section geometry: {source_id}")
    if record["width"] < 1 or record["height"] < 1 or record["x"] < 0 or record["y"] < 0:
        raise SectionRegistrationError(f"invalid donor section geometry: {source_id}")
    return {
        "id": source_id,
        "name": record["name"],
        "x": record["x"],
        "y": record["y"],
        "width": record["width"],
        "height": record["height"],
    }


def _donor_sections(donor: Path) -> dict[str, dict[str, Any]]:
    data = _load(donor / "src/data/region_map/region_map_sections.json")
    records = data.get("map_sections") if isinstance(data, dict) else None
    if not isinstance(records, list):
        raise SectionRegistrationError("donor region section source has no map_sections")
    result: dict[str, dict[str, Any]] = {}
    for record in records:
        if not isinstance(record, dict) or not isinstance(record.get("map_section"), str):
            raise SectionRegistrationError("malformed donor region section record")
        source_id = record["map_section"]
        if source_id in result:
            raise SectionRegistrationError(f"duplicate donor section {source_id}")
        result[source_id] = record
    return result


def build_sections(donor_root: str | Path, repo_root: Path = ROOT) -> dict[str, Any]:
    donor = Path(donor_root).resolve()
    repo_root = repo_root.resolve()
    _verify_donor(donor)
    manifest = _load(repo_root / MANIFEST.relative_to(ROOT))
    tail = _tail_symbols(manifest)
    source = _load(repo_root / HOST_SECTIONS.relative_to(ROOT))
    records = source.get("map_sections") if isinstance(source, dict) else None
    if not isinstance(records, list) or len(records) < EXPECTED_HOST_BASELINE_COUNT:
        raise SectionRegistrationError("host region section source has fewer than 210 records")
    if len(records) not in {EXPECTED_HOST_BASELINE_COUNT, EXPECTED_SECTION_COUNT}:
        raise SectionRegistrationError("host region section source has an unexpected tail")
    baseline = records[:EXPECTED_HOST_BASELINE_COUNT]
    baseline_ids = [record.get("id") for record in baseline if isinstance(record, dict)]
    if len(baseline_ids) != EXPECTED_HOST_BASELINE_COUNT or len(set(baseline_ids)) != len(baseline_ids):
        raise SectionRegistrationError("host region section baseline is malformed")
    identity = _load(repo_root / "data/johto/host_identity_baseline.json")
    expected_ids = [
        record.get("id")
        for record in identity.get("section_constants", [])
        if isinstance(record, dict)
    ]
    if baseline_ids != expected_ids:
        raise SectionRegistrationError("host region section baseline identity drifted")
    donor_records = _donor_sections(donor)
    new_bark = _record_for_donor(donor_records, "MAPSEC_NEW_BARK_TOWN")
    baseline[-1] = new_bark

    appended = []
    for symbol in tail:
        donor_symbol = "MAPSEC_" + symbol.removeprefix("MAPSEC_JOHTO_")
        record = _record_for_donor(donor_records, donor_symbol)
        record["id"] = symbol
        appended.append(record)
    result = baseline + appended
    if len(result) != EXPECTED_SECTION_COUNT:
        raise SectionRegistrationError("section registration did not produce sentinel position")
    if [record["id"] for record in result[SECTION_TAIL_START:SECTION_TAIL_END + 1]] != list(tail):
        raise SectionRegistrationError("Johto section tail identity/order drifted")
    return {"map_sections": result}


def render(data: dict[str, Any]) -> str:
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


def run(
    donor_root: str | Path,
    repo_root: Path = ROOT,
    check: bool = False,
) -> int:
    generated = render(build_sections(donor_root, repo_root))
    path = (repo_root / HOST_SECTIONS.relative_to(ROOT)).resolve()
    if check:
        if not path.is_file() or path.read_text(encoding="utf-8") != generated:
            raise SectionRegistrationError(f"generated section source drift: {path}")
        return 0
    path.write_text(generated, encoding="utf-8", newline="\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor-root", type=Path, required=True)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    try:
        return run(args.donor_root, args.repo_root, args.check)
    except (OSError, SectionRegistrationError, ValueError) as exc:
        print(f"johto section registration: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
