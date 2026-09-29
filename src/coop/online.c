#include "global.h"
#include "coop/online.h"
#include "coop/net_bridge.h"
#include "coop/presence_runtime.h"
#include "event_object_lock.h"
#include "event_object_movement.h"
#include "field_player_avatar.h"
#include "menu.h"
#include "script.h"
#include "sound.h"
#include "string_util.h"
#include "task.h"
#include "text.h"
#include "window.h"
#include "main.h"
#include "overworld.h"
#include "palette.h"
#include "region_map.h"
#include "constants/region_map_sections.h"
#include "constants/characters.h"
#include "constants/songs.h"

#define ONLINE_TIMEOUT_FRAMES 300
#define PAIRING_TIMEOUT_FRAMES 600

enum OnlinePage { ONLINE_HOME, ONLINE_NEARBY, ONLINE_INCOMING, ONLINE_OUTGOING, ONLINE_LOCATION, ONLINE_PAIRING, ONLINE_PAIRING_ENTRY, ONLINE_LAST_PARTNER };

static EWRAM_DATA struct CoopOnlineStatus sStatus;
static EWRAM_DATA u32 sRequestSerial;
static EWRAM_DATA u32 sPendingId;
static EWRAM_DATA u16 sWaitFrames;
static EWRAM_DATA u8 sWindowId;
static EWRAM_DATA u8 sPage;
static EWRAM_DATA u8 sCursor;
static EWRAM_DATA u8 sLastAction;
static EWRAM_DATA bool8 sPending;
static EWRAM_DATA bool8 sPairingPending;
static EWRAM_DATA u32 sPairingPendingId;
static EWRAM_DATA u16 sPairingWaitFrames;
static EWRAM_DATA struct CoopPairingStatus sPairingStatus;
static EWRAM_DATA bool8 sPairingStatusValid;
static EWRAM_DATA u8 sCode[8];
static EWRAM_DATA u8 sCodePosition;
static EWRAM_DATA bool8 sInvitePrompt;
static EWRAM_DATA bool8 sMenuOpen;

static const struct WindowTemplate sOnlineWindow = {
    .bg = 0, .tilemapLeft = 2, .tilemapTop = 1,
    // Reuse the removed pause menu's tiles. Higher tiles overlap field tilemaps.
    .width = 26, .height = 18, .paletteNum = 15, .baseBlock = 8,
};

static const u8 sOnline[] = _("ONLINE");
static const u8 sPairing[] = _("Pair by code");
static const u8 sLastPartner[] = _("Last partner");
static const u8 sInviteLastPartner[] = _("Invite last partner");
static const u8 sPairingCreate[] = _("Create a code");
static const u8 sPairingEnter[] = _("Enter a code");
static const u8 sPairingHint[] = _("Give this code to your partner.");
static const u8 sPairingControls[] = _("UP/DOWN: letter  A: next");
static const u8 sPairingControls2[] = _("LEFT: back  START: join");
static const u8 sPairingJoined[] = _("Joined. Stay where you are.");
static const u8 sPairingInvalid[] = _("Code invalid or expired.");
static const u8 sPairingUnavailable[] = _("Pairing unavailable.");
static const u8 sPairingReady[] = _("Codes expire in 10 minutes.");
static const char sPairingAlphabet[] = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
static const u8 sNearby[] = _("Nearby players");
static const u8 sIncoming[] = _("Invitations");
static const u8 sOutgoing[] = _("Sent invitations");
static const u8 sCancel[] = _("Cancel invitation");
static const u8 sNoOutgoing[] = _("No sent invitations.");
static const u8 sWhere[] = _("Where is my partner?");
static const u8 sLastSharedLocation[] = _("Partner location:");
static const u8 sLocationUnavailable[] = _("Location unavailable.");
static const u8 sCancelled[] = _("Invitation cancelled.");
static const u8 sJoinQuestion[] = _("Join this player's group?");
static const u8 sYesJoin[] = _("Yes - join");
static const u8 sNoDecline[] = _("No - decline");
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
static const u8 sUnknown[] = _("Status not available.");
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
    if (sPage == ONLINE_PAIRING) return 3;
    if (sPage == ONLINE_PAIRING_ENTRY) return 0;
    if (sPage == ONLINE_LAST_PARTNER) return 2;
    return sPage == ONLINE_HOME ? (sStatus.flags & COOP_ONLINE_GROUPED ? 7 : (sStatus.flags & COOP_ONLINE_HAS_LAST_PARTNER ? 7 : 6))
                               : (sPage == ONLINE_LOCATION ? 2 : (sPage == ONLINE_INCOMING ? 5 : 4));
}

