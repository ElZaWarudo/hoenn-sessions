#include "global.h"
#include "item.h"
#include "johto/bug_contest.h"
#include "pokemon.h"
#include "pokemon_storage_system.h"
#include "random.h"
#include "string_util.h"
#include "constants/items.h"
#include "constants/species.h"

enum JohtoBugContestPhase
{
    BUG_CONTEST_PHASE_ACTIVE,
    BUG_CONTEST_PHASE_ENDING,
};

struct JohtoBugContestState
{
    struct Pokemon originalParty[PARTY_SIZE];
    struct Mail originalMail[MAIL_COUNT];
    u8 originalPartyCount;
    u8 phase;
    u8 endReason;
    u16 selectedSlot;
    u8 selectedDisplayIndex;
    u8 selectedPlacement;
    bool8 judged;
    bool8 transferred;
    bool8 rewardClaimed;
    u16 startingSafariBalls;
    u16 loanedSafariBalls;
    u16 selectedSpecies;
    u16 reward;
    u32 startTime;
    struct Pokemon selectedMon;
    u8 selectedName[POKEMON_NAME_LENGTH + 1];
};

static EWRAM_DATA struct JohtoBugContestState sBugContestState;
static EWRAM_DATA struct JohtoBugContestState *sBugContest = NULL;

static const u16 sFirstPlaceRewards[] =
{
    ITEM_MOON_STONE,
    ITEM_SUN_STONE,
    ITEM_LEAF_STONE,
};

static const u16 sSecondPlaceRewards[] =
{
    ITEM_FIRE_STONE,
    ITEM_THUNDER_STONE,
    ITEM_WATER_STONE,
};

static const u16 sThirdPlaceRewards[] =
{
    ITEM_ORAN_BERRY,
    ITEM_CHERI_BERRY,
    ITEM_PERSIM_BERRY,
    ITEM_PECHA_BERRY,
    ITEM_RAWST_BERRY,
    ITEM_ASPEAR_BERRY,
    ITEM_CHESTO_BERRY,
};

static bool32 HasFreePCSlot(void)
{
    u8 box;
    for (box = 0; box < TOTAL_BOXES_COUNT; box++)
    {
        if (GetFirstFreeBoxSpot(box) >= 0)
            return TRUE;
    }
    return FALSE;
}

bool32 JohtoBugContest_IsContestSpecies(u16 species)
{
    switch (species)
    {
    case SPECIES_CATERPIE:
    case SPECIES_WEEDLE:
    case SPECIES_METAPOD:
    case SPECIES_KAKUNA:
    case SPECIES_PARAS:
    case SPECIES_VENONAT:
    case SPECIES_BUTTERFREE:
    case SPECIES_BEEDRILL:
    case SPECIES_SCYTHER:
    case SPECIES_PINSIR:
        return TRUE;
    default:
        return FALSE;
    }
}

u8 JohtoBugContest_GetPlacement(u32 maxHp, u16 draw)
{
    if (maxHp < 41)
        return 3;
    if (maxHp <= 46)
        return draw < 50 ? 2 : 3;
    if (maxHp == 47)
        return draw < 75 ? 1 : 2;
    return 1;
}

u16 JohtoBugContest_GetRewardForPlacement(u8 placement, u16 draw)
{
    switch (placement)
    {
    case 1:
        return sFirstPlaceRewards[draw % ARRAY_COUNT(sFirstPlaceRewards)];
    case 2:
        return sSecondPlaceRewards[draw % ARRAY_COUNT(sSecondPlaceRewards)];
    case 3:
        return sThirdPlaceRewards[draw % ARRAY_COUNT(sThirdPlaceRewards)];
    default:
        return ITEM_NONE;
    }
}

static u8 DisplayIndexForSpecies(u16 species)
{
    switch (species)
    {
    case SPECIES_CATERPIE: return 22;
    case SPECIES_WEEDLE: return 23;
    case SPECIES_METAPOD: return 24;
    case SPECIES_KAKUNA: return 25;
    case SPECIES_PARAS: return 26;
    case SPECIES_VENONAT: return 27;
    case SPECIES_BUTTERFREE: return 28;
    case SPECIES_BEEDRILL: return 29;
    case SPECIES_SCYTHER: return 30;
    case SPECIES_PINSIR: return 31;
    default: return 0;
    }
}

static void RestoreOriginalParty(void)
{
    memcpy(gPlayerParty, sBugContest->originalParty, sizeof(sBugContest->originalParty));
    gPlayerPartyCount = sBugContest->originalPartyCount;
    memcpy(gSaveBlock1Ptr->mail, sBugContest->originalMail, sizeof(sBugContest->originalMail));
}

