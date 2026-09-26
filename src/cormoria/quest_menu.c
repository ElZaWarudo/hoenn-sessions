#include "global.h"
#include "cormoria/quest_menu.h"
#include "cormoria/quest_state.h"
#include "bg.h"
#include "event_data.h"
#include "field_screen_effect.h"
#include "gpu_regs.h"
#include "main.h"
#include "menu.h"
#include "menu_helpers.h"
#include "overworld.h"
#include "palette.h"
#include "script.h"
#include "sound.h"
#include "sprite.h"
#include "strings.h"
#include "string_util.h"
#include "task.h"
#include "text_window.h"
#include "window.h"
#include "constants/rgb.h"
#include "constants/songs.h"
#include "constants/characters.h"

/*
 * This is a data-preserving port of Dreamstone Mysteries' quest journal.
 * The donor implementation stored quest bits in saveblock2.  Cormoria keeps
 * the same five meanings and script-facing ordering, but reads and writes
 * them through CormoriaQuestState so the journal follows the active world
 * during a ROM handoff.
 *
 * Donor: https://github.com/dsmyst/dreamstone-mysteries
 * Revision: f7997186345885bfa23a170e5f573851fc034b9b
 * src/quests.c SHA-256: e900f39cbb96d7f60db819e9b2eb643d401f8fb9e3b851dbf91a9ed35e6d88fa
 * include/quests.h SHA-256: 7c0ab6da59e5597a2b524101cc202fc468bf6b2a06cb83eafd182285fe8236f0
 * src/strings.c SHA-256: 34a0883a890e255cb57f0ce33e8315ba9f2e1df54c2f2fc6a22434debdf21578
 */

#define CORMORIA_QUEST_VISIBLE_ROWS 4
#define CORMORIA_QUEST_FILTER_COUNT 5
#define CORMORIA_QUEST_FILTER_ALL 0
#define CORMORIA_QUEST_FILTER_INACTIVE 1
#define CORMORIA_QUEST_FILTER_ACTIVE 2
#define CORMORIA_QUEST_FILTER_REWARD 3
#define CORMORIA_QUEST_FILTER_COMPLETED 4

struct CormoriaQuestEntry
{
    const u8 *name;
    const u8 *description;
    const u8 *completedDescription;
    const u8 *location;
    u8 firstSubquest;
    u8 subquestCount;
};

struct CormoriaSubquestEntry
{
    const u8 *name;
    const u8 *description;
    const u8 *location;
};

static const u8 sTextUnknown[] = _("??????");
static const u8 sTextNoQuests[] = _("No quests discovered yet.");
static const u8 sTextFilterAll[] = _("All");
static const u8 sTextFilterInactive[] = _("Inactive");
static const u8 sTextFilterActive[] = _("Active");
static const u8 sTextFilterReward[] = _("Reward");
static const u8 sTextFilterCompleted[] = _("Done");
static const u8 *const sFilterNames[CORMORIA_QUEST_FILTER_COUNT] =
{
    sTextFilterAll,
    sTextFilterInactive,
    sTextFilterActive,
    sTextFilterReward,
    sTextFilterCompleted,
};
static const u8 sTextStatusInactive[] = _("Inactive");
static const u8 sTextStatusActive[] = _("Active");
static const u8 sTextStatusReward[] = _("Reward");
static const u8 sTextStatusCompleted[] = _("Done");
static const u8 sTextFavorite[] = _("F: ");
static const u8 sTextLocation[] = _("Location: ");
static const u8 sTextBack[] = _("B: back");
static const u8 sTextReturnForReward[] = _("Return to this location to receive your reward!");
static const u8 sTextStartForDetails[] = _("Start this quest for more details.");
static const u8 sTextHiddenSubquest[] = _("Complete this step to reveal its details.");
static const u8 sTextStateUnavailable[] = _("Quest data unavailable.");

// Journal table fields are pointers; _() is only valid for array initializers.
// The text preprocessor emits a charmap-encoded compound literal here.

