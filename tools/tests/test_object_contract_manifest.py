import hashlib
import struct
import unittest

from tools.coop.object_contract_manifest import (
    DESCRIPTOR, DESCRIPTOR_SIZE, ENTRY, HEADER, MAGIC, TABLES,
    ManifestError, manifest_from_rom, require_same_scalar_tables,
    symbols_from_nm,
)


BASE = 0x08000000


def fixture(pointer_shift=0, table_shift=0, scalar_change=False, null_change=False):
    rom = bytearray(2048)
    descriptor = bytearray(DESCRIPTOR_SIZE)
    HEADER.pack_into(descriptor, 0, MAGIC, 1, len(TABLES))
    for index in range(len(TABLES)):
        pointer_count = 0 if index == 4 else 1
        ENTRY.pack_into(descriptor, HEADER.size + index * ENTRY.size,
                        12 if pointer_count else 4, pointer_count,
                        *([4] + [0] * 47 if pointer_count else [0] * 48))
    rom[16:16 + len(descriptor)] = descriptor
    symbols = {DESCRIPTOR: (BASE + 16, len(descriptor))}
    for index, name in enumerate(TABLES):
        offset = 768 + table_shift + index * 128
        stride = 4 if index == 4 else 12
        payload = bytearray(stride * 2)
        for row in range(2):
            struct.pack_into("<I", payload, row * stride, index * 10 + row)
            if stride == 12:
                struct.pack_into("<I", payload, row * stride + 4,
                                 BASE + pointer_shift + 0x1000 + index * 16 + row * 4)
                struct.pack_into("<I", payload, row * stride + 8, 900 + row)
        if scalar_change and index == 2:
            payload[12] ^= 1
        if null_change and index == 2:
            struct.pack_into("<I", payload, 4, 0)
        rom[offset:offset + len(payload)] = payload
        symbols[name] = (BASE + offset, len(payload))
    return bytes(rom), symbols


class ObjectContractManifestTests(unittest.TestCase):
    def test_pointer_relocation_does_not_change_scalar_hash(self):
        left_rom, left_symbols = fixture()
        right_rom, right_symbols = fixture(pointer_shift=0x10000, table_shift=64)
        left = manifest_from_rom(left_rom, left_symbols)
        right = manifest_from_rom(right_rom, right_symbols)
        require_same_scalar_tables({"main": left, "cormoria": right})
        self.assertNotEqual(left["tables"]["gItemsInfo"]["raw_sha256"],
                            right["tables"]["gItemsInfo"]["raw_sha256"])
        self.assertNotEqual(left["tables"]["gItemsInfo"]["address"],
                            right["tables"]["gItemsInfo"]["address"])
        self.assertEqual(left["tables"]["gItemsInfo"]["scalar_sha256"],
                         right["tables"]["gItemsInfo"]["scalar_sha256"])
        self.assertEqual(left["tables"]["gItemsInfo"]["pointer_presence_sha256"],
                         right["tables"]["gItemsInfo"]["pointer_presence_sha256"])
        self.assertEqual(left["rom_sha256"], hashlib.sha256(left_rom).hexdigest())

    def test_scalar_drift_rejected(self):
        left_rom, left_symbols = fixture()
        right_rom, right_symbols = fixture(pointer_shift=0x10000, scalar_change=True)
        with self.assertRaisesRegex(ManifestError, "disagree"):
            require_same_scalar_tables({"main": manifest_from_rom(left_rom, left_symbols),
                                        "cormoria": manifest_from_rom(right_rom, right_symbols)})

    def test_null_pointer_drift_rejected_even_when_scalar_bytes_match(self):
        left_rom, left_symbols = fixture()
        right_rom, right_symbols = fixture(null_change=True)
        left = manifest_from_rom(left_rom, left_symbols)
        right = manifest_from_rom(right_rom, right_symbols)
        self.assertEqual(left["tables"]["gMovesInfo"]["scalar_sha256"],
                         right["tables"]["gMovesInfo"]["scalar_sha256"])
        with self.assertRaisesRegex(ManifestError, "disagree"):
            require_same_scalar_tables({"main": left, "cormoria": right})

    def test_pointer_layout_and_bounds_are_validated(self):
        rom, symbols = fixture()
        malformed = bytearray(rom)
        struct.pack_into("<H", malformed, 16 + HEADER.size + 4, 10)
        with self.assertRaisesRegex(ManifestError, "pointer offsets"):
            manifest_from_rom(bytes(malformed), symbols)
        outside = dict(symbols)
        outside["gItemsInfo"] = (BASE + len(rom) - 8, 24)
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
