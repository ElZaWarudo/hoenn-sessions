#include "global.h"
#include "coop/online.h"
#include "coop/net_bridge.h"
#include "event_object_lock.h"
#include "menu.h"
#include "script.h"
#include "sound.h"
#include "string_util.h"
#include "task.h"
#include "text.h"
#include "window.h"
#include "main.h"
#include "constants/characters.h"
#include "constants/songs.h"

#define ONLINE_TIMEOUT_FRAMES 300

enum OnlinePage { ONLINE_HOME, ONLINE_NEARBY, ONLINE_INCOMING };

static EWRAM_DATA struct CoopOnlineStatus sStatus;
static EWRAM_DATA u32 sRequestSerial;
static EWRAM_DATA u32 sPendingId;
static EWRAM_DATA u16 sWaitFrames;
static EWRAM_DATA u8 sWindowId;
static EWRAM_DATA u8 sPage;
static EWRAM_DATA u8 sCursor;
static EWRAM_DATA u8 sLastAction;
static EWRAM_DATA bool8 sPending;

static const struct WindowTemplate sOnlineWindow = {
    .bg = 0, .tilemapLeft = 2, .tilemapTop = 1,
    // Above the field text and border tiles; ends below BG tile 1024.
    .width = 26, .height = 18, .paletteNum = 15, .baseBlock = 0x220,
};

static const u8 sOnline[] = _("ONLINE");
static const u8 sNearby[] = _("Nearby players");
static const u8 sIncoming[] = _("Invitations");
static const u8 sLeave[] = _("Leave group");
static const u8 sRefresh[] = _("Refresh");
static const u8 sBack[] = _("Back");
static const u8 sInvite[] = _("Send invitation");
static const u8 sAccept[] = _("Accept");
static const u8 sDecline[] = _("Decline");
static const u8 sNext[] = _("Next player");
static const u8 sNextInvite[] = _("Next invitation");
static const u8 sLoading[] = _("Connecting...");
static const u8 sSending[] = _("Sending...");
static const u8 sUnavailable[] = _("Unavailable. Try Refresh.");
static const u8 sStale[] = _("Selection expired. Refresh.");
static const u8 sFailed[] = _("Could not finish. Refresh.");
static const u8 sReady[] = _("Choose an option.");
static const u8 sSent[] = _("Invitation sent.");
static const u8 sJoined[] = _("Group joined.");
static const u8 sDeclined[] = _("Invitation declined.");
static const u8 sLeft[] = _("Group left.");
static const u8 sNoGroup[] = _("You are not in a group.");
static const u8 sUnknown[] = _("Group status not available.");
static const u8 sGroupWith[] = _("Grouped with:");
static const u8 sNobody[] = _("No eligible nearby players.");
static const u8 sNoInvites[] = _("No pending invitations.");
static const u8 sPaging[] = _("LEFT/RIGHT: page   B: back");
static const u8 sCursorText[] = {CHAR_RIGHT_ARROW, EOS};

static bool8 CanAct(u8 action);

static void Print(const u8 *text, u8 x, u8 y)
{
    AddTextPrinterParameterized(sWindowId, FONT_SMALL, text, x, y, TEXT_SKIP_DRAW, NULL);
}

// Split long names into two complete lines, including the extra-symbol underscore.
static void PrintName(const u8 *name, u8 y)
{
    u8 line[33];
    u32 i, j, part;
    for (part = 0; part < 2; part++)
    {
        j = 0;
        for (i = part * 16; i < (part + 1) * 16 && name[i] != 0; i++)
        {
            u8 c = name[i];
            if (c >= 'a' && c <= 'z') line[j++] = CHAR_a + c - 'a';
            else if (c >= 'A' && c <= 'Z') line[j++] = CHAR_A + c - 'A';
            else if (c >= '0' && c <= '9') line[j++] = CHAR_0 + c - '0';
            else if (c == '.') line[j++] = CHAR_PERIOD;
            else if (c == '-') line[j++] = CHAR_HYPHEN;
            else if (c == '_') { line[j++] = CHAR_EXTRA_SYMBOL; line[j++] = CHAR_UNDERSCORE; }
            else line[j++] = CHAR_QUESTION_MARK;
        }
        line[j] = EOS;
        Print(line, 8, y + part * 12);
        if (i < (part + 1) * 16)
            break;
    }
}