static const struct CormoriaSubquestEntry sSubquests[CORMORIA_SUBQUEST_COUNT] =
{
    {COMPOUND_STRING("My First Day"),
     COMPOUND_STRING("It's my first day! I need to go to the Tenebris Lab with my Welcome Package. I shouldn't be late at any cost!"),
     COMPOUND_STRING("Tenebris Laboratory")},
    {COMPOUND_STRING("Lab Supplies"),
     COMPOUND_STRING("I need to pick up the lab's supplies from the Poké Mart in Fennilahl Town. It has my starter Pokémon! And some important stuff."),
     COMPOUND_STRING("Fennilahl Town")},
    {COMPOUND_STRING("Missing Supplies"),
     COMPOUND_STRING("Professor Tenebris has already taken the supplies! I need to tell Asst. Prof. Rue as soon as possible."),
     COMPOUND_STRING("Tenebris Laboratory")},
    {COMPOUND_STRING("The First Dreamstone"),
     COMPOUND_STRING("The first dreamstone is atop Mt. Ceram. I need to cross Route 3 to Gastree City, then head north towards the Ceram Base Camp."),
     COMPOUND_STRING("Mt. Ceram")},
    {COMPOUND_STRING("Mysterious Area"),
     COMPOUND_STRING("The dreamstone transported me and Gabrielle to a mysterious area! I don't recognise the Pokémon here. I need to find a way back."),
     COMPOUND_STRING("Mysterious Area")},
    {COMPOUND_STRING("Silversun Sighting"),
     COMPOUND_STRING("Someone matching Prof. Tenebris' description was spotted in Silversun City! Team Somber is there too, so I need to go there fast."),
     COMPOUND_STRING("Silversun City")},
    {COMPOUND_STRING("Of Drama & Desire"),
     COMPOUND_STRING("Team Somber's hideout is somewhere here. If they've taken Prof Tenebris hostage, it could spell trouble. Gotta find them!"),
     COMPOUND_STRING("Silversun City")},
    {COMPOUND_STRING("Knowledge of a Past Era"),
     COMPOUND_STRING("I need to talk to Martha the historian, who lives at the Mirroh Base Camp. Can she give us a clue as to Team Somber's motives?"),
     COMPOUND_STRING("Mirroh Base Camp")},
    {COMPOUND_STRING("Showdown at Mt. Mirroh!"),
     COMPOUND_STRING("What could Team Somber possibly want with the Ancient Terror? I'll find my answers at Mt. Mirroh...if I hurry!"),
     COMPOUND_STRING("Mt. Mirroh")},
    {COMPOUND_STRING("Stop Melea!"),
     COMPOUND_STRING("I got warped into the past again! And Somber Admin Melea is here too. I've got to stop her from catching the Ancient Terror!"),
     COMPOUND_STRING("Ancient Mirroh")},
    {COMPOUND_STRING("No Way Out"),
     COMPOUND_STRING("Somber Admin Melea escaped with the Ancient Terror! But Kohla found his way here too, so there should be another exit out."),
     COMPOUND_STRING("Ancient Mirroh")},
    {COMPOUND_STRING("Reach Rivetshore City"),
     COMPOUND_STRING("A massive heatwave swept across Cormoria! It's Team Somber's doing. I've got to reach Rivetshore City to chase them!"),
     COMPOUND_STRING("Rivetshore City")},
    {COMPOUND_STRING("Board the S.S. Elegant"),
     COMPOUND_STRING("The Gym Leaders are going to board the S.S. Elegant and track down Team Somber. I've got to get on board too!"),
     COMPOUND_STRING("Rivetshore City")},
    {COMPOUND_STRING("Get Off the Ship!"),
     COMPOUND_STRING("We've stopped at an uncharted island, but civilians can't get off. Gabrielle and Breech are waiting at the storage hold for me!"),
     COMPOUND_STRING("S.S. Elegant")},
    {COMPOUND_STRING("Explore the Island"),
     COMPOUND_STRING("Gabrielle and Breech got me off the ship. Now I need to find Team Somber and stop them! But I can't let the leaders catch me."),
     COMPOUND_STRING("Uncharted Island")},
    {COMPOUND_STRING("A Ranger's First Assignment"),
     COMPOUND_STRING("Ranger Chief Ravine has asked me to deliver an important package to the Ranger Institute at Ivy River. I can't let him down!"),
     COMPOUND_STRING("Ranger Institute")},
    {COMPOUND_STRING("Fieldwork: Mega Evolution"),
     COMPOUND_STRING("Scientists at the Ivy River Ranger Institute need specimens of different Pokémon to study their potential for Mega Evolution."),
     COMPOUND_STRING("Ranger Institute")},
    {COMPOUND_STRING("The Final Test"),
     COMPOUND_STRING("This is the final test I need to complete to become a fully-fledged Pokémon Ranger. Can I track the mythical Pokémon?"),
     COMPOUND_STRING("Ranger Institute")},
    {COMPOUND_STRING("Help the Mayor!"),
     COMPOUND_STRING("The mayor of Pelluca City has asked me to deal with the Qwilsquad! I need to find a way into their hideout by the riverbank."),
     COMPOUND_STRING("Pelluca City")},
    {COMPOUND_STRING("Save the Citizens!"),
     COMPOUND_STRING("The city is flooded and some citizens are drowning! Leader Jania gave me the HM Surf. I need to save the drowning citizens!"),
     COMPOUND_STRING("Pelluca City")},
};

