#ifndef GUARD_CORMORIA_QUEST_MENU_H
#define GUARD_CORMORIA_QUEST_MENU_H

#include "gba/types.h"
#include "main.h"

/* Opens the Cormoria quest journal and returns through callback after B. */
void CormoriaQuestMenu_Init(MainCallback callback);
void CB2_InitCormoriaQuestMenu(void);

/* Used by quest scripts so announcements and the journal share one table. */
void CormoriaQuestMenu_CopyQuestName(u8 *dst, u8 questId);

#endif // GUARD_CORMORIA_QUEST_MENU_H
