#include "global.h"
#include "constants/battle_ai.h"
#include "constants/battle_partner.h"
#include "constants/items.h"
#include "constants/moves.h"
#include "constants/species.h"
#include "constants/trainers.h"
#include "data.h"
#include "sprite.h"
#include "test/test.h"

extern const u32 gTrainerFrontPic_EliteFourLanceFrlg[];
extern const u16 gTrainerPalette_EliteFourLanceFrlg[];
extern const u8 gJohtoTrainerBackPic_Lance[];
extern const u16 gJohtoTrainerBackPalette_Lance[];

static const struct Trainer sLanceProduction[DIFFICULTY_COUNT][PARTNER_COUNT] =
{
#include "../../src/data/battle_partners.h"
};

TEST("Johto Lance partner generated production roster is source exact")
{
    const struct Trainer *lance = &sLanceProduction[DIFFICULTY_NORMAL][PARTNER_LANCE];
    const struct TrainerMon *party = lance->party;

    static const u16 species[] = {SPECIES_DRAGONITE, SPECIES_DRAGONAIR, SPECIES_CHARIZARD};
    static const u8 levels[] = {42, 35, 36};
    static const u8 evs[3][6] = {{0, 252, 252, 6, 0, 0}, {252, 0, 0, 6, 252, 0}, {0, 252, 0, 252, 6, 0}};
    static const u16 moves[3][4] = {
        {MOVE_HYPER_BEAM, MOVE_THUNDER, MOVE_SAFEGUARD, MOVE_OUTRAGE},
        {MOVE_BLIZZARD, MOVE_THUNDER_WAVE, MOVE_FLAMETHROWER, MOVE_QUICK_ATTACK},
        {MOVE_FLAMETHROWER, MOVE_WING_ATTACK, MOVE_DOUBLE_TEAM, MOVE_STEEL_WING},
    };
    u32 i, j;

    EXPECT_EQ((u32)lance->partySize, 3);
    EXPECT_EQ(lance->trainerClass, TRAINER_CLASS_ELITE_FOUR);
    EXPECT_EQ((u32)lance->encounterMusic, TRAINER_ENCOUNTER_MUSIC_ELITE_FOUR);
    EXPECT_EQ((u32)lance->gender, TRAINER_GENDER_MALE);
    EXPECT_EQ(lance->trainerPic, TRAINER_PIC_JOHTO_PARTNER_LANCE);
    EXPECT_EQ((u32)lance->multiTeamSize, MULTI_TEAM_SIZE_HALF);
    EXPECT_EQ(lance->aiFlags, AI_FLAG_BASIC_TRAINER);
    EXPECT_EQ(party[0].species, SPECIES_DRAGONITE);
    EXPECT_EQ(party[0].lvl, 42);
    EXPECT_EQ((u32)party[0].nature, NATURE_ADAMANT);
    EXPECT_EQ(party[0].iv, TRAINER_PARTY_IVS(31, 31, 31, 31, 31, 31));
    EXPECT_EQ(party[0].ev[0], 0);
    EXPECT_EQ(party[0].ev[1], 252);
    EXPECT_EQ(party[0].ev[2], 252);
    EXPECT_EQ(party[0].ev[3], 6);
    EXPECT_EQ(party[0].ev[4], 0);
    EXPECT_EQ(party[0].ev[5], 0);
    EXPECT_EQ(party[0].heldItem, ITEM_NONE);
    EXPECT_EQ(party[0].moves[0], MOVE_HYPER_BEAM);
    EXPECT_EQ(party[0].moves[1], MOVE_THUNDER);
    EXPECT_EQ(party[0].moves[2], MOVE_SAFEGUARD);
    EXPECT_EQ(party[0].moves[3], MOVE_OUTRAGE);
    EXPECT_EQ(party[1].species, SPECIES_DRAGONAIR);
    EXPECT_EQ(party[1].lvl, 35);
    EXPECT_EQ(party[1].ev[0], 252);
    EXPECT_EQ(party[1].ev[3], 6);
    EXPECT_EQ(party[1].ev[4], 252);
    EXPECT_EQ(party[1].ev[5], 0);
    EXPECT_EQ(party[2].species, SPECIES_CHARIZARD);
    EXPECT_EQ(party[2].lvl, 36);
    EXPECT_EQ(party[2].ev[1], 252);
    EXPECT_EQ(party[2].ev[3], 252);
    EXPECT_EQ(party[2].ev[4], 6);
    for (i = 0; i < 3; i++)
    {
        EXPECT_EQ(party[i].species, species[i]);
        EXPECT_EQ(party[i].lvl, levels[i]);
        EXPECT_EQ((u32)party[i].nature, NATURE_ADAMANT);
        EXPECT_EQ(party[i].iv, TRAINER_PARTY_IVS(31, 31, 31, 31, 31, 31));
        EXPECT_EQ(party[i].heldItem, ITEM_NONE);
        EXPECT(party[i].ev != NULL);
        for (j = 0; j < 6; j++)
            EXPECT_EQ(party[i].ev[j], evs[i][j]);
        for (j = 0; j < 4; j++)
            EXPECT_EQ(party[i].moves[j], moves[i][j]);
    }
}

TEST("Johto Lance partner graphic binds the existing front and four-frame back")
{
    const struct TrainerPicInfo *pic = &gTrainerPicInfo[TRAINER_PIC_JOHTO_PARTNER_LANCE];

    EXPECT(pic->frontPic != NULL);
    EXPECT_EQ(pic->frontPic->imageData, gTrainerFrontPic_EliteFourLanceFrlg);
    EXPECT_EQ(pic->frontPic->paletteData, gTrainerPalette_EliteFourLanceFrlg);
    EXPECT(pic->backPic != NULL);
    EXPECT_EQ(pic->backPic->coordinates.size, 8);
    EXPECT_EQ(pic->backPic->coordinates.y_offset, 4);
    EXPECT_EQ(pic->backPic->image.data, gJohtoTrainerBackPic_Lance);
    EXPECT_EQ(pic->backPic->image.size, TRAINER_PIC_SIZE);
    EXPECT_EQ(pic->backPic->paletteData, gJohtoTrainerBackPalette_Lance);
    EXPECT(pic->backPic->animation != NULL);
    EXPECT(pic->backPic->image.relativeFrames);
    {
        static const u8 frames[] = {0, 1, 2, 0, 3};
        static const u8 durations[] = {24, 9, 24, 9, 50};
        u32 i;
        const union AnimCmd *throw = pic->backPic->animation[1];
        EXPECT_EQ((u32)pic->backPic->animation[0][0].frame.imageValue, 3);
        EXPECT_EQ(pic->backPic->animation[0][1].type, -1);
        for (i = 0; i < 5; i++)
        {
            EXPECT_EQ((u32)throw[i].frame.imageValue, frames[i]);
            EXPECT_EQ((u32)throw[i].frame.duration, durations[i]);
            EXPECT(pic->backPic->animation[2][i].frame.imageValue < 4);
        }
        EXPECT_EQ(throw[5].type, -1);
        EXPECT_EQ(pic->backPic->animation[2][5].type, -1);
    }
}