static const struct CormoriaQuestEntry sQuests[CORMORIA_QUEST_COUNT] =
{
    {COMPOUND_STRING("Lab Assistant"),
     COMPOUND_STRING("From today, I'm going to be a Lab Assistant at the Tenebris Laboratory!"),
     COMPOUND_STRING("I've done what I can back at the lab. Now it's time to head out and chase the dreamstones around Cormoria."),
     COMPOUND_STRING("Tenebris Laboratory"), 0, 3},
    {COMPOUND_STRING("Find the Dreamstone!"),
     COMPOUND_STRING("I need to solve the mystery of the dreamstones around Cormoria."),
     COMPOUND_STRING("A total failure! I couldn't stop Team Somber or find Professor Tenebris, and now I'm off the case. I guess that's it...?"),
     COMPOUND_STRING("Mt. Ceram"), 3, 8},
    {COMPOUND_STRING("Dreamstone Mysteries"),
     COMPOUND_STRING("It doesn't matter if I'm off the case. I'll stop Team Somber, find Prof. Tenebris and solve the mystery of the dreamstones!"),
     COMPOUND_STRING("I've solved the mystery of the dreamstones! Tenebris is back and Team Somber is done. All's well that ends well!"),
     COMPOUND_STRING("Cormoria"), 11, 4},
    {COMPOUND_STRING("Food Poisoning"),
     COMPOUND_STRING("The Azurill in the house on Route 1 has food poisoning. She needs a Pecha Berry!"),
     COMPOUND_STRING("Azurill has recovered and is happy again! A Pecha Berry a day keeps the doctor away."),
     COMPOUND_STRING("Route 1"), 0, 0},
    {COMPOUND_STRING("A Hiker's Treasure"),
     COMPOUND_STRING("A conflict-averse hiker in Fennilahl Town has lost a Strange Rock in Route 2. Apparently a small pink Pokémon stole it!"),
     COMPOUND_STRING("I got Breech his stone back...and found him a new companion! I hope to see him and Clefairy again soon."),
     COMPOUND_STRING("Fennilahl Town"), 0, 0},
    {COMPOUND_STRING("A Lost Skitty"),
     COMPOUND_STRING("Someone's Skitty in Gastree City has gone missing! It probably climbed up a tree or something..."),
     COMPOUND_STRING("Skitty is reunited with her trainer!"),
     COMPOUND_STRING("Gastree City"), 0, 0},
    {COMPOUND_STRING("Historical Preservation"),
     COMPOUND_STRING("An archaeologist in Gastree City has asked me to find all ten ancient statues across Cormoria and scan them with the Archaeolens!"),
     COMPOUND_STRING("I found all ten statues!"),
     COMPOUND_STRING("Gastree City"), 0, 0},
    {COMPOUND_STRING("Modern Matcha"),
     COMPOUND_STRING("The lady at the Gastree Teahouse wants to craft a new tea blend. She needs 1 Revival Herb, 1 Energy Powder and 1 Shoal Salt."),
     COMPOUND_STRING("The modern blend is done! But will it do well on the menu? Or will it be a flop? Silly question! It's a hit of course!"),
     COMPOUND_STRING("Gastree City"), 0, 0},
    {COMPOUND_STRING("Cyndaquil's New Move"),
     COMPOUND_STRING("A trainer in Ceram Base Camp wants his Cyndaquil to learn...Acid Spray? Where can I get the TM for Acid Spray?"),
     COMPOUND_STRING("I taught the Cyndaquil the move Acid Spray! I hope the trainer learns more about battling and they become strong together!"),
     COMPOUND_STRING("Ceram Base Camp"), 0, 0},
    {COMPOUND_STRING("Precious Pearls"),
     COMPOUND_STRING("A rich lady in Galecrest City has had her pearls stolen! I'd better find the robber. Maybe I'll get a huge reward..."),
     COMPOUND_STRING("I found the robber and returned the pearls! But the horrid lady charged me money for being late! Is that why she's rich?"),
     COMPOUND_STRING("Galecrest City"), 0, 0},
    {COMPOUND_STRING("Love Is Sacrifice"),
     COMPOUND_STRING("A down-on-his-luck man in Galecrest City wants to do something special for his wife. He wants to gift her a Blue Flute!"),
     COMPOUND_STRING("With the Blue Flute (and Jigglypuff), the house is singing! Hard times come and go, but love and music remain!"),
     COMPOUND_STRING("Galecrest City"), 0, 0},
    {COMPOUND_STRING("Malevolent Masterpiece"),
     COMPOUND_STRING("A (self-proclaimed) famous artist in Silversun City wants Black Sludge to create the perfect shade of black paint."),
     COMPOUND_STRING("The Black Sludge created the perfect shade of black paint! Maybe this artist is really a maestro after all."),
     COMPOUND_STRING("Silversun City"), 0, 0},
    {COMPOUND_STRING("I Can't Find My Wife!"),
     COMPOUND_STRING("A man in the Silversun Sewers has gotten separated from his wife. If I don't hurry, she might get attacked by the Sewer Scourge!"),
     COMPOUND_STRING("Husband and wife have been successfully reunited. They also got a Furfrou to help. All's well that ends well!"),
     COMPOUND_STRING("Silversun Sewers"), 0, 0},
    {COMPOUND_STRING("Career Crisis"),
     COMPOUND_STRING("A fisherman on Route 6 wants to become a chef! He wants to sample some of Pelluca's famous Apple Pie and try to recreate it!"),
     COMPOUND_STRING("The fisherman enjoyed the Apple Pie! Thank you for the Trolling Rod and all the best!"),
     COMPOUND_STRING("Route 6"), 0, 0},
    {COMPOUND_STRING("Pokémon Ranger Badge"),
     COMPOUND_STRING("My path towards becoming a fully-fledged Pokémon Ranger!"),
     COMPOUND_STRING("I cleared the interview, delivered the package, helped the scientists and now I'm a fully-fledged Pokémon Ranger!"),
     COMPOUND_STRING("Ranger Institute"), 15, 3},
    {COMPOUND_STRING("Pelluca's Leadership Tussle"),
     COMPOUND_STRING("Pelluca City is in trouble! The mayor and the Qwilsquad Gang leader are tussling for power and the city is suffering!"),
     COMPOUND_STRING("The mayor and the Qwilsquad boss have agreed to cooperate and develop the city they both love. All the best!"),
     COMPOUND_STRING("Pelluca City"), 18, 2},
    {COMPOUND_STRING("A Chef's Icy Troubles"),
     COMPOUND_STRING("The refrigerators at the Pelluca Restaurant are broken and their ingredients are going bad. I need to bring them a Nevermelt Ice."),
     COMPOUND_STRING("The Nevermelt Ice can keep the ingredients cool...until they get a Rotom Fridge."),
     COMPOUND_STRING("Pelluca Restaurant"), 0, 0},
    {COMPOUND_STRING("The Healers Need Help!"),
     COMPOUND_STRING("A Chansey is lost inside Mt Mirroh! They've always healed me when I needed it, and now it's my turn to repay them!"),
     COMPOUND_STRING("Chansey is reunited with the nurse and they're off to Winterlily Hollow! I hope their situation improves soon..."),
     COMPOUND_STRING("Mt. Mirroh"), 0, 0},
    {COMPOUND_STRING("Percy's Gone Missing!"),
     COMPOUND_STRING("The Rivetshore Construction CEO's beloved Percy has gone missing! I need to find it. Did it fall for a prank, perhaps?"),
     COMPOUND_STRING("Percy and the CEO are reunited! I thought I'd get money, but this rare flute is even cooler!"),
     COMPOUND_STRING("Rivetshore City"), 0, 0},
    {COMPOUND_STRING("Mean Old Grandma"),
     COMPOUND_STRING("Two brothers in Rivetshore City want to game but their grandma won't buy them a console! Just like my younger days."),
     COMPOUND_STRING("The brothers love their new PocketBoy! I hope they don't get addicted to it... Maybe I should play just one round."),
     COMPOUND_STRING("Rivetshore City"), 0, 0},
};

