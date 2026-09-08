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
        cls.ledger = symbols.build_ledger(DONOR)

    def test_actual_selected_lexical_inventory_is_deterministic(self) -> None:
        self.assertEqual(self.ledger, symbols.build_ledger(DONOR))
        self.assertEqual(self.ledger["completeness"]["scope"], "selected-lexical-only")
        self.assertEqual(self.ledger["completeness"]["selected_map_count"], 239)
        self.assertEqual(self.ledger["lexical_counts"], {"flags": 551, "vars": 72, "trainers": 288})
        self.assertEqual(self.ledger["allocated_counts"], {"flags": 539, "vars": 63, "trainers": 284})
        self.assertEqual(len(self.ledger["source_files"]), 478)
        self.assertTrue(self.ledger["source_files"][0]["sha256"])
        self.assertTrue(self.ledger["identities"]["flags"][0]["references"])

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

    def test_append_preserves_existing_ids_and_rejects_provenance_drift(self) -> None:
        candidate = copy.deepcopy(self.ledger)
        for kind, prefix, start in (
            ("flags", "FLAG_SYNTHETIC_APPEND", symbols.FLAG_START),
            ("vars", "VAR_SYNTHETIC_APPEND", symbols.VAR_START),
            ("trainers", "TRAINER_SYNTHETIC_APPEND", symbols.TRAINER_START),
        ):
            entry = copy.deepcopy(candidate["identities"][kind][-1])
            entry["symbol"] = prefix
            entry["qualified"] = "JOHTO_" + prefix
            entry["ordinal"] = len(candidate["identities"][kind])
            entry["runtime_id"] = start + entry["ordinal"]
            candidate["identities"][kind].append(entry)
        candidate["allocated_counts"] = {
            kind: len(candidate["identities"][kind]) for kind in ("flags", "vars", "trainers")
        }
        candidate["lexical_counts"] = {
            kind: self.ledger["lexical_counts"][kind] + 1 for kind in ("flags", "vars", "trainers")
        }
        updated = symbols.append_ledger(self.ledger, candidate)
        for kind in ("flags", "vars", "trainers"):
            self.assertEqual(updated["identities"][kind][0], self.ledger["identities"][kind][0])
            self.assertEqual(updated["identities"][kind][-1]["ordinal"], len(self.ledger["identities"][kind]))
        drifted = copy.deepcopy(candidate)
        drifted["provenance"]["donor_revision"] = "drift"
        with self.assertRaisesRegex(symbols.ContentSymbolError, "provenance drift"):
            symbols.append_ledger(self.ledger, drifted)

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
            self.assertEqual(candidate["lexical_counts"]["flags"], 552)
            self.assertEqual(candidate["allocated_counts"]["flags"], 540)
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
                    self.assertEqual(updated["identities"]["flags"][-1]["ordinal"], 539)
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


if __name__ == "__main__":
    unittest.main()
