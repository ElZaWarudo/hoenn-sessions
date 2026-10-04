"""Read-only population checks for the shared hoenn-box80-v1 fixture ABI.

Never writes or repairs saves. Species/level expectations supplement the
generic logical-field witness gate with the actual shared Pokémon layout.
"""
from __future__ import annotations

import hashlib
import struct

from live_harness_oracles import OracleFailure, FlashSave, logical_field

# src/pokemon.c sSubstructOffsets[SUBSTRUCT_TYPE_0]. The profile is explicit:
# another ROM family must supply its own ABI rather than guessing offsets.
GROWTH_OFFSETS = (0, 0, 0, 0, 0, 0, 1, 1, 2, 3, 2, 3,
                  1, 1, 2, 3, 2, 3, 1, 1, 2, 3, 2, 3)


def box_species(record: bytes) -> int | None:
    if len(record) != 80:
        raise OracleFailure("hoenn-box80-v1: wrong boxed Pokémon size")
    if not record[19] & 2:
        return None
    if record[19] & 1:
        raise OracleFailure("fixture Pokémon is a Bad Egg")
    personality, trainer = struct.unpack_from("<II", record)
    words = tuple(word ^ personality ^ trainer for (word,) in struct.iter_unpack("<I", record[32:]))
    if sum(word + (word >> 16) for word in words) & 0xFFFF != struct.unpack_from("<H", record, 28)[0]:
        raise OracleFailure("fixture Pokémon checksum failed")
    growth = GROWTH_OFFSETS[personality % 24] * 3
    species = words[growth] & 0x7FF
    if not species:
        raise OracleFailure("fixture Pokémon hasSpecies contradicts species zero")
    return species


def check_population(save: FlashSave, descriptor: bytes, recipe: dict) -> dict:
    if recipe.get("abi") != "hoenn-box80-v1":
        raise OracleFailure("fixture population needs an explicit supported ABI")
    party = logical_field(save, descriptor, 0x0101)
    pc = logical_field(save, descriptor, 0x0301)
    bag = logical_field(save, descriptor, 0x0106)
    if len(party) != 604 or len(pc) != 33600 or len(bag) % 4:
        raise OracleFailure("fixture population ABI differs from descriptor")
    count = party[0]
    if count > 6:
        raise OracleFailure("fixture party count exceeds capacity")
    species = [box_species(party[4 + i * 100:84 + i * 100]) for i in range(count)]
    if species != recipe["party_species"] or any(s is None for s in species):
        raise OracleFailure("fixture party species differ from recipe")
    if any(party[88 + i * 100] != recipe["party_level"] for i in range(count)):
        raise OracleFailure("fixture party levels differ from recipe")
    occupied = [(i, box_species(pc[i:i + 80])) for i in range(0, len(pc), 80) if pc[i + 19] & 2]
    if [s for _, s in occupied] != recipe["pc_species"]:
        raise OracleFailure("fixture PC species differ from recipe")
    items = [(i, *struct.unpack_from("<HH", bag, i)) for i in range(0, len(bag), 4)]
    selected = []
    for item, quantity in recipe["bag_items"]:
        matches = [(i, amount) for i, ident, amount in items if ident == item]
        if not matches or sum(amount for _, amount in matches) != quantity:
            raise OracleFailure("fixture Bag item/quantity differs from recipe")
        selected.append(matches[0][0])
    if not count or not occupied or not selected:
        raise OracleFailure("fixture recipe must populate party, PC and Bag")
    def witness(fid: int, value: bytes, offset: int, size: int) -> dict:
        return {"field_id": fid, "sha256": hashlib.sha256(value).hexdigest(),
                "offset": offset, "size": size, "min_nonzero_bytes": 1}
    return {"save_sha256": save.sha256, "generation": save.generation,
            "party_species": species, "party_level": recipe["party_level"],
            "pc_species": [s for _, s in occupied], "bag_items": recipe["bag_items"],
            "shared_witnesses": [witness(0x0101, party, 0, 1),
                                 witness(0x0301, pc, occupied[0][0] + 19, 1),
                                 witness(0x0106, bag, selected[0], 4)]}
