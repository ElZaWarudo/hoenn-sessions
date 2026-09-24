// Object-event graphics descriptors extracted from the pinned Dreamstone
// source.  Every symbol is Cormoria-scoped to keep future world additions
// from colliding with the Hoenn and Johto tables.

#define CORMORIA_PERSON_INFO(pic_table, palette_tag, palette_slot) \
{ \
    .tileTag = TAG_NONE, \
    .paletteTag = palette_tag, \
    .reflectionPaletteTag = OBJ_EVENT_PAL_TAG_NONE, \
    .size = 256, \
    .width = 16, \
    .height = 32, \
    .paletteSlot = palette_slot, \
    .shadowSize = SHADOW_SIZE_M, \
    .inanimate = FALSE, \
    .compressed = FALSE, \
    .tracks = TRACKS_FOOT, \
    .oam = &gObjectEventBaseOam_16x32, \
    .subspriteTables = sOamTables_16x32, \
    .anims = sAnimTable_Standard, \
    .images = pic_table, \
    .affineAnims = gDummySpriteAffineAnimTable, \
}

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaBugCatcherF = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_BugCatcherF, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaLeaderAriana = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_LeaderAriana, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaLeaderCarona = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_LeaderCarona, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaLeaderGloria = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_LeaderGloria, OBJ_EVENT_PAL_TAG_NPC_4, PALSLOT_NPC_4);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaLeaderInger = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_LeaderInger, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaLeaderJania = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_LeaderJania, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaLeaderRaazi = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_LeaderRaazi, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaLeaderViniel = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_LeaderViniel, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaProfTenebris = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_ProfTenebris, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaSkiierF = CORMORIA_PERSON_INFO(
    sCormoriaPicTable_SkiierF, OBJ_EVENT_PAL_TAG_NPC_1, PALSLOT_NPC_1);

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaGubukingNormal = {
    .tileTag = TAG_NONE,
    .paletteTag = OBJ_EVENT_PAL_TAG_CORMORIA_GUBUKING,
    .reflectionPaletteTag = OBJ_EVENT_PAL_TAG_BRIDGE_REFLECTION,
    .size = 512,
    .width = 16,
    .height = 32,
    .paletteSlot = PALSLOT_PLAYER,
    .shadowSize = SHADOW_SIZE_M,
    .inanimate = FALSE,
    .compressed = FALSE,
    .tracks = TRACKS_FOOT,
    .oam = &gObjectEventBaseOam_16x32,
    .subspriteTables = sOamTables_16x32,
    .anims = sAnimTable_BrendanMayNormal,
    .images = sCormoriaPicTable_GubukingNormal,
    .affineAnims = gDummySpriteAffineAnimTable,
};
const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaShububuNormal = {
    .tileTag = TAG_NONE,
    .paletteTag = OBJ_EVENT_PAL_TAG_CORMORIA_SHUBUBU,
    .reflectionPaletteTag = OBJ_EVENT_PAL_TAG_BRIDGE_REFLECTION,
    .size = 512,
    .width = 16,
    .height = 32,
    .paletteSlot = PALSLOT_PLAYER,
    .shadowSize = SHADOW_SIZE_M,
    .inanimate = FALSE,
    .compressed = FALSE,
    .tracks = TRACKS_FOOT,
    .oam = &gObjectEventBaseOam_16x32,
    .subspriteTables = sOamTables_16x32,
    .anims = sAnimTable_BrendanMayNormal,
    .images = sCormoriaPicTable_ShububuNormal,
    .affineAnims = gDummySpriteAffineAnimTable,
};

const struct ObjectEventGraphicsInfo gObjectEventGraphicsInfo_CormoriaTMHMBall = {
    .tileTag = TAG_NONE,
    .paletteTag = OBJ_EVENT_PAL_TAG_NPC_3,
    .reflectionPaletteTag = OBJ_EVENT_PAL_TAG_NONE,
    .size = 256,
    .width = 16,
    .height = 32,
    .paletteSlot = PALSLOT_NPC_1,
    .shadowSize = SHADOW_SIZE_M,
    .inanimate = TRUE,
    .compressed = FALSE,
    .tracks = TRACKS_NONE,
    .oam = &gObjectEventBaseOam_16x32,
    .subspriteTables = sOamTables_16x32,
    .anims = sAnimTable_Following,
    .images = sCormoriaPicTable_TMHMBall,
    .affineAnims = gDummySpriteAffineAnimTable,
};

#undef CORMORIA_PERSON_INFO
