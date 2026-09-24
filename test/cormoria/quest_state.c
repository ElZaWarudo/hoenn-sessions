#include "global.h"
#include "cormoria/quest_state.h"
#include "load_save.h"
#include "malloc.h"
#include "test/test.h"
#include "world/event_save.h"

TEST("Cormoria quest state keeps each donor bit independently")
{
    bool8 value = TRUE;
    u16 quest;
    u16 bit;

    WorldEventSave_InitializeCurrent();
    for (quest = 0; quest < CORMORIA_QUEST_COUNT; quest++)
    {
        for (bit = 0; bit < CORMORIA_QUEST_BITS_PER_QUEST; bit++)
        {
            EXPECT(CormoriaQuestState_Get(quest, bit, &value));
            EXPECT(!value);
            EXPECT(CormoriaQuestState_Set(quest, bit, TRUE));
            EXPECT(CormoriaQuestState_Get(quest, bit, &value));
            EXPECT(value);
        }
    }
    EXPECT(WorldEventSave_GetFlag(CORMORIA_QUEST_FLAG_BASE));
    EXPECT(WorldEventSave_GetFlag(CORMORIA_SUBQUEST_FLAG_BASE - 1));
    EXPECT(CormoriaQuestState_Set(0, CORMORIA_QUEST_ACTIVE, FALSE));
    EXPECT(CormoriaQuestState_Get(0, CORMORIA_QUEST_UNLOCKED, &value));
    EXPECT(value);
    EXPECT(CormoriaQuestState_Get(0, CORMORIA_QUEST_ACTIVE, &value));
    EXPECT(!value);
}

TEST("Cormoria subquest completions occupy the following 21 flags")
{
    bool8 completed = TRUE;
    u16 subquest;

    WorldEventSave_InitializeCurrent();
    for (subquest = 0; subquest < CORMORIA_SUBQUEST_COUNT; subquest++)
    {
        EXPECT(CormoriaQuestState_GetSubquest(subquest, &completed));
        EXPECT(!completed);
        EXPECT(CormoriaQuestState_SetSubquest(subquest, TRUE));
        EXPECT(WorldEventSave_GetFlag(CORMORIA_SUBQUEST_FLAG_BASE + subquest));
    }
    EXPECT(CormoriaQuestState_GetSubquest(20, &completed));
    EXPECT(completed);
    EXPECT(!WorldEventSave_GetFlag(CORMORIA_QUEST_FLAG_END));
}

TEST("Cormoria quest adapter rejects invalid indices and corrupt record without mutation")
{
    struct WorldEventSaveV1 *before = Alloc(sizeof(*before));
    bool8 value = TRUE;

    ASSUME(before != NULL);
    WorldEventSave_InitializeCurrent();
    *before = gSaveblock1.world_event;
    EXPECT(!CormoriaQuestState_Get(CORMORIA_QUEST_COUNT, CORMORIA_QUEST_UNLOCKED, &value));
    EXPECT(!CormoriaQuestState_Set(CORMORIA_QUEST_COUNT, CORMORIA_QUEST_UNLOCKED, TRUE));
    EXPECT(!CormoriaQuestState_Get(0, CORMORIA_QUEST_BITS_PER_QUEST, &value));
    EXPECT(!CormoriaQuestState_Set(0, CORMORIA_QUEST_BITS_PER_QUEST, TRUE));
    EXPECT(!CormoriaQuestState_Set(0, (enum CormoriaQuestBit)-1, TRUE));
    EXPECT(!CormoriaQuestState_Get(0, CORMORIA_QUEST_UNLOCKED, NULL));
    EXPECT(!CormoriaQuestState_GetSubquest(CORMORIA_SUBQUEST_COUNT, &value));
    EXPECT(!CormoriaQuestState_SetSubquest(CORMORIA_SUBQUEST_COUNT, TRUE));
    EXPECT(!CormoriaQuestState_GetSubquest(0, NULL));
    EXPECT(value);
    EXPECT_EQ(memcmp(before, &gSaveblock1.world_event, sizeof(*before)), 0);

    gSaveblock1.world_event.crc32 ^= 1;
    *before = gSaveblock1.world_event;
    EXPECT(!CormoriaQuestState_Get(0, CORMORIA_QUEST_UNLOCKED, &value));
    EXPECT(!CormoriaQuestState_Set(0, CORMORIA_QUEST_UNLOCKED, TRUE));
    EXPECT(!CormoriaQuestState_GetSubquest(0, &value));
    EXPECT(!CormoriaQuestState_SetSubquest(0, TRUE));
    EXPECT(value);
    EXPECT_EQ(memcmp(before, &gSaveblock1.world_event, sizeof(*before)), 0);
    Free(before);
}