static const struct BgTemplate sBgTemplates[] =
{
    {
        .bg = 0,
        .charBaseIndex = 0,
        .mapBaseIndex = 31,
        .screenSize = 0,
        .paletteMode = 0,
        .priority = 0,
        .baseTile = 0,
    },
};

static const struct WindowTemplate sWindowTemplates[] =
{
    {
        .bg = 0,
        .tilemapLeft = 1,
        .tilemapTop = 1,
        .width = 28,
        .height = 2,
        .paletteNum = 15,
        .baseBlock = 8,
    },
    {
        .bg = 0,
        .tilemapLeft = 1,
        .tilemapTop = 4,
        .width = 28,
        .height = 7,
        .paletteNum = 15,
        .baseBlock = 64,
    },
    {
        .bg = 0,
        .tilemapLeft = 1,
        .tilemapTop = 12,
        .width = 28,
        .height = 7,
        .paletteNum = 15,
        .baseBlock = 260,
    },
    DUMMY_WIN_TEMPLATE,
};

static MainCallback sReturnCallback;
static u8 sWindowIds[3];
static u8 sQuestIds[CORMORIA_QUEST_COUNT];
static u8 sQuestCount;
static u8 sCursor;
static u8 sScroll;
static u8 sFilter;
static bool8 sAlphabetical;
static bool8 sSubquestMode;
static u8 sParentQuest;
static u8 sSubquestCount;
static u8 sSubquestScroll;
static bool8 sClosing;
static bool8 sStateUnavailable;

