"""Contract tests for the pinned, append-only Johto content ledger."""

from __future__ import annotations

import copy
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.johto import content_symbols as symbols


DONOR = Path(os.environ["JOHTO_DONOR"]) if os.environ.get("JOHTO_DONOR") else None


class ContentSymbolLedgerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if DONOR is None:
            raise unittest.SkipTest("set JOHTO_DONOR to run pinned corpus checks")
        # The checked-in ledger is the saved-ordinal result of append-update;
        # build_ledger itself remains a scanner-ordered source candidate.
        cls.ledger = json.loads(symbols.OUTPUT_PATH.read_text(encoding="utf-8"))
        cls.candidate = symbols.build_ledger(DONOR)

    def _initial_predecessor(self) -> dict[str, object]:
        predecessor = copy.deepcopy(self.ledger)
        for kind, count in symbols.INITIAL_COUNTS.items():
            predecessor["identities"][kind] = predecessor["identities"][kind][:count]
        predecessor["allocated_counts"] = dict(symbols.INITIAL_COUNTS)
        predecessor["lexical_counts"] = {"flags": 551, "vars": 72, "trainers": 288}
        predecessor["completeness"] = {
            "scope": "selected-lexical-only",
            "selected_map_count": symbols.INITIAL_SELECTED_MAP_COUNT,
            "source_classes": ["map.json", "scripts.inc"],
            "pending_transitive_closure": symbols.PENDING_CLOSURE,
        }
        predecessor["provenance"] = {
            key: self.ledger["provenance"][key]
            for key in ("repository", "donor_revision", "donor_tree", "manifest_path",
                        "manifest_source_revision")
        }
        predecessor["provenance"].update({
            "manifest_sha256": symbols.INITIAL_MANIFEST_SHA256,
            "selected_source_count": symbols.INITIAL_SELECTED_SOURCE_COUNT,
        })
        predecessor["source_files"] = predecessor["source_files"][:symbols.INITIAL_SELECTED_SOURCE_COUNT]
        predecessor["provenance"]["source_inventory_sha256"] = symbols._source_inventory_digest(
            predecessor["source_files"]
        )
        predecessor["provenance"]["identity_membership"] = symbols._identity_membership(
            predecessor["identities"]
        )
        predecessor["provenance"]["identity_semantics_sha256"] = symbols._identity_semantics_digest(
            predecessor["identities"]
        )
        return predecessor

    def test_actual_selected_lexical_inventory_is_deterministic(self) -> None:
        self.assertEqual(self.candidate, symbols.build_ledger(DONOR))
        self.assertEqual(self.ledger["completeness"]["scope"], "selected-lexical-only")
        self.assertEqual(self.ledger["completeness"]["selected_map_count"], 407)
        self.assertEqual(self.candidate["lexical_counts"], {"flags": 626, "vars": 78, "trainers": 416})
        self.assertEqual(self.ledger["allocated_counts"], {"flags": 614, "vars": 69, "trainers": 412})
        self.assertEqual(len(self.ledger["source_files"]), 814)
        self.assertTrue(self.ledger["source_files"][0]["sha256"])
        self.assertTrue(self.ledger["identities"]["flags"][0]["references"])
        self.assertEqual(self.ledger["provenance"]["predecessor_transition"]["from"]["selected_map_count"], 239)
        self.assertEqual(self.ledger["provenance"]["predecessor_transition"]["to"]["added_map_count"], 168)

    def test_ordinals_are_qualified_and_joey_is_foundation_identity(self) -> None:
        for kind in ("flags", "vars", "trainers"):
            entries = self.ledger["identities"][kind]
            self.assertEqual([entry["ordinal"] for entry in entries], list(range(len(entries))))
            self.assertTrue(all(entry["qualified"].startswith("JOHTO_") for entry in entries))
        joey = self.ledger["identities"]["trainers"][0]
        self.assertEqual(joey["symbol"], "TRAINER_JOEY")
        self.assertEqual(joey["ordinal"], 0)
        self.assertEqual(joey["runtime_id"], symbols.TRAINER_START)

    def test_only_reviewed_transient_aliases_are_shared(self) -> None:
        self.assertEqual(
            [item["symbol"] for item in self.ledger["aliases"]["flags"]],
            list(symbols.FLAG_ALIAS_SYMBOLS),
        )
        self.assertEqual(
            [item["symbol"] for item in self.ledger["aliases"]["vars"]],
            list(symbols.VAR_ALIAS_SYMBOLS),
        )
        self.assertTrue(all(item["target"] == item["symbol"] for item in self.ledger["aliases"]["flags"]))
        self.assertTrue(all(item["target"] == item["symbol"] for item in self.ledger["aliases"]["vars"]))
        self.assertNotIn("FLAG_SYS_B_DASH", [item["symbol"] for item in self.ledger["aliases"]["flags"]])
        self.assertNotIn("VAR_SAFARI_ZONE_STATE", [item["symbol"] for item in self.ledger["aliases"]["vars"]])

    def test_header_uses_qualified_names_without_redefining_joey(self) -> None:
        header = symbols.render_header(self.ledger)
        self.assertIn('#include "constants/johto_trainers.h"', header)
        self.assertIn('#include "constants/johto_events.h"', header)
        self.assertIn("#define JOHTO_TRAINER_ABE (JOHTO_TRAINER_ID_MIN +", header)
        self.assertNotIn("#define JOHTO_TRAINER_JOEY ", header)
        self.assertIn("#define JOHTO_FLAG_TEMP_1 FLAG_TEMP_1", header)
        self.assertNotIn("#define TRAINER_JOEY ", header)

    def test_header_rejects_scanner_order_before_append_allocation(self) -> None:
        with self.assertRaisesRegex(symbols.ContentSymbolError, "initial allocated identity"):
            symbols.render_header(self.candidate)

    def test_append_rejects_unknown_identity_with_recomputed_attestations(self) -> None:
        for kind, prefix, start in (
            ("flags", "FLAG_UNDEFINED_SCRATCH", symbols.FLAG_START),
            ("vars", "VAR_UNDEFINED_SCRATCH", symbols.VAR_START),
            ("trainers", "TRAINER_UNDEFINED_SCRATCH", symbols.TRAINER_START),
        ):
            candidate = copy.deepcopy(self.ledger)
            entry = copy.deepcopy(candidate["identities"][kind][-1])
            entry["symbol"] = prefix
            entry["qualified"] = "JOHTO_" + prefix
            entry["ordinal"] = len(candidate["identities"][kind])
            entry["runtime_id"] = start + entry["ordinal"]
            candidate["identities"][kind].append(entry)
            candidate["allocated_counts"] = {
                role: len(candidate["identities"][role])
                for role in ("flags", "vars", "trainers")
            }
            candidate["provenance"]["identity_membership"] = symbols._identity_membership(
                candidate["identities"]
            )
            candidate["provenance"]["identity_semantics_sha256"] = symbols._identity_semantics_digest(
                candidate["identities"]
            )
            with self.assertRaisesRegex(symbols.ContentSymbolError, "pinned donor"):
                symbols.append_ledger(self.ledger, candidate)
            with self.assertRaisesRegex(symbols.ContentSymbolError, "identity membership"):
                symbols.append_ledger(self.ledger, candidate, DONOR)

    def test_append_rejects_same_scope_source_inventory_drift(self) -> None:
        for mutation in ("hash", "path", "order"):
            candidate = copy.deepcopy(self.candidate)
            if mutation == "hash":
                candidate["source_files"][0]["sha256"] = "0" * 64
            elif mutation == "path":
                candidate["source_files"][0]["path"] += ".drift"
            else:
                candidate["source_files"][0], candidate["source_files"][1] = (
                    candidate["source_files"][1], candidate["source_files"][0]
                )
            candidate["provenance"]["source_inventory_sha256"] = symbols._source_inventory_digest(
                candidate["source_files"]
            )
            with self.assertRaisesRegex(symbols.ContentSymbolError, "source inventory"):
                symbols.append_ledger(self.ledger, candidate)

    def test_append_requires_donor_for_forged_same_scope_references(self) -> None:
        forged_reference = {
            "path": "data/maps/InventedMap/scripts.inc",
            "kind": "script",
            "line": 1,
            "column": 1,
        }
        for kind in ("flags", "vars", "trainers"):
            candidate = copy.deepcopy(self.candidate)
            candidate["identities"][kind][0]["references"].append(forged_reference)
            candidate["provenance"]["identity_semantics_sha256"] = symbols._identity_semantics_digest(
                candidate["identities"]
            )
            with self.assertRaisesRegex(symbols.ContentSymbolError, "pinned donor authentication"):
                symbols.append_ledger(self.ledger, candidate)
            with self.assertRaisesRegex(symbols.ContentSymbolError, "semantic evidence"):
                symbols.append_ledger(self.ledger, candidate, DONOR)

    def test_append_rejects_wrong_lexical_count_for_each_role(self) -> None:
        for kind in ("flags", "vars", "trainers"):
            candidate = copy.deepcopy(self.candidate)
            candidate["lexical_counts"][kind] -= 1
            with self.assertRaisesRegex(symbols.ContentSymbolError, "pinned donor authentication"):
                symbols.append_ledger(self.ledger, candidate)
            with self.assertRaisesRegex(symbols.ContentSymbolError, "lexical counts"):
                symbols.append_ledger(self.ledger, candidate, DONOR)

    def test_validation_rejects_malformed_lexical_counts_shape(self) -> None:
        for malformed in (
            {},
            {"flags": 626, "vars": 78, "trainers": 416, "extra": 0},
            {"flags": 626, "vars": "78", "trainers": 416},
            {"flags": 626, "vars": 78, "trainers": True},
        ):
            candidate = copy.deepcopy(self.candidate)
            candidate["lexical_counts"] = malformed
            with self.assertRaisesRegex(symbols.ContentSymbolError, "lexical counts"):
                symbols.validate_ledger(candidate, allocated=False)

    def test_approved_manifest_expansion_requires_validated_predecessor(self) -> None:
        predecessor = self._initial_predecessor()
        updated = symbols.append_ledger(predecessor, self.candidate, DONOR)
        self.assertEqual(updated["completeness"]["selected_map_count"], 407)
        self.assertEqual(updated["allocated_counts"], {"flags": 614, "vars": 69, "trainers": 412})
        for kind in ("flags", "vars", "trainers"):
            self.assertEqual(updated["identities"][kind][:symbols.INITIAL_COUNTS[kind]],
                             predecessor["identities"][kind])

        invalid = copy.deepcopy(self.candidate)
        invalid["provenance"]["predecessor_transition"]["from"]["manifest_sha256"] = "drift"
        with self.assertRaisesRegex(symbols.ContentSymbolError, "predecessor transition"):
            symbols.validate_ledger(invalid, allocated=False)

    def test_scope_expansion_always_requires_donor_authentication(self) -> None:
        predecessor = self._initial_predecessor()
        candidate = copy.deepcopy(self.candidate)
        candidate["identities"] = copy.deepcopy(predecessor["identities"])
        candidate["allocated_counts"] = copy.deepcopy(predecessor["allocated_counts"])
        candidate["lexical_counts"] = copy.deepcopy(predecessor["lexical_counts"])
        forged_suffix = candidate["source_files"][symbols.INITIAL_SELECTED_SOURCE_COUNT:]
        self.assertEqual(len(forged_suffix), symbols.EXPANDED_LATER_MAP_COUNT * 2)
        for index, source in enumerate(forged_suffix):
            source["map"] = f"MAP_FORGED_{index // 2}"
            source["map_name"] = f"ForgedMap{index // 2}"
            source["path"] = f"data/maps/ForgedMap{index // 2}/{source['kind']}.{index}"
            source["sha256"] = f"{index % 16:x}" * 64
        candidate["provenance"]["source_inventory_sha256"] = symbols._source_inventory_digest(
            candidate["source_files"]
        )
        candidate["provenance"]["identity_membership"] = symbols._identity_membership(
            candidate["identities"]
        )
        candidate["provenance"]["identity_semantics_sha256"] = symbols._identity_semantics_digest(
            candidate["identities"]
        )

        with self.assertRaisesRegex(symbols.ContentSymbolError, "pinned donor authentication"):
            symbols.append_ledger(predecessor, candidate)
        with self.assertRaisesRegex(symbols.ContentSymbolError, "source inventory"):
            symbols.append_ledger(predecessor, candidate, DONOR)

    def test_expanded_candidate_rejects_missing_donor_identity_after_recomputed_attestations(self) -> None:
        predecessor = self._initial_predecessor()
        starts = {"flags": symbols.FLAG_START, "vars": symbols.VAR_START, "trainers": symbols.TRAINER_START}
        for kind in ("flags", "vars", "trainers"):
            candidate = copy.deepcopy(self.candidate)
            predecessor_symbols = {entry["symbol"] for entry in predecessor["identities"][kind]}
            appended = [
                entry for entry in candidate["identities"][kind]
                if entry["symbol"] not in predecessor_symbols
            ]
            self.assertTrue(appended)
            omitted = appended[0]["symbol"]
            candidate["identities"][kind] = [
                entry for entry in candidate["identities"][kind]
                if entry["symbol"] != omitted
            ]
            for ordinal, entry in enumerate(candidate["identities"][kind]):
                entry["ordinal"] = ordinal
                entry["runtime_id"] = starts[kind] + ordinal
            candidate["allocated_counts"] = {
                role: len(candidate["identities"][role])
                for role in ("flags", "vars", "trainers")
            }
            candidate["provenance"]["identity_membership"] = symbols._identity_membership(
                candidate["identities"]
            )
            candidate["provenance"]["identity_semantics_sha256"] = symbols._identity_semantics_digest(
                candidate["identities"]
            )
            with self.assertRaisesRegex(symbols.ContentSymbolError, "membership is incomplete"):
                symbols.append_ledger(predecessor, candidate, DONOR)

    def test_validation_rejects_alias_capacity_and_foundation_collisions(self) -> None:
        bad_alias = copy.deepcopy(self.ledger)
        bad_alias["aliases"]["flags"][0]["target"] = "FLAG_SYS_B_DASH"
        with self.assertRaisesRegex(symbols.ContentSymbolError, "alias target"):
            symbols.validate_ledger(bad_alias)
        bad_capacity = copy.deepcopy(self.ledger)
        bad_capacity["capacities"]["flags"] = 1
        with self.assertRaisesRegex(symbols.ContentSymbolError, "capacities"):
            symbols.validate_ledger(bad_capacity)
        bad_ordinal = copy.deepcopy(self.ledger)
        bad_ordinal["identities"]["trainers"][1]["ordinal"] = 0
        with self.assertRaisesRegex(symbols.ContentSymbolError, "contiguous"):
            symbols.validate_ledger(bad_ordinal)

    def test_new_lexically_first_symbol_appends_and_checks_through_cli(self) -> None:
        files, references = symbols._source_files(DONOR, symbols._manifest_maps())
        # Real pinned definition; simulate its discovery by an expanded scanner.
        references["FLAG_121_FAIRY_GEM"] = copy.deepcopy(next(iter(references.values())))
        with mock.patch.object(symbols, "_source_files", return_value=(files, references)):
            candidate = symbols.build_ledger(DONOR)
            self.assertEqual(candidate["lexical_counts"]["flags"], 627)
            self.assertEqual(candidate["allocated_counts"]["flags"], 615)
            self.assertEqual(candidate["identities"]["flags"][0]["symbol"], "FLAG_121_FAIRY_GEM")
            with tempfile.TemporaryDirectory() as directory:
                ledger_path = Path(directory) / "ledger.json"
                header_path = Path(directory) / "header.h"
                ledger_path.write_text(symbols._canonical_json(self.ledger), encoding="utf-8")
                header_path.write_text(symbols.render_header(self.ledger), encoding="utf-8")
                with mock.patch.object(symbols, "OUTPUT_PATH", ledger_path), mock.patch.object(symbols, "HEADER_PATH", header_path):
                    before = (ledger_path.read_bytes(), header_path.read_bytes())
                    self.assertEqual(symbols.main(["--donor", str(DONOR), "--check"]), 1)
                    self.assertEqual(before, (ledger_path.read_bytes(), header_path.read_bytes()))
                    self.assertEqual(symbols.main(["--donor", str(DONOR), "--append-update"]), 0)
                    updated = json.loads(ledger_path.read_text(encoding="utf-8"))
                    for kind in ("flags", "vars", "trainers"):
                        old_ids = {entry["symbol"]: (entry["ordinal"], entry["runtime_id"])
                                   for entry in self.ledger["identities"][kind]}
                        new_ids = {entry["symbol"]: (entry["ordinal"], entry["runtime_id"])
                                   for entry in updated["identities"][kind]}
                        self.assertEqual(old_ids, {symbol: new_ids[symbol] for symbol in old_ids})
                    self.assertEqual(updated["identities"]["flags"][-1]["ordinal"], 614)
                    self.assertEqual(updated["identities"]["flags"][-1]["symbol"], "FLAG_121_FAIRY_GEM")
                    self.assertEqual(symbols.main(["--donor", str(DONOR), "--check"]), 0)
                    after = (ledger_path.read_bytes(), header_path.read_bytes())
                    self.assertEqual(symbols.main(["--donor", str(DONOR), "--append-update"]), 0)
                    self.assertEqual(after, (ledger_path.read_bytes(), header_path.read_bytes()))

    def test_initial_bindings_cannot_be_reordered_even_with_consistent_ids(self) -> None:
        changed = copy.deepcopy(self.ledger)
        entries = changed["identities"]["flags"]
        entries[0], entries[1] = entries[1], entries[0]
        for ordinal, entry in enumerate(entries):
            entry["ordinal"] = ordinal
            entry["runtime_id"] = symbols.FLAG_START + ordinal
        with self.assertRaisesRegex(symbols.ContentSymbolError, "initial allocated identity"):
            symbols.validate_ledger(changed)
        with self.assertRaisesRegex(symbols.ContentSymbolError, "initial allocated identity"):
            symbols.append_ledger(changed, self.ledger)

    def _assert_expanded_bindings_cannot_be_reordered(
        self,
        kind: str,
        first_ordinal: int,
        runtime_start: int,
    ) -> None:
        changed = copy.deepcopy(self.ledger)
        entries = changed["identities"][kind]
        entries[first_ordinal], entries[first_ordinal + 1] = (
            entries[first_ordinal + 1],
            entries[first_ordinal],
        )
        for ordinal, entry in enumerate(entries):
            entry["ordinal"] = ordinal
            entry["runtime_id"] = runtime_start + ordinal

        for operation in (
            lambda: symbols.validate_ledger(changed),
            lambda: symbols.render_header(changed),
            lambda: symbols.append_ledger(changed, self.candidate, DONOR),
        ):
            with self.subTest(kind=kind, operation=operation):
                with self.assertRaisesRegex(
                    symbols.ContentSymbolError,
                    "sealed allocated identity",
                ):
                    operation()

    def test_expanded_flag_bindings_cannot_be_reordered(self) -> None:
        self.assertEqual(
            [
                self.ledger["identities"]["flags"][539]["symbol"],
                self.ledger["identities"]["flags"][540]["symbol"],
            ],
            ["FLAG_BATTLED_DEOXYS", "FLAG_CAUGHT_MEW"],
        )
        self._assert_expanded_bindings_cannot_be_reordered(
            "flags", 539, symbols.FLAG_START
        )

    def test_expanded_variable_bindings_cannot_be_reordered(self) -> None:
        self._assert_expanded_bindings_cannot_be_reordered(
            "vars", 63, symbols.VAR_START
        )

    def test_expanded_trainer_bindings_cannot_be_reordered(self) -> None:
        self._assert_expanded_bindings_cannot_be_reordered(
            "trainers", 284, symbols.TRAINER_START
        )

    def test_check_rejects_corrupt_header_without_rewriting_either_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            ledger_path = Path(directory) / "ledger.json"
            header_path = Path(directory) / "header.h"
            ledger_path.write_text(symbols._canonical_json(self.ledger), encoding="utf-8")
            header_path.write_text("#define JOHTO_TRAINER_JOEY 0\n", encoding="utf-8")
            before = (ledger_path.read_bytes(), header_path.read_bytes())
            with mock.patch.object(symbols, "OUTPUT_PATH", ledger_path), mock.patch.object(symbols, "HEADER_PATH", header_path):
                self.assertEqual(symbols.main(["--donor", str(DONOR), "--check"]), 1)
            self.assertEqual(before, (ledger_path.read_bytes(), header_path.read_bytes()))

    def test_check_mode_does_not_rewrite_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            ledger_path = Path(directory) / "content_symbols.json"
            header_path = Path(directory) / "johto_content.h"
            ledger_path.write_text(json.dumps(self.ledger, indent=2) + "\n", encoding="utf-8")
            header_path.write_text(symbols.render_header(self.ledger), encoding="utf-8")
            before = (ledger_path.read_bytes(), header_path.read_bytes())
            with mock.patch.object(symbols, "OUTPUT_PATH", ledger_path), mock.patch.object(symbols, "HEADER_PATH", header_path):
                self.assertEqual(symbols.main(["--donor", str(DONOR), "--check"]), 0)
            self.assertEqual((ledger_path.read_bytes(), header_path.read_bytes()), before)


