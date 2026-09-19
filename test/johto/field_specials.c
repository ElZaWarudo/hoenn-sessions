#include "global.h"
#include "event_data.h"
#include "field_specials.h"
#include "move.h"
#include "pokemon.h"
#include "string_util.h"
#include "test/test.h"
#include "constants/moves.h"
#include "constants/script_menu.h"
#include "constants/species.h"
#include "constants/vars.h"

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

static void SetFrontierTutorSelection(u16 tutorId, u16 selection)
{
    VarSet(VAR_TEMP_FRONTIER_TUTOR_ID, tutorId);
    VarSet(VAR_TEMP_FRONTIER_TUTOR_SELECTION, selection);
}

TEST("GetBattleFrontierTutorMoveIndex maps every native and elemental tutor move")
{
    static const enum Move sExpectedMoves[][10] =
    {
        {
            MOVE_SOFT_BOILED,
            MOVE_SEISMIC_TOSS,
            MOVE_DREAM_EATER,
            MOVE_MEGA_PUNCH,
            MOVE_MEGA_KICK,
            MOVE_BODY_SLAM,
            MOVE_ROCK_SLIDE,
            MOVE_COUNTER,
            MOVE_THUNDER_WAVE,
            MOVE_SWORDS_DANCE,
        },
        {
            MOVE_DEFENSE_CURL,
            MOVE_SNORE,
            MOVE_MUD_SLAP,
            MOVE_SWIFT,
            MOVE_ICY_WIND,
            MOVE_ENDURE,
            MOVE_PSYCH_UP,
            MOVE_ICE_PUNCH,
            MOVE_THUNDER_PUNCH,
            MOVE_FIRE_PUNCH,
        },
        {
            MOVE_FRENZY_PLANT,
            MOVE_BLAST_BURN,
            MOVE_HYDRO_CANNON,
        },
    };
    static const u8 sMoveCounts[] = {10, 10, 3};
    u32 tutorId;
    u32 selection;

    for (tutorId = 0; tutorId < ARRAY_COUNT(sExpectedMoves); tutorId++)
    {
        for (selection = 0; selection < sMoveCounts[tutorId]; selection++)
        {
            SetFrontierTutorSelection(tutorId, selection);
            gSpecialVar_0x8005 = 0xFFFF;
            GetBattleFrontierTutorMoveIndex();
            EXPECT_EQ(gSpecialVar_0x8005, sExpectedMoves[tutorId][selection]);
        }
    }
}

TEST("BufferBattleFrontierTutorMoveName maps and buffers one move from each tutor")
{
    static const struct
    {
        u16 tutorId;
        u16 selection;
        enum Move move;
    } sCases[] =
    {
        {0, 0, MOVE_SOFT_BOILED},
        {1, 8, MOVE_THUNDER_PUNCH},
        {2, 2, MOVE_HYDRO_CANNON},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sCases); i++)
    {
        SetFrontierTutorSelection(sCases[i].tutorId, sCases[i].selection);
        gSpecialVar_0x8005 = sCases[i].move;
        BufferBattleFrontierTutorMoveName();
        EXPECT_EQ(gSpecialVar_0x8005, sCases[i].move);
        EXPECT_EQ(StringCompare(gStringVar1, GetMoveName(sCases[i].move)), 0);
    }
}

TEST("Battle Frontier tutor mapping rejects invalid tutors, selections, and cancellation")
{
    static const struct
    {
        u16 tutorId;
        u16 selection;
    } sInvalidCases[] =
    {
        {0, 10},
        {1, 10},
        {2, 3},
        {0, MULTI_B_PRESSED},
        {1, MULTI_B_PRESSED},
        {2, MULTI_B_PRESSED},
        {3, 0},
        {0xFFFF, 0},
        {0, 0xFFFF},
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(sInvalidCases); i++)
    {
        SetFrontierTutorSelection(sInvalidCases[i].tutorId, sInvalidCases[i].selection);
        gSpecialVar_0x8005 = MOVE_POUND;
        GetBattleFrontierTutorMoveIndex();
        EXPECT_EQ(gSpecialVar_0x8005, MOVE_NONE);
    }
}
