"""Content-table and resource evidence for the Cormoria manifest.

These readers inventory donor data. They do not render, convert, or install it.
"""
from __future__ import annotations

import json
import re
from collections import deque
from collections import defaultdict
from pathlib import Path
from typing import Any


def collect(donor: Path, host: Path, maps: list[dict[str, Any]], symbols: dict[str, Any],
            by_symbol: dict[str, set[str]], sources: Any, api: Any,
            script_blocks: dict[str, Any], constants: dict[str, str]) -> dict[str, Any]:
    selected_maps = {entry["source_id"] for entry in maps}
    result: dict[str, Any] = {}
    wild_path = "src/data/wild_encounters.json"
    wild = json.loads(api.read(donor, wild_path))
    sources.add(wild_path, "wild encounters for all campaign maps")
    result["wild_encounters"] = []
    for group in wild["wild_encounter_groups"]:
        entries = [entry for entry in group.get("encounters", []) if entry.get("map") in selected_maps]
        if entries:
            result["wild_encounters"].append({"source_group": group["label"], "fields": group.get("fields", []),
                                              "encounters": entries})
    trainer_path = "src/data/trainers.h"
    trainer_text = api.read(donor, trainer_path)
    sources.add(trainer_path, "trainer source records")
    sources.add("src/data/trainers.party", "trainer human-readable evidence")
    trainer_starts = list(re.finditer(r"\[(DIFFICULTY_\w+)\]\[(TRAINER_\w+)\]\s*=", trainer_text))
    result["trainers"] = []
    for entry in symbols["trainers"]:
        name = entry["source_symbol"]
        records = []
        for index, match in enumerate(trainer_starts):
            if match.group(2) != name:
                continue
            end = trainer_starts[index + 1].start() if index + 1 < len(trainer_starts) else len(trainer_text)
            body = trainer_text[match.start():end].strip()
            records.append({"difficulty": match.group(1), "line": trainer_text.count("\n", 0, match.start()) + 1,
                            "record_sha256": api.sha(body.encode()),
                            "species": sorted(set(re.findall(r"\bSPECIES_\w+", body))),
                            "moves": sorted(set(re.findall(r"\bMOVE_\w+", body))),
                            "items": sorted(set(re.findall(r"\bITEM_\w+", body))),
                            "presentation": sorted(set(re.findall(r"\bTRAINER_(?:PIC|CLASS)_\w+", body)))})
        if not records:
            raise api.ManifestError(f"{entry['owners'][0]}: missing trainer record {name}")
        result["trainers"].append({"source_symbol": name, "source": trainer_path, "records": records})
    heal_path = "src/data/heal_locations.h"
    heal_text = api.read(donor, heal_path)
    sources.add(heal_path, "campaign healing and blackout destinations")
    result["heal_destinations"] = []
    for match in re.finditer(r"\[(HEAL_LOCATION_\w+)\s*-\s*1\]\s*=\s*\{MAP_GROUP\((\w+)\),\s*MAP_NUM\((\w+)\),\s*(\d+),\s*(\d+)\}", heal_text):
        name, group, number, x, y = match.groups()
        if group != number:
            raise api.ManifestError(f"{heal_path}: mismatched heal map {name}")
        if "MAP_" + group in selected_maps:
            result["heal_destinations"].append({"source_symbol": name, "map": "MAP_" + group,
                                                "x": int(x), "y": int(y)})
    heal_aux = "src/data/heal_locations_pkm_center.h"
    sources.add(heal_aux, "campaign healer reception tables")
    result["heal_reception_source"] = heal_aux
    # Follow the object graphics declaration graph, including frame/animation
    # tables, instead of assuming equal enum spellings mean equal graphics.
    record_index = {}
    for path in sorted((donor / "src/data/object_events").glob("*.h")):
        relative = path.relative_to(donor).as_posix()
        for name, body in api.record_blocks(api.read(donor, relative)).items():
            record_index[name] = (relative, body)
    pokemon_graphics = "src/data/graphics/pokemon.h"
    record_index.update({name: (pokemon_graphics, body)
                         for name, body in api.record_blocks(api.read(donor, pokemon_graphics)).items()})
    pointer_path = "src/data/object_events/object_event_graphics_info_pointers.h"
    pointer_text = api.read(donor, pointer_path)
    engine_objects_path = "src/event_object_movement.c"
    engine_objects = api.read(donor, engine_objects_path)
    for name, body in api.record_blocks(engine_objects).items():
        record_index.setdefault(name, (engine_objects_path, body))
    palette_index = {tag: name for name, tag in re.findall(r"\{(gObjectEventPal_\w+),\s*(OBJ_EVENT_PAL_TAG_\w+)\}", engine_objects)}
    object_map = dict(re.findall(r"\[(OBJ_EVENT_GFX_\w+)\]\s*=\s*&?(\w+)", pointer_text))
    graphics_expressions = defaultdict(set)
    for entry in maps:
        path = f"data/maps/{entry['source_name']}/map.json"
        for index, event in enumerate(json.loads(api.read(donor, path)).get("object_events", [])):
            graphics_expressions[event["graphics_id"]].add(f"{path}:object_events[{index}]")
    for block in script_blocks.values():
        for number, raw in block["lines"]:
            for expression in re.findall(r"\bOBJ_EVENT_GFX_\w+(?:\([^)]*\))?", api.STRING.sub("", raw.split("@", 1)[0])):
                graphics_expressions[expression].add(f"{block['path']}:{number}")
    result["graphics_expressions"] = [{"expression": expression, "owners": sorted(owners)}
                                      for expression, owners in sorted(graphics_expressions.items())]
    objects = sorted(name for name in by_symbol if name.startswith("OBJ_EVENT_GFX_"))
    queue = deque()
    result["object_graphics"] = []
    result["species_overworld_graphics"] = []
    host_species_path = "include/constants/species.h"
    host_graphics_path = "include/constants/event_objects.h"
    host_species_text = api.read(host, host_species_path)
    host_graphics_text = api.read(host, host_graphics_path)
    host_species = set(re.findall(r"(?m)^\s*(?:#define\s+)?(SPECIES_\w+)\s*(?:=|,|[ \t]+\S)", host_species_text))
    for expression, owners in sorted(graphics_expressions.items()):
        match = re.fullmatch(r"(OBJ_EVENT_GFX_SPECIES(?:_SHINY)?(?:_FEMALE)?)\((\w+)\)", expression)
        if not match:
            continue
        constructor, species_name = match.groups()
        species = "SPECIES_" + species_name
        if species not in constants or species not in host_species:
            raise api.ManifestError(f"{sorted(owners)[0]}: unknown shared-host species {species}")
        if not re.search(rf"#define\s+{constructor}\(", host_graphics_text):
            raise api.ManifestError(f"{sorted(owners)[0]}: missing host overworld constructor {constructor}")
        sources.add("include/constants/species.h", expression)
        sources.add("include/constants/event_objects.h", expression)
        result["species_overworld_graphics"].append({
            "expression": expression, "species": species, "target_expression": expression,
            "binding": "shared_host_roster_and_overworld_art",
            "source_definition": "include/constants/species.h",
            "host_species_definition": host_species_path,
            "host_species_header_sha256": api.sha((host / host_species_path).read_bytes()),
            "host_constructor_definition": host_graphics_path,
            "host_constructor_header_sha256": api.sha((host / host_graphics_path).read_bytes()),
            "owners": sorted(owners),
        })
    for name in objects:
        if name not in object_map:
            if name.startswith("OBJ_EVENT_GFX_SPECIES"):
                sources.add("include/constants/event_objects.h", name)
                result["object_graphics"].append({"source_symbol": name,
                                                 "binding": "species overworld graphics constructor",
                                                 "status": "shared_host_constructor_binding_recorded",
                                                 "owners": sorted(by_symbol[name])})
                continue
            # Dynamic graphics slots carry a runtime setter obligation.
            if name.startswith("OBJ_EVENT_GFX_VAR_"):
                result["object_graphics"].append({"source_symbol": name, "binding": "script-controlled graphics slot",
                                                 "owners": sorted(by_symbol[name]),
                                                 "status": "requires_U4_preserved_setter_expression"})
                continue
            raise api.ManifestError(f"{sorted(by_symbol[name])[0]}: missing object graphics {name}")
        record = object_map[name]
        queue.append(record)
        result["object_graphics"].append({"source_symbol": name, "target_symbol": "Cormoria_" + name,
                                         "record": record, "owners": sorted(by_symbol[name])})
    sources.add(pointer_path, "object graphics identity table")
    visited = set()
    while queue:
        name = queue.popleft()
        if name in visited:
            continue
        visited.add(name)
        if name not in record_index:
            raise api.ManifestError(f"object graphics: missing record {name}")
        path, body = record_index[name]
        sources.scan_assets(path, name, body)
        queue.extend(sorted(set(api.TOKEN.findall(body)) & record_index.keys() - {name}))
        queue.extend(palette_index[token] for token in api.TOKEN.findall(body) if token in palette_index)
    result["object_graphics_records"] = sorted(visited)
    # Song symbols resolve to a pinned source song plus MIDI conversion options.
    sources.add("sound/song_table.inc", "music and sound identity table")
    midi_config = "sound/songs/midi/midi.cfg"
    config = api.read(donor, midi_config)
    sources.add(midi_config, "song conversion/voicegroup contracts")
    songs = sorted(name for name in by_symbol if name.startswith(("MUS_", "SE_")))
    song_table = set(re.findall(r"(?m)^\s*song\s+(\w+)", api.read(donor, "sound/song_table.inc")))
    result["audio"] = []
    audio_roots = set()
    for name in songs:
        song = name.lower()
        if song not in song_table:
            if name in {"MUS_NONE", "MUS_DUMMY"}:
                result["audio"].append({"source_symbol": name, "binding": "no_song_sentinel"})
                continue
            raise api.ManifestError(f"{sorted(by_symbol[name])[0]}: missing song {name}")
        candidates = [f"sound/songs/{song}.s", f"sound/songs/midi/{song}.mid"]
        found = [path for path in candidates if (donor / path).is_file()]
        if len(found) != 1:
            raise api.ManifestError(f"audio {name}: expected one source, found {found}")
        path = found[0]
        sources.add(path, name)
        options = None
        if path.endswith(".mid"):
            match = re.search(rf"(?m)^{re.escape(song)}\.mid:\s*(.*)$", config)
            if not match:
                raise api.ManifestError(f"{midi_config}: missing MIDI conversion for {song}")
            options = match.group(1).strip()
            bank = re.search(r"-G(\d+)", options)
            if bank:
                audio_roots.add(f"voicegroup{int(bank.group(1)):03d}")
        else:
            audio_roots.update(re.findall(r"\bvoicegroup\w+", api.read(donor, path)))
        result["audio"].append({"source_symbol": name, "target_symbol": "Cormoria_" + name,
                                "source": path, "midi_options": options})
    sound_records = {}
    for path in sorted((donor / "sound").rglob("*.inc")):
        relative = path.relative_to(donor).as_posix()
        current = None
        for raw in api.read(donor, relative).splitlines():
            label = api.LABEL.match(raw)
            if label:
                current = label.group(1)
                if current in sound_records:
                    raise api.ManifestError(f"{relative}: duplicate sound label {current}")
                sound_records[current] = [relative, []]
            elif current:
                sound_records[current][1].append(raw)
    queue = deque(sorted(audio_roots))
    visited_audio = set()
    while queue:
        name = queue.popleft()
        if name in visited_audio:
            continue
        visited_audio.add(name)
        if name not in sound_records:
            raise api.ManifestError(f"audio: missing voice/sample label {name}")
        path, lines = sound_records[name]
        body = "\n".join(lines)
        sources.scan_assets(path, name, body)
        for token in api.TOKEN.findall(api.STRING.sub("", body)):
            if token in sound_records:
                queue.append(token)
            elif token.startswith(("voicegroup", "DirectSoundWaveData_", "ProgrammableWaveData_", "KeySplitTable_")):
                raise api.ManifestError(f"{path}: missing voice/sample dependency {token}")
    result["audio_dependency_labels"] = sorted(visited_audio)
    # Record includes needed to review native source translation units; headers
    # do not become runtime ports simply by appearing in this evidence graph.
    queue = deque(entry["definition"]["path"] for entry in symbols["native_bindings"])
    included = set()
    while queue:
        path = queue.popleft()
        if path in included:
            continue
        included.add(path)
        text = api.read(donor, path)
        # This is declaration/provenance evidence across a semantic boundary.
        # Expanding every engine header into assets would import the entire
        # donor species database, including disabled/generated configurations.
        # Actual native implementation assets are scanned at their binding;
        # selected object/tileset/song records have separate closed graphs.
        sources.add(path, "native translation-unit evidence")
        for include in re.findall(r'^\s*#include\s+"([^"]+)"', text, re.M):
            candidates = [str(Path(path).parent / include).replace("\\", "/"), "include/" + include]
            target = next((candidate for candidate in candidates if (donor / candidate).is_file()), None)
            if target is None:
                generated = {"constants/layouts.h": "data/layouts/layouts.json",
                             "layouts.h": "data/layouts/layouts.json",
                             "constants/map_groups.h": "data/maps/map_groups.json",
                             "map_groups.h": "data/maps/map_groups.json",
                             "data/wild_encounters.h": "src/data/wild_encounters.json",
                             "data/region_map/region_map_entries.h": "src/data/region_map/region_map_sections.json"}
                if include in generated:
                    sources.add(generated[include], f"{path}: generated {include}")
                    continue
                raise api.ManifestError(f"{path}: missing native include {include}")
            queue.append(target)
    result["native_include_evidence"] = sorted(included)
    return result


