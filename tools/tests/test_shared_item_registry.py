"""Shared item identities must remain stable across ROM worlds."""

from __future__ import annotations

import copy
import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from tools import shared_item_registry


class SharedItemRegistryTests(unittest.TestCase):
    def test_pinned_cormoria_items_have_global_ids(self) -> None:
        registry = shared_item_registry.validate(require_installed=True)
        self.assertEqual(len(registry["items"]), 30)
        by_symbol = {row["symbol"]: row for row in registry["items"]}
        self.assertEqual(by_symbol["ITEM_RARE_SHARD"]["id"], 912)
        self.assertEqual(by_symbol["ITEM_GACHA_TOKEN"]["id"], 899)
        self.assertEqual(by_symbol["ITEM_HM_SPLASH"]["id"], 902)
        self.assertEqual(by_symbol["ITEM_HM_SPLASH"]["machine_slot"], "ITEM_HM10")

    def test_machine_item_requires_its_shared_slot(self) -> None:
        registry = shared_item_registry.validate()
        for slot in (None, "ITEM_HM09", "ITEM_HM11"):
            tampered = copy.deepcopy(registry)
            if slot is None:
                del tampered["items"][12]["machine_slot"]
            else:
                tampered["items"][12]["machine_slot"] = slot
            with self.subTest(slot=slot), self.assertRaises(shared_item_registry.ItemRegistryError):
                shared_item_registry.validate(registry=tampered)

    def test_old_item_identity_cannot_be_rebound(self) -> None:
        source_root = shared_item_registry.ROOT
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative in ("include/constants/items.h", "include/constants/tms_hms.h",
                             "data/rom_worlds.json", "data/cormoria/symbol_ledger.json"):
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((source_root / relative).read_bytes())
            item_header = root / "include/constants/items.h"
            text = item_header.read_text(encoding="utf-8")
            item_header.write_text(text.replace("ITEM_POKE_BALL = 1,", "ITEM_POKE_BALL = 4,"),
                                   encoding="utf-8")
            with self.assertRaisesRegex(shared_item_registry.ItemRegistryError,
                                        "pre-Cormoria item identities"):
                shared_item_registry.validate(root=root, registry=shared_item_registry.validate())

    def test_duplicate_or_shifted_item_id_is_rejected(self) -> None:
        original = shared_item_registry.validate()
        for replacement in (890, 920):
            tampered = copy.deepcopy(original)
            tampered["items"][1]["id"] = replacement
            with self.subTest(replacement=replacement), self.assertRaises(shared_item_registry.ItemRegistryError):
                shared_item_registry.validate(registry=tampered)

    def test_donor_numeric_id_cannot_replace_shared_identity(self) -> None:
        tampered = copy.deepcopy(shared_item_registry.validate())
        tampered["items"][9]["donor_id"] = 890
        with self.assertRaises(shared_item_registry.ItemRegistryError):
            shared_item_registry.validate(registry=tampered)

    def test_third_world_may_append_without_reassigning_cormoria(self) -> None:
        previous = shared_item_registry.validate()
        previous_bytes = json.dumps(previous, sort_keys=True).encode()
        previous_digest = hashlib.sha256(previous_bytes).hexdigest()
        extended = copy.deepcopy(previous)
        extended["items"].append({"id": 920, "symbol": "ITEM_THIRD_WORLD_TOKEN",
                                  "introduced_by_world": "third"})
        with self.assertRaises(shared_item_registry.ItemRegistryError):
            shared_item_registry.validate(registry=extended,
                                          known_worlds={"main", "cormoria", "third"})
        self.assertEqual(len(shared_item_registry.validate(
            registry=extended, known_worlds={"main", "cormoria", "third"},
            previous_bytes=previous_bytes, trusted_previous_sha256=previous_digest)["items"]), 31)
        with self.assertRaisesRegex(shared_item_registry.ItemRegistryError,
                                    "reserved but not installed"):
            shared_item_registry.validate(
                registry=extended, known_worlds={"main", "cormoria", "third"},
                previous_bytes=previous_bytes, trusted_previous_sha256=previous_digest,
                require_installed=True)
        later_bytes = json.dumps(extended, sort_keys=True).encode()
        renamed = copy.deepcopy(extended)
        renamed["items"][-1]["symbol"] = "ITEM_THIRD_WORLD_RENAMED"
        with self.assertRaisesRegex(shared_item_registry.ItemRegistryError,
                                    "previously released item identities changed"):
            shared_item_registry.validate(
                registry=renamed, known_worlds={"main", "cormoria", "third"},
                previous_bytes=later_bytes,
                trusted_previous_sha256=hashlib.sha256(later_bytes).hexdigest())


if __name__ == "__main__":
    unittest.main()