class ContentSymbolSyntheticTests(unittest.TestCase):
    def test_donor_pin_is_fail_closed(self) -> None:
        with mock.patch.object(symbols, "_git_revision", return_value=("wrong", "tree")):
            with self.assertRaisesRegex(symbols.ContentSymbolError, "pin mismatch"):
                symbols.build_ledger(Path("/does/not/matter"))

    def test_post_read_donor_authentication_rechecks_pin_cleanliness_and_hashes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            donor = Path(directory)
            source = donor / "selected.inc"
            source.write_text("original\n", encoding="utf-8")
            expected_hash = symbols._sha256(source)
            with mock.patch.object(
                symbols,
                "_git_revision",
                return_value=(symbols.DONOR_REVISION, symbols.DONOR_TREE),
            ), mock.patch.object(symbols, "_assert_donor_clean") as clean:
                symbols._assert_donor_stable(
                    donor,
                    symbols.DONOR_REVISION,
                    symbols.DONOR_TREE,
                    {"selected.inc": expected_hash},
                )
                clean.assert_called_once_with(donor)

                source.write_text("changed\n", encoding="utf-8")
                with self.assertRaisesRegex(symbols.ContentSymbolError, "donor source changed"):
                    symbols._assert_donor_stable(
                        donor,
                        symbols.DONOR_REVISION,
                        symbols.DONOR_TREE,
                        {"selected.inc": expected_hash},
                    )

            with mock.patch.object(symbols, "_git_revision", return_value=("changed", "tree")):
                with self.assertRaisesRegex(symbols.ContentSymbolError, "donor revision changed"):
                    symbols._assert_donor_stable(
                        donor,
                        symbols.DONOR_REVISION,
                        symbols.DONOR_TREE,
                        {"selected.inc": expected_hash},
                    )


if __name__ == "__main__":
    unittest.main()
