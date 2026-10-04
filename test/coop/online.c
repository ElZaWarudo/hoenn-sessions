#include "global.h"
#include "event_data.h"
#include "dexnav.h"
#include "start_menu.h"
#include "window.h"
#include "menu.h"
#include "pokemon.h"
#include "text.h"
#include "string_util.h"
#include "coop/online.h"
#include "coop/net_bridge.h"
#include "coop/save.h"
#include "coop/trade_offer.h"
#include "coop/battle_consent.h"
#include "coop/friendly_battle.h"
#include "pokemon.h"
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

TEST("Cloud Coop transferred party unlocks Pokemon pause menu without local flag")
{
    bool8 hadPokemonFlag = FlagGet(FLAG_SYS_POKEMON_GET);
    u8 savedPartyCount = gPlayerPartyCount;
    u8 emptyMenuCount;

    FlagClear(FLAG_SYS_POKEMON_GET);
    gPlayerPartyCount = 0;
    emptyMenuCount = CoopStartMenu_TestBuildNormal();

    gPlayerPartyCount = 1;
    EXPECT_EQ(CoopStartMenu_TestBuildNormal(), emptyMenuCount + 1);

    gPlayerPartyCount = 0;
    FlagSet(FLAG_SYS_POKEMON_GET);
    EXPECT_EQ(CoopStartMenu_TestBuildNormal(), emptyMenuCount + 1);

    gPlayerPartyCount = savedPartyCount;
    if (!hadPokemonFlag)
        FlagClear(FLAG_SYS_POKEMON_GET);
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

TEST("Cloud Coop ONLINE creates and redeems bounded pairing codes")
{
    struct CoopBridgeMessage request, reply;
    struct CoopPairingStatus status;
    u8 payload[COOP_PAIRING_RECORD_SIZE] = {0};
    u8 i;
    InitOnlineMenu();
    request = MenuRequest();
    Reply(&request, COOP_ONLINE_READY);
    for (i = 0; i < 3; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&request));
    EXPECT_EQ(request.type, COOP_BRIDGE_MESSAGE_PAIRING_REQUEST);
    EXPECT_EQ(request.length, COOP_PAIRING_RECORD_SIZE);
    EXPECT_EQ(request.payload[4], COOP_PAIRING_CREATE);
    memcpy(payload, request.payload, 4);
    payload[4] = COOP_PAIRING_CREATED;
    memcpy(payload + 5, "HX7-4QK", 7);
    EXPECT(CoopBridgeMessage_Seal(&reply, COOP_BRIDGE_MESSAGE_PAIRING_STATUS,
                                 ++sHostSequence, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&reply));
    CoopNetBridge_Poll();
    CoopOnline_TestPoll();
    EXPECT(CoopNetBridge_GetPairingStatus(&status));
    EXPECT_EQ(status.result, COOP_PAIRING_CREATED);
    EXPECT_EQ(memcmp(status.code, "HX7-4QK", 7), 0);
    CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(!CoopOnline_TestInput(START_BUTTON));
    EXPECT(CoopNetBridge_DequeueGameToNetwork(&request));
    EXPECT_EQ(request.type, COOP_BRIDGE_MESSAGE_PAIRING_REQUEST);
    EXPECT_EQ(request.payload[4], COOP_PAIRING_REDEEM);
    EXPECT_EQ(request.payload[8], '-');
}

TEST("Cloud Coop pushed invitation refreshes before accepting and B can dismiss")
{
    struct CoopBridgeMessage ready, action;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};
    struct CoopBridgeMessage message;
    InitOnlineMenu();
    ready = MenuRequest();
    Reply(&ready, COOP_ONLINE_READY);
    CoopOnline_TestBeginInvite();
    ready = MenuRequest();
    EXPECT_EQ(ready.payload[8], COOP_ONLINE_REFRESH);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    memcpy(payload, ready.payload, 4);
    payload[4] = COOP_ONLINE_READY;
    payload[5] = COOP_ONLINE_HAS_INCOMING;
    payload[7] = 1;
    memcpy(payload + 48, "may", 3);
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 ++sHostSequence, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    CoopOnline_TestPoll();
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    action = MenuRequest();
    EXPECT_EQ(action.payload[8], COOP_ONLINE_ACCEPT);
    EXPECT(memcmp(action.payload + 4, ready.payload, 4) == 0);
    EXPECT(!CoopOnline_TestInput(B_BUTTON));
    EXPECT(CoopOnline_TestInput(B_BUTTON));
}