static void Task_CormoriaQuestMenu(u8 taskId);

static bool8 ReadQuestBit(u8 questId, enum CormoriaQuestBit bit)
{
    bool8 value = FALSE;

    if (questId >= CORMORIA_QUEST_COUNT)
        return FALSE;
    if (!CormoriaQuestState_Get(questId, bit, &value))
        sStateUnavailable = TRUE;
    return value;
}

static void UpdateScroll(u8 count, u8 *scroll, u8 cursor)
{
    u8 maxScroll;

    if (count == 0)
    {
        *scroll = 0;
        return;
    }

    maxScroll = count > CORMORIA_QUEST_VISIBLE_ROWS
        ? count - CORMORIA_QUEST_VISIBLE_ROWS
        : 0;
    if (cursor < *scroll)
        *scroll = cursor;
    else if (cursor >= *scroll + CORMORIA_QUEST_VISIBLE_ROWS)
        *scroll = cursor - CORMORIA_QUEST_VISIBLE_ROWS + 1;
    if (*scroll > maxScroll)
        *scroll = maxScroll;
}

static bool8 IsQuestInactive(u8 questId)
{
    return !ReadQuestBit(questId, CORMORIA_QUEST_ACTIVE)
        && !ReadQuestBit(questId, CORMORIA_QUEST_REWARD)
        && !ReadQuestBit(questId, CORMORIA_QUEST_COMPLETED);
}

static bool8 MatchesFilter(u8 questId)
{
    if (!ReadQuestBit(questId, CORMORIA_QUEST_UNLOCKED))
        return FALSE;
    switch (sFilter)
    {
    case CORMORIA_QUEST_FILTER_INACTIVE:
        return IsQuestInactive(questId);
    case CORMORIA_QUEST_FILTER_ACTIVE:
        return ReadQuestBit(questId, CORMORIA_QUEST_ACTIVE);
    case CORMORIA_QUEST_FILTER_REWARD:
        return ReadQuestBit(questId, CORMORIA_QUEST_REWARD);
    case CORMORIA_QUEST_FILTER_COMPLETED:
        return ReadQuestBit(questId, CORMORIA_QUEST_COMPLETED);
    default:
        return TRUE;
    }
}

static void BuildQuestList(void)
{
    u8 i;
    u8 count = 0;

    for (i = 0; i < CORMORIA_QUEST_COUNT; i++)
    {
        if (MatchesFilter(i))
            sQuestIds[count++] = i;
    }

    if (sAlphabetical)
    {
        for (i = 0; i < count; i++)
        {
            u8 j;
            for (j = i + 1; j < count; j++)
            {
                if (StringCompare(sQuests[sQuestIds[i]].name, sQuests[sQuestIds[j]].name) > 0)
                {
                    u8 swap = sQuestIds[i];
                    sQuestIds[i] = sQuestIds[j];
                    sQuestIds[j] = swap;
                }
            }
        }
    }

    /* Dreamstone puts favorites at the top while retaining the chosen order. */
    for (i = 0; i < count; i++)
    {
        u8 j;
        if (!ReadQuestBit(sQuestIds[i], CORMORIA_QUEST_FAVORITE))
            continue;
        for (j = i; j > 0 && !ReadQuestBit(sQuestIds[j - 1], CORMORIA_QUEST_FAVORITE); j--)
        {
            u8 swap = sQuestIds[j];
            sQuestIds[j] = sQuestIds[j - 1];
            sQuestIds[j - 1] = swap;
        }
    }
    sQuestCount = count;
    if (sCursor >= sQuestCount)
        sCursor = sQuestCount == 0 ? 0 : sQuestCount - 1;
    UpdateScroll(sQuestCount, &sScroll, sCursor);
}

static const u8 *QuestStatus(u8 questId)
{
    if (ReadQuestBit(questId, CORMORIA_QUEST_COMPLETED))
        return sTextStatusCompleted;
    if (ReadQuestBit(questId, CORMORIA_QUEST_REWARD))
        return sTextStatusReward;
    if (ReadQuestBit(questId, CORMORIA_QUEST_ACTIVE))
        return sTextStatusActive;
    return sTextStatusInactive;
}

static void PrintText(u8 windowId, u8 fontId, const u8 *text, u8 x, u8 y)
{
    AddTextPrinterParameterized(windowId, fontId, text, x, y, TEXT_SKIP_DRAW, NULL);
}

