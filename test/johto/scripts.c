#include "global.h"
#include "event_data.h"
#include "event_object_movement.h"
#include "fieldmap.h"
#include "load_save.h"
#include "script.h"
#include "script_movement.h"
#include "task.h"
#include "test/test.h"
#include "constants/event_objects.h"
#include "constants/event_object_movement.h"
#include "constants/vars.h"

extern bool8 ScrCmd_applymovement(struct ScriptContext *ctx);
extern void Script_JohtoApplyMovement(struct ScriptContext *ctx);

static const u8 sFaceRightMovement[] =
{
    MOVEMENT_ACTION_FACE_RIGHT,
    MOVEMENT_ACTION_STEP_END,
};

static const u8 sFinishedMovement[] =
{
    MOVEMENT_ACTION_STEP_END,
};

static void PutHalfword(u8 *dest, u16 value)
{
    dest[0] = value;
    dest[1] = value >> 8;
}

static void PutWord(u8 *dest, u32 value)
{
    dest[0] = value;
    dest[1] = value >> 8;
    dest[2] = value >> 16;
    dest[3] = value >> 24;
}

static void PutApplyMovementPayload(u8 *dest, u16 variableId, const u8 *movement)
{
    PutHalfword(dest, variableId);
    PutWord(dest + 2, (u32)(uintptr_t)movement);
}

static void ResetMovementFixture(void)
{
    SetSaveBlocksPointers(0);
    ResetTasks();
    memset(gObjectEvents, 0, sizeof(gObjectEvents));
    memset(gSprites, 0, sizeof(gSprites));
    gSaveBlock1Ptr->location.mapGroup = 1;
    gSaveBlock1Ptr->location.mapNum = 1;

    gObjectEvents[1].active = TRUE;
    gObjectEvents[1].isPlayer = TRUE;
    gObjectEvents[1].localId = LOCALID_PLAYER;
    gObjectEvents[1].mapGroup = 1;
    gObjectEvents[1].mapNum = 1;
    gObjectEvents[1].spriteId = 1;
    gSprites[1].inUse = TRUE;
    gSprites[1].data[0] = 1;
    gPlayerAvatar.objectEventId = 1;
}

static void SetObjectFixture(u8 objectEventId, u8 localId)
{
    gObjectEvents[objectEventId].active = TRUE;
    gObjectEvents[objectEventId].localId = localId;
    gObjectEvents[objectEventId].mapGroup = 1;
    gObjectEvents[objectEventId].mapNum = 1;
    gObjectEvents[objectEventId].spriteId = objectEventId;
    gObjectEvents[objectEventId].currentCoords.x = MAP_OFFSET + 5;
    gObjectEvents[objectEventId].currentCoords.y = MAP_OFFSET + 5;
    gObjectEvents[objectEventId].previousCoords = gObjectEvents[objectEventId].currentCoords;
    gObjectEvents[objectEventId].initialCoords = gObjectEvents[objectEventId].currentCoords;
    gSprites[objectEventId].inUse = TRUE;
    gSprites[objectEventId].data[0] = objectEventId;
}

static void InitMovementContext(struct ScriptContext *ctx, u8 *payload)
{
    InitScriptContext(ctx, NULL, NULL);
    ctx->scriptPtr = payload;
}

TEST("Johto movement consumes invalid local IDs without touching object or sprite slots")
{
    struct ScriptContext ctx;
    struct ObjectEvent objectEventBefore;
    struct Sprite spriteBefore;
    u8 payload[sizeof(u16) + sizeof(u32)];

    ResetMovementFixture();
    SetObjectFixture(0, 1);
    objectEventBefore = gObjectEvents[0];
    spriteBefore = gSprites[0];

    // 0x101 must not be truncated to the live local ID 1.
    EXPECT(VarSet(VAR_TEMP_0, 0x101));
    PutApplyMovementPayload(payload, VAR_TEMP_0, sFaceRightMovement);
    InitMovementContext(&ctx, payload);
    Script_JohtoApplyMovement(&ctx);

    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    EXPECT_EQ(memcmp(&gObjectEvents[0], &objectEventBefore, sizeof(objectEventBefore)), 0);
    EXPECT_EQ(memcmp(&gSprites[0], &spriteBefore, sizeof(spriteBefore)), 0);

    // A missing but representable local ID follows the same safe path.
    EXPECT(VarSet(VAR_TEMP_0, 2));
    PutApplyMovementPayload(payload, VAR_TEMP_0, sFaceRightMovement);
    InitMovementContext(&ctx, payload);
    Script_JohtoApplyMovement(&ctx);
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    EXPECT_EQ(memcmp(&gObjectEvents[0], &objectEventBefore, sizeof(objectEventBefore)), 0);
    EXPECT_EQ(memcmp(&gSprites[0], &spriteBefore, sizeof(spriteBefore)), 0);
}