TEST("Cloud Coop sender can cancel a displayed invitation")
{
    struct CoopBridgeMessage ready, action, message;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};
    InitOnlineMenu();
    ready = MenuRequest();
    Reply(&ready, COOP_ONLINE_READY);
    CoopOnline_TestInput(DPAD_DOWN);
    CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    ready = MenuRequest();
    EXPECT_EQ(ready.payload[8], COOP_ONLINE_REFRESH);
    memcpy(payload, ready.payload, 4);
    payload[4] = COOP_ONLINE_READY;
    payload[5] = COOP_ONLINE_HAS_OUTGOING;
    payload[10] = 1;
    memcpy(payload + 80, "may", 3);
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 ++sHostSequence, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    CoopOnline_TestPoll();
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    action = MenuRequest();
    EXPECT_EQ(action.payload[8], COOP_ONLINE_CANCEL);
    EXPECT(memcmp(action.payload + 4, ready.payload, 4) == 0);
}

TEST("Cloud Coop grouped Online opens partner location from a catalogued map")
{
    struct CoopBridgeMessage ready, message;
    struct CoopOnlineStatus status;
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};
    InitOnlineMenu();
    ready = MenuRequest();
    memcpy(payload, ready.payload, 4);
    payload[4] = COOP_ONLINE_READY;
    payload[5] = COOP_ONLINE_GROUPED | COOP_ONLINE_HAS_LOCATION;
    payload[14] = 1; // Hoenn's Slateport City is catalogued as map 0:1.
    memcpy(payload + 80, "may", 3);
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 ++sHostSequence, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    CoopOnline_TestPoll();
    EXPECT(CoopNetBridge_GetOnlineStatus(&status));
    EXPECT_EQ(status.location_map_group, 0);
    EXPECT_EQ(status.location_map_number, 1);
    CoopOnline_TestInput(DPAD_DOWN);
    CoopOnline_TestInput(DPAD_DOWN);
    CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopOnline_TestIsLocationPage());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(!CoopOnline_TestInput(B_BUTTON));
    EXPECT(!CoopOnline_TestIsLocationPage());
}

bool8 CoopOnline_TestIsLastPartnerPage(void);
bool8 CoopOnline_TestIsPairingPage(void);
const u8 *CoopOnline_TestResultText(void);
const u8 *CoopOnline_TestLastPartnerName(void);

static void ReplyStatus(const struct CoopBridgeMessage *request, u8 result, u8 flags, const char *partner)
{
    u8 payload[COOP_ONLINE_STATUS_SIZE] = {0};
    struct CoopBridgeMessage message;
    memcpy(payload, request->payload, 4);
    payload[4] = result;
    payload[5] = flags;
    if (flags & COOP_ONLINE_GROUPED) memcpy(payload + 80, "may", 3);
    if (partner != NULL) memcpy(payload + 112, partner, strlen(partner));
    EXPECT(CoopBridgeMessage_Seal(&message, COOP_BRIDGE_MESSAGE_ONLINE_STATUS,
                                 ++sHostSequence, 7, payload, sizeof(payload)));
    EXPECT(CoopNetBridge_EnqueueNetworkToGame(&message));
    CoopNetBridge_Poll();
    CoopOnline_TestPoll();
}

static void ExpectResultText(const u8 *expected)
{
    EXPECT_EQ(StringCompare(CoopOnline_TestResultText(), expected), 0);
}

