#ifndef GUARD_COOP_BATTLE_ITEMS_H
#define GUARD_COOP_BATTLE_ITEMS_H

#include "global.h"

struct CoopBattleAction;
struct Pokemon;

/* Bag items in co-op trainer battles (friendly battles keep the bag shut).
 *
 * Picking an item applies nothing and removes nothing: the choice becomes a
 * COOP_BATTLE_ACTION_ITEM that goes through lockstep, and on the resolved
 * turn both ROMs run the vanilla item action (HandleAction_UseItem and the
 * item battle scripts) with the same battler, item, party slot and move
 * slot. The acting ROM removes the item from its own bag when that action
 * runs; the partner's ROM never touches its bag. A battle that does not
 * complete restores the pre-battle party, so the items it used go back to
 * the bag as well; a completed battle keeps both, like damage. */

#define COOP_BATTLE_ITEM_LEDGER_SIZE 6

/* TRUE while a co-op trainer battle engine runs (not friendly). */
bool8 CoopBattleItems_IsBagOpen(void);
/* Selection gate for the bag's "Use": NULL when the item may be chosen,
 * otherwise the message to show. Nothing is sent for a refused item. */
const u8 *CoopBattleItems_RefuseItem(u16 item);
/* The party menu target's party: NULL when it is the local player's own,
 * otherwise the message to show (each player uses items only on its own
 * Pokemon; a Revive on the partner's fainted one is refused too). */
const u8 *CoopBattleItems_RefuseTarget(const struct Pokemon *party);
/* The local player picked this Pokemon as the item's target. */
void CoopBattleItems_ChooseTarget(struct Pokemon *mon);
/* Vanilla consumption: flutes and key items stay in the bag. */
bool8 CoopBattleItems_IsConsumed(u16 item);
/* Local ROM: the action for the local battler's confirmed bag choice. */
bool8 CoopBattleItems_MakeAction(u32 battler, struct CoopBattleAction *action);
/* Both ROMs: the engine inputs HandleAction_UseItem reads for an action. */
bool8 CoopBattleItems_BindAction(u32 battler, const struct CoopBattleAction *action);
/* HandleAction_UseItem: the acting ROM takes the item out of its bag. */
void CoopBattleItems_OnItemUsed(u32 battler, u16 item);
/* Battle setup: a new battle has used nothing yet. */
void CoopBattleItems_Begin(void);
/* Battle end: a completed battle keeps the removals; any other end returns
 * every item the battle took to the bag. */
void CoopBattleItems_Settle(bool8 completed);

#endif // GUARD_COOP_BATTLE_ITEMS_H
