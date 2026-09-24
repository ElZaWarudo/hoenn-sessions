/* Cormoria ItemInfo data from pinned Dreamstone Mysteries revision
 * f7997186345885bfa23a170e5f573851fc034b9b. Shared item IDs are
 * allocated by data/shared_item_ids.json, not donor numbers. */

    [ITEM_ANCIENT_STONE] =
    {
        .name = ITEM_NAME("Ancient Stone"),
        .price = 0,
        .holdEffect = HOLD_EFFECT_MEGA_STONE,
        .description = COMPOUND_STRING(
            "A strange stone\n"
            "found in Ancient\n"
            "Cormoria."),
        .pocket = POCKET_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .flingPower = 80,
        .iconPic = gItemIcon_Aerodactylite,
        .iconPalette = gItemIconPalette_Aerodactylite,
    },

    [ITEM_APPOINTMENT_LETTER] =
    {
        .name = ITEM_NAME("Appt. Letter"),
        .price = 0,
        .description = COMPOUND_STRING(
            "Appointment letter\n"
            "for the Ceram Base\n"
            "Camp Gym."),
        .importance = 2,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_Letter,
        .iconPalette = gItemIconPalette_LavaCookieAndLetter,
    },

    [ITEM_ARCHAEOLENS] =
    {
        .name = ITEM_NAME("Archaeolens"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A unique machine\n"
            "that can scan and\n"
            "record statues."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_SilphScope,
        .iconPalette = gItemIconPalette_SilphScope,
    },

    [ITEM_ARCHAEOLENS_2] =
    {
        .name = ITEM_NAME("Archaeolens 2.0"),
        .price = 0,
        .description = COMPOUND_STRING(
            "An upgrade to the\n"
            "Archaeolens that\n"
            "can scan carvings."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_SilphScope,
        .iconPalette = gItemIconPalette_SilphScope,
    },

    [ITEM_BACKSTAGE_PASS] =
    {
        .name = ITEM_NAME("Backstage Pass"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A backstage pass\n"
            "to the Silversun\n"
            "Theater."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_MysticTicket,
        .iconPalette = gItemIconPalette_MysticTicket,
    },

    [ITEM_DETECTIVE_STUDENT_ID] =
    {
        .name = ITEM_NAME("Student ID"),
        .pluralName = ITEM_PLURAL_NAME("Student IDs"),
        .price = 0,
        .description = COMPOUND_STRING(
            "One-day student ID\n"
            "for the Galecrest\n"
            "Detective Academy."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_TriPass,
        .iconPalette = gItemIconPalette_TriPass,
    },

    [ITEM_DIAMOND] =
    {
        .name = ITEM_NAME("Diamond"),
        .pluralName = ITEM_PLURAL_NAME("Diamond"),
        .description = COMPOUND_STRING(
            "A lustrous gleaming\n"
            "gem that symbolizes\n"
            "virtue."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_ExpCandyXL,
        .iconPalette = gItemIconPalette_ExpCandies,
    },

    [ITEM_DRIFBLIM_TRAVELS_PASS] =
    {
        .name = ITEM_NAME("Drifblim Pass"),
        .pluralName = ITEM_PLURAL_NAME("Drifblim Passes"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A commuter pass\n"
            "given by Drifblim\n"
            "Travels Pvt. Ltd."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_EonTicket,
        .iconPalette = gItemIconPalette_EonTicket,
    },

    [ITEM_FAKE_STUDENT_ID] =
    {
        .name = ITEM_NAME("Fake ID"),
        .pluralName = ITEM_PLURAL_NAME("Fake IDs"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A fake student ID\n"
            "for the Galecrest\n"
            "Detective Academy."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_ContestPass,
        .iconPalette = gItemIconPalette_ContestPass,
    },

    [ITEM_GACHA_TOKEN] =
    {
        .name = ITEM_NAME("Gacha Token"),
        .description = COMPOUND_STRING(
            "A one-time token\n"
            "to be used with a\n"
            "gacha machine."),
        .pocket = POCKET_KEY_ITEMS,
        .importance = 1,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_GimmighoulCoin,
        .iconPalette = gItemIconPalette_GimmighoulCoin,
    },

    [ITEM_HEAL_PASS] =
    {
        .name = ITEM_NAME("Heal Pass"),
        .pluralName = ITEM_PLURAL_NAME("Heal Passes"),
        .price = 0,
        .description = COMPOUND_STRING(
            "Trade with Nurse\n"
            "Joy on any route to\n"
            "heal your Pokémon."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_ContestPass,
        .iconPalette = gItemIconPalette_ContestPass,
    },

    [ITEM_HISTORIAN_MEDAL] =
    {
        .name = ITEM_NAME("Historian Medal"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A collectible item\n"
            "awarded to special\n"
            "persons."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_AbilityShield,
        .iconPalette = gItemIconPalette_AbilityShield,
    },

    [ITEM_HM_SPLASH] =
    {
        .name = ITEM_NAME("HM10"),
        .price = 0,
        .description = COMPOUND_STRING(
            "It's just a splash.\n"
            "It has no effect\n"
            "whatsoever."),            
        .importance = 1,
        .pocket = POCKET_TM_HM,
        .type = ITEM_USE_PARTY_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_TMHM,
        .secondaryId = MOVE_SPLASH,
    },

    [ITEM_LAB_WELCOMEPACKAGE] =
    {
        .name = ITEM_NAME("Lab Package"),
        .price = 0,
        .description = COMPOUND_STRING(
            "Just a bunch of\n"
            "notes and things.\n"
        ),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_Parcel,
        .iconPalette = gItemIconPalette_Parcel,
    },

    [ITEM_NUZKIT] =
    {
        .name = ITEM_NAME("Nuzkit"),
        .price = 0,
        .description = COMPOUND_STRING(
            "Mom says I should\n"
            "talk to my Pokémon\n"
            "to use it."),
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .importance = 1,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .flingPower = 30,
        .iconPic = gItemIcon_Sachet,
        .iconPalette = gItemIconPalette_Sachet,
    },

    [ITEM_ORPHANAGE_BOOK] =
    {
        .name = ITEM_NAME("Book"),
        .pluralName = ITEM_PLURAL_NAME("Books"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A book I found in\n"
            "the Silversun\n"
            "Blind Orphanage."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_RotomCatalog,
        .iconPalette = gItemIconPalette_RotomCatalog,
    },

    [ITEM_POCKET_BOY] =
    {
        .name = ITEM_NAME("PocketBoy"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A new-generation\n"
            "handheld gaming\n"
            "console. So cool!"),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_FameChecker,
        .iconPalette = gItemIconPalette_FameChecker,
    },

    [ITEM_POCKET_DRIVE] =
    {
        .name = ITEM_NAME("Pocket Drive"),
        .price = (I_PRICE >= GEN_7) ? 0 : 1000,
        .holdEffect = HOLD_EFFECT_DRIVE,
        .description = COMPOUND_STRING(
            "A small drive\n"
            "that was part of\n"
            "a PocketBoy."),
        .pocket = POCKET_KEY_ITEMS,
        .importance = 1,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_Drive,
        .iconPalette = gItemIconPalette_DouseDrive,
    },

    [ITEM_PURPLE_SCARF] =
    {
        .name = ITEM_NAME("Purple Scarf"),
        .pluralName = ITEM_PLURAL_NAME("Purple Scarves"),
        .price = 100,
        .description = COMPOUND_STRING(
            "A tattered old\n"
            "purple scarf for\n"
            "a young boy."),
        .pocket = POCKET_KEY_ITEMS,
        .importance = 1,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .flingPower = 10,
        .iconPic = gItemIcon_Scarf,
        .iconPalette = gItemIconPalette_PinkScarf,
    },

    [ITEM_RANGER_CARD] =
    {
        .name = ITEM_NAME("Ranger Card"),
        .price = 0,
        .description = COMPOUND_STRING(
            "The official card\n"
            "proving that I'm a\n"
            "Pokémon Ranger!"),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_CardKey,
        .iconPalette = gItemIconPalette_CardKey,
    },

    [ITEM_RANGER_CREST] =
    {
        .name = ITEM_NAME("Ranger Shield"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A collectible item\n"
            "for a distiguished\n"
            "Pokémon Ranger."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_AbilityShield,
        .iconPalette = gItemIconPalette_AbilityShield,
    },

    [ITEM_RANGER_PACKAGE] =
    {
        .name = ITEM_NAME("Ranger Package"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A package for the\n"
            "Ranger Institute\n"
            "at Ivy River.\n"
        ),
        .importance = 1,                
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_Parcel,
        .iconPalette = gItemIconPalette_Parcel,
    },

    [ITEM_RARE_SHARD] =
    {
        .name = ITEM_NAME("Rare Shard"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A surprisingly\n"
            "heavy shard of an\n"
            "unknown material."),
        .pocket = POCKET_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_StellarTeraShard,
        .iconPalette = gItemIconPalette_StellarTeraShard,
    },

    [ITEM_RETRO_DRIVE] =
    {
        .name = ITEM_NAME("Retro Port"),
        .price = (I_PRICE >= GEN_7) ? 0 : 1000,
        .holdEffect = HOLD_EFFECT_DRIVE,
        .description = COMPOUND_STRING(
            "A clunky part\n"
            "of the Retro64\n"
            "emulator."),
        .pocket = POCKET_KEY_ITEMS,
        .importance = 1,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_Drive,
        .iconPalette = gItemIconPalette_ChillDrive,
    },

    [ITEM_SMUGGLER_EMBLEM] =
    {
        .name = ITEM_NAME("Smuggler Emblem"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A medal-like item in\n"
            "dubious condition.\n"
            "Was it stolen?"),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_MagmaEmblem,
        .iconPalette = gItemIconPalette_MagmaEmblem,
    },

    [ITEM_STRANGE_ROCK] =
    {
        .name = ITEM_NAME("Strange Rock"),
        .price = 0,
        .description = COMPOUND_STRING(
            "A strange rock."),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_Everstone, //tera shard looks cool
        .iconPalette = gItemIconPalette_Everstone,
    },

    [ITEM_SWAP_DRIVE] =
    {
        .name = ITEM_NAME("Swap Chip"),
        .price = (I_PRICE >= GEN_7) ? 0 : 1000,
        .holdEffect = HOLD_EFFECT_DRIVE,
        .description = COMPOUND_STRING(
            "A component of\n"
            "the new Swap 2\n"
            "gaming console."),
        .pocket = POCKET_KEY_ITEMS,
        .importance = 1,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_Drive,
        .iconPalette = gItemIconPalette_BurnDrive,
    },

    [ITEM_TIME_SEED] =
    {
        .name = ITEM_NAME("Seed of Time"),
        .description = COMPOUND_STRING(
            "A strange, withered\n"
            "old seed. Where can\n"
            "it be planted?"),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_MiracleSeed,
        .iconPalette = gItemIconPalette_MiracleSeed,
    },

    [ITEM_TIME_WATER] =
    {
        .name = ITEM_NAME("Water of Time"),
        .description = COMPOUND_STRING(
            "A bottle of mystical\n"
            "glowing water. What\n"
            "is it used for?"),
        .importance = 1,
        .pocket = POCKET_KEY_ITEMS,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_FreshWater,
        .iconPalette = gItemIconPalette_FreshWater,
    },

    [ITEM_TREKKING_BOOTS] =
    {
        .name = ITEM_NAME("Trekking Boots"),
        .pluralName = ITEM_PLURAL_NAME("Trekking Boots"),
        .price = 999999,
        .description = COMPOUND_STRING(
            "A pair of heavyset\n"
            "boots perfect for\n"
            "tough treks."),
        .pocket = POCKET_KEY_ITEMS,
        .importance = 1,
        .type = ITEM_USE_BAG_MENU,
        .fieldUseFunc = ItemUseOutOfBattle_CannotUse,
        .iconPic = gItemIcon_HeavyDutyBoots,
        .iconPalette = gItemIconPalette_HeavyDutyBoots,
    },