static struct CoopBridgeMessage OpenLastPartnerPage(void)
{
    struct CoopBridgeMessage ready;
    u8 i;
    InitOnlineMenu();
    ready = MenuRequest();
    ReplyStatus(&ready, COOP_ONLINE_READY, COOP_ONLINE_HAS_LAST_PARTNER, "brendan_77");
    for (i = 0; i < 3; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopOnline_TestIsLastPartnerPage());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    return ready;
}

static struct CoopBridgeMessage InviteLastPartner(const struct CoopBridgeMessage *ready)
{
    struct CoopBridgeMessage action;
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    action = MenuRequest();
    EXPECT_EQ(action.length, COOP_ONLINE_REQUEST_SIZE);
    EXPECT(memcmp(action.payload, ready->payload, 4) != 0);
    EXPECT(memcmp(action.payload + 4, ready->payload, 4) == 0);
    EXPECT_EQ(action.payload[8], COOP_ONLINE_INVITE_LAST_PARTNER);
    EXPECT_EQ(action.payload[9], 0);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(CoopOnline_TestPending());
    ExpectResultText(COMPOUND_STRING("Sending..."));
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    return action;
}

TEST("Cloud Coop ungrouped Online offers Last partner and shows the partner name")
{
    struct CoopBridgeMessage ready;
    const u8 *name;
    u8 i;
    InitOnlineMenu();
    ready = MenuRequest();
    ReplyStatus(&ready, COOP_ONLINE_READY, COOP_ONLINE_HAS_LAST_PARTNER, "brendan_77");
    // Seven entries: Back sits at index 6 once Last partner is offered.
    for (i = 0; i < 6; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(CoopOnline_TestInput(A_BUTTON));
    for (i = 0; i < 3; i++) CoopOnline_TestInput(DPAD_UP);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopOnline_TestIsLastPartnerPage());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    name = CoopOnline_TestLastPartnerName();
    EXPECT(name != NULL);
    EXPECT_EQ(memcmp(name, "brendan_77", 11), 0);
    ExpectResultText(COMPOUND_STRING("Choose an option."));
    CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON)); // Back returns to ONLINE.
    EXPECT(!CoopOnline_TestIsLastPartnerPage());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop Invite last partner sends one request and reports success")
{
    struct CoopBridgeMessage ready = OpenLastPartnerPage();
    struct CoopBridgeMessage action = InviteLastPartner(&ready);
    ReplyStatus(&action, COOP_ONLINE_SUCCESS, COOP_ONLINE_HAS_LAST_PARTNER, "brendan_77");
    EXPECT(!CoopOnline_TestPending());
    EXPECT_EQ(CoopOnline_TestResult(), COOP_ONLINE_SUCCESS);
    ExpectResultText(COMPOUND_STRING("Invitation sent."));
    EXPECT(CoopOnline_TestIsLastPartnerPage());
}

