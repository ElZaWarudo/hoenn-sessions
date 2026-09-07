#include "global.h"
#include "event_data.h"
#include "dexnav.h"
#include "start_menu.h"
#include "window.h"
#include "menu.h"
#include "text.h"
#include "coop/online.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "test/test.h"

TEST("Cloud Coop Online window graphics preserve field borders and tilemaps")
{
    const struct WindowTemplate *window = CoopOnline_TestWindowTemplate();
    u32 endTile = window->baseBlock + window->width * window->height;

    // Field BG0 uses character base 2 (0x8000), with 32-byte 4bpp tiles.
    // Dialogue borders start at tile 0x200; field tilemaps start at 0xE000.
    // Check the actual allocation: base 0x220 previously overwrote both regions.
    EXPECT_EQ(window->bg, 0);
    EXPECT(window->width != 0 && window->height != 0);
    EXPECT(endTile <= 0x200);
    EXPECT(0x8000 + endTile * 32 <= 0xE000);
}

TEST("Cloud Coop Online fully unlocked pause menu fits the screen")
{
    bool8 dex = FlagGet(FLAG_SYS_POKEDEX_GET);
    bool8 pokemon = FlagGet(FLAG_SYS_POKEMON_GET);
    bool8 nav = FlagGet(FLAG_SYS_POKENAV_GET);
    bool8 dexnav = DN_FLAG_DEXNAV_GET != 0 && FlagGet(DN_FLAG_DEXNAV_GET);
    FlagSet(FLAG_SYS_POKEDEX_GET);
    FlagSet(FLAG_SYS_POKEMON_GET);
    FlagSet(FLAG_SYS_POKENAV_GET);
    if (DN_FLAG_DEXNAV_GET != 0)
        FlagSet(DN_FLAG_DEXNAV_GET);
    EXPECT_EQ(CoopStartMenu_TestBuildNormal(), DN_FLAG_DEXNAV_GET != 0 ? 11 : 10);
    EXPECT_EQ(CoopStartMenu_TestVisibleCount(), 8);
    if (!dex) FlagClear(FLAG_SYS_POKEDEX_GET);
    if (!pokemon) FlagClear(FLAG_SYS_POKEMON_GET);
    if (!nav) FlagClear(FLAG_SYS_POKENAV_GET);
    if (DN_FLAG_DEXNAV_GET != 0 && !dexnav) FlagClear(DN_FLAG_DEXNAV_GET);
}

TEST("Cloud Coop Character pause label fits without overwriting field tiles")
{
    u8 window;
    u32 width, height, base;
    InitStandardTextBoxWindows();
    window = AddStartMenuWindow(8);
    EXPECT_NE(window, WINDOW_NONE);
    width = GetWindowAttribute(window, WINDOW_WIDTH);
    height = GetWindowAttribute(window, WINDOW_HEIGHT);
    base = GetWindowAttribute(window, WINDOW_BASE_BLOCK);
    EXPECT(8 + GetStringWidth(FONT_NORMAL, COMPOUND_STRING("CHARACTER"), 0) <= width * 8);
    EXPECT(GetWindowAttribute(window, WINDOW_TILEMAP_LEFT) + width <= 29);
    EXPECT(base + width * height <= 0x200);
    RemoveStartMenuWindow();
    FreeAllWindowBuffers();
}

static u32 sHostSequence;

static void InitOnlineMenu(void)
{
    struct CoopBridgeMessage message;
    CoopSave_InitializeCurrent();
    CoopNetBridge_Init();
    while (CoopNetBridge_DequeueGameToNetwork(&message))
        ;
    sHostSequence = 1;
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_SESSION_READY,
                                 sHostSequence, 7, NULL, 0));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    while (CoopNetBridge_DequeueGameToNetwork(&message))
        ;
    CoopOnline_TestBegin();
}

static struct CoopBridgeMessage MenuRequest(void)
{
    struct CoopBridgeMessage message = {0};
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&message));
    EXPECT_EQ(message.type, COOP_BRIDGE_MESSAGE_ONLINE_REQUEST);
    return message;
}

static void Reply(const struct CoopBridgeMessage *request, u8 result)
{
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};
    struct CoopBridgeMessage message;
    memcpy(payload, request->payload, 4);
    payload[4] = result;
    payload[5] = COOP_ONLINE_HAS_NEARBY;
    payload[6] = 1;
    memcpy(payload + 16, "may", 3);
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 ++sHostSequence, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    CoopOnline_TestPoll();
}

TEST("Cloud Coop Online Back works during loading and duplicate A sends nothing")
{
    struct CoopBridgeMessage request;
    InitOnlineMenu();
    request = MenuRequest();
    EXPECT_EQ(request.payload[8], COOP_ONLINE_REFRESH);
    EXPECT(CoopOnline_TestPending());
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(CoopOnline_TestInput(B_BUTTON));
}

TEST("Cloud Coop Online mutation uses the displayed view once and Back stays available")
{
    struct CoopBridgeMessage request, invite;
    InitOnlineMenu();
    request = MenuRequest();
    Reply(&request, COOP_ONLINE_READY);
    EXPECT(!CoopOnline_TestInput(A_BUTTON)); // Open nearby and refresh.
    request = MenuRequest();
    Reply(&request, COOP_ONLINE_READY);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    invite = MenuRequest();
    EXPECT_EQ(invite.payload[8], COOP_ONLINE_INVITE);
    EXPECT(memcmp(invite.payload + 4, request.payload, 4) == 0);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(!CoopOnline_TestInput(B_BUTTON)); // Return to Online while request runs.
    EXPECT(CoopOnline_TestPending());
    EXPECT(CoopOnline_TestInput(B_BUTTON));
}

TEST("Cloud Coop Online timeout permits Refresh and ignores the retired reply")
{
    struct CoopBridgeMessage old, fresh;
    u32 i;
    InitOnlineMenu();
    old = MenuRequest();
    for (i = 0; i < 300; i++) CoopOnline_TestPoll();
    EXPECT(!CoopOnline_TestPending());
    EXPECT_EQ(CoopOnline_TestResult(), COOP_ONLINE_UNAVAILABLE);
    CoopOnline_TestInput(DPAD_DOWN);
    CoopOnline_TestInput(DPAD_DOWN);
    CoopOnline_TestInput(A_BUTTON);
    fresh = MenuRequest();
    Reply(&old, COOP_ONLINE_READY);
    EXPECT(CoopOnline_TestPending());
    Reply(&fresh, COOP_ONLINE_READY);
    EXPECT(!CoopOnline_TestPending());
    EXPECT_EQ(CoopOnline_TestResult(), COOP_ONLINE_READY);
}

TEST("Cloud Coop Online expired selection cannot submit an invitation")
{
    struct CoopBridgeMessage request;
    InitOnlineMenu();
    request = MenuRequest();
    Reply(&request, COOP_ONLINE_READY);
    CoopOnline_TestInput(A_BUTTON);
    request = MenuRequest();
    Reply(&request, COOP_ONLINE_STALE);
    CoopOnline_TestInput(A_BUTTON);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(!CoopOnline_TestInput(B_BUTTON));
    EXPECT(CoopOnline_TestInput(B_BUTTON));
}