static void WrapJournalText(u8 *text, u8 maxCharacters)
{
    u16 lineStart = 0;
    u16 lastSpace = 0;
    u16 i;

    for (i = 0; text[i] != EOS; i++)
    {
        if (text[i] == CHAR_NEWLINE)
        {
            lineStart = i + 1;
            lastSpace = 0;
        }
        else if (text[i] == CHAR_SPACE)
        {
            lastSpace = i;
        }
        else if (i - lineStart >= maxCharacters)
        {
            if (lastSpace > lineStart)
            {
                text[lastSpace] = CHAR_NEWLINE;
                lineStart = lastSpace + 1;
            }
            else
            {
                text[i] = CHAR_NEWLINE;
                lineStart = i + 1;
            }
            lastSpace = 0;
        }
    }
}

static void DrawHeader(void)
{
    u8 buffer[64];

    FillWindowPixelBuffer(sWindowIds[0], PIXEL_FILL(1));
    DrawStdWindowFrame(sWindowIds[0], FALSE);
    StringCopy(buffer, COMPOUND_STRING("Cormoria Quests "));
    ConvertIntToDecimalStringN(gStringVar1, sQuestCount, STR_CONV_MODE_LEFT_ALIGN, 2);
    StringAppend(buffer, gStringVar1);
    StringAppend(buffer, COMPOUND_STRING("/"));
    ConvertIntToDecimalStringN(gStringVar1, CORMORIA_QUEST_COUNT, STR_CONV_MODE_LEFT_ALIGN, 2);
    StringAppend(buffer, gStringVar1);
    PrintText(sWindowIds[0], FONT_NORMAL, buffer, 4, 0);
    StringCopy(buffer, sFilterNames[sFilter]);
    if (sAlphabetical)
        StringAppend(buffer, COMPOUND_STRING(" A-Z"));
    PrintText(sWindowIds[0], FONT_NORMAL, buffer, 160, 0);
    CopyWindowToVram(sWindowIds[0], COPYWIN_FULL);
}

static void DrawList(void)
{
    u8 i;
    u8 rowText[64];

    FillWindowPixelBuffer(sWindowIds[1], PIXEL_FILL(1));
    DrawStdWindowFrame(sWindowIds[1], FALSE);
    if (sStateUnavailable)
    {
        PrintText(sWindowIds[1], FONT_NORMAL, sTextStateUnavailable, 4, 24);
    }
    else if (sSubquestMode)
    {
        PrintText(sWindowIds[1], FONT_SMALL_NARROW, sQuests[sParentQuest].name, 4, 2);
        for (i = 0; i < CORMORIA_QUEST_VISIBLE_ROWS; i++)
        {
            u8 index = sSubquestScroll + i;
            u8 globalId;
            bool8 completed = FALSE;
            if (index >= sSubquestCount)
                break;
            globalId = sQuests[sParentQuest].firstSubquest + index;
            StringCopy(rowText, index == sCursor ? COMPOUND_STRING("> ") : COMPOUND_STRING("  "));
            if (!CormoriaQuestState_GetSubquest(globalId, &completed))
            {
                sStateUnavailable = TRUE;
                break;
            }
            if (!completed)
                StringAppend(rowText, sTextUnknown);
            else
                StringAppend(rowText, sSubquests[globalId].name);
            PrintText(sWindowIds[1], FONT_SMALL_NARROW, rowText, 4, 14 + i * 10);
        }
    }
    else if (sQuestCount == 0)
    {
        PrintText(sWindowIds[1], FONT_NORMAL, sTextNoQuests, 4, 24);
    }
    else
    {
        for (i = 0; i < CORMORIA_QUEST_VISIBLE_ROWS; i++)
        {
            u8 index = sScroll + i;
            if (index >= sQuestCount)
                break;
            StringCopy(rowText, index == sCursor ? COMPOUND_STRING("> ") : COMPOUND_STRING("  "));
            if (ReadQuestBit(sQuestIds[index], CORMORIA_QUEST_FAVORITE))
                StringAppend(rowText, sTextFavorite);
            StringAppend(rowText, sQuests[sQuestIds[index]].name);
            PrintText(sWindowIds[1], FONT_SMALL_NARROW, rowText, 4, 2 + i * 12);
            PrintText(sWindowIds[1], FONT_SMALL_NARROW, QuestStatus(sQuestIds[index]), 176, 2 + i * 12);
        }
    }
    CopyWindowToVram(sWindowIds[1], COPYWIN_FULL);
}

