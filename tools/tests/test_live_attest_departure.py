"""Narrow byte normalization and retained-evidence admission; no runtime/API."""
from contextlib import ExitStack
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_attest_departure as attest


def party():
    value = bytearray(604); value[0] = 5; value[534] = 23; value[589] = 255
    value[4] = 1  # Distinct occupied data must remain exact.
    return bytes(value)


class DepartureTests(unittest.TestCase):
    def setUp(self):
        self.before = party(); after = bytearray(self.before); after[534] = 0
        self.after = bytes(after)
        self.base = {"players": [], "legs": [{"name": "out", "source_world_id": 1,
                                               "destination_world_id": 2, "portal_id": 3}]}
        self.evidence = {"leg": "out", "players": {"a": {}, "b": {}}, "group": {"group_id": "group"}}
        self.saves, self.journals = {}, {}
        for name in ("a", "b"):
            identity = name.encode() * 20
            witnesses = [{"field_id": 0x0101, "offset": 0, "size": 604,
                          "sha256": hashlib.sha256(self.before).hexdigest(), "min_nonzero_bytes": 1},
                         {"field_id": 0x010B, "offset": 0, "size": 576, "sha256": "mail", "min_nonzero_bytes": 1}]
            witnesses += [{"field_id": fid, "offset": 0, "size": 1, "sha256": "unchanged", "min_nonzero_bytes": 1}
                          for fid in (0x010D, 0x0106, 0x0301)]
            self.base["players"].append({"name": name, "character_id": name,
                                        "source_save": name + "6", "source_sha256": name + "sha6",
                                        "seed_lineage": [{"path": name + str(g), "sha256": name + "sha" + str(g)} for g in range(1, 7)],
                                        "shared_witnesses": witnesses, "population_recipe": {"party": [1]},
                                        "custody_recipe": {"abi": "synthetic-mocked"}})
            for gen in (6, 7, 8):
                path = Path(name + str(gen))
                self.saves[str(path)] = SimpleNamespace(path=path, sha256=name + "sha" + str(gen),
                                                       generation=gen, lineage=identity,
                                                       party=self.before if gen == 6 else self.after)
            self.evidence["players"][name] = {"source": name + "7", "staged": name + "8", "journal": name + "journal",
                                                "journal_sha256": "f" * 64, "journal_source_sha256": name + "sha7"}
            self.journals[name + "journal"] = {"character_id": name, "phase": "committed",
                "intent": {"source_save_sha256": name + "sha7", "source_world_id": 1, "request": {"portal_id": 3}},
                "stage": {"destination_save_sha256": name + "sha8", "destination_world_id": 2},
                "terminal": {"committed": {"own_world_id": 2}}}

    def mocks(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        m = {}
        for name in ("preflight", "verify_leg"):
            m[name] = stack.enter_context(mock.patch.object(attest.harness, name, return_value={"strict": True}))
        stack.enter_context(mock.patch.object(attest.harness, "_paths", return_value=(Path("release"), Path("run"))))
        stack.enter_context(mock.patch.object(attest.harness, "_read_json", return_value={"shared_player_descriptor_hex": ""}))
        stack.enter_context(mock.patch.object(attest.harness, "_read_evidence_journal", side_effect=lambda item, _: self.journals[item["journal"]]))
        stack.enter_context(mock.patch.object(attest, "read_flash", side_effect=lambda path: self.saves[str(path)]))
        stack.enter_context(mock.patch.object(attest, "logical_field", side_effect=lambda save, desc, fid: save.party))
        for name in ("validate_lineages", "validate_player_fixture", "check_population"):
            m[name] = stack.enter_context(mock.patch.object(attest, name))
        return m

    def test_exact_empty_tail_change(self):
        result = attest.attest_party(self.before, self.after)
        self.assertEqual((result["offset"], result["old"], result["new"]), (534, 23, 0))

    def test_occupied_slot_or_other_tail_change_rejected(self):
        for offset in (4, 533, 535, 589, 590, 603):
            value = bytearray(self.after); value[offset] ^= 1
            with self.subTest(offset=offset), self.assertRaises(attest.OracleFailure):
                attest.attest_party(self.before, bytes(value))

    def test_nonempty_or_malformed_tail_rejected(self):
        for offset in (504, 588, 590, 534):
            before = bytearray(self.before); before[offset] = 0 if offset == 534 else 1
            after = bytearray(before); after[534] = 0
            with self.subTest(offset=offset), self.assertRaises(attest.OracleFailure):
                attest.attest_party(bytes(before), bytes(after))
        with self.assertRaises(attest.OracleFailure):
            attest.attest_party(self.before[:-1], self.after)

    def test_derivation_only_changes_authorized_fields(self):
        m = self.mocks(); original = copy.deepcopy(self.base)
        derived = attest.derive_plan(self.base, "out", self.evidence)
        self.assertEqual(self.base, original)
        self.assertEqual(derived["legs"], self.base["legs"])
        for old, new in zip(self.base["players"], derived["players"]):
            self.assertEqual(new["shared_witnesses"][1:], old["shared_witnesses"][1:])
            self.assertEqual(new["seed_lineage"][:-1], old["seed_lineage"])
            self.assertEqual(new["source_sha256"], old["name"] + "sha7")
            self.assertEqual(new["shared_witnesses"][0]["sha256"], hashlib.sha256(self.after).hexdigest())
        m["verify_leg"].assert_called_once_with(derived, "out", self.evidence, require_live_space=False)
        self.assertEqual(m["preflight"].call_args_list,
                         [mock.call(self.base, require_live_space=False), mock.call(derived, require_live_space=False)])

    def test_wrong_journal_actor_hash_world_commit_rejected(self):
        for mutate in (lambda j: j.update(character_id="other"),
                       lambda j: j["intent"].update(source_save_sha256="wrong"),
                       lambda j: j["stage"].update(destination_world_id=3),
                       lambda j: j["terminal"]["committed"].update(own_world_id=1)):
            with self.subTest(mutate=mutate):
                self.setUp(); m = self.mocks(); mutate(self.journals["ajournal"])
                with self.assertRaises(attest.OracleFailure):
                    attest.derive_plan(self.base, "out", self.evidence)
                m["verify_leg"].assert_not_called()

    def test_unpinned_journal_rejected(self):
        self.mocks(); del self.evidence["players"]["a"]["journal_sha256"]
        with self.assertRaises(attest.OracleFailure):
            attest.derive_plan(self.base, "out", self.evidence)

    def test_wrong_generation_or_lineage_rejected(self):
        for field, value in (("generation", 9), ("lineage", b"wrong"), ("sha256", "wrong")):
            with self.subTest(field=field):
                self.setUp(); self.mocks(); setattr(self.saves["a7"], field, value)
                with self.assertRaises(attest.OracleFailure):
                    attest.derive_plan(self.base, "out", self.evidence)

    def test_malformed_ancestry_rejected(self):
        m = self.mocks(); m["validate_lineages"].side_effect = attest.OracleFailure("reordered")
        with self.assertRaisesRegex(attest.OracleFailure, "reordered"):
            attest.derive_plan(self.base, "out", self.evidence)
        m["verify_leg"].assert_not_called()

    def test_mail_or_daycare_witness_failure_not_waived(self):
        for field in ("Mail", "Daycare"):
            with self.subTest(field=field):
                m = self.mocks(); m["validate_player_fixture"].side_effect = [None, attest.OracleFailure(field)]
                with self.assertRaisesRegex(attest.OracleFailure, field):
                    attest.derive_plan(self.base, "out", self.evidence)
                m["verify_leg"].assert_not_called()

    def test_staged_party_projection_failure_not_waived(self):
        m = self.mocks(); m["verify_leg"].side_effect = attest.OracleFailure("staged Party")
        with self.assertRaisesRegex(attest.OracleFailure, "staged Party"):
            attest.derive_plan(self.base, "out", self.evidence)

    def test_partial_party_witness_rejected(self):
        self.mocks(); self.base["players"][0]["shared_witnesses"][0]["size"] = 100
        with self.assertRaises(attest.OracleFailure):
            attest.derive_plan(self.base, "out", self.evidence)

    def test_missing_custody_recipe_or_required_witness_rejected_before_verification(self):
        for missing in ("custody_recipe", "population_recipe", 0x0101, 0x010B, 0x010D, 0x0106, 0x0301):
            with self.subTest(missing=missing):
                self.setUp(); m = self.mocks()
                player = self.base["players"][0]
                if isinstance(missing, str):
                    del player[missing]
                else:
                    player["shared_witnesses"] = [w for w in player["shared_witnesses"] if w["field_id"] != missing]
                with self.assertRaises(attest.OracleFailure):
                    attest.derive_plan(self.base, "out", self.evidence)
                m["preflight"].assert_not_called()
                m["verify_leg"].assert_not_called()

    def test_publication_reuses_identical_and_rejects_overwrite(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build" if Path("S:/cormoria-build").is_dir() else None) as folder:
            root = Path(folder)
            plan_path = root / "base.json"; plan_path.write_text(json.dumps(self.base))
            evidence_path = root / "evidence.json"
            for item in self.evidence["players"].values():
                item["template"] = "template"
            evidence_path.write_text(json.dumps(self.evidence))
            with mock.patch.object(attest.harness, "_paths", return_value=(root / "release", root)), \
                    mock.patch.object(attest, "capture_directory", return_value=root), \
                    mock.patch.object(attest, "derive_plan", return_value={"departure_attestation": {}}):
                target = root / "derived.json"
                self.assertEqual(attest.publish_plan(plan_path, "out", target), target.resolve())
                retained = target.read_bytes(); stamp = target.stat().st_mtime_ns
                attest.publish_plan(plan_path, "out", target)
                self.assertEqual(target.stat().st_mtime_ns, stamp)
                self.assertEqual(target.read_bytes(), retained)
                target.write_text("different")
                with self.assertRaises(attest.OracleFailure):
                    attest.publish_plan(plan_path, "out", target)
                self.assertEqual(target.read_text(), "different")
                with self.assertRaises(attest.OracleFailure):
                    attest.publish_plan(plan_path, "out", plan_path)

    def test_semantic_failure_publishes_no_output(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build" if Path("S:/cormoria-build").is_dir() else None) as folder:
            root = Path(folder)
            plan_path = root / "base.json"; plan_path.write_text(json.dumps(self.base))
            for item in self.evidence["players"].values():
                item["template"] = "template"
            (root / "evidence.json").write_text(json.dumps(self.evidence))
            with mock.patch.object(attest.harness, "_paths", return_value=(root / "release", root)), \
                    mock.patch.object(attest, "capture_directory", return_value=root), \
                    mock.patch.object(attest, "derive_plan", side_effect=attest.OracleFailure("stage")):
                with self.assertRaises(attest.OracleFailure):
                    attest.publish_plan(plan_path, "out", root / "derived.json")
                self.assertFalse((root / "derived.json").exists())
                self.assertFalse((root / "derived.json.tmp").exists())

    @unittest.skipUnless(os.name == "nt", "Windows namespace aliases")
    def test_extended_evidence_ordinary_output_alias_rejected_before_derivation(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build" if Path("S:/cormoria-build").is_dir() else None) as folder:
            root = Path(folder).resolve()
            extended = Path("\\\\?\\" + str(root))
            plan_path = root / "base.json"
            plan_path.write_text(json.dumps(self.base))
            evidence_path = root / "evidence.json"
            for item in self.evidence["players"].values():
                item["template"] = str(extended / "template.sav")
            evidence_path.write_text(json.dumps(self.evidence))
            originals = (plan_path.read_bytes(), evidence_path.read_bytes())
            with mock.patch.object(attest.harness, "_paths", return_value=(root / "release", root)), \
                    mock.patch.object(attest, "capture_directory", return_value=extended), \
                    mock.patch.object(attest, "derive_plan") as derive:
                with self.assertRaisesRegex(attest.OracleFailure, "protected inputs"):
                    attest.publish_plan(plan_path, "out", evidence_path)
                derive.assert_not_called()
            self.assertEqual((plan_path.read_bytes(), evidence_path.read_bytes()), originals)
            self.assertFalse(evidence_path.with_suffix(".json.tmp").exists())

    @unittest.skipUnless(os.name == "nt", "Windows namespace aliases")
    def test_namespace_comparison_preserves_unc_and_c_drive_restriction(self):
        for ordinary, extended in ((r"S:\Folder\Input.sav", r"\\?\S:\Folder\Input.sav"),
                                   (r"\\Server\Share\Folder\Input.sav", r"\\?\UNC\Server\Share\Folder\Input.sav")):
            with self.subTest(ordinary=ordinary):
                # Avoid a network lookup: the comparison receives already resolved namespace paths.
                with mock.patch.object(Path, "resolve", lambda path: path):
                    self.assertEqual(attest._comparison_path(Path(ordinary)), attest._comparison_path(Path(extended)))
        with mock.patch.object(Path, "resolve", lambda path: path):
            self.assertEqual(attest._comparison_path(Path(r"\\?\C:\Output\plan.json")).drive, "c:")
            self.assertTrue(attest._comparison_path(Path(r"\\?\S:\Release\nested\out.json")).is_relative_to(
                attest._comparison_path(Path(r"S:\Release"))))


if __name__ == "__main__":
    unittest.main()