TEST("Johto movement keeps a follower visible while ordinary applymovement retains hiding")
{
    struct ScriptContext ctx;
    u8 payload[sizeof(u16) + sizeof(u32)];

    ResetMovementFixture();
    SetObjectFixture(0, 1);
    SetObjectFixture(2, OBJ_EVENT_ID_FOLLOWER);

    // Johto's same-map native path moves the requested object and leaves the
    // follower alone, so no follower movement is queued.
    EXPECT(VarSet(VAR_TEMP_0, 1));
    PutApplyMovementPayload(payload, VAR_TEMP_0, sFaceRightMovement);
    InitMovementContext(&ctx, payload);
    Script_JohtoApplyMovement(&ctx);
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    EXPECT(!gObjectEvents[2].invisible);
    EXPECT(ScriptMovement_IsObjectMovementFinished(OBJ_EVENT_ID_FOLLOWER,
                                                   gSaveBlock1Ptr->location.mapNum,
                                                   gSaveBlock1Ptr->location.mapGroup));
    ResetTasks();

    // The host command still schedules the follower's enter-ball movement.
    EXPECT(VarSet(VAR_TEMP_0, 1));
    PutApplyMovementPayload(payload, VAR_TEMP_0, sFaceRightMovement);
    InitMovementContext(&ctx, payload);
    EXPECT(!ScrCmd_applymovement(&ctx));
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    EXPECT(!ScriptMovement_IsObjectMovementFinished(OBJ_EVENT_ID_FOLLOWER,
                                                    gSaveBlock1Ptr->location.mapNum,
                                                    gSaveBlock1Ptr->location.mapGroup));
    ResetTasks();
}

TEST("Johto follower movement resets frozen animation and completes through the host wait tracker")
{
    struct ScriptContext ctx;
    u8 payload[sizeof(u16) + sizeof(u32)];

    ResetMovementFixture();
    SetObjectFixture(2, OBJ_EVENT_ID_FOLLOWER);
    gObjectEvents[2].frozen = TRUE;
    gObjectEvents[2].directionOverwrite = DIR_WEST;
    gSprites[2].animCmdIndex = 3;

    EXPECT(VarSet(VAR_TEMP_0, OBJ_EVENT_ID_FOLLOWER));
    PutApplyMovementPayload(payload, VAR_TEMP_0, sFinishedMovement);
    InitMovementContext(&ctx, payload);
    Script_JohtoApplyMovement(&ctx);

    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
    EXPECT_EQ((u8)gObjectEvents[2].directionOverwrite, DIR_NONE);
    EXPECT_EQ(gSprites[2].animCmdIndex, 0);
    EXPECT(!ScriptMovement_IsObjectMovementFinished(OBJ_EVENT_ID_FOLLOWER,
                                                    gSaveBlock1Ptr->location.mapNum,
                                                    gSaveBlock1Ptr->location.mapGroup));

    // STEP_END is consumed by the real movement task, giving the native
    // wait tracker a concrete completion state without a timing stub.
    RunTasks();
    EXPECT(ScriptMovement_IsObjectMovementFinished(OBJ_EVENT_ID_FOLLOWER,
                                                   gSaveBlock1Ptr->location.mapNum,
                                                   gSaveBlock1Ptr->location.mapGroup));
    EXPECT(gObjectEvents[2].frozen);
    ScriptMovement_UnfreezeObjectEvents();
}