static void DrawDetails(void)
{
    u8 buffer[256];
    const u8 *description;
    const u8 *location;

    FillWindowPixelBuffer(sWindowIds[2], PIXEL_FILL(1));
    DrawStdWindowFrame(sWindowIds[2], FALSE);
    if (sStateUnavailable)
    {
        PrintText(sWindowIds[2], FONT_NORMAL, sTextStateUnavailable, 4, 2);
    }
    else if (sSubquestMode)
    {
        u8 completed = FALSE;
        u8 globalId = sQuests[sParentQuest].firstSubquest + sCursor;
        if (!CormoriaQuestState_GetSubquest(globalId, &completed))
        {
            sStateUnavailable = TRUE;
            PrintText(sWindowIds[2], FONT_NORMAL, sTextStateUnavailable, 4, 2);
        }
        else if (completed)
        {
            description = sSubquests[globalId].description;
            location = sSubquests[globalId].location;
            StringCopy(buffer, sTextLocation);
            StringAppend(buffer, location);
            StringAppend(buffer, COMPOUND_STRING("\n"));
            StringAppend(buffer, description);
            WrapJournalText(buffer, 39);
            PrintText(sWindowIds[2], FONT_SMALL_NARROW, buffer, 4, 2);
        }
        else
        {
            StringCopy(buffer, sTextHiddenSubquest);
            WrapJournalText(buffer, 39);
            PrintText(sWindowIds[2], FONT_SMALL_NARROW, buffer, 4, 2);
        }
    }
    else if (sQuestCount > 0)
    {
        u8 questId = sQuestIds[sCursor];
        if (ReadQuestBit(questId, CORMORIA_QUEST_COMPLETED))
            description = sQuests[questId].completedDescription;
        else if (ReadQuestBit(questId, CORMORIA_QUEST_REWARD))
            description = sTextReturnForReward;
        else if (ReadQuestBit(questId, CORMORIA_QUEST_ACTIVE))
            description = sQuests[questId].description;
        else
            description = sTextStartForDetails;
        location = sQuests[questId].location;
        StringCopy(buffer, sTextLocation);
        StringAppend(buffer, location);
        StringAppend(buffer, COMPOUND_STRING("\n"));
        StringAppend(buffer, description);
        WrapJournalText(buffer, 39);
        PrintText(sWindowIds[2], FONT_SMALL_NARROW, buffer, 4, 2);
        if (ReadQuestBit(questId, CORMORIA_QUEST_FAVORITE))
            PrintText(sWindowIds[2], FONT_SMALL_NARROW, COMPOUND_STRING("Favorite"), 164, 44);
    }
    if (!sStateUnavailable)
        PrintText(sWindowIds[2], FONT_SMALL_NARROW, sTextBack, 4, 44);
    CopyWindowToVram(sWindowIds[2], COPYWIN_FULL);
}

static void DrawJournal(void)
{
    DrawHeader();
    DrawList();
    DrawDetails();
}

static void RemoveJournalWindows(void)
{
    u8 i;

    for (i = 0; i < ARRAY_COUNT(sWindowIds); i++)
    {
        if (sWindowIds[i] != 0xFF)
        {
            ClearStdWindowAndFrameToTransparent(sWindowIds[i], FALSE);
            RemoveWindow(sWindowIds[i]);
            sWindowIds[i] = 0xFF;
        }
    }
    FreeAllWindowBuffers();
}

static void VBlankCB_CormoriaQuestMenu(void)
{
    LoadOam();
    ProcessSpriteCopyRequests();
    TransferPlttBuffer();
}

static void MainCB_CormoriaQuestMenu(void)
{
    RunTasks();
    AnimateSprites();
    BuildOamBuffer();
    DoScheduledBgTilemapCopiesToVram();
    UpdatePaletteFade();
}

static void BeginClose(u8 taskId)
{
    if (!sClosing)
    {
        sClosing = TRUE;
        BeginNormalPaletteFade(0xFFFFFFFF, 0, 0, 16, RGB_BLACK);
        gTasks[taskId].func = Task_CormoriaQuestMenu;
    }
}