static const u8 *ResultText(void)
{
    if (sPairingPending) return sSending;
    if (sPage == ONLINE_PAIRING || sPage == ONLINE_PAIRING_ENTRY)
    {
        if (!sPairingStatusValid) return sPairingReady;
        if (sPairingStatus.result == COOP_PAIRING_CREATED) return sPairingHint;
        if (sPairingStatus.result == COOP_PAIRING_JOINED) return sPairingJoined;
        if (sPairingStatus.result == COOP_PAIRING_INVALID) return sPairingInvalid;
        if (sPairingStatus.result == COOP_PAIRING_UNAVAILABLE) return sPairingUnavailable;
        return sPairingReady;
    }
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
        case COOP_ONLINE_INVITE_LAST_PARTNER: return sSent;
        case COOP_ONLINE_ACCEPT: return sJoined;
        case COOP_ONLINE_DECLINE: return sDeclined;
        case COOP_ONLINE_LEAVE: return sLeft;
        case COOP_ONLINE_CANCEL: return sCancelled;
        }
    }
    return sReady;
}

static bool8 OptionEnabled(u8 option)
{
    if (option == OptionCount() - 1) return TRUE;
    if (sPending) return FALSE;
    if (sPairingPending) return FALSE;
    if (sPage == ONLINE_PAIRING) return TRUE;
    if (sPage == ONLINE_LAST_PARTNER) return option != 0 || CanAct(COOP_ONLINE_INVITE_LAST_PARTNER);
    if (sPage == ONLINE_HOME)
        return option != 4 || !(sStatus.flags & COOP_ONLINE_GROUPED) || CanAct(COOP_ONLINE_LEAVE);
    if (sPage == ONLINE_LOCATION) return TRUE;
    if (sPage == ONLINE_NEARBY)
    {
        if (option == 0) return CanAct(COOP_ONLINE_INVITE);
        if (option == 1) return sStatus.nearby_count > 1;
    }
    else if (sPage == ONLINE_INCOMING)
    {
        if (option < 2) return CanAct(option == 0 ? COOP_ONLINE_ACCEPT : COOP_ONLINE_DECLINE);
        if (option == 2) return sStatus.incoming_count > 1;
    }
    else
    {
        if (option == 0) return CanAct(COOP_ONLINE_CANCEL);
        if (option == 1) return sStatus.outgoing_count > 1;
    }
    return TRUE;
}

