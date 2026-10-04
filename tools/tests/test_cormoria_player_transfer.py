import struct
import unittest

from tools.coop import player_transfer_manifest as schema


def synthetic_payload(spans=(101, 203, 307, 409)) -> bytes:
    fields = []
    cursor = [0] * schema.STORAGE_COUNT
    for field_id, (storage, owner) in schema.EXPECTED_FIELDS.items():
        # The real descriptor is sorted by storage.  This helper follows the
        # same stable ledger order and uses a byte-sized field for every ID.
        if storage == schema.STORAGE_SAVE_BLOCK1 and field_id == 0x0112:
            size = spans[storage] - cursor[storage]
        elif storage == schema.STORAGE_SAVE_BLOCK2 and field_id == 0x020E:
            size = spans[storage] - cursor[storage]
        elif storage == schema.STORAGE_POKEMON_STORAGE and field_id == 0x0304:
            size = spans[storage] - cursor[storage]
        elif storage == schema.STORAGE_SAVE_BLOCK3 and field_id == 0x0403:
            size = spans[storage] - cursor[storage]
        else:
            size = 1
        fields.append((field_id, storage, owner, cursor[storage], size, 0))
        cursor[storage] += size
    header = schema.HEADER_STRUCT.pack(
        schema.SCHEMA_MAGIC,
        schema.SCHEMA_VERSION,
        len(fields),
        schema.HEADER_SIZE + len(fields) * schema.FIELD_SIZE,
        schema.HEADER_SIZE,
        *spans,
        schema.HEADER_SIZE,
        schema.FIELD_SIZE,
        0,
    )
    return header + b"".join(schema.FIELD_STRUCT.pack(*field) for field in fields)


class PlayerTransferManifestTests(unittest.TestCase):
    def test_descriptor_covers_all_four_spans(self):
        decoded = schema.parse_schema_payload(synthetic_payload())
        self.assertEqual(decoded["field_count"], len(schema.EXPECTED_FIELDS))
        self.assertEqual(decoded["spans"]["save_block3"], 409)
        self.assertEqual(len(decoded["fields"]), len(schema.EXPECTED_FIELDS))
        owners = {field["id"]: field["ownership"] for field in decoded["fields"]}
        self.assertEqual(
            {field_id for field_id, owner in owners.items() if owner == schema.OWNER_LOCAL_PENDING},
            set(),
        )
        for field_id in (0x0104, 0x010D, 0x0112, 0x0113, 0x0203, 0x0206, 0x020D, 0x0403):
            self.assertEqual(owners[field_id], schema.OWNER_SHARED_PLAYER)
        for field_id in (0x0109, 0x0115, 0x0201):
            self.assertEqual(owners[field_id], schema.OWNER_WORLD_LOCAL)
        self.assertEqual(schema.REKEY_FIELD_IDS, {0x0102, 0x0103, 0x0106, 0x0113, 0x020D})
        self.assertEqual(decoded["rekey_field_ids"], sorted(schema.REKEY_FIELD_IDS))
        self.assertEqual(decoded["daycare_custody_field_ids"], sorted(schema.DAYCARE_CUSTODY_FIELD_IDS))
        schema.require_travel_ready(decoded)
        decoded["fields"][0]["ownership"] = schema.OWNER_LOCAL_PENDING
        with self.assertRaisesRegex(schema.ManifestError, "unresolved fields"):
            schema.require_travel_ready(decoded)

    def test_three_world_profiles_share_one_schema_contract(self):
        main = schema.parse_schema_payload(synthetic_payload((101, 203, 307, 409)))
        cormoria = schema.parse_schema_payload(synthetic_payload((101, 203, 307, 409)))
        third = schema.parse_schema_payload(synthetic_payload((101, 203, 307, 409)))
        schema.require_same_transfer_schema({"main": main, "cormoria": cormoria, "third": third})

        incompatible = schema.parse_schema_payload(synthetic_payload((111, 203, 307, 409)))
        with self.assertRaisesRegex(schema.ManifestError, "incompatible"):
            schema.require_same_transfer_schema({"main": main, "third": incompatible})

    def test_rejects_gap_overlap_and_ownership_drift(self):
        payload = bytearray(synthetic_payload())
        # The first field's offset is at payload offset HEADER + 4.
        struct.pack_into("<I", payload, schema.HEADER_SIZE + 4, 1)
        with self.assertRaises(schema.ManifestError):
            schema.parse_schema_payload(bytes(payload))

        payload = bytearray(synthetic_payload())
        # The player party field's owner byte is part of the stable contract.
        struct.pack_into("<B", payload, schema.HEADER_SIZE + schema.FIELD_SIZE + 3, schema.OWNER_LOCAL_PENDING)
        with self.assertRaises(schema.ManifestError):
            schema.parse_schema_payload(bytes(payload))

    def test_rejects_header_size_or_trailing_byte_drift(self):
        payload = bytearray(synthetic_payload()) + b"\0"
        with self.assertRaises(schema.ManifestError):
            schema.parse_schema_payload(bytes(payload))

        payload = bytearray(synthetic_payload())
        struct.pack_into("<I", payload, 32, schema.HEADER_SIZE + 1)
        with self.assertRaises(schema.ManifestError):
            schema.parse_schema_payload(bytes(payload))


if __name__ == "__main__":
    unittest.main()