static void Task_CormoriaQuestMenu(u8 taskId)
{
    if (sClosing)
    {
        if (!gPaletteFade.active)
        {
            RemoveJournalWindows();
            DestroyTask(taskId);
            SetMainCallback2(sReturnCallback);
        }
        return;
    }

    if (sStateUnavailable)
    {
        if (JOY_NEW(B_BUTTON))
            BeginClose(taskId);
        return;
    }

    if (JOY_NEW(DPAD_UP))
    {
        if (sSubquestMode)
        {
            if (sSubquestCount > 0)
                sCursor = sCursor == 0 ? sSubquestCount - 1 : sCursor - 1;
            UpdateScroll(sSubquestCount, &sSubquestScroll, sCursor);
        }
        else if (sQuestCount > 0)
        {
            sCursor = sCursor == 0 ? sQuestCount - 1 : sCursor - 1;
            UpdateScroll(sQuestCount, &sScroll, sCursor);
        }
        PlaySE(SE_RG_BAG_CURSOR);
        DrawJournal();
    }
    else if (JOY_NEW(DPAD_DOWN))
    {
        if (sSubquestMode)
        {
            if (sSubquestCount > 0)
                sCursor = (sCursor + 1) % sSubquestCount;
            UpdateScroll(sSubquestCount, &sSubquestScroll, sCursor);
        }
        else if (sQuestCount > 0)
        {
            sCursor = (sCursor + 1) % sQuestCount;
            UpdateScroll(sQuestCount, &sScroll, sCursor);
        }
        PlaySE(SE_RG_BAG_CURSOR);
        DrawJournal();
    }
    else if (JOY_NEW(A_BUTTON))
    {
        if (!sSubquestMode && sQuestCount > 0)
        {
            u8 questId = sQuestIds[sCursor];
            if (sQuests[questId].subquestCount > 0 && !IsQuestInactive(questId))
            {
                sParentQuest = questId;
                sSubquestCount = sQuests[questId].subquestCount;
                sCursor = 0;
                sSubquestScroll = 0;
                sSubquestMode = TRUE;
                DrawJournal();
            }
        }
    }
    else if (JOY_NEW(B_BUTTON))
    {
        if (sSubquestMode)
        {
            sSubquestMode = FALSE;
            sCursor = 0;
            sScroll = 0;
            sSubquestScroll = 0;
            BuildQuestList();
            DrawJournal();
        }
        else
        {
            BeginClose(taskId);
        }
    }
    else if (!sSubquestMode && JOY_NEW(R_BUTTON))
    {
        sFilter = (sFilter + 1) % CORMORIA_QUEST_FILTER_COUNT;
        sCursor = 0;
        sScroll = 0;
        BuildQuestList();
        DrawJournal();
    }
    else if (!sSubquestMode && JOY_NEW(START_BUTTON))
    {
        sAlphabetical = !sAlphabetical;
        sCursor = 0;
        sScroll = 0;
        BuildQuestList();
        DrawJournal();
    }
    else if (!sSubquestMode && JOY_NEW(SELECT_BUTTON) && sQuestCount > 0)
    {
        u8 questId = sQuestIds[sCursor];
        bool8 favorite = ReadQuestBit(questId, CORMORIA_QUEST_FAVORITE);
        if (!CormoriaQuestState_Set(questId, CORMORIA_QUEST_FAVORITE, !favorite))
            sStateUnavailable = TRUE;
        BuildQuestList();
        DrawJournal();
    }
}

void CormoriaQuestMenu_Init(MainCallback callback)
{
    sReturnCallback = callback;
    CleanupOverworldWindowsAndTilemaps();
    SetMainCallback2(CB2_InitCormoriaQuestMenu);
}

void CB2_InitCormoriaQuestMenu(void)
{
    u8 i;

    for (i = 0; i < ARRAY_COUNT(sWindowIds); i++)
        sWindowIds[i] = 0xFF;
    sQuestCount = 0;
    sCursor = 0;
    sScroll = 0;
    sFilter = CORMORIA_QUEST_FILTER_ALL;
    sAlphabetical = FALSE;
    sSubquestMode = FALSE;
    sParentQuest = 0;
    sSubquestCount = 0;
    sSubquestScroll = 0;
    sClosing = FALSE;
    sStateUnavailable = FALSE;
    {
        bool8 value;
        if (!CormoriaQuestState_Get(0, CORMORIA_QUEST_UNLOCKED, &value))
            sStateUnavailable = TRUE;
    }
    SetVBlankCallback(NULL);
    ResetTasks();
    ResetSpriteData();
    FreeAllSpritePalettes();
    ResetVramOamAndBgCntRegs();
    ResetBgsAndClearDma3BusyFlags(0);
    InitBgsFromTemplates(0, sBgTemplates, ARRAY_COUNT(sBgTemplates));
    ResetAllBgsCoordinates();
    SetGpuReg(REG_OFFSET_DISPCNT, DISPCNT_OBJ_1D_MAP | DISPCNT_OBJ_ON);
    SetGpuReg(REG_OFFSET_BLDCNT, 0);
    ShowBg(0);
    DeactivateAllTextPrinters();
    LoadMessageBoxAndBorderGfx();
    InitWindows(sWindowTemplates);
    for (i = 0; i < ARRAY_COUNT(sWindowIds); i++)
    {
        sWindowIds[i] = i;
        PutWindowTilemap(sWindowIds[i]);
    }
    ScheduleBgCopyTilemapToVram(0);
    BuildQuestList();
    DrawJournal();
    SetVBlankCallback(VBlankCB_CormoriaQuestMenu);
    SetMainCallback2(MainCB_CormoriaQuestMenu);
    CreateTask(Task_CormoriaQuestMenu, 0x50);
}

void CormoriaQuestMenu_CopyQuestName(u8 *dst, u8 questId)
{
    if (dst == NULL || questId >= CORMORIA_QUEST_COUNT)
        return;
    StringCopy(dst, sQuests[questId].name);
}