def resource_audit(donor: Path, host: Path, sources: Any, api: Any) -> dict[str, Any]:
    paths = sorted({entry["source"] for entry in sources.assets.values() if "source" in entry})
    donor_records = {path: (api.sha((donor / path).read_bytes()), (donor / path).stat().st_size)
                     for path in paths}
    sizes = {size for _, size in donor_records.values()}
    digests = {digest for digest, _ in donor_records.values()}
    host_matches: dict[str, list[str]] = {}
    # Git provides baseline blob sizes in one read. Walking/statting every file
    # in the expanded host graphics tree is both slower and nondeterministic
    # once U4 adds files. Hash only size-compatible, pinned baseline candidates.
    tree = api.git(host, "ls-tree", "-r", "-l", api.HOST_REVISION, "--", "graphics", "sound", "data/layouts", "data/tilesets")
    for row in tree.splitlines():
        metadata, relative = row.split("\t", 1)
        fields = metadata.split()
        if fields[1] == "blob" and int(fields[3]) in sizes:
            path = host / relative
            if path.is_file():
                digest = api.sha(path.read_bytes())
                if digest in digests:
                    host_matches.setdefault(digest, []).append(relative)
    entries = [{"source": path, "bytes": size, "sha256": digest,
                "identical_host_sources": host_matches.get(digest, [])}
               for path, (digest, size) in donor_records.items()]
    unique = {digest: size for digest, size in donor_records.values()}
    return {"measurement": "source file bytes; not converted/compressed/linker ROM size",
            "source_asset_count": len(entries), "source_asset_bytes": sum(entry["bytes"] for entry in entries),
            "unique_source_bytes": sum(unique.values()),
            "exact_host_duplicate_unique_bytes": sum(size for digest, size in unique.items() if digest in host_matches),
            "remaining_unique_source_bytes": sum(size for digest, size in unique.items() if digest not in host_matches),
            "entries": entries}


