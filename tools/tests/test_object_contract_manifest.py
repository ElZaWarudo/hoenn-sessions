import hashlib
import struct
import unittest

from tools.coop.object_contract_manifest import (
    DESCRIPTOR, DESCRIPTOR_SIZE, ENTRY, HEADER, MAGIC, MAX_TEXT_BYTES, TABLES,
    ManifestError, manifest_from_rom, require_same_scalar_tables,
    symbols_from_nm,
)


BASE = 0x08000000
TEXT_FIELDS = ((4, 8, 12), (4,), (4, 8), (4,), ())


def fixture(pointer_shift=0, table_shift=0, scalar_change=False,
            null_change=False, text_change=False):
    rom = bytearray(8192)
    descriptor = bytearray(DESCRIPTOR_SIZE)
    HEADER.pack_into(descriptor, 0, MAGIC, 2, len(TABLES))
    for index, fields in enumerate(TEXT_FIELDS):
        ENTRY.pack_into(descriptor, HEADER.size + index * ENTRY.size,
                        32 if fields else 4, len(fields),
                        *list(fields), *([0] * (48 - len(fields))),
                        len(fields), *list(fields), *([0] * (3 - len(fields))))
    rom[16:16 + len(descriptor)] = descriptor
    symbols = {DESCRIPTOR: (BASE + 16, len(descriptor))}
    for index, name in enumerate(TABLES):
        offset = 768 + table_shift + index * 128
        stride = 32 if TEXT_FIELDS[index] else 4
        payload = bytearray(stride * 2)
        for row in range(2):
            struct.pack_into("<I", payload, row * stride, index * 10 + row)
            for field_index, field in enumerate(TEXT_FIELDS[index]):
                text_offset = 2048 + pointer_shift + index * 256 + row * 96 + field_index * 24
                value = bytes((0xA0 + index, 0xA1 + row, 0xA2 + field_index, 0xFF))
                rom[text_offset:text_offset + len(value)] = value
                struct.pack_into("<I", payload, row * stride + field, BASE + text_offset)
        if scalar_change and index == 2:
            payload[32] ^= 1
        if null_change and index == 2:
            struct.pack_into("<I", payload, 4, 0)
        if text_change and index == 2:
            text_offset = 2048 + pointer_shift + index * 256
            rom[text_offset] ^= 1
        rom[offset:offset + len(payload)] = payload
        symbols[name] = (BASE + offset, len(payload))
    return bytes(rom), symbols


class ObjectContractManifestTests(unittest.TestCase):
    def test_pointer_and_table_relocation_preserve_content_hashes(self):
        left_rom, left_symbols = fixture()
        right_rom, right_symbols = fixture(pointer_shift=1024, table_shift=64)
        left = manifest_from_rom(left_rom, left_symbols)
        right = manifest_from_rom(right_rom, right_symbols)
        require_same_scalar_tables({"main": left, "cormoria": right})
        self.assertNotEqual(left["tables"]["gItemsInfo"]["raw_sha256"],
                            right["tables"]["gItemsInfo"]["raw_sha256"])
        self.assertNotEqual(left["tables"]["gItemsInfo"]["address"],
                            right["tables"]["gItemsInfo"]["address"])
        self.assertEqual(left["tables"]["gItemsInfo"]["display_text_sha256"],
                         right["tables"]["gItemsInfo"]["display_text_sha256"])
        self.assertEqual(left["rom_sha256"], hashlib.sha256(left_rom).hexdigest())

    def test_scalar_and_text_drift_rejected(self):
        left_rom, left_symbols = fixture()
        left = manifest_from_rom(left_rom, left_symbols)
        for change in ({"scalar_change": True}, {"text_change": True}):
            with self.subTest(change=change):
                right_rom, right_symbols = fixture(**change)
                right = manifest_from_rom(right_rom, right_symbols)
                if "text_change" in change:
                    self.assertEqual(left["tables"]["gMovesInfo"]["scalar_sha256"],
                                     right["tables"]["gMovesInfo"]["scalar_sha256"])
                with self.assertRaisesRegex(ManifestError, "disagree"):
                    require_same_scalar_tables({"main": left, "cormoria": right})

    def test_null_pointer_is_allowed_but_null_drift_rejected(self):
        left_rom, left_symbols = fixture()
        right_rom, right_symbols = fixture(null_change=True)
        left = manifest_from_rom(left_rom, left_symbols)
        right = manifest_from_rom(right_rom, right_symbols)
        self.assertEqual(left["tables"]["gMovesInfo"]["scalar_sha256"],
                         right["tables"]["gMovesInfo"]["scalar_sha256"])
        with self.assertRaisesRegex(ManifestError, "disagree"):
            require_same_scalar_tables({"main": left, "cormoria": right})
        require_same_scalar_tables({"main": right, "cormoria": right})

    def test_text_pointer_bounds_and_termination(self):
        rom, symbols = fixture()
        item_offset = symbols["gItemsInfo"][0] - BASE
        for pointer in (BASE - 4, BASE + len(rom), 0x0A000000):
            malformed = bytearray(rom)
            struct.pack_into("<I", malformed, item_offset + 4, pointer)
            with self.subTest(pointer=pointer), self.assertRaisesRegex(ManifestError, "outside"):
                manifest_from_rom(bytes(malformed), symbols)
        malformed = bytearray(rom)
        text_offset = len(malformed) - 2
        malformed[text_offset:] = b"AB"
        struct.pack_into("<I", malformed, item_offset + 4, BASE + text_offset)
        with self.assertRaisesRegex(ManifestError, "unterminated"):
            manifest_from_rom(bytes(malformed), symbols)
        malformed = bytearray(rom)
        text_offset = 4096
        malformed[text_offset:text_offset + MAX_TEXT_BYTES] = b"A" * MAX_TEXT_BYTES
        struct.pack_into("<I", malformed, item_offset + 4, BASE + text_offset)
        with self.assertRaisesRegex(ManifestError, "unterminated"):
            manifest_from_rom(bytes(malformed), symbols)

    def test_layout_and_symbol_bounds_are_validated(self):
        rom, symbols = fixture()
        malformed = bytearray(rom)
        struct.pack_into("<H", malformed, 16 + HEADER.size + 4, 10)
        with self.assertRaisesRegex(ManifestError, "pointer offsets"):
            manifest_from_rom(bytes(malformed), symbols)
        malformed = bytearray(rom)
        struct.pack_into("<H", malformed, 16 + HEADER.size + 102, 16)
        with self.assertRaisesRegex(ManifestError, "text offsets"):
            manifest_from_rom(bytes(malformed), symbols)
        outside = dict(symbols)
        outside["gItemsInfo"] = (BASE + len(rom) - 8, 64)
        with self.assertRaisesRegex(ManifestError, "outside"):
            manifest_from_rom(rom, outside)

    def test_malformed_elf_symbol_rejected(self):
        rom, symbols = fixture()
        listing = "\n".join(f"{name} R {address:x} {size:x}"
                            for name, (address, size) in symbols.items())
        self.assertEqual(symbols_from_nm(listing), symbols)
        with self.assertRaisesRegex(ManifestError, "duplicate"):
            symbols_from_nm(listing + "\n" + listing.splitlines()[0])
        with self.assertRaisesRegex(ManifestError, "descriptor size"):
            symbols_from_nm(listing.replace(f"{DESCRIPTOR_SIZE:x}", "1", 1))


if __name__ == "__main__":
    unittest.main()
