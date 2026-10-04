"""Canonical unit bytes only: never upload these as ROM-written fixtures."""
import copy
import hashlib
import struct
import sys
import unittest
from itertools import permutations
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
from live_fixture_custody import check_custody
from live_harness_oracles import OracleFailure, check_shared_witnesses


class CustodyTests(unittest.TestCase):
    def setUp(self):
        self.sender = b"\xbb\xbc\xbd\xbe\xbf\x00\x00\xff"
        self.trainer = bytes.fromhex("cdab3412")
        self.message = {"words": [1, 2, 3, 4, 5, 6, 7, 8, 65535], "species": 25, "item_id": 200}
        carrier = {"species": 25, "personality": 17, "ot_id": 0x1234ABCD}
        self.recipe = {"abi": "hoenn-mail-daycare-v1", "sender_name_hex": self.sender.hex(),
                       "trainer_id_hex": self.trainer.hex(),
                       "party_mail": [dict(carrier, slot=0, mail_index=0, mail=copy.deepcopy(self.message))],
                       "pc_mail": [{"slot": 6, "mail": copy.deepcopy(self.message)}],
                       "daycare": [dict(carrier, slot=0, mail=copy.deepcopy(self.message),
                                        ot_name_hex=self.sender.hex(), mon_name_hex="bbbcbdbe000000000000ff",
                                        game_language=2, mon_language=2, steps=0)]}
        self.party = bytearray(604)
        self.party[0] = 1
        self.party[4:84] = self.box(17, 200)
        self.party[89] = 0
        self.mail = bytearray(576)
        self.mail[:36] = self.letter()
        self.mail[216:252] = self.letter()
        self.daycare = bytearray(288)
        self.daycare[:80] = self.box(17, 0)
        self.daycare[80:116] = self.letter()
        self.daycare[116:124] = self.sender
        self.daycare[124:135] = bytes.fromhex(self.recipe["daycare"][0]["mon_name_hex"])
        self.daycare[135] = 0x22
        self.save = SimpleNamespace(sha256="a" * 64, generation=7)

    def box(self, personality, held):
        result = bytearray(80)
        trainer = 0x1234ABCD
        struct.pack_into("<II", result, 0, personality, trainer)
        result[19] = 2
        order = tuple(permutations((0, 1, 2, 3)))[personality % 24]
        words = [0] * 12
        # Canonical growth record order generated independently from the decoder.
        words[order.index(0) * 3] = 25 | (31 << 11) | (held << 16) | (63 << 26)
        struct.pack_into("<H", result, 28, sum(w + (w >> 16) for w in words) & 65535)
        struct.pack_into("<12I", result, 32, *(w ^ personality ^ trainer for w in words))
        return result

    def letter(self):
        return struct.pack("<9H", *self.message["words"]) + self.sender + self.trainer + struct.pack("<HH", 25, 200) + b"\x91\x92"

    def verify(self, recipe=None, party=None, mail=None, daycare=None):
        fields = {0x0101: bytes(self.party if party is None else party),
                  0x010B: bytes(self.mail if mail is None else mail),
                  0x010D: bytes(self.daycare if daycare is None else daycare)}
        with mock.patch("live_fixture_custody.logical_field", side_effect=lambda s, d, fid: fields[fid]):
            return check_custody(self.save, b"", self.recipe if recipe is None else recipe)

    def test_all_permutations_and_full_field_witnesses(self):
        for personality in range(24):
            with self.subTest(personality=personality):
                recipe = copy.deepcopy(self.recipe)
                recipe["party_mail"][0]["personality"] = personality
                recipe["daycare"][0]["personality"] = personality
                party, daycare = bytearray(self.party), bytearray(self.daycare)
                party[4:84] = self.box(personality, 200)
                daycare[:80] = self.box(personality, 0)
                result = self.verify(recipe, party=party, daycare=daycare)
                self.assertEqual([w["field_id"] for w in result["shared_witnesses"]], [0x0101, 0x010B, 0x010D])
                self.assertEqual(result["shared_witnesses"][1]["sha256"], hashlib.sha256(self.mail).hexdigest())

    def test_padding_is_semantically_ignored_but_hashed(self):
        changed = bytearray(self.mail)
        changed[34:36] = b"\xff\xff"
        self.assertNotEqual(self.verify()["shared_witnesses"], self.verify(mail=changed)["shared_witnesses"])
        changed = bytearray(self.daycare)
        changed[114:116] = b"\xff\xff"
        self.verify(daycare=changed)

    def test_last_pc_and_daycare_slots_and_dangling_duplicate_index(self):
        recipe = copy.deepcopy(self.recipe)
        recipe["pc_mail"][0]["slot"] = 15
        recipe["daycare"][0]["slot"] = 1
        mail, daycare = bytearray(self.mail), bytearray(self.daycare)
        mail[540:576] = self.letter()
        daycare[140:280] = self.daycare[:140]
        self.verify(recipe, mail=mail, daycare=daycare)
        recipe = copy.deepcopy(self.recipe)
        second = dict(recipe["party_mail"][0], slot=1)
        recipe["party_mail"].append(second)
        party = bytearray(self.party)
        party[0] = 2
        party[104:184] = self.box(17, 200)
        with self.assertRaisesRegex(OracleFailure, "duplicated"):
            self.verify(recipe, party=party)

    def test_valid_checksum_cannot_mask_wrong_species_or_trainer(self):
        for group in ("party_mail", "daycare"):
            for key, value in (("species", 26), ("ot_id", 1)):
                recipe = copy.deepcopy(self.recipe)
                recipe[group][0][key] = value
                with self.subTest(group=group, key=key), self.assertRaises(OracleFailure):
                    self.verify(recipe)

    def test_party_attachment_change_invalidates_retained_full_field_witness(self):
        baseline = self.verify()["shared_witnesses"]
        for boundary in ("mail index", "held item"):
            with self.subTest(boundary=boundary):
                party = bytearray(self.party)
                if boundary == "mail index":
                    party[89] = 255
                else:
                    party[4:84] = self.box(17, 0)
                with self.assertRaises(OracleFailure):
                    self.verify(party=party)
                values = {0x0101: bytes(party), 0x010B: bytes(self.mail), 0x010D: bytes(self.daycare)}
                fields = [{"id": fid, "ownership": 1} for fid in values]
                with mock.patch("live_harness_oracles.parse_schema_payload", return_value={"fields": fields}), \
                     mock.patch("live_harness_oracles.logical_field", side_effect=lambda s, d, fid: values[fid]):
                    with self.assertRaisesRegex(OracleFailure, "changed"):
                        check_shared_witnesses(self.save, b"", baseline)

    def test_carrier_mail_species_mismatch_and_unown_are_rejected(self):
        for group in ("party_mail", "daycare"):
            recipe = copy.deepcopy(self.recipe)
            recipe[group][0]["mail"]["species"] = 26
            with self.assertRaisesRegex(OracleFailure, "mail species must match"):
                self.verify(recipe)
            recipe = copy.deepcopy(self.recipe)
            recipe[group][0]["species"] = 201
            recipe[group][0]["mail"]["species"] = 201
            with self.assertRaisesRegex(OracleFailure, "non-Unown"):
                self.verify(recipe)
            for species in (1024, 1050):
                recipe[group][0]["species"] = species
                recipe[group][0]["mail"]["species"] = species
                with self.assertRaisesRegex(OracleFailure, "non-Unown"):
                    self.verify(recipe)

    def test_each_mail_boundary_rejects_unwritten_wrong_words_sender_and_item(self):
        for field, base in (("mail", 0), ("mail", 216), ("daycare", 80)):
            for relative, value in ((0, b"\xff" * 18), (0, b"\x00\x00"), (18, b"\x00"),
                                    (26, b"\x00"), (30, b"\x00\x00"), (32, b"\x00\x00")):
                with self.subTest(field=field, base=base, relative=relative):
                    damaged = bytearray(getattr(self, field))
                    damaged[base + relative:base + relative + len(value)] = value
                    with self.assertRaises(OracleFailure):
                        self.verify(**{field: damaged})

    def test_party_index_count_checksum_identity_and_held_item(self):
        for position, value in ((0, 0), (0, 7), (89, 255), (89, 6), (4 + 32, self.party[36] ^ 1),
                                (4 + 19, 0), (4 + 19, 3), (4 + 19, 6)):
            with self.subTest(position=position, value=value):
                changed = bytearray(self.party)
                changed[position] = value
                with self.assertRaises(OracleFailure):
                    self.verify(party=changed)
        for held in (0, 199):
            changed = bytearray(self.party)
            changed[4:84] = self.box(17, held)
            with self.assertRaisesRegex(OracleFailure, "held item"):
                self.verify(party=changed)
        changed = bytearray(self.party)
        changed[4:84] = self.box(18, 200)
        with self.assertRaisesRegex(OracleFailure, "identity"):
            self.verify(party=changed)

    def test_daycare_identity_checksum_names_languages_steps_and_item(self):
        for position in (32, 19, 116, 124, 135, 136):
            with self.subTest(position=position):
                changed = bytearray(self.daycare)
                changed[position] ^= 1
                with self.assertRaises(OracleFailure):
                    self.verify(daycare=changed)
        for personality, held in ((18, 0), (17, 200)):
            changed = bytearray(self.daycare)
            changed[:80] = self.box(personality, held)
            with self.assertRaises(OracleFailure):
                self.verify(daycare=changed)

    def test_bad_recipe_and_wrong_descriptor_sizes(self):
        for key, value in (("abi", "unknown"), ("party_mail", []), ("pc_mail", []), ("daycare", []),
                           ("sender_name_hex", "zz" * 8), ("trainer_id_hex", "00")):
            recipe = copy.deepcopy(self.recipe)
            recipe[key] = value
            with self.assertRaises(OracleFailure):
                self.verify(recipe)
        for group, key, values in (("party_mail", "slot", [True, -1, 6]),
                                   ("party_mail", "mail_index", [6, 255]),
                                   ("pc_mail", "slot", [0, 5, 16]),
                                   ("daycare", "slot", [2]), ("daycare", "steps", [-1]),
                                   ("daycare", "species", [0, 2048]),
                                   ("daycare", "game_language", [16])):
            for value in values:
                recipe = copy.deepcopy(self.recipe)
                recipe[group][0][key] = value
                with self.subTest(group=group, key=key, value=value), self.assertRaises(OracleFailure):
                    self.verify(recipe)
        for words in ([65535] * 9, [1], [True] * 9):
            recipe = copy.deepcopy(self.recipe)
            recipe["pc_mail"][0]["mail"]["words"] = words
            with self.assertRaises(OracleFailure):
                self.verify(recipe)
        for group in ("party_mail", "pc_mail", "daycare"):
            recipe = copy.deepcopy(self.recipe)
            recipe[group].append(copy.deepcopy(recipe[group][0]))
            with self.assertRaises(OracleFailure):
                self.verify(recipe)
        recipe = copy.deepcopy(self.recipe)
        recipe["extra"] = 1
        with self.assertRaises(OracleFailure):
            self.verify(recipe)
        for field in ("party", "mail", "daycare"):
            with self.assertRaisesRegex(OracleFailure, "ABI sizes"):
                self.verify(**{field: bytes(getattr(self, field))[:-1]})


if __name__ == "__main__":
    unittest.main()