def coop_audit(host: Path, symbols: dict[str, Any], heals: list[dict[str, Any]], api: Any) -> dict[str, Any]:
    registry = json.loads(api.git(host, "show", f"{api.HOST_REVISION}:data/coop/regional_identities.json"))
    additions = {"trainer": len({entry["target_id"] for entry in symbols["trainers"]}),
                 "event": len({entry["target_id"] for entry in symbols["flags"]}),
                 "fly": len(heals), "gym": 8}
    capacity_keys = {"trainer": "trainers", "event": "events", "fly": "fly_points", "gym": "gyms"}
    entries = []
    for kind, additional in additions.items():
        current = [entry["ordinal"] for entry in registry["identities"] if entry["kind"] == kind]
        start = max(current, default=-1) + 1
        capacity = registry["capacities"][capacity_keys[kind]]
        if start + additional > capacity:
            raise api.ManifestError(f"co-op {kind}: capacity {capacity} exhausted by {start}+{additional}")
        entries.append({"kind": kind, "existing_used": len(current), "proposed_start": start,
                        "reserved_count": additional, "capacity": capacity,
                        "status": "U3_must_select_semantic_events_and_publish_registry"})
    return {"baseline_revision": api.HOST_REVISION,
            "policy": "conservative reservation: all ledger flags and campaign heal destinations; no runtime registry mutation",
            "reservations": entries}
