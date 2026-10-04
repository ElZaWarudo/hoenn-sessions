#include "global.h"
#include "coop/battle_items.h"
#include "coop/battle_runtime.h"
#include "battle.h"
#include "item.h"
#include "pokemon.h"
#include "constants/battle.h"
#include "constants/items.h"
#include "constants/item_effects.h"

/* Consumed items the running battle took out of the bag, by kind. They go
 * back if the battle does not complete (its party changes are undone). */
struct CoopBattleItemLedger
{
    u16 items[COOP_BATTLE_ITEM_LEDGER_SIZE];
    u8 counts[COOP_BATTLE_ITEM_LEDGER_SIZE];
};

static EWRAM_DATA struct CoopBattleItemLedger sLedger = {0};

static const u8 sText_CoopItemRefused[] = _("That can't be used in a\nco-op battle.{PAUSE_UNTIL_PRESS}");
static const u8 sText_CoopItemLedgerFull[] = _("No other kind of item can be\nused in this battle.{PAUSE_UNTIL_PRESS}");
static const u8 sText_CoopItemOwnMonsOnly[] =_("Use items only on your own\nPOKéMON.{PAUSE_UNTIL_PRESS}");

bool8 CoopBattleItems_IsBagOpen(void)
{
    return CoopBattleRuntime_IsEngineActive() && !CoopBattleRuntime_IsFriendlyEngine();
}

bool8 CoopBattleItems_IsConsumed(u16 item)
{
    return GetItemConsumability(item) && !GetItemImportance(item);
}

static u8 LedgerFind(u16 item)
{
    u8 i;

    for (i = 0; i < COOP_BATTLE_ITEM_LEDGER_SIZE; i++)
        if (sLedger.counts[i] != 0 && sLedger.items[i] == item)
            return i;
    for (i = 0; i < COOP_BATTLE_ITEM_LEDGER_SIZE; i++)
        if (sLedger.counts[i] == 0)
            return i;
    return COOP_BATTLE_ITEM_LEDGER_SIZE;
}

const u8 *CoopBattleItems_RefuseItem(u16 item)
{
    if (!CoopBattleRuntime_IsSharedBattleItem(item))
        return sText_CoopItemRefused;
    if (CoopBattleItems_IsConsumed(item) && LedgerFind(item) == COOP_BATTLE_ITEM_LEDGER_SIZE)
        return sText_CoopItemLedgerFull;
    return NULL;
}

const u8 *CoopBattleItems_RefuseTarget(const struct Pokemon *party)
{
    return party == gParties[B_TRAINER_0] ? NULL : sText_CoopItemOwnMonsOnly;
}

void CoopBattleItems_ChooseTarget(struct Pokemon *mon)
{
    gBattleStruct->coopItemPersonality = GetMonData(mon, MON_DATA_PERSONALITY);
}

/* Items that act on the battler itself (X items, Dire Hit, Guard Spec). */
static bool8 TargetsActiveBattler(u16 item)
{
    switch (GetItemBattleUsage(item))
    {
    case EFFECT_ITEM_INCREASE_STAT:
    case EFFECT_ITEM_INCREASE_ALL_STATS:
    case EFFECT_ITEM_SET_FOCUS_ENERGY:
    case EFFECT_ITEM_SET_MIST:
        return TRUE;
    default:
        return FALSE;
    }
}

static bool8 RestoresOneMove(u16 item)
{
    return GetItemBattleUsage(item) == EFFECT_ITEM_RESTORE_PP
        && (GetItemEffect(item)[4] & ITEM4_HEAL_PP_ONE);
}

bool8 CoopBattleItems_BindAction(u32 battler, const struct CoopBattleAction *action)
{
    struct Pokemon *mon;

    if (action == NULL || action->kind != COOP_BATTLE_ACTION_ITEM
     || !CoopBattleRuntime_IsSharedBattleItem(action->item)
     || action->index >= COOP_BATTLE_MULTI_PARTY_SIZE || action->target >= MAX_MON_MOVES
     || (action->target != 0 && !RestoresOneMove(action->item)))
        return FALSE;
    mon = &GetBattlerParty(battler)[action->index];
    if (GetMonData(mon, MON_DATA_SPECIES) == SPECIES_NONE || GetMonData(mon, MON_DATA_IS_EGG)
     || (TargetsActiveBattler(action->item) && action->index != gBattlerPartyIndexes[battler])
     || (RestoresOneMove(action->item)
      && GetMonData(mon, MON_DATA_MOVE1 + action->target) == MOVE_NONE))
        return FALSE;
    gBattleStruct->itemPartyIndex[battler] = action->index;
    gBattleStruct->itemMoveIndex[battler] = action->target;
    gBattleStruct->chosenItem[battler] = action->item;
    return TRUE;
}

bool8 CoopBattleItems_MakeAction(u32 battler, struct CoopBattleAction *action)
{
    struct Pokemon *party = GetBattlerParty(battler);
    u16 item = gBattleResources->bufferB[battler][1] | (gBattleResources->bufferB[battler][2] << 8);
    u8 slot;

    for (slot = 0; slot < COOP_BATTLE_MULTI_PARTY_SIZE; slot++)
    {
        if (GetMonData(&party[slot], MON_DATA_SPECIES) != SPECIES_NONE
         && GetMonData(&party[slot], MON_DATA_PERSONALITY) == gBattleStruct->coopItemPersonality)
            break;
    }
    if (slot == COOP_BATTLE_MULTI_PARTY_SIZE)
        return FALSE;
    action->kind = COOP_BATTLE_ACTION_ITEM;
    action->index = slot;
    action->target = RestoresOneMove(item) ? gBattleStruct->itemMoveIndex[battler] : 0;
    action->item = item;
    return CoopBattleItems_BindAction(battler, action);
}

void CoopBattleItems_OnItemUsed(u32 battler, u16 item)
{
    u8 i;

    /* Only the local member's own battler draws on this ROM's bag. */
    if (!CoopBattleItems_IsBagOpen()
     || battler != GetBattlerAtPosition(B_POSITION_PLAYER_LEFT)
     || !CoopBattleItems_IsConsumed(item) || !RemoveBagItem(item, 1))
        return;
    i = LedgerFind(item);
    if (i < COOP_BATTLE_ITEM_LEDGER_SIZE)
    {
        sLedger.items[i] = item;
        sLedger.counts[i]++;
    }
}

void CoopBattleItems_Begin(void)
{
    memset(&sLedger, 0, sizeof(sLedger));
}

void CoopBattleItems_Settle(bool8 completed)
{
    u8 i;

    if (!completed)
    {
        for (i = 0; i < COOP_BATTLE_ITEM_LEDGER_SIZE; i++)
            if (sLedger.counts[i] != 0)
                AddBagItem(sLedger.items[i], sLedger.counts[i]);
    }
    memset(&sLedger, 0, sizeof(sLedger));
}
