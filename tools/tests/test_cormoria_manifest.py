"""Contract tests for the pinned Cormoria inventory; never modify a checkout."""
from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.cormoria import region_manifest as manifest


class ManifestUnitTests(unittest.TestCase):
    def test_allocation_rejects_collision_capacity_and_duplicates(self):
        with self.assertRaisesRegex(manifest.ManifestError, "duplicate"):
            manifest.allocate(["a", "a"], 0x8000, 2, set(), "flags")
        with self.assertRaisesRegex(manifest.ManifestError, "capacity"):
            manifest.allocate(["a", "b"], 0x8000, 1, set(), "flags")
        with self.assertRaisesRegex(manifest.ManifestError, "collision"):
            manifest.allocate(["a"], 0x8000, 2, {0x8000}, "flags")
        self.assertEqual(manifest.allocate(["b", "a"], 0x8000, 2, set(), "flags"),
                         {"a": 0x8000, "b": 0x8001})

    def test_asset_conversion_is_explicit_and_missing_asset_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tiles.png").write_bytes(b"png fixture")
            self.assertEqual(manifest.resolve_asset(root, "tiles.4bpp.lz", "map:1"),
                             ("tiles.png", "png->4bpp->lz"))
            with self.assertRaisesRegex(manifest.ManifestError, "map:7.*missing asset"):
                manifest.resolve_asset(root, "absent.4bpp.lz", "map:7")
            with self.assertRaisesRegex(manifest.ManifestError, "outside"):
                manifest.resolve_asset(root, "../secret.bin", "map:8")

    def test_missing_label_native_and_dynamic_target_report_owner(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").mkdir()
            script = root / "data/scripts.inc"
            script.write_text("Entry::\n\tcall MissingLabel\n\tend\n")
            with self.assertRaisesRegex(manifest.ManifestError, "scripts.inc:2.*MissingLabel"):
                manifest.ScriptGraph(root, {}).close(["data/scripts.inc"])
            script.write_text("Entry::\n\tcallnative MissingNative\n\tend\n")
            with self.assertRaisesRegex(manifest.ManifestError, "scripts.inc:2.*MissingNative"):
                manifest.ScriptGraph(root, {}).close(["data/scripts.inc"])
            script.write_text("Entry::\n\tsetdynamicwarp 99, 99, 0, 0, 0\n\tend\n")
            with self.assertRaisesRegex(manifest.ManifestError, "scripts.inc:2.*numeric map"):
                manifest.ScriptGraph(root, {}).close(["data/scripts.inc"])

    def test_shared_label_closure_preserves_fallthrough_and_text(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").mkdir()
            (root / "data/map.inc").write_text("Entry::\n\tcall Shared\n\tend\n")
            (root / "data/common.inc").write_text(
                'Shared::\n\tmsgbox Text\nTail::\n\treturn\nText::\n\t.string "Hello$"\nUnused::\n\tend\n')
            graph = manifest.ScriptGraph(root, {})
            graph.close(["data/map.inc"])
            self.assertEqual(set(graph.selected), {"Entry", "Shared", "Tail", "Text"})

    def test_word_pointer_closes_shared_label_dependency(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").mkdir()
            (root / "data/map.inc").write_text("Entry::\n\t.4byte Shared\n")
            (root / "data/common.inc").write_text("Shared::\n\tend\n")
            graph = manifest.ScriptGraph(root, {})
            graph.close(["data/map.inc"])
            self.assertIn("Shared", graph.selected)

    def test_multipart_asset_records_every_input(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "a.png").write_bytes(b"a")
            (root / "b.png").write_bytes(b"b")
            (root / "graphics.h").write_text('const u32 Gfx[] = INCBIN_U32("a.4bpp", "b.4bpp");')
            sources = manifest.Sources(root)
            sources.scan_assets("graphics.h", "ObjectRecord")
            self.assertEqual(set(sources.assets), {"a.4bpp", "b.4bpp"})

    def test_native_effect_annotation_is_not_a_function_name(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").mkdir()
            (root / "data/map.inc").write_text("Entry::\n\tcallnative Native, requests_effects=1\n\tend\n")
            graph = manifest.ScriptGraph(root, {}, {"Native": [{"path": "src/native.c", "line": 1}]})
            graph.close(["data/map.inc"])
            self.assertEqual(graph.native_calls[0]["symbol"], "Native")

    def test_alignment_does_not_connect_unrelated_labels(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").mkdir()
            (root / "data/map.inc").write_text("Entry::\n\tcall Shared\n\tend\n")
            (root / "data/common.inc").write_text("Shared::\n\treturn\n\t.align 2\nUnrelated::\n\tend\n")
            graph = manifest.ScriptGraph(root, {})
            graph.close(["data/map.inc"])
            self.assertNotIn("Unrelated", graph.selected)

    def test_reachable_duplicate_label_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").mkdir()
            (root / "data/a.inc").write_text("Entry::\n\tend\n")
            (root / "data/b.inc").write_text("Entry::\n\tend\n")
            with self.assertRaisesRegex(manifest.ManifestError, "duplicate reachable label Entry"):
                manifest.ScriptGraph(root, {}).close(["data/a.inc"])

    def test_nested_macro_fixed_specials_and_constants_are_dependencies(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "asm/macros").mkdir(parents=True)
            (root / "asm/macros/event.inc").write_text(
                ".macro outer\n\tinner\n.endm\n.macro inner\n\tspecial Native\n\tspecialvar VAR_RESULT, OtherNative\n\tsetvar VAR_RESULT, FIXED_MODE\n.endm\n")
            graph = manifest.ScriptGraph(root, {"VAR_RESULT": "1", "FIXED_MODE": "2"},
                                         {"Native": [], "OtherNative": []})
            graph.commands["outer"] = ["data/map.inc:1"]
            manifest.macro_closure(root, graph, manifest.Sources(root))
            self.assertEqual({entry["symbol"] for entry in graph.native_calls}, {"Native", "OtherNative"})
            self.assertIn("FIXED_MODE", {entry["symbol"] for entry in graph.references})

    def test_native_callback_audio_is_inventoried_without_other_modules(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src").mkdir()
            (root / "src/game.c").write_text(
                "void Start(void) { SetCallback(Menu); }\nstatic void Menu(void) { PlayBGM(MUS_GAME); }\n")
            (root / "src/other.c").write_text("void Unrelated(void) { PlayBGM(MUS_UNRELATED); }\n")
            functions = manifest.c_functions(root)
            refs, _ = manifest.native_module_references(root, [{"symbol": "Start", "owner": "data/map.inc:1"}], functions)
            self.assertIn("MUS_GAME", {entry["symbol"] for entry in refs})
            self.assertNotIn("MUS_UNRELATED", {entry["symbol"] for entry in refs})

    def test_pin_and_dirty_checkout_rejected(self):
        with mock.patch.object(manifest, "git", side_effect=["wrong"]):
            with self.assertRaisesRegex(manifest.ManifestError, "revision mismatch"):
                manifest.verify_donor(Path("unused"))
        with mock.patch.object(manifest, "git", side_effect=[manifest.DONOR_REVISION, " M file"]):
            with self.assertRaisesRegex(manifest.ManifestError, "dirty donor"):
                manifest.verify_donor(Path("unused"))


@unittest.skipUnless(os.environ.get("CORMORIA_DONOR"), "set CORMORIA_DONOR for pinned corpus tests")
class PinnedCorpusTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.donor = Path(os.environ["CORMORIA_DONOR"])
        cls.bundle = manifest.build_bundle(cls.donor)

    def test_pin_counts_namespaces_and_signed_components(self):
        region = self.bundle["region_manifest.json"]
        self.assertEqual(region["provenance"]["revision"], manifest.DONOR_REVISION)
        self.assertEqual([len(g["maps"]) for g in region["groups"]], [28, 32, 40, 31, 30, 4])
        self.assertEqual(len(region["maps"]), 165)
        self.assertEqual(len(region["layouts"]), 165)
        self.assertEqual(len(region["sections"]), 51)
        self.assertEqual(len(region["tilesets"]), 59)
        self.assertTrue(all(m["target_name"].startswith("Cormoria_") for m in region["maps"]))
        self.assertTrue(all(79 <= m["group"] <= 84 and m["index"] < 128 for m in region["maps"]))

    def test_deterministic_complete_source_hashes_and_check(self):
        self.assertEqual(self.bundle, manifest.build_bundle(self.donor))
        self.assertTrue(all(len(e["sha256"]) == 64 for e in self.bundle["source_manifest.json"]["files"]))
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            manifest.write_bundle(self.bundle, destination)
            self.assertTrue(manifest.check_bundle(self.bundle, destination))
            (destination / "symbol_ledger.json").write_text("{}\n")
            self.assertFalse(manifest.check_bundle(self.bundle, destination))
        self.assertNotIn(str(self.donor), json.dumps(self.bundle))

    def test_content_and_shared_roster_bindings_remain_explicit(self):
        region = self.bundle["region_manifest.json"]
        ledger = self.bundle["symbol_ledger.json"]
        self.assertFalse(region["runtime_ready"])
        self.assertTrue(region["content"]["wild_encounters"])
        self.assertEqual(len(region["content"]["trainers"]), len(ledger["trainers"]))
        self.assertEqual(len(region["content"]["heal_destinations"]), 17)
        self.assertTrue(region["content"]["audio_dependency_labels"])
        self.assertTrue(region["content"]["species_overworld_graphics"])
        self.assertTrue(all(entry["binding"] == "shared_host_roster_and_overworld_art"
                            for entry in region["content"]["species_overworld_graphics"]))
        self.assertTrue(all(entry["status"] == "planned_adapter_not_implemented"
                            for entry in ledger["native_bindings"]))
        self.assertTrue(all(entry["source_id"] == other["source_id"]
                            for category in ("flags", "vars", "trainers", "heals")
                            for entry in ledger[category] for other in ledger[category]
                            if entry["target_id"] == other["target_id"]))
        self.assertIn("princess-phoenix", self.bundle["DONOR_CREDITS.md"])


if __name__ == "__main__":
    unittest.main()
