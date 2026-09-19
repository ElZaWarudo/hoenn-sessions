import copy
import importlib.util
import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")
SPEC = importlib.util.spec_from_file_location("johto_world", ROOT / "tools/johto/import_world.py")
world = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(world)


def load(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def script_block(raw: bytes, label: str) -> bytes:
    marker = (label + "::").encode("ascii")
    start = raw.index(marker)
    following = re.search(rb"(?m)^[A-Za-z_][A-Za-z0-9_]*::", raw[start + len(marker):])
    end = len(raw) if following is None else start + len(marker) + following.start()
    return raw[start:end]


class JohtoWorldPlannerTest(unittest.TestCase):
    def setUp(self):
        self.manifest = load(ROOT / world.MANIFEST_PATH)
        self.scenery = load(ROOT / world.SCENERY_PATH)
        self.scenery_raw = (ROOT / world.SCENERY_PATH).read_bytes()
        self.content = load(ROOT / world.CONTENT_PATH)
        self.content_raw = (ROOT / world.CONTENT_PATH).read_bytes()
        self.asset_manifest = load(ROOT / world.ASSET_MANIFEST_PATH)
        self.asset_manifest_raw = (ROOT / world.ASSET_MANIFEST_PATH).read_bytes()

    def _temporary_ledgers(
        self,
        mutate_manifest=None,
        mutate_scenery=None,
        mutate_content=None,
        mutate_asset=None,
        mutate_manifest_bytes=None,
        mutate_asset_bytes=None,
        mutate_script_bytes=None,
    ):
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        manifest = copy.deepcopy(self.manifest)
        scenery = copy.deepcopy(self.scenery)
        content = copy.deepcopy(self.content)
        asset_manifest = copy.deepcopy(self.asset_manifest)
        if mutate_manifest:
            mutate_manifest(manifest)
        if mutate_scenery:
            mutate_scenery(scenery)
        if mutate_content:
            mutate_content(content)
        if mutate_asset:
            mutate_asset(asset_manifest)
        manifest_path = root / world.MANIFEST_PATH
        scenery_path = root / world.SCENERY_PATH
        content_path = root / world.CONTENT_PATH
        asset_path = root / world.ASSET_MANIFEST_PATH
        script_path = root / world.CAMPAIGN_SCRIPTS_PATH
        for path in (manifest_path, scenery_path, content_path, asset_path, script_path):
            path.parent.mkdir(parents=True, exist_ok=True)
        manifest_raw = (json.dumps(manifest, indent=2) + "\n").replace("\n", "\r\n").encode("utf-8")
        if mutate_manifest_bytes:
            manifest_raw = mutate_manifest_bytes(manifest_raw)
        manifest_path.write_bytes(manifest_raw)
        if mutate_scenery:
            scenery_path.write_text(json.dumps(scenery), encoding="utf-8")
        else:
            scenery_path.write_bytes(self.scenery_raw)
        if mutate_content:
            content_path.write_text(json.dumps(content), encoding="utf-8")
        else:
            content_path.write_bytes(self.content_raw)
        asset_raw = self.asset_manifest_raw
        if mutate_asset:
            asset_raw = (json.dumps(asset_manifest, indent=2) + "\n").replace("\n", "\r\n").encode("utf-8")
        if mutate_asset_bytes:
            asset_raw = mutate_asset_bytes(asset_raw)
        asset_path.write_bytes(asset_raw)
        script_raw = (ROOT / world.CAMPAIGN_SCRIPTS_PATH).read_bytes()
        if mutate_script_bytes:
            script_raw = mutate_script_bytes(script_raw)
        script_path.write_bytes(script_raw)
        return temporary, root

    def _assert_refusal_without_mutation(self, root, reason):
        before = {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()}
        with self.assertRaisesRegex(world.WorldPlanError, reason):
            world.build_plan(root, DONOR)
        after = {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()}
        self.assertEqual(after, before)

    def _minimal_registration_root(self):
        """Copy only text/JSON inputs needed before registration materializes maps."""
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        relatives = (
            world.MANIFEST_PATH,
            world.SCENERY_PATH,
            world.CONTENT_PATH,
            world.ASSET_MANIFEST_PATH,
            world.CAMPAIGN_SCRIPTS_PATH,
            world.HOST_IDENTITY_BASELINE_PATH,
            world.MAP_GROUPS_PATH,
            world.EVENT_SCRIPTS_PATH,
        )
        for relative in relatives:
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, target)
        for directory in (ROOT / "data/scripts", ROOT / "data/text"):
            for source in directory.rglob("*.inc"):
                relative = source.relative_to(ROOT)
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
        for source in (ROOT / "data/maps").rglob("scripts.inc"):
            relative = source.relative_to(ROOT)
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
        return temporary, root

    def test_report_matches_independent_donor_and_manifest_counts(self):
        plan = world.build_plan(ROOT, DONOR)
        maps = self.manifest["maps"]
        self.assertEqual(Counter(item["proposed_host"]["group"] for item in maps), {75: 128, 76: 111, 77: 128, 78: 40})
        event_totals = Counter()
        donor_paths = [f"data/maps/{item['source_name']}/map.json" for item in maps]
        donor_blobs = world._git_blobs(DONOR, donor_paths)
        for item in maps:
            relative = f"data/maps/{item['source_name']}/map.json"
            source = world._git_json(donor_blobs[relative], relative)
            for field in ("object_events", "warp_events", "coord_events", "bg_events"):
                self.assertIsInstance(source[field], list)
                self.assertLessEqual(len(source[field]), 255)
                event_totals[field] += len(source[field])
        selected_warps = sum(
            edge["classification"] == "selected"
            for item in maps
            for edge in item["warp_targets"]
        )
        selected_connections = sum(
            edge["kind"] == "connection" and edge["classification"] == "selected"
            for item in maps
            for edge in item["edges"]
        )
        self.assertEqual(dict(event_totals), plan["source_event_totals"])
        self.assertEqual(
            plan["event_totals"],
            {"object_events": 3358, "warp_events": 1174, "coord_events": 408, "bg_events": 758},
        )
        self.assertEqual(
            plan["event_totals"]["warp_events"],
            sum(len(item["warp_targets"]) for item in plan["maps"]),
        )
        self.assertEqual((selected_warps, selected_connections), (1172, 191))
        self.assertEqual(plan["selected_map_count"], 407)
        self.assertEqual(plan["host_adapter_map_count"], 2)
        self.assertEqual(plan["production_map_count"], 409)
        self.assertEqual((plan["original_map_count"], plan["later_map_count"]), (239, 168))

    def test_normalized_topology_repairs_invalid_warps_and_removes_debug_edges(self):
        plan = world.build_plan(ROOT, DONOR)
        by_source = {item["source_map"]: item for item in plan["maps"]}

        def warp(source_map, index):
            return by_source[source_map]["warp_targets"][index]

        self.assertEqual(warp("MAP_SSAQUA_1F", 0)["dest_warp_id"], "0")
        self.assertEqual(
            (warp("MAP_FUCHSIA_ROUTE19GATE", 0)["dest_map"], warp("MAP_FUCHSIA_ROUTE19GATE", 0)["dest_warp_id"]),
            ("MAP_FUCHSIA_CITY", "2"),
        )
        self.assertEqual(warp("MAP_FUCHSIA_ROUTE19GATE", 1)["dest_warp_id"], "0")
        self.assertEqual(warp("MAP_VIRIDIAN_CITY", 2)["target_map_id"], "MAP_KANTO_LATER_VIRIDIAN_CITY_HOUSE2")
        self.assertEqual(warp("MAP_VIRIDIAN_CITY", 2)["dest_warp_id"], "0")
        self.assertEqual(warp("MAP_NEW_BARK_TOWN_LAB", 1)["dest_warp_id"], "5")
        self.assertEqual(warp("MAP_CINNABAR_ISLAND_POKEMON_CENTER", 0)["dest_warp_id"], "0")
        self.assertEqual(len(by_source["MAP_NEW_BARK_TOWN"]["warp_targets"]), 6)
        self.assertEqual(len(by_source["MAP_ECRUTEAK_CITY"]["warp_targets"]), 14)
        self.assertEqual(len(by_source["MAP_ROUTE40"]["warp_targets"]), 9)
        self.assertEqual(len(by_source["MAP_CINNABAR_ISLAND"]["warp_targets"]), 1)
        self.assertEqual(len(by_source["MAP_ROUTE40"]["connections"]), 2)
        self.assertEqual(len(by_source["MAP_VERMILION_CITY"]["connections"]), 3)
        self.assertEqual(len(by_source["MAP_ROUTE22"]["warp_targets"]), 0)
        self.assertEqual(
            [connection["map"] for connection in by_source["MAP_ROUTE22"]["connections"]],
            ["MAP_VIRIDIAN_CITY"],
        )
        self.assertEqual(
            [connection["map"] for connection in by_source["MAP_ROUTE26NORTH"]["connections"]],
            ["MAP_ROUTE26", "MAP_ROUTE28"],
        )
        reception = warp("MAP_RECEPTION_GATE", 4)
        self.assertEqual(
            (reception["dest_map"], reception["dest_warp_id"], reception["classification"], reception["target_map_id"]),
            ("MAP_DYNAMIC", "WARP_ID_DYNAMIC", "required_host_adapter", None),
        )
        self.assertEqual(
            plan["external_edges"],
            [
                {
                    "source_map": "MAP_GOLDENROD_CITY_DEPARTMENT_STORE_ELEVATOR",
                    "kind": "warp",
                    "index": 0,
                    "target": "MAP_DYNAMIC",
                    "classification": "required_host_adapter",
                },
                {
                    "source_map": "MAP_RECEPTION_GATE",
                    "kind": "warp",
                    "index": 4,
                    "target": "MAP_DYNAMIC",
                    "classification": "required_host_adapter",
                },
            ],
        )
        elevator = by_source["MAP_GOLDENROD_CITY_DEPARTMENT_STORE_ELEVATOR"]["edges"][0]
        self.assertEqual(
            (elevator["target"], elevator["warp_id"], elevator["classification"]),
            ("MAP_DYNAMIC", "WARP_ID_DYNAMIC", "required_host_adapter"),
        )
        self.assertEqual(plan["external_edge_counts"], {"excluded_debug": 0, "host_adapter": 2, "runtime_policy": 0, "era_boundary": 0})
        self.assertEqual(plan["topology_notes"][0]["status"], "none")

    def test_normalized_topology_keeps_warp_and_edge_views_identical(self):
        plan = world.build_plan(ROOT, DONOR)
        for record in plan["maps"]:
            world._cross_validate_world_representations(record)
        self.assertEqual(plan["selected_warp_count"], 1172)
        self.assertEqual(plan["selected_connection_count"], 189)

    def test_reviewed_external_script_destinations_are_preserved_exactly(self):
        plan = world.build_plan(ROOT, DONOR)
        expected = world._validate_preserved_external_script_destinations(ROOT)
        self.assertEqual(len(expected), 8)
        self.assertEqual(plan["preserved_external_script_destinations"], expected)

        temporary, root = self._temporary_ledgers(
            mutate_script_bytes=lambda raw: raw.replace(
                b"MAP_BIRTH_ISLAND_EXTERIOR, 13, 23",
                b"MAP_DYNAMIC, 13, 23",
                1,
            )
        )
        with temporary:
            self._assert_refusal_without_mutation(root, "preserved external script destination drift")

    def test_duplicate_topology_representations_fail_closed(self):
        record = copy.deepcopy(next(item for item in self.manifest["maps"] if item["source_map"] == "MAP_ROUTE40"))
        source_path = f"data/maps/{record['source_name']}/map.json"
        source = world._git_json(world._git_blob(DONOR, source_path), source_path)
        mutations = {
            "warp-target": lambda data: data["warp_targets"][0].__setitem__("dest_map", "MAP_DYNAMIC"),
            "warp-edge": lambda data: next(e for e in data["edges"] if e["kind"] == "warp" and e["index"] == 0).__setitem__("target", "MAP_DYNAMIC"),
            "connection-edge": lambda data: next(e for e in data["edges"] if e["kind"] == "connection" and e["edge_index"] == 2).__setitem__("offset", 999),
            "connection-classification": lambda data: next(e for e in data["edges"] if e["kind"] == "connection" and e["edge_index"] == 0).__setitem__("classification", "excluded_debug_edge"),
            "connection-target-map-id": lambda data: next(e for e in data["edges"] if e["kind"] == "connection" and e["edge_index"] == 0).__setitem__("target_map_id", "MAP_DYNAMIC"),
            "duplicate-edge": lambda data: data["edges"].append(copy.deepcopy(data["edges"][0])),
        }
        reasons = {
            "warp-target": "warp_targets\\[0\\] differs from donor",
            "warp-edge": "warp edge 0 differs from warp_targets",
            "connection-edge": "connection edge 2 differs from connections",
            "connection-classification": "connection edge 0 classification differs from selected-map identity",
            "connection-target-map-id": "connection edge 0 target_map_id differs from selected-map identity",
            "duplicate-edge": "edges do not cover warp_targets and connections exactly",
        }
        target_map_ids = {
            item["source_map"]: item["identity_namespace"]["map"]
            for item in self.manifest["maps"]
        }
        for label, mutation in mutations.items():
            with self.subTest(label=label):
                mutated = copy.deepcopy(record)
                mutation(mutated)
                with self.assertRaisesRegex(world.WorldPlanError, reasons[label]):
                    world._cross_validate_world_representations(mutated, source, target_map_ids)

    def test_companion_ledgers_seal_all_semantic_fields(self):
        mutations = {
            "scenery-layout": {
                "mutate_scenery": lambda data: data["layouts"][0].__setitem__("target_layout_id", "LAYOUT_ALTERED")
            },
            "content-source": {
                "mutate_content": lambda data: data["source_files"][0].__setitem__("sha256", "0" * 64)
            },
        }
        for label, arguments in mutations.items():
            with self.subTest(label=label):
                temporary, root = self._temporary_ledgers(**arguments)
                with temporary:
                    self._assert_refusal_without_mutation(root, "semantics differ from the sealed digest")

    def test_reordered_and_wrong_era_identities_fail_with_targeted_reasons(self):
        mutations = {
            "reordered": (
                lambda data: data["maps"].__setitem__(slice(0, 2), [data["maps"][1], data["maps"][0]]),
                "map ordinals must be unique, ordered",
            ),
            "wrong-era": (lambda data: data["maps"][239].__setitem__("era", "JOHTO"), "wrong era identity"),
        }
        for label, (mutation, reason) in mutations.items():
            with self.subTest(label=label):
                temporary, root = self._temporary_ledgers(mutate_manifest=mutation)
                with temporary:
                    self._assert_refusal_without_mutation(root, reason)

    def test_duplicate_donor_identities_fail_before_target_aliases_can_hide_them(self):
        mutations = {
            "source-map": (
                lambda data: data["maps"][1].__setitem__("source_map", data["maps"][0]["source_map"]),
                "duplicate source_map identity at ordinal 1",
            ),
            "source-name": (
                lambda data: data["maps"][1].__setitem__("source_name", data["maps"][0]["source_name"]),
                "duplicate source_name identity at ordinal 1",
            ),
        }
        for label, (mutation, reason) in mutations.items():
            with self.subTest(label=label):
                temporary, root = self._temporary_ledgers(mutate_manifest=mutation)
                with temporary:
                    self._assert_refusal_without_mutation(root, reason)

    def test_ordinals_reject_booleans_in_region_and_asset_inventories(self):
        temporary, root = self._temporary_ledgers(
            mutate_manifest=lambda data: data["maps"][0].__setitem__("ordinal", False)
        )
        with temporary:
            self._assert_refusal_without_mutation(root, "map ordinal at index 0 must be an integer")
        temporary, root = self._temporary_ledgers(
            mutate_asset=lambda data: data["layouts"][1].__setitem__("ordinal", True)
        )
        with temporary:
            mutated = json.loads((root / world.ASSET_MANIFEST_PATH).read_bytes())
            mutated_digest = world._canonical_json_sha256(mutated)
            with mock.patch.object(world, "EXPECTED_ASSET_MANIFEST_SHA256", mutated_digest):
                self._assert_refusal_without_mutation(root, "asset layout ordinal at index 1 must be an integer")

    def test_region_manifest_is_bound_to_fixed_asset_manifest_anchor(self):
        def alternate_target_alias(data):
            data["maps"][0]["identity_namespace"]["map"] = "MAP_JOHTO_NEW_BARK_TOWN"
            data["maps"][0]["identity_namespace"]["layout"] = "LAYOUT_JOHTO_NEW_BARK_TOWN"

        temporary, root = self._temporary_ledgers(
            mutate_manifest=alternate_target_alias,
        )
        with temporary:
            self._assert_refusal_without_mutation(root, "scenery identity mismatch")

    def test_complete_asset_manifest_semantics_are_sealed_before_fields_are_trusted(self):
        mutations = {
            "source-hash": lambda data: data["layouts"][0]["assets"][0].__setitem__("source_sha256", "0" * 64),
            "dimensions": lambda data: data["layouts"][0].__setitem__("width", data["layouts"][0]["width"] + 1),
            "tileset": lambda data: data["layouts"][0].__setitem__("primary_tileset", "gTileset_Altered"),
            "extra-key": lambda data: data.__setitem__("unexpected", True),
            "extra-record": lambda data: data["layouts"].append(copy.deepcopy(data["layouts"][-1])),
        }
        for label, mutation in mutations.items():
            with self.subTest(label=label):
                temporary, root = self._temporary_ledgers(mutate_asset=mutation)
                with temporary:
                    self._assert_refusal_without_mutation(root, "asset manifest semantics differ from the sealed digest")

    def test_whole_document_anchors_ignore_json_formatting_only(self):
        def encodings(document):
            reordered = dict(reversed(list(document.items())))
            return {
                "lf": (json.dumps(document, indent=2) + "\n").encode("utf-8"),
                "crlf": (json.dumps(document, indent=2) + "\n").replace("\n", "\r\n").encode("utf-8"),
                "cr-only": (json.dumps(document, indent=2) + "\n").replace("\n", "\r").encode("utf-8"),
                "no-final-newline": json.dumps(document, indent=2).encode("utf-8"),
                "whitespace-and-key-order": json.dumps(reordered, separators=(", ", ": ")).encode("utf-8"),
            }

        for document_name, document, argument in (
            ("asset", self.asset_manifest, "mutate_asset_bytes"),
            ("region", self.manifest, "mutate_manifest_bytes"),
        ):
            for encoding, raw in encodings(document).items():
                with self.subTest(document=document_name, encoding=encoding):
                    temporary, root = self._temporary_ledgers(**{argument: lambda _raw, value=raw: value})
                    with temporary:
                        companions_before = {
                            path: (root / path).read_bytes()
                            for path in (world.SCENERY_PATH, world.CONTENT_PATH)
                        }
                        world.build_plan(root, DONOR)
                        self.assertEqual(
                            companions_before,
                            {path: (root / path).read_bytes() for path in companions_before},
                        )

    def test_companion_recorded_manifest_digest_mutation_is_rejected(self):
        mutations = (
            (
                "scenery",
                {"mutate_scenery": lambda data: data["provenance"].__setitem__("region_manifest_sha256", "0" * 64)},
                "scenery ledger semantics differ from the sealed digest",
            ),
            (
                "content",
                {"mutate_content": lambda data: data["provenance"].__setitem__("manifest_sha256", "0" * 64)},
                "content-symbol ledger semantics differ from the sealed digest",
            ),
            (
                "scenery-asset",
                {"mutate_scenery": lambda data: data["provenance"].__setitem__("asset_manifest_sha256", "0" * 64)},
                "scenery ledger semantics differ from the sealed digest",
            ),
        )
        for label, arguments, reason in mutations:
            with self.subTest(label=label):
                temporary, root = self._temporary_ledgers(**arguments)
                with temporary:
                    self._assert_refusal_without_mutation(root, reason)

    def test_whole_document_anchors_reject_duplicate_keys_and_malformed_json_without_mutation(self):
        cases = (
            (
                "asset-duplicate",
                {"mutate_asset_bytes": lambda raw: raw.replace(b"{", b'{"duplicate":null,"duplicate":null,', 1)},
                "duplicate JSON key: duplicate",
            ),
            (
                "region-duplicate",
                {"mutate_manifest_bytes": lambda raw: raw.replace(b"{", b'{"duplicate":null,"duplicate":null,', 1)},
                "duplicate JSON key: duplicate",
            ),
            ("asset-malformed", {"mutate_asset_bytes": lambda raw: raw.rstrip()[:-1]}, "not valid JSON"),
            ("region-malformed", {"mutate_manifest_bytes": lambda raw: raw.rstrip()[:-1]}, "not valid JSON"),
        )
        for label, arguments, reason in cases:
            with self.subTest(label=label):
                temporary, root = self._temporary_ledgers(**arguments)
                with temporary:
                    self._assert_refusal_without_mutation(root, reason)

    def test_region_manifest_semantic_mutation_reaches_fixed_canonical_anchor(self):
        temporary, root = self._temporary_ledgers(
            mutate_manifest=lambda data: data.__setitem__("unexpected_semantic_field", True)
        )
        with temporary:
            self._assert_refusal_without_mutation(root, "sealed asset-manifest anchor")

    def test_malformed_groups_sections_and_event_counts_are_rejected(self):
        mutations = {
            "boolean-group": lambda data: data["maps"][0]["proposed_host"].__setitem__("group", True),
            "section-overflow": lambda data: data["maps"][0]["resolved_section"].__setitem__("id", 256),
            "section-boolean": lambda data: data["maps"][0]["resolved_section"].__setitem__("id", False),
        }
        for label, mutation in mutations.items():
            with self.subTest(label=label):
                temporary, root = self._temporary_ledgers(mutate_manifest=mutation)
                with temporary, self.assertRaises(world.WorldPlanError):
                    world.build_plan(root, DONOR)
        source = {field: [] for field in world.EVENT_FIELDS}
        source["warp_events"] = {}
        with self.assertRaisesRegex(world.WorldPlanError, "must be an array"):
            world._validate_event_arrays(source, "Malformed")
        source["warp_events"] = [{}] * 256
        with self.assertRaisesRegex(world.WorldPlanError, "0..255"):
            world._validate_event_arrays(source, "Overflow")

    def test_external_edges_have_exact_source_backed_identities(self):
        plan = world.build_plan(ROOT, DONOR)
        expected = [
            {
                "source_map": edge[0],
                "kind": edge[1],
                "index": edge[2],
                "target": edge[3],
                "classification": edge[4],
            }
            for edge in world.EXPECTED_EXTERNAL_EDGES
        ]
        self.assertEqual(plan["external_edges"], expected)
        self.assertEqual(
            plan["external_edge_counts"],
            {"excluded_debug": 0, "host_adapter": 2, "runtime_policy": 0, "era_boundary": 0},
        )
        for edge in self.manifest["external_edges"]:
            source_name = next(
                item["source_name"] for item in self.manifest["maps"] if item["source_map"] == edge["source_map"]
            )
            relative = f"data/maps/{source_name}/map.json"
            source = world._git_json(world._git_blob(DONOR, relative), relative)
            records = source["warp_events" if edge["kind"] == "warp" else "connections"]
            record = records[edge.get("index", edge.get("edge_index"))]
            self.assertEqual(record["dest_map" if edge["kind"] == "warp" else "map"], edge["target"])

    def test_campaign_label_inventory_includes_prelude_outside_source_blocks(self):
        raw = b"CampaignPreludeOnly::\n" + (ROOT / world.CAMPAIGN_SCRIPTS_PATH).read_bytes()
        blocks, labels = world._campaign_blocks(raw)
        self.assertEqual(len(blocks), 407)
        self.assertIn("CampaignPreludeOnly", labels)

    def test_host_label_inventory_uses_only_committed_reachable_include_closure(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "World Planner Test"], cwd=repo, check=True)
            event_scripts = repo / world.EVENT_SCRIPTS_PATH
            reachable = repo / "data/scripts/reachable.inc"
            unreachable = repo / "data/scripts/unreachable.inc"
            event_scripts.parent.mkdir(parents=True)
            reachable.parent.mkdir(parents=True)
            event_scripts.write_text('.include "data/scripts/reachable.inc"\nRootLabel::\n', encoding="utf-8")
            reachable.write_text("CommittedReachable::\n", encoding="utf-8")
            unreachable.write_text("CommittedUnreachable::\n", encoding="utf-8")
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-qm", "fixture"], cwd=repo, check=True)
            reachable.write_text("DirtyReachable::\n", encoding="utf-8")
            (repo / "data/scripts/untracked.inc").write_text("UntrackedLabel::\n", encoding="utf-8")

            labels = world._assembly_labels(repo)
            self.assertEqual(labels, {"RootLabel", "CommittedReachable"})
            for rejected in ("DirtyReachable", "CommittedUnreachable", "UntrackedLabel"):
                with self.subTest(rejected=rejected), self.assertRaisesRegex(
                    world.WorldPlanError, "event script label is unresolved"
                ):
                    world._rewrite_event_script(rejected, "Fixture", set(), {}, labels, "MAP_FIXTURE")

    def test_event_script_assembly_replaces_legacy_map_include_once(self):
        rendered = world._render_event_script_assembly(ROOT)
        self.assertEqual(rendered.count(world.LEGACY_NEW_BARK_INCLUDE), 0)
        self.assertEqual(rendered.count(world.CAMPAIGN_INCLUDE), 1)

    def test_event_script_assembly_preserves_authenticated_runtime_prefix(self):
        current = (ROOT / world.EVENT_SCRIPTS_PATH).read_bytes().replace(b"\r\n", b"\n")
        injected = current.replace(
            world.CAMPAIGN_INCLUDE,
            b"J15_RuntimeSelector_Preserved::\n\tend\n" + world.CAMPAIGN_INCLUDE,
            1,
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / world.EVENT_SCRIPTS_PATH
            target.parent.mkdir(parents=True)
            target.write_bytes(injected)
            rendered = world._render_event_script_assembly(root)
            self.assertIn(b"J15_RuntimeSelector_Preserved::", rendered)
            self.assertEqual(rendered.count(world.REGISTERED_ADAPTER_MARKER), 1)
            self.assertIn(b"Johto_ReceptionGate_Overland_MapScripts::", rendered)

            target.write_bytes(rendered)
            self.assertEqual(world._render_event_script_assembly(root), rendered)

            target.write_bytes(injected.replace(b"Sail back to JOHTO?", b"Sail elsewhere now?", 1))
            with self.assertRaisesRegex(world.WorldPlanError, "cannot authenticate pinned"):
                world._render_event_script_assembly(root)

    def test_registration_outputs_use_one_aggregate_campaign_and_remove_debug_events(self):
        outputs = world._registration_outputs(ROOT, DONOR, world.build_plan(ROOT, DONOR))
        self.assertEqual(len(outputs), 414)
        self.assertFalse(any(path.name == "scripts.inc" for path in outputs))
        event_scripts = outputs[world.EVENT_SCRIPTS_PATH]
        self.assertEqual(event_scripts.count(world.LEGACY_NEW_BARK_INCLUDE), 0)
        self.assertEqual(event_scripts.count(world.CAMPAIGN_INCLUDE), 1)
        goldenrod = json.loads(outputs[Path("data/maps/GoldenrodCity_House1/map.json")])
        self.assertEqual(len(goldenrod["bg_events"]), 1)
        self.assertNotIn("ToggleShinies", json.dumps(goldenrod))
        stored_plan = json.loads(outputs[world.WORLD_PLAN_PATH])
        self.assertTrue(stored_plan["production_write_ready"])
        self.assertEqual(stored_plan["blockers"], [])

    def test_original_kanto_transport_adapters_are_append_only_and_reachable(self):
        plan = world.build_plan(ROOT, DONOR)
        outputs = world._registration_outputs(ROOT, DONOR, plan)
        groups = json.loads(outputs[world.MAP_GROUPS_PATH])
        vermilion_adapter, saffron_adapter = world.HOST_ADAPTER_MAPS

        self.assertEqual(plan["selected_map_count"], 407)
        self.assertEqual(plan["host_adapter_map_count"], 2)
        self.assertEqual(len(plan["maps"]), 407)
        self.assertEqual(groups[vermilion_adapter["group"]][vermilion_adapter["index"]], vermilion_adapter["name"])
        self.assertEqual(groups[saffron_adapter["group"]][saffron_adapter["index"]], saffron_adapter["name"])
        baseline = json.loads((ROOT / world.HOST_IDENTITY_BASELINE_PATH).read_text(encoding="utf-8"))
        for adapter in world.HOST_ADAPTER_MAPS:
            self.assertEqual(
                groups[adapter["group"]][:-1],
                [record["name"] for record in baseline["groups"][adapter["group"]]],
            )

        vermilion = json.loads(outputs[Path("data/maps/KantoOriginal_VermilionCity_PortInside/map.json")])
        self.assertEqual(vermilion["id"], "MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE")
        self.assertEqual(vermilion["layout"], "LAYOUT_KANTO_LATER_VERMILION_CITY_PORT_INSIDE")
        self.assertEqual(
            [(event["x"], event["y"], event["graphics_id"]) for event in vermilion["object_events"]],
            [(8, 10, "OBJ_EVENT_GFX_SAILOR"), (8, 13, "OBJ_EVENT_GFX_SS_TIDAL")],
        )
        self.assertEqual(
            vermilion["warp_events"],
            [{"x": 8, "y": 2, "elevation": 0, "dest_map": "MAP_VERMILION_CITY", "dest_warp_id": "0"}],
        )

        station = json.loads(outputs[Path("data/maps/KantoOriginal_SaffronCity_TrainStation/map.json")])
        self.assertEqual(station["id"], "MAP_KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION")
        self.assertEqual(station["layout"], "LAYOUT_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION")
        self.assertEqual(
            [(event["x"], event["y"], event["graphics_id"]) for event in station["object_events"]],
            [(137, 20, "OBJ_EVENT_GFX_POLICEMAN"), (152, 5, "OBJ_EVENT_GFX_JOHTO_TRAIN_FRONT")],
        )
        self.assertEqual((station["warp_events"][0]["dest_map"], station["warp_events"][0]["dest_warp_id"]), ("MAP_SAFFRON_CITY", "15"))
        self.assertEqual((station["warp_events"][1]["x"], station["warp_events"][1]["y"]), (140, 16))

        predecessor = json.loads(world._base_bytes(ROOT, world.SAFFRON_KIOSK["path"]))
        saffron = json.loads(outputs[world.SAFFRON_KIOSK["path"]])
        self.assertEqual(saffron["warp_events"][:-1], predecessor["warp_events"])
        self.assertEqual(
            saffron["warp_events"][-1],
            {
                "x": 36,
                "y": 41,
                "elevation": 0,
                "dest_map": "MAP_KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION",
                "dest_warp_id": "0",
            },
        )
        self.assertEqual(saffron["object_events"][:-1], predecessor["object_events"])
        self.assertEqual(saffron["object_events"][-1]["script"], world.SAFFRON_KIOSK["script"])

        assembly = outputs[world.EVENT_SCRIPTS_PATH]
        for label in (
            b"KantoOriginal_VermilionCity_PortInside_MapScripts::",
            b"KantoOriginal_VermilionCity_PortInside_EventScript_Sailor::",
            b"KantoOriginal_SaffronCity_TrainStation_MapScripts::",
            b"KantoOriginal_SaffronCity_TrainStation_EventScript_Attendant::",
            b"KantoOriginal_SaffronCity_EventScript_TrainKiosk::",
        ):
            self.assertEqual(assembly.count(label), 1)
        self.assertIn(b"warpsilent MAP_OLIVINE_CITY_PORT_INSIDE, 8, 16", assembly)
        self.assertIn(b"warp MAP_GOLDENROD_CITY_TRAIN_STATION, 19, 16", assembly)
        self.assertIn(b"warp MAP_KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION, 140, 16", assembly)
        self.assertNotIn(b"MAP_SSANNE", world.ADAPTER_EVENT_SCRIPTS)

        # Trace the complete return path: kiosk interaction enters the station,
        # whose exit resolves to the appended outdoor warp beside that kiosk.
        self.assertEqual((world.SAFFRON_KIOSK["x"], world.SAFFRON_KIOSK["y"]), (36, 40))
        self.assertEqual(
            (saffron["warp_events"][15]["x"], saffron["warp_events"][15]["y"]),
            (world.SAFFRON_KIOSK["return_x"], world.SAFFRON_KIOSK["return_y"]),
        )

    def test_overland_boundary_materializes_controlled_round_trips(self):
        outputs = world._registration_outputs(ROOT, DONOR, world.build_plan(ROOT, DONOR))

        reception = json.loads(outputs[Path("data/maps/ReceptionGate/map.json")])
        self.assertEqual(reception["shared_scripts_map"], "Johto_ReceptionGate_Overland")
        self.assertEqual(
            reception["warp_events"][4],
            {
                "x": 20,
                "y": 9,
                "elevation": 0,
                "dest_map": "MAP_DYNAMIC",
                "dest_warp_id": "WARP_ID_DYNAMIC",
            },
        )
        self.assertEqual(
            reception["coord_events"][-1],
            {
                "type": "trigger",
                "x": 19,
                "y": 9,
                "elevation": 0,
                "var": "VAR_TEMP_0",
                "var_value": "0",
                "script": "Johto_ReceptionGate_Overland_EventScript_ChooseKanto",
            },
        )

        route26 = json.loads(outputs[Path("data/maps/Route26North/map.json")])
        self.assertEqual(
            [connection["map"] for connection in route26["connections"]],
            ["MAP_ROUTE26", "MAP_ROUTE28"],
        )

        route13 = json.loads(outputs[Path("data/maps/Route13/map.json")])
        calcium = next(event for event in route13["bg_events"] if event["type"] == "hidden_item")
        self.assertEqual(
            calcium["flag"],
            "JOHTO_FLAG_ITEM_ROUTE13_CALCIUM",
        )
        species_objects = [
            event["graphics_id"]
            for event in route13["object_events"]
            if event["graphics_id"].startswith("OBJ_EVENT_GFX_SPECIES(")
        ]
        self.assertIn("OBJ_EVENT_GFX_SPECIES(NIDORINO)", species_objects)
        self.assertIn("OBJ_EVENT_GFX_SPECIES(NIDORINA)", species_objects)
        route20 = json.loads(outputs[Path("data/maps/Route20/map.json")])
        self.assertIn(
            "OBJ_EVENT_GFX_SPECIES_SHINY(MAGIKARP)",
            [event["graphics_id"] for event in route20["object_events"]],
        )
        route19_cave = json.loads(outputs[Path("data/maps/Route19_Cave/map.json")])
        self.assertEqual(route19_cave["music"], "MUS_ABNORMAL_WEATHER")
        goldenrod = json.loads(outputs[Path("data/maps/GoldenrodCity/map.json")])
        movements = [event["movement_type"] for event in goldenrod["object_events"]]
        self.assertIn("MOVEMENT_TYPE_INVISIBLE", movements)
        self.assertNotIn("MOVEMENT_TYPE_TOWER_BEAM", movements)

        azalea = json.loads(outputs[Path("data/maps/AzaleaTown/map.json")])
        self.assertEqual(azalea["music"], "MUS_FALLARBOR")
        self.assertIn(
            "OBJ_EVENT_GFX_JOHTO_SILVER",
            [event["graphics_id"] for event in azalea["object_events"]],
        )
        celadon = json.loads(outputs[Path("data/maps/CeladonCity/map.json")])
        self.assertEqual(celadon["music"], "MUS_RG_CELADON")
        self.assertIn(
            "OBJ_EVENT_GFX_JOHTO_SHARED_BEAUTY",
            [event["graphics_id"] for event in celadon["object_events"]],
        )
        for map_name, battle_scene in (
            ("PokemonLeague_WillsRoom", "MAP_BATTLE_SCENE_SIDNEY"),
            ("PokemonLeague_KogasRoom", "MAP_BATTLE_SCENE_PHOEBE"),
            ("PokemonLeague_BrunosRoom", "MAP_BATTLE_SCENE_GLACIA"),
            ("PokemonLeague_KarensRoom", "MAP_BATTLE_SCENE_DRAKE"),
        ):
            with self.subTest(map_name=map_name):
                materialized = json.loads(outputs[Path(f"data/maps/{map_name}/map.json")])
                self.assertEqual(materialized["battle_scene"], battle_scene)
        mountain_side = json.loads(outputs[Path("data/maps/MtSilver_MountainSide/map.json")])
        self.assertEqual(mountain_side["object_events"][18]["movement_range_y"], 15)
        self.assertEqual(mountain_side["object_events"][19]["movement_range_y"], 15)

        later = json.loads(outputs[Path("data/maps/Route22/map.json")])
        self.assertEqual(later["shared_scripts_map"], "KantoLater_Route22_Overland")
        self.assertEqual(later["warp_events"], [])
        self.assertEqual(
            [connection["map"] for connection in later["connections"]],
            ["MAP_KANTO_LATER_VIRIDIAN_CITY"],
        )
        self.assertEqual(
            (
                later["object_events"][-1]["x"],
                later["object_events"][-1]["y"],
                later["object_events"][-1]["script"],
            ),
            (12, 10, "KantoLater_Route22_Overland_EventScript_Attendant"),
        )

        original_predecessor = json.loads(world._base_bytes(ROOT, world.ORIGINAL_ROUTE22_GATE["path"]))
        original = json.loads(outputs[world.ORIGINAL_ROUTE22_GATE["path"]])
        self.assertEqual(original["shared_scripts_map"], "KantoOriginal_Route22_Overland")
        self.assertEqual(original["connections"], original_predecessor["connections"])
        self.assertEqual(original["warp_events"], original_predecessor["warp_events"])
        self.assertEqual(original["coord_events"], original_predecessor["coord_events"])
        self.assertEqual(original["bg_events"], original_predecessor["bg_events"])
        self.assertEqual(original["object_events"][:-1], original_predecessor["object_events"])
        self.assertEqual(
            (
                original["object_events"][-1]["x"],
                original["object_events"][-1]["y"],
                original["object_events"][-1]["script"],
            ),
            (8, 12, "KantoOriginal_Route22_Overland_EventScript_Attendant"),
        )

    def test_linker_closure_predecessors_are_exact(self):
        outputs = world._registration_outputs(ROOT, DONOR, world.build_plan(ROOT, DONOR))
        aliases = world._object_graphics_aliases(ROOT)
        reverse_aliases = {target: source for source, target in aliases.items()}

        relative = Path("data/maps/AzaleaTown/map.json")
        expected = outputs[relative]
        predecessor = json.loads(expected)
        predecessor["music"] = "MUS_HG_AZALEA"
        for event in predecessor["object_events"]:
            graphics_id = event.get("graphics_id")
            if graphics_id in reverse_aliases:
                event["graphics_id"] = reverse_aliases[graphics_id]
        predecessor_raw = (json.dumps(predecessor, indent=2, ensure_ascii=False) + "\n").encode()
        self.assertTrue(world._is_pre_linker_closure_map(ROOT, DONOR, relative, predecessor_raw, expected))

        relative = Path("data/maps/GoldenrodCity/map.json")
        expected = outputs[relative]
        predecessor = json.loads(expected)
        donor_map = json.loads(world._git_blob(DONOR, relative.as_posix()))
        for index, source_event in enumerate(donor_map["object_events"]):
            if source_event["movement_type"] == "MOVEMENT_TYPE_TOWER_BEAM":
                predecessor["object_events"][index]["movement_type"] = "MOVEMENT_TYPE_TOWER_BEAM"
        predecessor_raw = (json.dumps(predecessor, indent=2, ensure_ascii=False) + "\n").encode()
        self.assertTrue(world._is_pre_map_field_alias_map(DONOR, relative, predecessor_raw, expected))

        relative = Path("data/maps/Route19_Cave/map.json")
        expected = outputs[relative]
        predecessor = json.loads(expected)
        predecessor["music"] = "MUS_WEATHER_KYOGRE"
        predecessor_raw = (json.dumps(predecessor, indent=2, ensure_ascii=False) + "\n").encode()
        self.assertTrue(world._is_pre_map_field_alias_map(DONOR, relative, predecessor_raw, expected))

        predecessor["object_events"][0]["x"] += 1
        mutated = (json.dumps(predecessor, indent=2, ensure_ascii=False) + "\n").encode()
        self.assertFalse(world._is_pre_linker_closure_map(ROOT, DONOR, relative, mutated, expected))

        relative = Path("data/maps/VermilionCity/map.json")
        expected = outputs[relative]
        predecessor = json.loads(expected)
        for event in predecessor["object_events"]:
            if event.get("graphics_id") == "OBJ_EVENT_GFX_SNORLAX":
                event["graphics_id"] = "OBJ_EVENT_GFX_BIG_SNORLAX"
        predecessor_raw = (json.dumps(predecessor, indent=2, ensure_ascii=False) + "\n").encode()
        self.assertTrue(world._is_pre_linker_closure_map(ROOT, DONOR, relative, predecessor_raw, expected))

    def test_overland_scripts_prepare_before_departure_and_commit_only_on_matching_arrival(self):
        assembly = world._render_event_script_assembly(ROOT)
        selector_start = assembly.index(b"Johto_ReceptionGate_Overland_EventScript_ChooseKanto::")
        selector = assembly[
            selector_start:assembly.index(b"KantoOriginal_Route22_Overland_MapScripts::", selector_start)
        ]
        lifecycle = (
            b"call EventScript_ChooseKantoEra",
            b"special Johto_RecordCurrentHeal",
            b"special Johto_PrepareKantoTravel",
            b"switch JOHTO_VAR_PENDING_KANTO_DESTINATION",
        )
        positions = [selector.index(command) for command in lifecycle]
        self.assertEqual(positions, sorted(positions))
        self.assertIn(b"setdynamicwarp MAP_ROUTE22, 255, 9, 12", assembly)
        self.assertIn(b"setdynamicwarp MAP_KANTO_LATER_ROUTE22, 255, 13, 10", assembly)
        self.assertNotIn(b"special Johto_CommitKantoTravel", selector)
        failed = script_block(assembly, "Johto_ReceptionGate_Overland_EventScript_TravelFailed")
        self.assertIn(b"special Johto_CancelKantoTravel", failed)
        self.assertIn(b"applymovement OBJ_EVENT_ID_PLAYER, Common_Movement_WalkLeft", failed)

        for label, destination in (
            ("KantoOriginal_Route22_Overland_EventScript_Arrive", b"2"),
            ("KantoLater_Route22_Overland_EventScript_Arrive", b"3"),
            ("Johto_ReceptionGate_Overland_EventScript_ArriveFromKanto", b"1"),
            ("KantoOriginal_Transport_EventScript_Arrive", b"2"),
        ):
            with self.subTest(label=label):
                block = script_block(assembly, label)
                target_check = b"goto_if_ne JOHTO_VAR_PENDING_KANTO_DESTINATION, " + destination
                self.assertLess(block.index(target_check), block.index(b"special Johto_CommitKantoTravel"))
                self.assertIn(b"goto_if_eq VAR_RESULT, FALSE", block)

        later_arrival = script_block(assembly, "KantoLater_Route22_Overland_EventScript_Arrive")
        self.assertLess(
            later_arrival.index(b"special Johto_CommitKantoTravel"),
            later_arrival.index(b"call Johto_EventScript_InitializeLaterKantoOnce"),
        )

        for label in (
            "KantoOriginal_VermilionCity_PortInside_EventScript_Sailor",
            "KantoOriginal_SaffronCity_TrainStation_EventScript_Attendant",
            "Kanto_Overland_EventScript_ReturnToJohto",
        ):
            with self.subTest(label=label):
                block = script_block(assembly, label)
                lifecycle = (
                    b"special Johto_ChooseJohto",
                    b"special Johto_RecordCurrentHeal",
                    b"special Johto_PrepareKantoTravel",
                    b"warp",
                )
                positions = [block.index(command) for command in lifecycle]
                self.assertEqual(positions, sorted(positions))
                self.assertNotIn(b"special Johto_CommitKantoTravel", block)

    def test_reception_gate_offers_group_travel_before_solo_departure(self):
        assembly = world._render_event_script_assembly(ROOT).replace(b"\r\n", b"\n")
        label = "Johto_ReceptionGate_Overland_EventScript_ChooseKanto"
        start = assembly.index(f"{label}::".encode())
        selector = assembly[
            start:assembly.index(b"KantoOriginal_Route22_Overland_MapScripts::", start)
        ]
        choose = selector.index(b"call EventScript_ChooseKantoEra")
        begin = selector.index(b"special Special_CoopGroupTravelBegin")
        solo = selector.index(f"{label}_SoloTravel::".encode())
        heal = selector.index(b"special Johto_RecordCurrentHeal")
        movement = selector.index(b"applymovement OBJ_EVENT_ID_PLAYER, Common_Movement_WalkRight")
        self.assertLess(choose, begin)
        self.assertLess(begin, solo)
        self.assertLess(solo, heal)
        self.assertLess(heal, movement)
        self.assertIn(b"setvar VAR_0x8004, 5", selector)
        self.assertIn(b"setvar VAR_0x8004, 6", selector)
        self.assertIn(
            f"\tcase 2, {label}_GroupTravelOriginal\n".encode(),
            selector,
        )
        self.assertIn(
            f"\tcase 3, {label}_GroupTravelLater\n".encode(),
            selector,
        )
        self.assertIn(
            b"special Special_CoopGroupTravelBegin\n"
            + f"\tgoto_if_eq VAR_RESULT, 0, {label}_SoloTravel\n".encode()
            + f"\tgoto_if_eq VAR_RESULT, 1, {label}_GroupTravelWaiting\n".encode()
            + b"\tgoto Johto_ReceptionGate_Overland_EventScript_TravelFailed\n",
            selector,
        )
        self.assertIn(
            f"{label}_SoloTravel::\n".encode()
            + b"\tspecial Johto_RecordCurrentHeal\n"
            + b"\tgoto_if_eq VAR_RESULT, FALSE, Johto_ReceptionGate_Overland_EventScript_TravelFailed\n"
            + b"\tspecial Johto_PrepareKantoTravel\n"
            + b"\tgoto_if_eq VAR_RESULT, FALSE, Johto_ReceptionGate_Overland_EventScript_TravelFailed\n"
            + b"\tswitch JOHTO_VAR_PENDING_KANTO_DESTINATION\n",
            selector,
        )
        waiting = script_block(assembly, f"{label}_GroupTravelWaiting")
        self.assertIn(b"end", waiting)
        self.assertNotIn(b"release", waiting)

        campaign = (ROOT / "data/johto/campaign_scripts.inc").read_bytes()
        route_labels = {
            b"Johto_GoldenrodCity_TrainStation_GoldenrodCity_TrainStation_EventScript_BoardTrain_GroupTravelOriginal": 1,
            b"Johto_GoldenrodCity_TrainStation_GoldenrodCity_TrainStation_EventScript_BoardTrain_GroupTravelLater": 2,
            b"Johto_OlivineCity_PortInside_OlivinePort_EventScript_ChoseVermilion_GroupTravelOriginal": 3,
            b"Johto_OlivineCity_PortInside_OlivinePort_EventScript_ChoseVermilion_GroupTravelLater": 4,
            b"Johto_ReceptionGate_Overland_EventScript_ChooseKanto_GroupTravelOriginal": 5,
            b"Johto_ReceptionGate_Overland_EventScript_ChooseKanto_GroupTravelLater": 6,
        }
        combined = campaign + assembly
        for route_label, route in route_labels.items():
            self.assertIn(
                route_label + b"::\n\tsetvar VAR_0x8004, " + str(route).encode(),
                combined,
            )
        self.assertEqual(assembly.count(b"EventScript_CoopGroupTravelOffer::"), 1)
        self.assertIn(b"special Special_CoopGroupTravelGetOffer", assembly)
        for transport in (b"MAGNET TRAIN", b"ferry", b"S.S. AQUA", b"through the gate"):
            self.assertIn(transport, assembly)
        self.assertEqual(assembly.count(b"to original KANTO?$"), 4)
        self.assertEqual(assembly.count(b"to KANTO three years later?$"), 4)
        self.assertGreater(
            assembly.index(b"EventScript_CoopGroupTravelOffer::"),
            assembly.index(world.REGISTERED_ADAPTER_MARKER),
        )

    def test_consistent_host_baseline_and_group_mutation_is_rejected(self):
        temporary, root = self._minimal_registration_root()
        with temporary:
            baseline_path = root / world.HOST_IDENTITY_BASELINE_PATH
            groups_path = root / world.MAP_GROUPS_PATH
            baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
            groups = json.loads(groups_path.read_text(encoding="utf-8"))
            group_name = "gMapGroup_IndoorVermilion_Frlg"
            baseline["groups"][group_name][-1]["name"] = "MutatedVermilionHouse"
            groups[group_name][7] = "MutatedVermilionHouse"
            baseline_path.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
            groups_path.write_text(json.dumps(groups, indent=2) + "\n", encoding="utf-8")

            with self.assertRaisesRegex(world.WorldPlanError, "host identity baseline semantics drifted"):
                world._validate_group_predecessor(root)

    def test_original_kanto_adapter_outputs_are_deterministic(self):
        plan = world.build_plan(ROOT, DONOR)
        first = world._registration_outputs(ROOT, DONOR, plan)
        second = world._registration_outputs(ROOT, DONOR, world.build_plan(ROOT, DONOR))
        self.assertEqual(first, second)

    def test_stale_world_plan_fails_check_and_write_repairs_it_in_transaction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            ledger = root / world.WORLD_PLAN_PATH
            ledger.parent.mkdir(parents=True)
            stale = world._base_bytes(ROOT, world.WORLD_PLAN_PATH)
            self.assertIsNotNone(stale)
            self.assertEqual(world._sha256(stale), world.TRUSTED_BOOTSTRAP_SHA256[world.WORLD_PLAN_PATH.as_posix()])
            ledger.write_bytes(stale)
            plan = {"production_write_ready": True, "blockers": []}
            expected = (json.dumps(plan, indent=2, sort_keys=True) + "\n").encode("utf-8")

            with mock.patch.object(world, "build_plan", return_value=plan), mock.patch.object(
                world, "_registration_outputs", return_value={world.WORLD_PLAN_PATH: expected}
            ), mock.patch.object(world, "_validate_group_predecessor"), mock.patch("builtins.print"):
                with self.assertRaisesRegex(world.WorldPlanError, "materialized output is missing or stale"):
                    world.run(root, DONOR, check=True)
                self.assertEqual(ledger.read_bytes(), stale)
                self.assertEqual(world.run(root, DONOR, write=True), 0)

            self.assertEqual(ledger.read_bytes(), expected)

    def test_sealed_goldenrod_debug_events_require_exact_donor_records(self):
        relative = "data/maps/GoldenrodCity_House1/map.json"
        source = world._git_json(world._git_blob(DONOR, relative), relative)
        self.assertEqual(
            world._sealed_bg_event_removals("MAP_GOLDENROD_CITY_HOUSE1", source["bg_events"]),
            frozenset({1, 2}),
        )
        changed = copy.deepcopy(source["bg_events"])
        changed[1]["x"] = 99
        with self.assertRaisesRegex(world.WorldPlanError, "sealed background event 1 drifted"):
            world._sealed_bg_event_removals("MAP_GOLDENROD_CITY_HOUSE1", changed)

    def test_altered_external_edge_category_is_rejected(self):
        def mutate(data):
            data["external_edges"][0]["classification"] = "required_host_adapter"

        temporary, root = self._temporary_ledgers(mutate_manifest=mutate)
        with temporary, self.assertRaisesRegex(world.WorldPlanError, "external edge"):
            world.build_plan(root, DONOR)

    def test_general_safari_layouts_are_runtime_closed_and_reported(self):
        plan = world.build_plan(ROOT, DONOR)
        self.assertEqual(tuple(plan["pending_general_layouts"]), world.PENDING_GENERAL_LAYOUTS)
        self.assertEqual(tuple(plan["pending_general_maps"]), world.PENDING_GENERAL_MAPS)
        self.assertEqual(plan["general_runtime_ready_map_count"], 407)
        self.assertTrue(plan["production_write_ready"])
        self.assertEqual(plan["blockers"], [])

        scenery = json.loads((ROOT / world.SCENERY_PATH).read_bytes())
        self.assertTrue(scenery["runtime_readiness"]["ready"])
        self.assertEqual(scenery["runtime_readiness"]["pending"], [])

        def regress(data):
            data["runtime_readiness"]["general_pending_layouts"].append("LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH")

        temporary, root = self._temporary_ledgers(mutate_scenery=regress)
        with temporary, self.assertRaisesRegex(world.WorldPlanError, "semantics differ"):
            world.build_plan(root, DONOR)

    def test_write_refuses_when_authenticated_assembly_closure_is_unavailable(self):
        temporary, root = self._minimal_registration_root()
        with temporary:
            before = {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()}
            with self.assertRaisesRegex(world.WorldPlanError, "cannot resolve host assembly revision"):
                world.run(root, DONOR, check=True)
            with self.assertRaisesRegex(world.WorldPlanError, "cannot resolve host assembly revision"):
                world.run(root, DONOR, write=True)
            after = {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()}
            self.assertEqual(after, before)

    def test_write_refuses_mutated_group_predecessor_without_mutation(self):
        temporary, root = self._minimal_registration_root()
        with temporary:
            groups = root / world.MAP_GROUPS_PATH
            groups.write_bytes(groups.read_bytes().replace(b"gMapGroup_TownsAndRoutes", b"gMapGroup_Altered", 1))
            before = groups.read_bytes()
            with self.assertRaisesRegex(world.WorldPlanError, "host map group order prefix drifted"):
                world.run(root, DONOR, write=True)
            self.assertEqual(groups.read_bytes(), before)

    def test_install_rolls_back_every_output_after_mid_replace_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            existing = root / "data/existing.bin"
            existing.parent.mkdir(parents=True)
            existing.write_bytes(b"before")
            outputs = {
                Path("data/existing.bin"): b"after",
                Path("data/new.bin"): b"new",
                Path("maps/new/map.json"): b"map",
            }
            original_replace = os.replace
            install_replaces = 0

            def fail_second_install(source, destination):
                nonlocal install_replaces
                if str(source).endswith(".new"):
                    install_replaces += 1
                    if install_replaces == 2:
                        raise OSError("injected replace failure")
                return original_replace(source, destination)

            with mock.patch.object(world.os, "replace", side_effect=fail_second_install):
                with self.assertRaisesRegex(OSError, "injected replace failure"):
                    world._install_registration_outputs(root, outputs)
            self.assertEqual(existing.read_bytes(), b"before")
            self.assertFalse((root / "data/new.bin").exists())
            self.assertFalse((root / "maps").exists())
            self.assertEqual(
                [path for path in root.rglob("*") if path.is_file() and path.name.startswith(".")],
                [],
            )

    def test_staging_io_failures_leave_no_outputs_temps_or_new_directories(self):
        original_named_temporary_file = world.tempfile.NamedTemporaryFile
        for operation in ("write", "flush", "fsync"):
            with self.subTest(operation=operation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                existing = root / "data/existing.bin"
                existing.parent.mkdir(parents=True)
                existing.write_bytes(b"before")
                outputs = {
                    Path("new/nested/output.bin"): b"new",
                    Path("data/existing.bin"): b"after",
                }

                def failing_temporary_file(*args, **kwargs):
                    handle = original_named_temporary_file(*args, **kwargs)
                    setattr(handle, operation, mock.Mock(side_effect=OSError(f"injected {operation} failure")))
                    return handle

                patcher = (
                    mock.patch.object(world.os, "fsync", side_effect=OSError("injected fsync failure"))
                    if operation == "fsync"
                    else mock.patch.object(world.tempfile, "NamedTemporaryFile", side_effect=failing_temporary_file)
                )
                with patcher, self.assertRaisesRegex(OSError, f"injected {operation} failure"):
                    world._install_registration_outputs(root, outputs)

                self.assertEqual(existing.read_bytes(), b"before")
                self.assertFalse((root / "new").exists())
                self.assertEqual(
                    [path for path in root.rglob("*") if path.is_file() and path.name.startswith(".")],
                    [],
                )

    def test_git_blob_reads_pinned_bytes_despite_dirty_file_or_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "World Planner Test"], cwd=repo, check=True)
            tracked = repo / "data/maps/Test/map.json"
            tracked.parent.mkdir(parents=True)
            committed = b'{"name":"committed"}\n'
            tracked.write_bytes(committed)
            subprocess.run(["git", "add", "data/maps/Test/map.json"], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-qm", "fixture"], cwd=repo, check=True)
            revision = subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=repo, check=True, capture_output=True, text=True
            ).stdout.strip()
            tracked.write_bytes(b'{"name":"dirty"}\n')
            dirty_before = tracked.read_bytes()
            with mock.patch.object(world, "DONOR_REVISION", revision):
                self.assertEqual(world._git_blob(repo, "data/maps/Test/map.json"), committed)
            self.assertEqual(tracked.read_bytes(), dirty_before)

            outside = repo / "outside.json"
            outside.write_bytes(b'{"name":"outside"}\n')
            tracked.unlink()
            try:
                os.symlink(outside, tracked)
            except OSError:
                return
            with mock.patch.object(world, "DONOR_REVISION", revision):
                self.assertEqual(world._git_blob(repo, "data/maps/Test/map.json"), committed)
            self.assertTrue(tracked.is_symlink())

    def test_git_blob_rejects_escape_missing_and_non_blob_paths(self):
        with self.assertRaisesRegex(world.WorldPlanError, "unsafe donor object path"):
            world._git_blob(DONOR, "../outside")
        with self.assertRaisesRegex(world.WorldPlanError, "does not exist"):
            world._git_blob(DONOR, "data/maps/DefinitelyMissing/map.json")
        with self.assertRaisesRegex(world.WorldPlanError, "not a blob"):
            world._git_blob(DONOR, "data/maps")

    def test_git_blob_rejects_all_ascii_controls_before_invoking_git(self):
        injected = [
            f"data/maps/Test{chr(code)}Injected/map.json" for code in (*range(0x20), 0x7F)
        ]
        injected.append("data/maps/Test\r\nInjected/map.json")
        with mock.patch.object(world.subprocess, "run") as git_run:
            for path in injected:
                with self.subTest(path=repr(path)), self.assertRaisesRegex(
                    world.WorldPlanError, "unsafe donor object path"
                ):
                    world._git_blob(DONOR, path)
            git_run.assert_not_called()

    def test_post_read_donor_pin_recheck_fails_closed_without_mutation(self):
        temporary, root = self._temporary_ledgers()
        original = world._verify_donor
        calls = 0

        def drift_after_read(donor):
            nonlocal calls
            calls += 1
            if calls == 1:
                return original(donor)
            raise world.WorldPlanError("donor pin mismatch after pinned reads")

        with temporary, mock.patch.object(world, "_verify_donor", side_effect=drift_after_read):
            self._assert_refusal_without_mutation(root, "donor pin mismatch after pinned reads")
        self.assertEqual(calls, 2)

    def test_render_is_deterministic(self):
        first = world.render(world.build_plan(ROOT, DONOR))
        second = world.render(world.build_plan(ROOT, DONOR))
        self.assertEqual(first, second)
        self.assertEqual(first, json.dumps(json.loads(first), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    unittest.main()
