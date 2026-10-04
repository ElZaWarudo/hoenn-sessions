#include "global.h"
#include "item.h"
#include "move.h"
#include "pokemon.h"

// Compiler-derived locations of address-bearing fields in the five shared ID
// tables. Text offsets identify the bounded display strings checked by the
// ROM manifest; graphics, scripts, callbacks, and menus remain outside scope.
#define PTR(type, field) offsetof(type, field)
#define GFX(field) (offsetof(struct SpeciesInfo, overworldData) + offsetof(struct ObjectEventGraphicsInfo, field))
#define GFX_F(field) (offsetof(struct SpeciesInfo, overworldDataFemale) + offsetof(struct ObjectEventGraphicsInfo, field))

_Static_assert(sizeof(void *) == 4, "object scalar descriptor requires GBA pointers");
// AdditionalEffect currently contains no pointers. Keep these checks in sync
// with the audited scalar-only layout before hashing its bytes verbatim.
_Static_assert(offsetof(struct AdditionalEffect, chance) == 3, "audit AdditionalEffect fields before changing the object contract");
_Static_assert(sizeof(struct AdditionalEffect) == 4, "audit AdditionalEffect fields before changing the object contract");

struct ObjectScalarLayout
{
    u16 stride;
    u16 pointerCount;
    u16 pointerOffsets[48];
    u16 textCount;
    u16 textOffsets[3];
};

struct ObjectScalarDescriptor
{
    u32 magic;
    u16 version;
    u16 tableCount;
    struct ObjectScalarLayout tables[5];
    u16 additionalEffectStride;
    u16 moveAdditionalEffectsOffset;
};

// The byte containing numAdditionalEffects and its mask are obtained from this
// compiler-initialized MoveInfo, so the ROM reader need not model C bitfields.
const struct MoveInfo gCoopMoveCountProbe
    __attribute__((used, section(".rodata.coop_player_transfer"))) =
{
    .numAdditionalEffects = 7,
};

const struct ObjectScalarDescriptor gCoopObjectScalarDescriptor
    __attribute__((used, section(".rodata.coop_player_transfer"))) =
{
    .magic = 0x3143534F, // OSC1
    .version = 3,
    .tableCount = 5,
    .tables = {
        {
            .stride = sizeof(struct ItemInfo),
            .pointerCount = 8,
            .pointerOffsets = {
                PTR(struct ItemInfo, fieldUseFunc), PTR(struct ItemInfo, description),
                PTR(struct ItemInfo, effect), PTR(struct ItemInfo, name),
                PTR(struct ItemInfo, pluralName), PTR(struct ItemInfo, iconPic),
                PTR(struct ItemInfo, iconPalette), PTR(struct ItemInfo, shopCriteriaFunc),
            },
            .textCount = 3,
            .textOffsets = { PTR(struct ItemInfo, name), PTR(struct ItemInfo, pluralName), PTR(struct ItemInfo, description) },
        },
        {
            .stride = sizeof(struct SpeciesInfo),
            .pointerCount = 1 + 6 + 6
#if P_GENDER_DIFFERENCES
                + 5
#endif
#if P_FOOTPRINTS
                + 1
#endif
#if OW_POKEMON_OBJECT_EVENTS
                + 5
#if P_GENDER_DIFFERENCES
                + 5
#endif
#if OW_PKMN_OBJECTS_SHARE_PALETTES == FALSE
                + 2
#if P_GENDER_DIFFERENCES
                + 2
#endif
#endif
#endif
                ,
            .pointerOffsets = {
                PTR(struct SpeciesInfo, description),
                PTR(struct SpeciesInfo, frontAnimFrames),
                PTR(struct SpeciesInfo, frontPic), PTR(struct SpeciesInfo, backPic),
                PTR(struct SpeciesInfo, palette), PTR(struct SpeciesInfo, shinyPalette),
                PTR(struct SpeciesInfo, iconSprite),
#if P_GENDER_DIFFERENCES
                PTR(struct SpeciesInfo, frontPicFemale), PTR(struct SpeciesInfo, backPicFemale),
                PTR(struct SpeciesInfo, paletteFemale), PTR(struct SpeciesInfo, shinyPaletteFemale),
                PTR(struct SpeciesInfo, iconSpriteFemale),
#endif
#if P_FOOTPRINTS
                PTR(struct SpeciesInfo, footprint),
#endif
                PTR(struct SpeciesInfo, levelUpLearnset), PTR(struct SpeciesInfo, teachableLearnset),
                PTR(struct SpeciesInfo, eggMoveLearnset), PTR(struct SpeciesInfo, evolutions),
                PTR(struct SpeciesInfo, formSpeciesIdTable), PTR(struct SpeciesInfo, formChangeTable),
#if OW_POKEMON_OBJECT_EVENTS
                GFX(oam), GFX(subspriteTables), GFX(anims), GFX(images), GFX(affineAnims),
#if P_GENDER_DIFFERENCES
                GFX_F(oam), GFX_F(subspriteTables), GFX_F(anims), GFX_F(images), GFX_F(affineAnims),
#endif
#if OW_PKMN_OBJECTS_SHARE_PALETTES == FALSE
                PTR(struct SpeciesInfo, overworldPalette), PTR(struct SpeciesInfo, overworldShinyPalette),
#if P_GENDER_DIFFERENCES
                PTR(struct SpeciesInfo, overworldPaletteFemale), PTR(struct SpeciesInfo, overworldShinyPaletteFemale),
#endif
#endif
#endif
            },
            .textCount = 1,
            .textOffsets = { PTR(struct SpeciesInfo, description) },
        },
        {
            .stride = sizeof(struct MoveInfo),
            .pointerCount = 4,
            .pointerOffsets = {
                PTR(struct MoveInfo, name), PTR(struct MoveInfo, description),
                PTR(struct MoveInfo, additionalEffects), PTR(struct MoveInfo, battleAnimScript),
            },
            .textCount = 2,
            .textOffsets = { PTR(struct MoveInfo, name), PTR(struct MoveInfo, description) },
        },
        {
            .stride = sizeof(struct AbilityInfo),
            .pointerCount = 1,
            .pointerOffsets = { PTR(struct AbilityInfo, description) },
            .textCount = 1,
            .textOffsets = { PTR(struct AbilityInfo, description) },
        },
        { .stride = sizeof(struct TmHmIndexKey), .pointerCount = 0 },
    },
    .additionalEffectStride = sizeof(struct AdditionalEffect),
    .moveAdditionalEffectsOffset = PTR(struct MoveInfo, additionalEffects),
};
