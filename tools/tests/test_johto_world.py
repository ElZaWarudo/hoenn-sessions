import copy
import importlib.util
import json
import os
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
        self.assertEqual(plan["event_totals"]["warp_events"], 1175)
        self.assertEqual(
            plan["event_totals"]["warp_events"],
            sum(len(item["warp_targets"]) for item in plan["maps"]),
        )
        self.assertEqual((selected_warps, selected_connections), (1172, 191))
        self.assertEqual(plan["selected_map_count"], 407)
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
        self.assertEqual(
            plan["external_edges"],
            [{
                "source_map": "MAP_GOLDENROD_CITY_DEPARTMENT_STORE_ELEVATOR",
                "kind": "warp",
                "index": 0,
                "target": "MAP_DYNAMIC",
                "classification": "required_host_adapter",
            }],
        )
        elevator = by_source["MAP_GOLDENROD_CITY_DEPARTMENT_STORE_ELEVATOR"]["edges"][0]
        self.assertEqual(
            (elevator["target"], elevator["warp_id"], elevator["classification"]),
            ("MAP_DYNAMIC", "WARP_ID_DYNAMIC", "required_host_adapter"),
        )
        self.assertEqual(plan["external_edge_counts"], {"excluded_debug": 0, "host_adapter": 1, "runtime_policy": 0, "era_boundary": 0})
        self.assertEqual(plan["topology_notes"][0]["status"], "none")

    def test_normalized_topology_keeps_warp_and_edge_views_identical(self):
        plan = world.build_plan(ROOT, DONOR)
        for record in plan["maps"]:
            world._cross_validate_world_representations(record)
        self.assertEqual(plan["selected_warp_count"], 1174)
        self.assertEqual(plan["selected_connection_count"], 191)

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
            {"excluded_debug": 0, "host_adapter": 1, "runtime_policy": 0, "era_boundary": 0},
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
        self.assertFalse(plan["production_write_ready"])

        def regress(data):
            data["runtime_readiness"]["general_pending_layouts"].append("LAYOUT_FUCHSIA_CITY_SAFARI_ZONE_BEACH")

        temporary, root = self._temporary_ledgers(mutate_scenery=regress)
        with temporary, self.assertRaisesRegex(world.WorldPlanError, "semantics differ"):
            world.build_plan(root, DONOR)

    def test_write_materializes_only_the_generated_world_plan(self):
        temporary, root = self._temporary_ledgers()
        with temporary:
            before = {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()}
            world.run(root, DONOR, write=True)
            after = {path.relative_to(root): path.read_bytes() for path in root.rglob("*") if path.is_file()}
            self.assertEqual(set(after) - set(before), {world.WORLD_PLAN_PATH})
            self.assertEqual(json.loads((root / world.WORLD_PLAN_PATH).read_text()), world.build_plan(root, DONOR))
            world.run(root, DONOR, check=True)

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
