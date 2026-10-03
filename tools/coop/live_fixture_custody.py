"""Read-only semantic custody checks; unit byte fixtures are never ROM proof.

hoenn-mail-daycare-v1 uses the signed box80 descriptor aggregate sizes and
APCS-GNU source layouts. Padding is ignored here but retained in field hashes.
"""
from __future__ import annotations

import hashlib
import struct

from live_fixture_population import GROWTH_OFFSETS, box_species
from live_harness_oracles import FlashSave, OracleFailure, logical_field


def _fail(message: str) -> None:
    raise OracleFailure("custody: " + message)


def _keys(value: object, expected: set[str], name: str) -> None:
    if not isinstance(value, dict) or set(value) != expected:
        _fail(name + " recipe fields differ")


def _integer(value: object, low: int, high: int, name: str) -> int:
    if type(value) is not int or not low <= value <= high:
        _fail(name + " is out of range")
    return value


def _hex(value: object, size: int, name: str) -> bytes:
    if not isinstance(value, str) or len(value) != size * 2:
        _fail(name + " needs exact-length hex")
    try:
        data = bytes.fromhex(value)
    except ValueError:
        _fail(name + " needs hex")
    if len(data) != size:
        _fail(name + " needs exact-length hex")
    return data


def _mail_recipe(value: dict) -> None:
    _keys(value, {"words", "species", "item_id"}, "mail")
    words = value["words"]
    if not isinstance(words, list) or len(words) != 9:
        _fail("mail needs nine words")
    for word in words:
        _integer(word, 0, 65535, "mail word")
    if all(word == 65535 for word in words):
        _fail("mail message is unwritten")
    _integer(value["species"], 1, 2047, "mail species")
    _integer(value["item_id"], 199, 210, "mail item")


def _mail(record: bytes, expected: dict, sender: bytes, trainer: bytes) -> None:
    if len(record) != 36:
        _fail("Mail ABI size differs")
    words = list(struct.unpack_from("<9H", record))
    if all(word == 65535 for word in words):
        _fail("mail message is unwritten")
    if words != expected["words"]:
        _fail("mail words differ")
    if record[18:26] != sender or record[26:30] != trainer:
        _fail("mail sender/trainer differ")
    if struct.unpack_from("<HH", record, 30) != (expected["species"], expected["item_id"]):
        _fail("mail species/item differ")


def _carrier(record: bytes, expected: dict, held: int) -> None:
    species = box_species(record)  # validates checksum and Bad Egg before decrypting
    if species is None or species != expected["species"]:
        _fail("carrier species absent or different")
    personality, trainer = struct.unpack_from("<II", record)
    if (personality, trainer) != (expected["personality"], expected["ot_id"]):
        _fail("carrier identity differs")
    offset = 32 + GROWTH_OFFSETS[personality % 24] * 12
    growth = struct.unpack_from("<I", record, offset)[0] ^ personality ^ trainer
    if (growth >> 16) & 1023 != held:
        _fail("carrier held item differs")
    if record[19] & 4:
        _fail("carrier is an egg")


def check_custody(save: FlashSave, descriptor: bytes, recipe: dict) -> dict:
    """Require party mail, PC mailbox and daycare mail witnesses from a recipe."""
    _keys(recipe, {"abi", "sender_name_hex", "trainer_id_hex", "party_mail", "pc_mail", "daycare"}, "custody")
    if recipe["abi"] != "hoenn-mail-daycare-v1":
        _fail("unsupported ABI")
    sender = _hex(recipe["sender_name_hex"], 8, "sender name")
    trainer = _hex(recipe["trainer_id_hex"], 4, "trainer ID")
    for name in ("party_mail", "pc_mail", "daycare"):
        if not isinstance(recipe[name], list) or not recipe[name]:
            _fail(name + " recipe is empty")
        slots = set()
        for entry in recipe[name]:
            common = {"slot", "mail"}
            carrier = {"species", "personality", "ot_id"}
            extra = {"mail_index"} if name == "party_mail" else {"ot_name_hex", "mon_name_hex", "game_language", "mon_language", "steps"}
            _keys(entry, common if name == "pc_mail" else common | carrier | extra, name)
            low, high = (6, 15) if name == "pc_mail" else ((0, 5) if name == "party_mail" else (0, 1))
            slot = _integer(entry["slot"], low, high, name + " slot")
            if slot in slots:
                _fail(name + " repeats a slot")
            slots.add(slot)
            _mail_recipe(entry["mail"])
            if name != "pc_mail":
                _integer(entry["species"], 1, 2047, "carrier species")
                # This fixture profile covers ordinary species, not Unown's
                # 30000+ letter encoding in Mail.species.
                if (entry["species"] == 201 or 1024 <= entry["species"] <= 1050
                        or entry["mail"]["species"] != entry["species"]):
                    _fail("mail species must match a non-Unown carrier")
                for key in ("personality", "ot_id"):
                    _integer(entry[key], 0, 0xFFFFFFFF, key)
            if name == "party_mail":
                _integer(entry["mail_index"], 0, 5, "party mail index")
            elif name == "daycare":
                _hex(entry["ot_name_hex"], 8, "daycare OT name")
                _hex(entry["mon_name_hex"], 11, "daycare mon name")
                for key in ("game_language", "mon_language"):
                    _integer(entry[key], 0, 15, key)
                _integer(entry["steps"], 0, 0xFFFFFFFF, "daycare steps")
    party = logical_field(save, descriptor, 0x0101)
    mail = logical_field(save, descriptor, 0x010B)
    daycare = logical_field(save, descriptor, 0x010D)
    if (len(party), len(mail), len(daycare)) != (604, 576, 288):
        _fail("descriptor ABI sizes differ")
    if party[0] > 6:
        _fail("party count exceeds capacity")
    used = set()
    for entry in recipe["party_mail"]:
        slot, index = entry["slot"], entry["mail_index"]
        if slot >= party[0]:
            _fail("party mail slot is absent")
        base = 4 + slot * 100
        _carrier(party[base:base + 80], entry, entry["mail"]["item_id"])
        if party[base + 85] != index or index in used:
            _fail("party mail index is dangling or duplicated")
        used.add(index)
        _mail(mail[index * 36:(index + 1) * 36], entry["mail"], sender, trainer)
    for entry in recipe["pc_mail"]:
        slot = entry["slot"]
        _mail(mail[slot * 36:(slot + 1) * 36], entry["mail"], sender, trainer)
    for entry in recipe["daycare"]:
        base = entry["slot"] * 140
        record = daycare[base:base + 140]
        _carrier(record[:80], entry, 0)
        _mail(record[80:116], entry["mail"], sender, trainer)
        if record[116:124] != _hex(entry["ot_name_hex"], 8, "daycare OT name") or record[124:135] != _hex(entry["mon_name_hex"], 11, "daycare mon name"):
            _fail("daycare names differ")
        if record[135] != entry["game_language"] | entry["mon_language"] << 4:
            _fail("daycare languages differ")
        if struct.unpack_from("<I", record, 136)[0] != entry["steps"]:
            _fail("daycare steps differ")
    return {"save_sha256": save.sha256, "generation": save.generation,
            "abi": recipe["abi"], "shared_witnesses": [
                {"field_id": fid, "sha256": hashlib.sha256(value).hexdigest(),
                 "offset": 0, "size": len(value), "min_nonzero_bytes": 1}
                for fid, value in ((0x0101, party), (0x010B, mail), (0x010D, daycare))]}