static void ReturnLoanedSafariBalls(void)
{
    u16 current = CountTotalItemQuantityInBag(ITEM_SAFARI_BALL);
    u16 removeCount = 0;
    if (current > sBugContest->startingSafariBalls)
    {
        removeCount = current - sBugContest->startingSafariBalls;
        if (removeCount > sBugContest->loanedSafariBalls)
            removeCount = sBugContest->loanedSafariBalls;
    }
    if (removeCount != 0)
        RemoveBagItem(ITEM_SAFARI_BALL, removeCount);
}

static void FinishAndFree(void)
{
    RestoreOriginalParty();
    ReturnLoanedSafariBalls();
    memset(&sBugContestState, 0, sizeof(sBugContestState));
    sBugContest = NULL;
}

enum JohtoBugContestStatus JohtoBugContest_Begin(u32 now)
{
    u8 count, i;
    u16 species, isEgg;
    u32 hp;
    struct JohtoBugContestState *state;

    if (sBugContest != NULL)
        return JOHTO_BUG_CONTEST_ALREADY_ACTIVE;

    count = gPlayerPartyCount;
    species = GetMonData(&gPlayerParty[0], MON_DATA_SPECIES);
    hp = GetMonData(&gPlayerParty[0], MON_DATA_HP);
    isEgg = GetMonData(&gPlayerParty[0], MON_DATA_IS_EGG);
    if (count == 0 || count > PARTY_SIZE || species == SPECIES_NONE || hp == 0 || isEgg)
        return JOHTO_BUG_CONTEST_INVALID_PARTY;
    if (!HasFreePCSlot())
        return JOHTO_BUG_CONTEST_PC_FULL;
    if (!CheckBagHasSpace(ITEM_SAFARI_BALL, 30))
        return JOHTO_BUG_CONTEST_BAG_FULL;

    state = &sBugContestState;
    memset(state, 0, sizeof(*state));
    state->originalPartyCount = count;
    memcpy(state->originalParty, gPlayerParty, sizeof(state->originalParty));
    memcpy(state->originalMail, gSaveBlock1Ptr->mail, sizeof(state->originalMail));
    state->startingSafariBalls = CountTotalItemQuantityInBag(ITEM_SAFARI_BALL);
    state->loanedSafariBalls = 30;
    if (!AddBagItem(ITEM_SAFARI_BALL, state->loanedSafariBalls))
    {
        memset(state, 0, sizeof(*state));
        return JOHTO_BUG_CONTEST_BAG_FULL;
    }

    for (i = 1; i < PARTY_SIZE; i++)
        ZeroMonData(&gPlayerParty[i]);
    gPlayerPartyCount = 1;
    state->phase = BUG_CONTEST_PHASE_ACTIVE;
    state->startTime = now;
    sBugContest = state;
    return JOHTO_BUG_CONTEST_OK;
}

bool32 JohtoBugContest_IsActive(void)
{
    return sBugContest != NULL && sBugContest->phase == BUG_CONTEST_PHASE_ACTIVE;
}

bool32 JohtoBugContest_IsEnding(void)
{
    return sBugContest != NULL && sBugContest->phase == BUG_CONTEST_PHASE_ENDING;
}

bool32 JohtoBugContest_IsSerializationBlocked(void)
{
    /* The snapshot remains authoritative through judging, transfer and
     * reward retries.  Any save during that lifetime would serialize the
     * temporary one-mon party instead of the player's original party. */
    return sBugContest != NULL;
}

bool32 JohtoBugContest_CheckTime(u32 now)
{
    if (!JohtoBugContest_IsActive())
        return FALSE;
    if ((u32)(now - sBugContest->startTime) < JOHTO_BUG_CONTEST_TIME_LIMIT_FRAMES)
        return FALSE;
    sBugContest->phase = BUG_CONTEST_PHASE_ENDING;
    sBugContest->endReason = JOHTO_BUG_CONTEST_END_TIMEOUT;
    return TRUE;
}

enum JohtoBugContestStatus JohtoBugContest_RequestEnd(enum JohtoBugContestEndReason reason)
{
    if (sBugContest == NULL)
        return JOHTO_BUG_CONTEST_NO_CONTEST;
    if (sBugContest->phase == BUG_CONTEST_PHASE_ENDING)
        return JOHTO_BUG_CONTEST_OK;
    sBugContest->phase = BUG_CONTEST_PHASE_ENDING;
    sBugContest->endReason = reason;
    return JOHTO_BUG_CONTEST_OK;
}

enum JohtoBugContestStatus JohtoBugContest_Judge(u16 slot)
{
    u16 species, isEgg;
    u32 maxHp;
    u8 placement;