static void Draw(void)
{
    u8 i;
    bool8 known = sStatus.request_id != 0 && (sStatus.result == COOP_ONLINE_READY || sStatus.result == COOP_ONLINE_SUCCESS);
    u8 pageText[8];
    u8 *end;
    const u8 *options[8];
    u8 locationName[32];
    const struct MapHeader *mapHeader;
    if (sWindowId == WINDOW_NONE) return;
    if (sPage != ONLINE_PAIRING_ENTRY && sCursor >= OptionCount()) sCursor = OptionCount() - 1;
    FillWindowPixelBuffer(sWindowId, PIXEL_FILL(1));
    if (sPage == ONLINE_PAIRING || sPage == ONLINE_PAIRING_ENTRY)
    {
        u8 codeText[8];
        Print(sPairing, 8, 0);
        Print(ResultText(), 8, 16);
        if (sPage == ONLINE_PAIRING_ENTRY)
        {
            for (i = 0; i < 7; i++) codeText[i] = i == 3 ? CHAR_HYPHEN : CHAR_A + sCode[i] - 'A';
            codeText[7] = EOS;
            // Digits use a different font block from letters.
            for (i = 0; i < 7; i++) if (sCode[i] >= '0' && sCode[i] <= '9') codeText[i] = CHAR_0 + sCode[i] - '0';
            Print(codeText, 64, 48);
            Print(sCursorText, 64 + (sCodePosition >= 3 ? sCodePosition + 1 : sCodePosition) * 8, 64);
            Print(sPairingControls, 8, 88);
            Print(sPairingControls2, 8, 104);
        }
        else
        {
            if (sPairingStatusValid && sPairingStatus.result == COOP_PAIRING_CREATED)
            {
                for (i = 0; i < 7; i++) codeText[i] = sPairingStatus.code[i];
                codeText[7] = 0;
                PrintName(codeText, 36);
                Print(sPairingReady, 8, 58);
            }
            Print(sPairingCreate, 16, 78);
            Print(sPairingEnter, 16, 94);
            Print(sBack, 16, 110);
            Print(sCursorText, 4, 78 + sCursor * 16);
        }
        CopyWindowToVram(sWindowId, COPYWIN_GFX);
        return;
    }
    Print(sInvitePrompt ? sJoinQuestion : (sPage == ONLINE_HOME ? sOnline : (sPage == ONLINE_NEARBY ? sNearby : (sPage == ONLINE_INCOMING ? sIncoming : (sPage == ONLINE_OUTGOING ? sOutgoing : (sPage == ONLINE_LAST_PARTNER ? sLastPartner : sWhere))))), 8, 0);
    Print(ResultText(), 8, 14);
    if (sPage == ONLINE_HOME)
    {
        Print(!known ? sUnknown : (sStatus.flags & COOP_ONLINE_GROUPED ? sGroupWith : sNoGroup), 8, 28);
        if (known && (sStatus.flags & COOP_ONLINE_GROUPED)) PrintName(sStatus.group_name, 40);
        options[0] = sNearby; options[1] = sIncoming; options[2] = sOutgoing;
        if (sStatus.flags & COOP_ONLINE_GROUPED)
        {
            options[3] = sWhere; options[4] = sLeave; options[5] = sRefresh; options[6] = sBack;
        }
        else if (sStatus.flags & COOP_ONLINE_HAS_LAST_PARTNER)
        { options[3] = sLastPartner; options[4] = sPairing; options[5] = sRefresh; options[6] = sBack; }
        else { options[3] = sPairing; options[4] = sRefresh; options[5] = sBack; }
    }
    else if (sPage == ONLINE_LOCATION)
    {
        if (known && (sStatus.flags & COOP_ONLINE_HAS_LOCATION))
        {
            mapHeader = Overworld_GetMapHeaderByGroupAndId(sStatus.location_map_group,
                                                             sStatus.location_map_number);
            if (mapHeader != NULL && mapHeader->regionMapSectionId != MAPSEC_NONE)
            {
                Print(sLastSharedLocation, 8, 30);
                GetMapNameGeneric(locationName, mapHeader->regionMapSectionId);
                Print(locationName, 8, 44);
            }
            else Print(sLocationUnavailable, 8, 30);
        }
        else Print(sLocationUnavailable, 8, 30);
        options[0] = sRefresh; options[1] = sBack;
    }
    else if (sPage == ONLINE_LAST_PARTNER)
    {
        if (known && (sStatus.flags & COOP_ONLINE_HAS_LAST_PARTNER)) PrintName(sStatus.last_partner_name, 32);
        else Print(sUnknown, 8, 32);
        options[0] = sInviteLastPartner; options[1] = sBack;
    }
    else
    {
        u8 count = sPage == ONLINE_NEARBY ? sStatus.nearby_count : (sPage == ONLINE_INCOMING ? sStatus.incoming_count : sStatus.outgoing_count);
        u8 page = sPage == ONLINE_NEARBY ? sStatus.nearby_page : (sPage == ONLINE_INCOMING ? sStatus.incoming_page : sStatus.outgoing_page);
        if (known && count != 0)
        {
            end = ConvertIntToDecimalStringN(pageText, page + 1, STR_CONV_MODE_LEFT_ALIGN, 2);
            *end++ = CHAR_SLASH;
            ConvertIntToDecimalStringN(end, count, STR_CONV_MODE_LEFT_ALIGN, 2);
            Print(pageText, 176, 0);
        }
        if (sPage == ONLINE_NEARBY)
        {
            if (!known) Print(sUnknown, 8, 30);
            else if (sStatus.nearby_count) PrintName(sStatus.nearby_name, 30);
            else Print(sNobody, 8, 30);
            options[0] = sInvite; options[1] = sNext; options[2] = sRefresh; options[3] = sBack;
        }
        else if (sPage == ONLINE_INCOMING)
        {
            if (!known) Print(sUnknown, 8, 30);
            else if (sStatus.incoming_count) PrintName(sStatus.incoming_name, 30);
            else Print(sNoInvites, 8, 30);
            options[0] = sInvitePrompt ? sYesJoin : sAccept;
            options[1] = sInvitePrompt ? sNoDecline : sDecline;
            options[2] = sNextInvite;
            options[3] = sRefresh; options[4] = sBack;
        }
        else
        {
            if (!known) Print(sUnknown, 8, 30);
            else if (sStatus.outgoing_count) PrintName(sStatus.group_name, 30);
            else Print(sNoOutgoing, 8, 30);
            options[0] = sCancel; options[1] = sNextInvite;
            options[2] = sRefresh; options[3] = sBack;
        }
    }
    for (i = 0; i < OptionCount(); i++)
    {
        static const u8 disabledColors[] = {TEXT_COLOR_WHITE, TEXT_COLOR_LIGHT_GRAY, TEXT_COLOR_WHITE};
        u8 optionY = sPage == ONLINE_HOME ? 60 + i * (OptionCount() > 7 ? 10 : 11) : 66 + i * 12;
        if (OptionEnabled(i)) Print(options[i], 16, optionY);
        else AddTextPrinterParameterized3(sWindowId, FONT_SMALL, 16, optionY, disabledColors, TEXT_SKIP_DRAW, options[i]);
    }
    Print(sCursorText, 4, sPage == ONLINE_HOME ? 60 + sCursor * (OptionCount() > 7 ? 10 : 11) : 66 + sCursor * 12);
    if (sPage != ONLINE_HOME && sPage != ONLINE_LOCATION) Print(sPaging, 8, 130);
    CopyWindowToVram(sWindowId, COPYWIN_GFX);
}