static u8 OptionCount(void)
{
    return sPage == ONLINE_HOME ? (sStatus.flags & COOP_ONLINE_GROUPED ? 5 : 4)
                               : (sPage == ONLINE_INCOMING ? 5 : 4);
}

static const u8 *ResultText(void)
{
    if (sPending) return sLastAction == COOP_ONLINE_REFRESH ? sLoading : sSending;
    switch (sStatus.result)
    {
    case COOP_ONLINE_UNAVAILABLE: return sUnavailable;
    case COOP_ONLINE_STALE: return sStale;
    case COOP_ONLINE_FAILED: return sFailed;
    case COOP_ONLINE_SUCCESS:
        switch (sLastAction)
        {
        case COOP_ONLINE_INVITE: return sSent;
        case COOP_ONLINE_ACCEPT: return sJoined;
        case COOP_ONLINE_DECLINE: return sDeclined;
        case COOP_ONLINE_LEAVE: return sLeft;
        }
    }
    return sReady;
}

static bool8 OptionEnabled(u8 option)
{
    if (option == OptionCount() - 1) return TRUE;
    if (sPending) return FALSE;
    if (sPage == ONLINE_HOME)
        return option != 2 || !(sStatus.flags & COOP_ONLINE_GROUPED) || CanAct(COOP_ONLINE_LEAVE);
    if (sPage == ONLINE_NEARBY)
    {
        if (option == 0) return CanAct(COOP_ONLINE_INVITE);
        if (option == 1) return sStatus.nearby_count > 1;
    }
    else
    {
        if (option < 2) return CanAct(option == 0 ? COOP_ONLINE_ACCEPT : COOP_ONLINE_DECLINE);
        if (option == 2) return sStatus.incoming_count > 1;
    }
    return TRUE;
}

static void Draw(void)
{
    u8 i;
    u8 pageText[8];
    u8 *end;
    const u8 *options[5];
    if (sWindowId == WINDOW_NONE) return;
    if (sCursor >= OptionCount()) sCursor = OptionCount() - 1;
    FillWindowPixelBuffer(sWindowId, PIXEL_FILL(1));
    Print(sPage == ONLINE_HOME ? sOnline : (sPage == ONLINE_NEARBY ? sNearby : sIncoming), 8, 0);
    Print(ResultText(), 8, 14);
    if (sPage == ONLINE_HOME)
    {
        Print(sStatus.request_id == 0 ? sUnknown : (sStatus.flags & COOP_ONLINE_GROUPED ? sGroupWith : sNoGroup), 8, 28);
        if (sStatus.flags & COOP_ONLINE_GROUPED) PrintName(sStatus.group_name, 40);
        options[0] = sNearby; options[1] = sIncoming;
        if (sStatus.flags & COOP_ONLINE_GROUPED)
        {
            options[2] = sLeave; options[3] = sRefresh; options[4] = sBack;
        }
        else { options[2] = sRefresh; options[3] = sBack; }
    }
    else
    {
        u8 count = sPage == ONLINE_NEARBY ? sStatus.nearby_count : sStatus.incoming_count;
        u8 page = sPage == ONLINE_NEARBY ? sStatus.nearby_page : sStatus.incoming_page;
        if (count != 0)
        {
            end = ConvertIntToDecimalStringN(pageText, page + 1, STR_CONV_MODE_LEFT_ALIGN, 2);
            *end++ = CHAR_SLASH;
            ConvertIntToDecimalStringN(end, count, STR_CONV_MODE_LEFT_ALIGN, 2);
            Print(pageText, 176, 0);
        }
        if (sPage == ONLINE_NEARBY)
        {
            if (sStatus.nearby_count) PrintName(sStatus.nearby_name, 30);
            else Print(sNobody, 8, 30);
            options[0] = sInvite; options[1] = sNext; options[2] = sRefresh; options[3] = sBack;
        }
        else
        {
            if (sStatus.incoming_count) PrintName(sStatus.incoming_name, 30);
            else Print(sNoInvites, 8, 30);
            options[0] = sAccept; options[1] = sDecline; options[2] = sNextInvite;
            options[3] = sRefresh; options[4] = sBack;
        }
    }
    for (i = 0; i < OptionCount(); i++)
    {
        static const u8 disabledColors[] = {TEXT_COLOR_WHITE, TEXT_COLOR_LIGHT_GRAY, TEXT_COLOR_WHITE};
        if (OptionEnabled(i)) Print(options[i], 16, 66 + i * 12);
        else AddTextPrinterParameterized3(sWindowId, FONT_SMALL, 16, 66 + i * 12, disabledColors, TEXT_SKIP_DRAW, options[i]);
    }
    Print(sCursorText, 4, 66 + sCursor * 12);
    if (sPage != ONLINE_HOME) Print(sPaging, 8, 130);
    CopyWindowToVram(sWindowId, COPYWIN_GFX);
}