    if (!JohtoBugContest_IsEnding())
        return JOHTO_BUG_CONTEST_NOT_ENDING;
    if (sBugContest->judged)
        return slot == sBugContest->selectedSlot ? JOHTO_BUG_CONTEST_OK : JOHTO_BUG_CONTEST_SELECTION_LOCKED;
    if (slot == 0 || slot >= gPlayerPartyCount || slot >= PARTY_SIZE)
    {
        sBugContest->reward = ITEM_NONE;
        return JOHTO_BUG_CONTEST_INVALID_SELECTION;
    }
    species = GetMonData(&gPlayerParty[slot], MON_DATA_SPECIES);
    isEgg = GetMonData(&gPlayerParty[slot], MON_DATA_IS_EGG);
    if (species == SPECIES_NONE || isEgg || !JohtoBugContest_IsContestSpecies(species))
    {
        sBugContest->reward = ITEM_NONE;
        return JOHTO_BUG_CONTEST_INVALID_SELECTION;
    }

    CopyMon(&sBugContest->selectedMon, &gPlayerParty[slot], sizeof(sBugContest->selectedMon));
    sBugContest->selectedSlot = slot;
    sBugContest->selectedSpecies = species;
    sBugContest->selectedDisplayIndex = DisplayIndexForSpecies(species);
    GetMonData(&gPlayerParty[slot], MON_DATA_NICKNAME, sBugContest->selectedName);
    maxHp = GetMonData(&gPlayerParty[slot], MON_DATA_MAX_HP);
    placement = JohtoBugContest_GetPlacement(maxHp, Random() % 100);
    sBugContest->selectedPlacement = placement;
    sBugContest->reward = JohtoBugContest_GetRewardForPlacement(placement, Random());
    sBugContest->judged = TRUE;
    return JOHTO_BUG_CONTEST_OK;
}

enum JohtoBugContestStatus JohtoBugContest_TransferSelected(void)
{
    if (sBugContest == NULL)
        return JOHTO_BUG_CONTEST_NO_CONTEST;
    if (!sBugContest->judged)
        return JOHTO_BUG_CONTEST_NOT_JUDGED;
    if (sBugContest->transferred)
        return JOHTO_BUG_CONTEST_OK;
    if (CopyMonToPC(&sBugContest->selectedMon) != MON_GIVEN_TO_PC)
        return JOHTO_BUG_CONTEST_TRANSFER_FAILED;
    sBugContest->transferred = TRUE;
    return JOHTO_BUG_CONTEST_OK;
}

enum JohtoBugContestStatus JohtoBugContest_ClaimReward(void)
{
    if (sBugContest == NULL)
        return JOHTO_BUG_CONTEST_NO_CONTEST;
    if (!sBugContest->transferred)
        return JOHTO_BUG_CONTEST_TRANSFER_FAILED;
    if (sBugContest->rewardClaimed)
        return JOHTO_BUG_CONTEST_OK;
    if (sBugContest->reward == ITEM_NONE || !AddBagItem(sBugContest->reward, 1))
        return JOHTO_BUG_CONTEST_REWARD_FAILED;
    sBugContest->rewardClaimed = TRUE;
    return JOHTO_BUG_CONTEST_OK;
}

enum JohtoBugContestStatus JohtoBugContest_Exit(void)
{
    if (sBugContest == NULL)
        return JOHTO_BUG_CONTEST_NO_CONTEST;
    if (!sBugContest->transferred || !sBugContest->rewardClaimed)
        return JOHTO_BUG_CONTEST_EXIT_BLOCKED;
    FinishAndFree();
    return JOHTO_BUG_CONTEST_OK;
}

enum JohtoBugContestStatus JohtoBugContest_Abort(void)
{
    if (sBugContest == NULL)
        return JOHTO_BUG_CONTEST_NO_CONTEST;
    if (sBugContest->transferred)
        return JOHTO_BUG_CONTEST_EXIT_BLOCKED;
    FinishAndFree();
    return JOHTO_BUG_CONTEST_OK;
}

u8 JohtoBugContest_GetSelectedDisplayIndex(void)
{
    return sBugContest != NULL ? sBugContest->selectedDisplayIndex : 0;
}

u16 JohtoBugContest_GetSelectedSpecies(void)
{
    return sBugContest != NULL ? sBugContest->selectedSpecies : SPECIES_NONE;
}

const u8 *JohtoBugContest_GetSelectedName(void)
{
    static const u8 sEmptyName[] = _("?");
    return sBugContest != NULL && sBugContest->judged ? sBugContest->selectedName : sEmptyName;
}

u16 JohtoBugContest_GetReward(void)
{
    return sBugContest != NULL ? sBugContest->reward : ITEM_NONE;
}

u8 JohtoBugContest_GetSelectedPlacement(void)
{
    return sBugContest != NULL ? sBugContest->selectedPlacement : 0;
}

#if TESTING
void JohtoBugContest_TestReset(void)
{
    if (sBugContest != NULL)
        FinishAndFree();
}
#endif