static bool8 CanAct(u8 action)
{
    if (sPending) return FALSE;
    if (action == COOP_ONLINE_REFRESH) return TRUE;
    if (sStatus.request_id == 0 || (sStatus.result != COOP_ONLINE_READY && sStatus.result != COOP_ONLINE_SUCCESS)) return FALSE;
    if (action == COOP_ONLINE_INVITE) return sStatus.nearby_count != 0 && !(sStatus.flags & COOP_ONLINE_GROUPED);
    if (action == COOP_ONLINE_ACCEPT || action == COOP_ONLINE_DECLINE) return sStatus.incoming_count != 0;
    if (action == COOP_ONLINE_CANCEL) return sStatus.outgoing_count != 0;
    if (action == COOP_ONLINE_INVITE_LAST_PARTNER) return !(sStatus.flags & COOP_ONLINE_GROUPED) && (sStatus.flags & COOP_ONLINE_HAS_LAST_PARTNER);
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

static void SendPairing(u8 action)
{
    struct CoopPairingRequest request = {0};
    u8 i;
    if (sPairingPending || sPending) return;
    if (++sRequestSerial == 0) ++sRequestSerial;
    request.request_id = sRequestSerial;
    request.action = action;
    if (action == COOP_PAIRING_REDEEM)
        for (i = 0; i < 7; i++) request.code[i] = sCode[i];
    sPairingStatusValid = FALSE;
    if (CoopNetBridge_SendPairingRequest(&request))
    {
        sPairingPending = TRUE;
        sPairingPendingId = request.request_id;
        sPairingWaitFrames = 0;
    }
    else
    {
        sPairingStatus = (struct CoopPairingStatus){ .result = COOP_PAIRING_UNAVAILABLE };
        sPairingStatusValid = TRUE;
    }
    Draw();
}

static void Poll(void)
{
    struct CoopOnlineStatus status;
    struct CoopPairingStatus pairing;
    if (sPairingPending)
    {
        if (CoopNetBridge_GetPairingStatus(&pairing) && pairing.request_id == sPairingPendingId)
        {
            sPairingPending = FALSE;
            sPairingStatusValid = TRUE;
            sPairingStatus = pairing;
            if (pairing.result == COOP_PAIRING_JOINED)
            {
                sPage = ONLINE_PAIRING;
                sCursor = 0;
                Send(COOP_ONLINE_REFRESH, 0);
            }
            Draw();
        }
        else if (++sPairingWaitFrames >= PAIRING_TIMEOUT_FRAMES)
        {
            sPairingPending = FALSE;
            sPairingStatus = (struct CoopPairingStatus){ .result = COOP_PAIRING_UNAVAILABLE };
            sPairingStatusValid = TRUE;
            Draw();
        }
    }
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
    u8 count = sPage == ONLINE_NEARBY ? sStatus.nearby_count : (sPage == ONLINE_INCOMING ? sStatus.incoming_count : sStatus.outgoing_count);
    u8 page = sPage == ONLINE_NEARBY ? sStatus.nearby_page : (sPage == ONLINE_INCOMING ? sStatus.incoming_page : sStatus.outgoing_page);
    if (count < 2) return;
    page = delta < 0 ? (page == 0 ? count - 1 : page - 1) : (page + 1) % count;
    Send(COOP_ONLINE_REFRESH, page);
}

// Back is always available; leaving a submenu does not cancel an accepted request.
static bool8 HandleInput(u16 keys)
{
    if (sPage == ONLINE_PAIRING_ENTRY)
    {
        u8 index;
        if (keys & B_BUTTON) { sPage = ONLINE_PAIRING; sCursor = 1; Draw(); return FALSE; }
        if (sPairingPending) return FALSE;
        if (keys & DPAD_LEFT) { if (sCodePosition != 0) sCodePosition--; Draw(); }
        if (keys & DPAD_RIGHT) { if (sCodePosition < 5) sCodePosition++; Draw(); }
        index = sCodePosition >= 3 ? sCodePosition + 1 : sCodePosition;
        if (keys & (DPAD_UP | DPAD_DOWN))
        {
            u8 letter = 0;
            while (letter < 32 && sPairingAlphabet[letter] != sCode[index]) letter++;
            letter = keys & DPAD_UP ? (letter + 1) % 32 : (letter + 31) % 32;
            sCode[index] = sPairingAlphabet[letter];
            Draw();
        }
        if ((keys & A_BUTTON) && sCodePosition < 5) { sCodePosition++; Draw(); }
        else if ((keys & A_BUTTON) || (keys & START_BUTTON)) SendPairing(COOP_PAIRING_REDEEM);
        return FALSE;
    }
    if ((keys & B_BUTTON) || ((keys & A_BUTTON) && sCursor == OptionCount() - 1))
    {
        if (sPage == ONLINE_HOME || sInvitePrompt) return TRUE;
        sPage = ONLINE_HOME;
        sCursor = 0;
        Draw();
        return FALSE;
    }
    if (keys & DPAD_UP) sCursor = sCursor == 0 ? OptionCount() - 1 : sCursor - 1;
    if (keys & DPAD_DOWN) sCursor = (sCursor + 1) % OptionCount();
    if (keys & (DPAD_UP | DPAD_DOWN)) Draw();
    if (sPending || sPairingPending) return FALSE;
    if (sPage != ONLINE_HOME && sPage != ONLINE_LOCATION && sPage != ONLINE_PAIRING && sPage != ONLINE_LAST_PARTNER && (keys & (DPAD_LEFT | DPAD_RIGHT))) NextPage(keys & DPAD_LEFT ? -1 : 1);
    if (!(keys & A_BUTTON)) return FALSE;
    if (sPage == ONLINE_HOME)
    {
        if (sCursor < 3) { sPage = sCursor == 0 ? ONLINE_NEARBY : (sCursor == 1 ? ONLINE_INCOMING : ONLINE_OUTGOING); sCursor = 0; Send(COOP_ONLINE_REFRESH, 0); }
        else if (sCursor == 3 && (sStatus.flags & COOP_ONLINE_GROUPED)) { sPage = ONLINE_LOCATION; sCursor = 0; Draw(); }
        else if (sCursor == 4 && (sStatus.flags & COOP_ONLINE_GROUPED)) Send(COOP_ONLINE_LEAVE, 0);
        else if (!(sStatus.flags & COOP_ONLINE_GROUPED) && (sStatus.flags & COOP_ONLINE_HAS_LAST_PARTNER) && sCursor == 3) { sPage = ONLINE_LAST_PARTNER; sCursor = 0; Draw(); }
        else if (!(sStatus.flags & COOP_ONLINE_GROUPED) && sCursor == (sStatus.flags & COOP_ONLINE_HAS_LAST_PARTNER ? 4 : 3)) { sPage = ONLINE_PAIRING; sCursor = 0; sPairingStatusValid = FALSE; Draw(); }
        else Send(COOP_ONLINE_REFRESH, 0);
    }
    else if (sPage == ONLINE_NEARBY)
    {
        if (sCursor == 0) Send(COOP_ONLINE_INVITE, sStatus.nearby_page);
        else if (sCursor == 1) NextPage(1);
        else Send(COOP_ONLINE_REFRESH, 0);
    }
    else if (sPage == ONLINE_INCOMING)
    {
        if (sCursor < 2) {
            Send(sCursor == 0 ? COOP_ONLINE_ACCEPT : COOP_ONLINE_DECLINE, sStatus.incoming_page);
            sInvitePrompt = FALSE;
        }
        else if (sCursor == 2) NextPage(1);
        else Send(COOP_ONLINE_REFRESH, 0);
    }
    else if (sPage == ONLINE_OUTGOING)
    {
        if (sCursor == 0) Send(COOP_ONLINE_CANCEL, sStatus.outgoing_page);
        else if (sCursor == 1) NextPage(1);
        else Send(COOP_ONLINE_REFRESH, 0);
    }
    else if (sPage == ONLINE_PAIRING)
    {
        if (sCursor == 0) SendPairing(COOP_PAIRING_CREATE);
        else if (sCursor == 1)
        {
            u8 i;
            for (i = 0; i < 7; i++) sCode[i] = i == 3 ? '-' : 'A';
            sCode[7] = 0;
            sCodePosition = 0;
            sPairingStatusValid = FALSE;
            sPage = ONLINE_PAIRING_ENTRY;
            Draw();
        }
    }
    else if (sPage == ONLINE_LAST_PARTNER) Send(COOP_ONLINE_INVITE_LAST_PARTNER, 0);
    else Send(COOP_ONLINE_REFRESH, 0);
    return FALSE;
}

static void Begin(bool8 invited)
{
    sStatus = (struct CoopOnlineStatus){0};
    sPending = FALSE;
    sPairingPending = FALSE;
    sPairingStatusValid = FALSE;
    sInvitePrompt = invited;
    sPage = invited ? ONLINE_INCOMING : ONLINE_HOME;
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
        sMenuOpen = FALSE;
        sInvitePrompt = FALSE;
        ScriptUnfreezeObjectEvents();
        UnlockPlayerFieldControls();
        DestroyTask(taskId);
    }
}

