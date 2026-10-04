#include "global.h"
#include "constants/battle_ai.h"
#include "constants/battle_partner.h"
#include "constants/species.h"
#include "constants/trainers.h"
#include "data.h"
#include "sprite.h"
#include "test/test.h"

extern const u8 gCormoriaTrainerBackPic_Gabrielle[];
extern const u16 gCormoriaTrainerBackPalette_Gabrielle[];

static const struct Trainer sPartnerRoster[DIFFICULTY_COUNT][PARTNER_COUNT] =
{
#include "../../src/data/battle_partners.h"
};

TEST("Cormoria Route 6 Gabrielle has the donor partner party at an appended ID")
{
    const struct Trainer *partner = &sPartnerRoster[DIFFICULTY_NORMAL][PARTNER_CORMORIA_GABRIELLE];
    const struct TrainerMon *party = partner->party;
    u32 i;

    EXPECT_EQ(PARTNER_CORMORIA_GABRIELLE, PARTNER_LANCE + 1);
    EXPECT_EQ((u32)partner->partySize, 3);
    EXPECT_EQ(partner->trainerClass, TRAINER_CLASS_PKMN_TRAINER_2);
    EXPECT_EQ(partner->trainerPic, TRAINER_PIC_CORMORIA_GABRIELLE);
    EXPECT_EQ((u32)partner->gender, TRAINER_GENDER_FEMALE);
    EXPECT_EQ((u32)partner->encounterMusic, TRAINER_ENCOUNTER_MUSIC_FEMALE);
    EXPECT_EQ(party[0].species, SPECIES_BALTOY);
    EXPECT_EQ(party[0].lvl, 29);
    EXPECT_EQ(party[1].species, SPECIES_MAWILE);
    EXPECT_EQ(party[1].lvl, 32);
    EXPECT_EQ(party[2].species, SPECIES_ZOROARK_HISUI);
    EXPECT_EQ(party[2].lvl, 33);
    for (i = 0; i < 3; i++)
        EXPECT_EQ(party[i].iv, TRAINER_PARTY_IVS(24, 24, 24, 24, 24, 24));
}

TEST("Cormoria Gabrielle uses donor four-frame back art")
{
    const struct TrainerPicInfo *pic = &gTrainerPicInfo[TRAINER_PIC_CORMORIA_GABRIELLE];

    EXPECT(pic->frontPic != NULL);
    EXPECT(pic->backPic != NULL);
    EXPECT_EQ(pic->backPic->image.data, gCormoriaTrainerBackPic_Gabrielle);
    EXPECT_EQ(pic->backPic->paletteData, gCormoriaTrainerBackPalette_Gabrielle);
    EXPECT_EQ(pic->backPic->image.size, TRAINER_PIC_SIZE);
    EXPECT(pic->backPic->image.relativeFrames);
}