static bool8 CanAct(u8 action)
{
    if (sPending) return FALSE;
    if (action == COOP_ONLINE_REFRESH) return TRUE;
    if (sStatus.request_id == 0 || (sStatus.result != COOP_ONLINE_READY && sStatus.result != COOP_ONLINE_SUCCESS)) return FALSE;
    if (action == COOP_ONLINE_INVITE) return sStatus.nearby_count != 0 && !(sStatus.flags & COOP_ONLINE_GROUPED);
    if (action == COOP_ONLINE_ACCEPT || action == COOP_ONLINE_DECLINE) return sStatus.incoming_count != 0;
    return (sStatus.flags & COOP_ONLINE_GROUPED) != 0;
}

static void Send(u8 action, u8 page)
{
    struct CoopOnlineRequest request;
    if (!CanAct(action)) return;
    if (++sRequestSerial == 0) ++sRequestSerial;
    request = (struct CoopOnlineRequest){ .request_id = sRequestSerial,
        .view_id = action == COOP_ONLINE_REFRESH ? 0 : sStatus.request_id,
        .action = action, .page = page };
    sLastAction = action;
    if (CoopNetBridge_SendOnlineRequest(&request))
    {
        sPending = TRUE;
        sPendingId = request.request_id;
        sWaitFrames = 0;
    }
    else
        sStatus = (struct CoopOnlineStatus){ .result = COOP_ONLINE_UNAVAILABLE };
    Draw();
}

static void Poll(void)
{
    struct CoopOnlineStatus status;
    if (!sPending) return;
    if (CoopNetBridge_GetOnlineStatus(&status) && status.request_id == sPendingId)
    {
        sPending = FALSE;
        sStatus = status;
        Draw();
    }
    else if (++sWaitFrames >= ONLINE_TIMEOUT_FRAMES)
    {
        sPending = FALSE;
        sStatus = (struct CoopOnlineStatus){ .result = COOP_ONLINE_UNAVAILABLE };
        Draw();
    }
}

static void NextPage(s8 delta)
{
    u8 count = sPage == ONLINE_NEARBY ? sStatus.nearby_count : sStatus.incoming_count;
    u8 page = sPage == ONLINE_NEARBY ? sStatus.nearby_page : sStatus.incoming_page;
    if (count < 2) return;
    page = delta < 0 ? (page == 0 ? count - 1 : page - 1) : (page + 1) % count;
    Send(COOP_ONLINE_REFRESH, page);
}

