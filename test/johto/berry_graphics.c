#include "global.h"
#include "load_save.h"
#include "berry.h"
#include "event_object_movement.h"
#include "palette.h"
#include "sprite.h"
#include "constants/berry.h"
#include "constants/johto_berry_plots.h"
#include "test/test.h"

#if TESTING
extern void JohtoBerryGraphics_TestRender(struct ObjectEvent *, struct Sprite *, u8, u8);
extern const struct SpriteFrameImage *JohtoBerryGraphics_TestImages(u8);
extern u16 JohtoBerryGraphics_TestPaletteTag(u8, u8);
#endif

TEST("Johto berry renderer selects donor frames and semantic palettes for every supported species and stage")
{
#if TESTING
    struct ObjectEvent object = {0};
    struct Sprite *sprite = &gSprites[0];
    u8 berryId, stage;
    SetSaveBlocksPointers(0);
    object.trainerRange_berryTreeId = JOHTO_BERRY_PLOTS_FIRST;
    for (berryId = BERRY_ID_CHERI; berryId <= BERRY_ID_SITRUS; berryId++)
    {
        for (stage = 0; stage < 7; stage++)
        {
            *sprite = (struct Sprite){0};
            JohtoBerryGraphics_TestRender(&object, sprite, berryId, stage);
            EXPECT_EQ(sprite->images, JohtoBerryGraphics_TestImages(berryId));
            EXPECT(sprite->images[stage].data != NULL);
            EXPECT_EQ(sprite->images[stage].size, stage < 3 ? 128 : 256);
            EXPECT(sprite->anims != NULL);
            EXPECT_EQ(GetSpritePaletteTagByPaletteNum(sprite->oam.paletteNum), JohtoBerryGraphics_TestPaletteTag(berryId, stage));
        }
    }
#endif
}

TEST("Johto berry renderer keeps host graphics for plots outside the selected 90..109 range")
{
#if TESTING
    struct ObjectEvent object = {0};
    struct Sprite *sprite = &gSprites[0];
    const struct SpriteFrameImage *hostImages;
    SetSaveBlocksPointers(0);
    object.trainerRange_berryTreeId = 0;
    *sprite = (struct Sprite){0};
    JohtoBerryGraphics_TestRender(&object, sprite, BERRY_ID_CHERI, 0);
    hostImages = sprite->images;
    EXPECT(hostImages != JohtoBerryGraphics_TestImages(BERRY_ID_CHERI));
#endif
}

TEST("Johto berry renderer fails closed for unsupported stage and species without unsafe indexing")
{
#if TESTING
    struct ObjectEvent object = {0};
    struct Sprite *sprite = &gSprites[0];
    SetSaveBlocksPointers(0);
    object.trainerRange_berryTreeId = JOHTO_BERRY_PLOTS_FIRST;
    *sprite = (struct Sprite){0};
    JohtoBerryGraphics_TestRender(&object, sprite, BERRY_ID_CHERI, 7);
    EXPECT(sprite->images == NULL);
    *sprite = (struct Sprite){0};
    JohtoBerryGraphics_TestRender(&object, sprite, 0, 0);
    EXPECT(sprite->images != JohtoBerryGraphics_TestImages(BERRY_ID_CHERI));
#endif
}
