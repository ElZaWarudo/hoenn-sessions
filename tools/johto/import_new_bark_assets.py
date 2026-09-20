"""Import pinned Heart & Soul scenery into Crossroads' FRLG layout format.

Run from any directory with Python 3. Downloads source assets only, never a ROM.
The original uses 640 primary tiles/metatiles and 7 primary palettes, matching
our FRLG layouts, but stores Emerald's 16-bit metatile attributes.
"""

import hashlib
import json
from pathlib import Path
import struct
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[2]
REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
REPOSITORY = "https://github.com/PokemonHnS-Development/pokemonHnS"
BASE = f"https://raw.githubusercontent.com/PokemonHnS-Development/pokemonHnS/{REVISION}"
TILESETS = ("primary/johto_general", "secondary/new_bark_town")


def convert_attributes(data: bytes) -> bytes:
    """Retain behavior and layer; unused Emerald bits must remain empty."""
    result = bytearray()
    for (value,) in struct.iter_unpack("<H", data):
        if value & 0x0F00 or value >> 12 > 2:
            raise ValueError(f"Unsupported metatile attributes: {value:#06x}")
        behavior = value & 0xFF
        # HnS's HEADBUTT_TREE reuses Crossroads' INDIGO_PLATEAU_SIGN_1 ID.
        # Headbutt encounters are outside this scenery import; keep trees inert.
        if behavior == 0xA1:
            behavior = 0
        result.extend(struct.pack("<I", behavior | ((value >> 12) << 29)))
    return bytes(result)


def asset_paths() -> list[str]:
    """Return the complete source-relative scenery asset list without I/O."""
    paths = [f"data/layouts/NewBarkTown/{name}.bin" for name in ("map", "border")]
    for tileset in TILESETS:
        paths.extend(f"data/tilesets/{tileset}/{name}" for name in
                     ("tiles.png", "metatiles.bin", "metatile_attributes.bin"))
        paths.extend(f"data/tilesets/{tileset}/palettes/{number:02}.pal"
                     for number in range(13))
    return paths


def main() -> None:
    assets = []
    for path in asset_paths():
        with urlopen(f"{BASE}/{path}", timeout=30) as response:
            original = response.read()
        converted = convert_attributes(original) if path.endswith("metatile_attributes.bin") else original
        target = ROOT / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(converted)
        assets.append({"path": path, "source_sha256": hashlib.sha256(original).hexdigest(),
                       "import_sha256": hashlib.sha256(converted).hexdigest()})
    manifest = ROOT / "data/johto/new_bark_source.json"
    manifest.parent.mkdir(parents=True, exist_ok=True)
    manifest.write_text(json.dumps({"repository": REPOSITORY, "revision": REVISION,
                                   "assets": assets}, indent=2) + "\n", encoding="utf-8")
    print(f"Imported {len(assets)} pinned assets; attributes converted to FRLG format.")


if __name__ == "__main__":
    main()
