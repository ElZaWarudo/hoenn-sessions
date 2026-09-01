from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
GENERATOR_PATH = ROOT / "tools" / "coop" / "generate_regional_identities.py"
SOURCE_PATH = ROOT / "data" / "coop" / "regional_identities.json"
LOCK_PATH = ROOT / "data" / "coop" / "regional_identities.lock.json"

SPEC = importlib.util.spec_from_file_location("regional_identities_generator", GENERATOR_PATH)
assert SPEC is not None and SPEC.loader is not None
generator = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = generator
SPEC.loader.exec_module(generator)


class RegionalIdentityGeneratorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.source = json.loads(SOURCE_PATH.read_text(encoding="utf-8"))

    def test_checked_in_outputs_are_current_and_digest_is_stable(self) -> None:
        registry = generator.validate_registry(self.source)
        self.assertEqual(registry.version, 1)
        self.assertEqual(registry.digest.hex(), "43918833dec646d6a583d124686c8540")
        self.assertEqual(len(registry.entries), 912)
        result = subprocess.run(
            [sys.executable, str(GENERATOR_PATH), "--check"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_canonical_digest_ignores_json_object_key_order(self) -> None:
        reversed_root = dict(reversed(list(self.source.items())))
        original = generator.canonical_registry_bytes(self.source)
        reordered = generator.canonical_registry_bytes(reversed_root)
        self.assertEqual(original, reordered)

    def test_explicit_ordinals_cannot_have_gaps_or_sorted_reassignment(self) -> None:
        broken = copy.deepcopy(self.source)
        broken["identities"][1]["ordinal"] = 2000
        with self.assertRaisesRegex(generator.RegistryError, "contiguous"):
            generator.validate_registry(broken)

        broken = copy.deepcopy(self.source)
        broken["identities"][0], broken["identities"][1] = (
            broken["identities"][1],
            broken["identities"][0],
        )
        with self.assertRaisesRegex(generator.RegistryError, "ordinal order"):
            generator.validate_registry(broken)

    def test_capacity_and_badge_abi_drift_fail_closed(self) -> None:
        broken = copy.deepcopy(self.source)
        broken["capacities"]["trainers"] = 4096
        with self.assertRaisesRegex(generator.RegistryError, "persisted ABI 2048"):
            generator.validate_registry(broken)

        broken = copy.deepcopy(self.source)
        first_badge = next(
            entry
            for entry in broken["identities"]
            if entry["id"] == "HOENN:BADGE_STONE"
        )
        first_badge["badge_bit"] = 7
        with self.assertRaisesRegex(generator.RegistryError, "duplicate badge bit"):
            generator.validate_registry(broken)

    def test_ambiguous_same_region_legacy_flags_are_rejected(self) -> None:
        broken = copy.deepcopy(self.source)
        # HOENN fly and event entries are distinct kinds but share the same
        # regional raw-flag namespace at the engine boundary.
        event = next(
            entry
            for entry in broken["identities"]
            if entry["id"] == "HOENN:EVENT_POKEDEX_OBTAINED"
        )
        fly = next(
            entry
            for entry in broken["identities"]
            if entry["id"] == "HOENN:FLY_LITTLEROOT"
        )
        event["legacy_value"] = fly["legacy_value"]
        with self.assertRaisesRegex(generator.RegistryError, "ambiguous legacy flag"):
            generator.validate_registry(broken)

    def test_identity_kind_and_unknown_fields_are_rejected(self) -> None:
        broken = copy.deepcopy(self.source)
        broken["identities"][0]["id"] = "HOENN:EVENT_WRONG_KIND"
        with self.assertRaisesRegex(generator.RegistryError, "TRAINER_ prefix"):
            generator.validate_registry(broken)

        broken = copy.deepcopy(self.source)
        broken["surprise"] = True
        with self.assertRaisesRegex(generator.RegistryError, "unknown surprise"):
            generator.validate_registry(broken)

    def test_every_ordinary_hoenn_trainer_has_a_stable_boundary_assignment(self) -> None:
        registry = generator.validate_registry(self.source)
        trainers = [
            entry
            for entry in registry.entries
            if entry.kind == "trainer" and entry.region == "HOENN"
        ]
        self.assertEqual(len(trainers), 854)
        self.assertEqual(
            (trainers[0].qualified_id, trainers[0].ordinal, trainers[0].legacy_value),
            ("HOENN:TRAINER_SAWYER_1", 0, 1),
        )
        self.assertEqual(
            (
                trainers[-1].qualified_id,
                trainers[-1].ordinal,
                trainers[-1].legacy_value,
            ),
            ("HOENN:TRAINER_MAY_PLACEHOLDER", 853, 854),
        )
        wally = next(
            entry
            for entry in trainers
            if entry.qualified_id == "HOENN:TRAINER_WALLY_1"
        )
        self.assertEqual((wally.ordinal, wally.legacy_value), (518, 519))

        generated_c = generator.render_c_source(registry)
        self.assertIn(
            '_Static_assert(TRAINER_WALLY_VR_1 == 519, "regional identity legacy value drift: TRAINER_WALLY_VR_1");',
            generated_c,
        )
        self.assertIn(
            '_Static_assert(FLAG_HIDE_OAK_IN_PALLET_TOWN == 2057, "regional identity legacy value drift: FLAG_HIDE_OAK_IN_PALLET_TOWN");',
            generated_c,
        )

    def test_lock_rejects_tail_deletion_and_existing_assignment_changes(self) -> None:
        lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))

        deleted_event = copy.deepcopy(self.source)
        deleted_event["registry_version"] = 2
        deleted_event["identities"].pop()
        with self.assertRaisesRegex(generator.RegistryError, "cannot be removed"):
            generator.updated_lock_history(lock, deleted_event)

        deleted_trainer = copy.deepcopy(self.source)
        deleted_trainer["registry_version"] = 2
        trainer_index = max(
            index
            for index, entry in enumerate(deleted_trainer["identities"])
            if entry["kind"] == "trainer"
        )
        deleted_trainer["identities"].pop(trainer_index)
        with self.assertRaisesRegex(generator.RegistryError, "cannot be removed"):
            generator.updated_lock_history(lock, deleted_trainer)

        renamed = copy.deepcopy(self.source)
        renamed["registry_version"] = 2
        brock = next(
            entry
            for entry in renamed["identities"]
            if entry["id"] == "KANTO:TRAINER_BROCK"
        )
        brock["id"] = "KANTO:TRAINER_BROCK_RENAMED"
        with self.assertRaisesRegex(generator.RegistryError, "immutable"):
            generator.updated_lock_history(lock, renamed)

    def test_lock_accepts_only_a_new_trailing_assignment_and_version(self) -> None:
        lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
        appended = copy.deepcopy(self.source)
        appended["registry_version"] = 2
        appended["identities"].append(
            {
                "kind": "event",
                "ordinal": 4,
                "id": "HOENN:EVENT_TEST_APPEND_ONLY",
                "legacy_value": None,
            }
        )
        updated = generator.updated_lock_history(lock, appended)
        self.assertEqual(len(updated["snapshots"]), 2)
        self.assertEqual(updated["snapshots"][-1], appended)

        skipped = copy.deepcopy(appended)
        skipped["registry_version"] = 3
        with self.assertRaisesRegex(generator.RegistryError, "exactly one"):
            generator.updated_lock_history(lock, skipped)


if __name__ == "__main__":
    unittest.main()