static void Open(bool8 invited)
{
    u8 taskId;
    // CreateTask returns zero on exhaustion, which may belong to another task.
    if (GetTaskCount() == NUM_TASKS)
    {
        ScriptUnfreezeObjectEvents();
        UnlockPlayerFieldControls();
        return;
    }
    CoopPresenceRuntime_HidePartnerName();
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
    sMenuOpen = TRUE;
    Begin(invited);
    CopyWindowToVram(sWindowId, COPYWIN_FULL);
}

void CoopOnline_Open(void)
{
    Open(FALSE);
}

bool8 CoopOnline_IsOpen(void)
{
    return sMenuOpen;
}

void CoopOnline_PollInviteNotice(void)
{
    if (sMenuOpen || gMain.callback1 != CB1_Overworld || gMain.callback2 != CB2_Overworld
     || gPaletteFade.active || ArePlayerFieldControlsLocked() || ScriptContext_IsEnabled()
     || GetTaskCount() == NUM_TASKS || !CoopNetBridge_TakeInviteNotice())
        return;
    FreezeObjectEvents();
    PlayerFreeze();
    StopPlayerAvatar();
    LockPlayerFieldControls();
    Open(TRUE);
}

#if TESTING
const struct WindowTemplate *CoopOnline_TestWindowTemplate(void) { return &sOnlineWindow; }
void CoopOnline_TestBegin(void) { sWindowId = WINDOW_NONE; Begin(FALSE); }
void CoopOnline_TestBeginInvite(void) { sWindowId = WINDOW_NONE; Begin(TRUE); }
bool8 CoopOnline_TestInput(u16 keys) { return HandleInput(keys); }
void CoopOnline_TestPoll(void) { Poll(); }
bool8 CoopOnline_TestPending(void) { return sPending; }
u8 CoopOnline_TestResult(void) { return sStatus.result; }
bool8 CoopOnline_TestIsLocationPage(void) { return sPage == ONLINE_LOCATION; }
bool8 CoopOnline_TestIsLastPartnerPage(void) { return sPage == ONLINE_LAST_PARTNER; }
bool8 CoopOnline_TestIsPairingPage(void) { return sPage == ONLINE_PAIRING; }
const u8 *CoopOnline_TestResultText(void) { return ResultText(); }
// Mirrors Draw's ONLINE_LAST_PARTNER body: the name printed, or NULL for "Status not available."
const u8 *CoopOnline_TestLastPartnerName(void)
{
    bool8 known = sStatus.request_id != 0 && (sStatus.result == COOP_ONLINE_READY || sStatus.result == COOP_ONLINE_SUCCESS);
    return sPage == ONLINE_LAST_PARTNER && known && (sStatus.flags & COOP_ONLINE_HAS_LAST_PARTNER) ? sStatus.last_partner_name : NULL;
}
#endif