// Back is always available; leaving a submenu does not cancel an accepted request.
static bool8 HandleInput(u16 keys)
{
    if ((keys & B_BUTTON) || ((keys & A_BUTTON) && sCursor == OptionCount() - 1))
    {
        if (sPage == ONLINE_HOME) return TRUE;
        sPage = ONLINE_HOME;
        sCursor = 0;
        Draw();
        return FALSE;
    }
    if (keys & DPAD_UP) sCursor = sCursor == 0 ? OptionCount() - 1 : sCursor - 1;
    if (keys & DPAD_DOWN) sCursor = (sCursor + 1) % OptionCount();
    if (keys & (DPAD_UP | DPAD_DOWN)) Draw();
    if (sPending) return FALSE;
    if (sPage != ONLINE_HOME && (keys & (DPAD_LEFT | DPAD_RIGHT))) NextPage(keys & DPAD_LEFT ? -1 : 1);
    if (!(keys & A_BUTTON)) return FALSE;
    if (sPage == ONLINE_HOME)
    {
        if (sCursor < 2) { sPage = sCursor == 0 ? ONLINE_NEARBY : ONLINE_INCOMING; sCursor = 0; Send(COOP_ONLINE_REFRESH, 0); Draw(); }
        else if (sCursor == 2 && (sStatus.flags & COOP_ONLINE_GROUPED)) Send(COOP_ONLINE_LEAVE, 0);
        else Send(COOP_ONLINE_REFRESH, 0);
    }
    else if (sPage == ONLINE_NEARBY)
    {
        if (sCursor == 0) Send(COOP_ONLINE_INVITE, sStatus.nearby_page);
        else if (sCursor == 1) NextPage(1);
        else Send(COOP_ONLINE_REFRESH, 0);
    }
    else
    {
        if (sCursor < 2) Send(sCursor == 0 ? COOP_ONLINE_ACCEPT : COOP_ONLINE_DECLINE, sStatus.incoming_page);
        else if (sCursor == 2) NextPage(1);
        else Send(COOP_ONLINE_REFRESH, 0);
    }
    return FALSE;
}

static void Begin(void)
{
    sStatus = (struct CoopOnlineStatus){0};
    sPending = FALSE;
    sPage = ONLINE_HOME;
    sCursor = 0;
    Send(COOP_ONLINE_REFRESH, 0);
}

static void Task_Online(u8 taskId)
{
    Poll();
    if (HandleInput(gMain.newKeys))
    {
        PlaySE(SE_SELECT);
        ClearStdWindowAndFrame(sWindowId, TRUE);
        RemoveWindow(sWindowId);
        sWindowId = WINDOW_NONE;
        sPending = FALSE;
        ScriptUnfreezeObjectEvents();
        UnlockPlayerFieldControls();
        DestroyTask(taskId);
    }
}

void CoopOnline_Open(void)
{
    u8 taskId, i;
    // CreateTask returns zero on exhaustion, which may belong to another task.
    for (i = 0; i < NUM_TASKS && gTasks[i].isActive; i++)
        ;
    if (i == NUM_TASKS)
    {
        ScriptUnfreezeObjectEvents();
        UnlockPlayerFieldControls();
        return;
    }
    taskId = CreateTask(Task_Online, 0x50);
    sWindowId = AddWindow(&sOnlineWindow);
    if (taskId == TASK_NONE || sWindowId == WINDOW_NONE)
    {
        if (taskId != TASK_NONE) DestroyTask(taskId);
        if (sWindowId != WINDOW_NONE) RemoveWindow(sWindowId);
        sWindowId = WINDOW_NONE;
        ScriptUnfreezeObjectEvents();
        UnlockPlayerFieldControls();
        return;
    }
    PutWindowTilemap(sWindowId);
    DrawStdWindowFrame(sWindowId, FALSE);
    Begin();
    CopyWindowToVram(sWindowId, COPYWIN_FULL);
}

#if TESTING
void CoopOnline_TestBegin(void) { sWindowId = WINDOW_NONE; Begin(); }
bool8 CoopOnline_TestInput(u16 keys) { return HandleInput(keys); }
void CoopOnline_TestPoll(void) { Poll(); }
bool8 CoopOnline_TestPending(void) { return sPending; }
u8 CoopOnline_TestResult(void) { return sStatus.result; }
#endif
