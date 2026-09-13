#include "global.h"
#include "event_data.h"
#include "pokemon.h"
#include "test/test.h"
#include "constants/species.h"

void SwitchMonAbility(void);

static void SetAbilityTestMon(enum Species species, u8 abilityNum)
{
    ZeroPlayerPartyMons();
    CreateMon(&gPlayerParty[0], species, 5, 0, OTID_STRUCT_PLAYER_ID);
    SetMonData(&gPlayerParty[0], MON_DATA_ABILITY_NUM, &abilityNum);
    gPlayerPartyCount = 1;
}

static u8 GetAbilityTestSlot(void)
{
    return GetMonData(&gPlayerParty[0], MON_DATA_ABILITY_NUM);
}

TEST("SwitchMonAbility toggles Pidgey's ordinary ability slots")
{
    SetAbilityTestMon(SPECIES_PIDGEY, 0);
    gSpecialVar_0x8004 = 0;

    SwitchMonAbility();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT_EQ(GetAbilityTestSlot(), 1);

    SwitchMonAbility();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT_EQ(GetAbilityTestSlot(), 0);
}

TEST("SwitchMonAbility preserves donor hidden-slot-to-slot-zero behavior")
{
    SetAbilityTestMon(SPECIES_PIDGEY, 2);
    gSpecialVar_0x8004 = 0;

    SwitchMonAbility();
    EXPECT_EQ(gSpecialVar_Result, TRUE);
    EXPECT_EQ(GetAbilityTestSlot(), 0);
}

TEST("SwitchMonAbility refuses absent or equal alternate abilities")
{
    SetAbilityTestMon(SPECIES_BULBASAUR, 0);
    gSpecialVar_0x8004 = 0;
    SwitchMonAbility();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT_EQ(GetAbilityTestSlot(), 0);

    SetAbilityTestMon(SPECIES_VENUSAUR_MEGA, 0);
    gSpecialVar_0x8004 = 0;
    SwitchMonAbility();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT_EQ(GetAbilityTestSlot(), 0);
}

TEST("SwitchMonAbility refuses cancel and party-count boundary indices")
{
    SetAbilityTestMon(SPECIES_PIDGEY, 0);

    gSpecialVar_0x8004 = 0xFF;
    SwitchMonAbility();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT_EQ(GetAbilityTestSlot(), 0);

    gSpecialVar_0x8004 = gPlayerPartyCount;
    SwitchMonAbility();
    EXPECT_EQ(gSpecialVar_Result, FALSE);
    EXPECT_EQ(GetAbilityTestSlot(), 0);
}