TEST("Cloud Coop Invite last partner reports failure and blocks a resend")
{
    struct CoopBridgeMessage ready = OpenLastPartnerPage();
    struct CoopBridgeMessage action = InviteLastPartner(&ready);
    ReplyStatus(&action, COOP_ONLINE_FAILED, 0, NULL);
    EXPECT(!CoopOnline_TestPending());
    EXPECT_EQ(CoopOnline_TestResult(), COOP_ONLINE_FAILED);
    ExpectResultText(COMPOUND_STRING("Could not finish. Refresh."));
    EXPECT(CoopOnline_TestLastPartnerName() == NULL);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop Invite last partner reports unavailable on bridge result and timeout")
{
    struct CoopBridgeMessage ready = OpenLastPartnerPage();
    struct CoopBridgeMessage action = InviteLastPartner(&ready);
    u32 i;
    ReplyStatus(&action, COOP_ONLINE_UNAVAILABLE, 0, NULL);
    EXPECT(!CoopOnline_TestPending());
    ExpectResultText(COMPOUND_STRING("Unavailable. Try Refresh."));
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    ready = OpenLastPartnerPage();
    action = InviteLastPartner(&ready);
    for (i = 0; i < 300; i++) CoopOnline_TestPoll();
    EXPECT(!CoopOnline_TestPending());
    EXPECT_EQ(CoopOnline_TestResult(), COOP_ONLINE_UNAVAILABLE);
    ExpectResultText(COMPOUND_STRING("Unavailable. Try Refresh."));
    ReplyStatus(&action, COOP_ONLINE_SUCCESS, COOP_ONLINE_HAS_LAST_PARTNER, "brendan_77");
    ExpectResultText(COMPOUND_STRING("Unavailable. Try Refresh."));
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop Online omits Last partner without the last-partner flag")
{
    struct CoopBridgeMessage ready;
    u8 i;
    InitOnlineMenu();
    ready = MenuRequest();
    ReplyStatus(&ready, COOP_ONLINE_READY, 0, NULL);
    // Six entries: Back sits at index 5.
    for (i = 0; i < 5; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(CoopOnline_TestInput(A_BUTTON));
    CoopOnline_TestInput(DPAD_UP);
    CoopOnline_TestInput(DPAD_UP);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopOnline_TestIsPairingPage());
    EXPECT(!CoopOnline_TestIsLastPartnerPage());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
}

TEST("Cloud Coop grouped Online omits Last partner even with the flag")
{
    struct CoopBridgeMessage ready;
    u8 i;
    InitOnlineMenu();
    ready = MenuRequest();
    ReplyStatus(&ready, COOP_ONLINE_READY, COOP_ONLINE_GROUPED | COOP_ONLINE_HAS_LAST_PARTNER, "brendan_77");
    for (i = 0; i < 3; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopOnline_TestIsLocationPage());
    EXPECT(!CoopOnline_TestIsLastPartnerPage());
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    EXPECT(!CoopOnline_TestInput(B_BUTTON));
    for (i = 0; i < 6; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON)); // Index 6 is Leave group, not Pair by code.
    ready = MenuRequest();
    EXPECT_EQ(ready.payload[8], COOP_ONLINE_LEAVE);
    EXPECT(!CoopOnline_TestIsPairingPage());
}

TEST("Cloud Coop grouped Online offers Trade with partner only when a trade can start")
{
    struct CoopBridgeMessage ready;
    u8 i;
    ZeroPlayerPartyMons();
    CreateMon(&gPlayerParty[0], SPECIES_TREECKO, 5, 0x01020304, OTID_STRUCT_PRESET(0x0A0B0C0D));
    gPlayerPartyCount = CalculatePlayerPartyCount();
    InitOnlineMenu();
    ready = MenuRequest();
    ReplyStatus(&ready, COOP_ONLINE_READY, COOP_ONLINE_GROUPED, NULL);
    // One Pokemon cannot be traded: the entry is shown but disabled.
    for (i = 0; i < 4; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));

    CreateMon(&gPlayerParty[1], SPECIES_ZIGZAGOON, 4, 0x0BADF00D, OTID_STRUCT_PRESET(0x22224444));
    gPlayerPartyCount = CalculatePlayerPartyCount();
    EXPECT(CoopTradeOffer_CanBegin());
    // Index 4 closes the menu and starts the trade script; nothing is sent yet.
    EXPECT_EQ(CoopOnline_TestInput(A_BUTTON), 2);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    // Nine entries: Back sits at index 8.
    for (i = 0; i < 4; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT_EQ(CoopOnline_TestInput(A_BUTTON), TRUE);
}

TEST("Cloud Coop grouped Online offers Battle partner and its rules only when a battle can start")
{
    struct CoopBridgeMessage ready;
    struct CoopBattleFriendlyRules rules;
    u8 i;
    ZeroPlayerPartyMons();
    CreateMon(&gPlayerParty[0], SPECIES_TREECKO, 5, 0x01020304, OTID_STRUCT_PRESET(0x0A0B0C0D));
    CalculateMonStats(&gPlayerParty[0]);
    gPlayerPartyCount = CalculatePlayerPartyCount();
    InitOnlineMenu();
    ready = MenuRequest();
    ReplyStatus(&ready, COOP_ONLINE_READY, COOP_ONLINE_GROUPED, NULL);
    EXPECT(CoopFriendly_CanBegin());
    // Index 5, between Trade with partner and Leave group, opens the rules.
    for (i = 0; i < 5; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(CoopOnline_TestIsBattlePage());
    CoopOnline_TestGetBattleRules(&rules);
    EXPECT_EQ(rules.format, COOP_BATTLE_FRIENDLY_SINGLES);
    EXPECT_EQ(rules.level_mode, COOP_BATTLE_FRIENDLY_LEVELS_AS_IS);
    EXPECT_EQ(rules.count, 1); // one usable Pokemon
    // Doubles needs two Pokemon on each side; the count cannot exceed the party.
    EXPECT(!CoopOnline_TestInput(DPAD_RIGHT));
    CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(DPAD_RIGHT));
    CoopOnline_TestGetBattleRules(&rules);
    EXPECT_EQ(rules.format, COOP_BATTLE_FRIENDLY_SINGLES);
    EXPECT_EQ(rules.level_mode, COOP_BATTLE_FRIENDLY_LEVELS_50);
    EXPECT_EQ(rules.count, 1);

    CreateMon(&gPlayerParty[1], SPECIES_ZIGZAGOON, 4, 0x0BADF00D, OTID_STRUCT_PRESET(0x22224444));
    CalculateMonStats(&gPlayerParty[1]);
    CreateMon(&gPlayerParty[2], SPECIES_WURMPLE, 3, 0x0BADF00E, OTID_STRUCT_PRESET(0x22224444));
    CalculateMonStats(&gPlayerParty[2]);
    gPlayerPartyCount = CalculatePlayerPartyCount();
    CoopOnline_TestInput(DPAD_UP);
    CoopOnline_TestInput(DPAD_UP);
    EXPECT(!CoopOnline_TestInput(A_BUTTON)); // Doubles
    CoopOnline_TestGetBattleRules(&rules);
    EXPECT_EQ(rules.format, COOP_BATTLE_FRIENDLY_DOUBLES);
    EXPECT_EQ(rules.count, 2);
    CoopOnline_TestInput(DPAD_DOWN);
    CoopOnline_TestInput(DPAD_DOWN);
    CoopOnline_TestInput(DPAD_RIGHT); // 3
    CoopOnline_TestGetBattleRules(&rules);
    EXPECT_EQ(rules.count, 3);
    CoopOnline_TestInput(DPAD_RIGHT); // wraps to the doubles minimum
    CoopOnline_TestGetBattleRules(&rules);
    EXPECT_EQ(rules.count, 2);
    CoopOnline_TestInput(DPAD_LEFT); // wraps to the party maximum
    CoopOnline_TestGetBattleRules(&rules);
    EXPECT_EQ(rules.count, 3);
    // Send closes the menu for the challenge script; nothing is sent yet.
    CoopOnline_TestInput(DPAD_DOWN);
    EXPECT_EQ(CoopOnline_TestInput(A_BUTTON), 3);
    EXPECT(CoopBridgeQueue_IsEmpty(&gCoopNetBridge.game_to_network));
    CoopOnline_TestGetBattleRules(&rules);
    EXPECT_EQ(rules.format, COOP_BATTLE_FRIENDLY_DOUBLES);
    EXPECT_EQ(rules.level_mode, COOP_BATTLE_FRIENDLY_LEVELS_50);
    EXPECT_EQ(rules.count, 3);

    // A co-op battle request in flight disables the entry.
    EXPECT(!CoopOnline_TestInput(B_BUTTON));
    EXPECT(!CoopOnline_TestIsBattlePage());
    EXPECT(CoopBattleConsent_Begin(COOP_BATTLE_KIND_FRIENDLY));
    EXPECT(!CoopFriendly_CanBegin());
    for (i = 0; i < 5; i++) CoopOnline_TestInput(DPAD_DOWN);
    EXPECT(!CoopOnline_TestInput(A_BUTTON));
    EXPECT(!CoopOnline_TestIsBattlePage());
}
