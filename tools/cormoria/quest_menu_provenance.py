"""Pinned provenance for the Cormoria quest journal import.

The journal is a data-preserving adaptation of the donor's quest table and
text.  The runtime deliberately does not copy the donor saveblock fields.
"""

DONOR_REPOSITORY = "https://github.com/dsmyst/dreamstone-mysteries"
DONOR_REVISION = "f7997186345885bfa23a170e5f573851fc034b9b"

DONOR_SOURCE_SHA256 = {
    "src/quests.c": "e900f39cbb96d7f60db819e9b2eb643d401f8fb9e3b851dbf91a9ed35e6d88fa",
    "include/quests.h": "7c0ab6da59e5597a2b524101cc202fc468bf6b2a06cb83eafd182285fe8236f0",
    "src/strings.c": "34a0883a890e255cb57f0ce33e8315ba9f2e1df54c2f2fc6a22434debdf21578",
}

DONOR_ASSET_SHA256 = {
    "graphics/quest_menu/menu.png": "7e10158abb8881afa7cf82a0e3455d640bf38f762cffd7235b564f322c726a90",
    "graphics/quest_menu/menu.pal": "2d545fd905a3e961109662ad75aa911c866bdeb8ef6abae1b15745518ecd7d75",
    "graphics/quest_menu/menu.bin": "4778b55a306f0d93160df3ed10bfb8622ad8508829b8f17d80206a45587e545c",
}


def provenance() -> dict[str, object]:
    """Return a stable, serialisable provenance record for release tooling."""

    return {
        "repository": DONOR_REPOSITORY,
        "revision": DONOR_REVISION,
        "source_sha256": dict(DONOR_SOURCE_SHA256),
        "asset_sha256": dict(DONOR_ASSET_SHA256),
        "save_storage": "CormoriaQuestState world-event flags",
        "legacy_save_compatibility": False,
    }
