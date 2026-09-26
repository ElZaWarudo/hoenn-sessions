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
static const u8 sTextFavorite[] = _("[F] ");
static const u8 sTextLocation[] = _("Location: ");
static const u8 sTextBack[] = _("B: back");
static const u8 sTextReturnForReward[] = _("Return to this location to receive your reward!");
static const u8 sTextStartForDetails[] = _("Start this quest for more details.");
static const u8 sTextHiddenSubquest[] = _("Complete this step to reveal its details.");
static const u8 sTextStateUnavailable[] = _("Quest data unavailable.");

static const struct CormoriaSubquestEntry sSubquests[CORMORIA_SUBQUEST_COUNT] =
{
    {_("My First Day"),
     _("It's my first day! I need to go to the Tenebris Lab with my Welcome Package. I shouldn't be late at any cost!"),
     _("Tenebris Laboratory")},
    {_("Lab Supplies"),
     _("I need to pick up the lab's supplies from the Poké Mart in Fennilahl Town. It has my starter Pokémon! And some important stuff."),
     _("Fennilahl Town")},
    {_("Missing Supplies"),
     _("Professor Tenebris has already taken the supplies! I need to tell Asst. Prof. Rue as soon as possible."),
     _("Tenebris Laboratory")},
    {_("The First Dreamstone"),
     _("The first dreamstone is atop Mt. Ceram. I need to cross Route 3 to Gastree City, then head north towards the Ceram Base Camp."),
     _("Mt. Ceram")},
    {_("Mysterious Area"),
     _("The dreamstone transported me and Gabrielle to a mysterious area! I don't recognise the Pokémon here. I need to find a way back."),
     _("Mysterious Area")},
    {_("Silversun Sighting"),
     _("Someone matching Prof. Tenebris' description was spotted in Silversun City! Team Somber is there too, so I need to go there fast."),
     _("Silversun City")},
    {_("Of Drama & Desire"),
     _("Team Somber's hideout is somewhere here. If they've taken Prof Tenebris hostage, it could spell trouble. Gotta find them!"),
     _("Silversun City")},
    {_("Knowledge of a Past Era"),
     _("I need to talk to Martha the historian, who lives at the Mirroh Base Camp. Can she give us a clue as to Team Somber's motives?"),
     _("Mirroh Base Camp")},
    {_("Showdown at Mt. Mirroh!"),
     _("What could Team Somber possibly want with the Ancient Terror? I'll find my answers at Mt. Mirroh...if I hurry!"),
     _("Mt. Mirroh")},
    {_("Stop Melea!"),
     _("I got warped into the past again! And Somber Admin Melea is here too. I've got to stop her from catching the Ancient Terror!"),
     _("Ancient Mirroh")},
    {_("No Way Out"),
     _("Somber Admin Melea escaped with the Ancient Terror! But Kohla found his way here too, so there should be another exit out."),
     _("Ancient Mirroh")},
    {_("Reach Rivetshore City"),
     _("A massive heatwave swept across Cormoria! It's Team Somber's doing. I've got to reach Rivetshore City to chase them!"),
     _("Rivetshore City")},
    {_("Board the S.S. Elegant"),
     _("The Gym Leaders are going to board the S.S. Elegant and track down Team Somber. I've got to get on board too!"),
     _("Rivetshore City")},
    {_("Get Off the Ship!"),
     _("We've stopped at an uncharted island, but civilians can't get off. Gabrielle and Breech are waiting at the storage hold for me!"),
     _("S.S. Elegant")},
    {_("Explore the Island"),
     _("Gabrielle and Breech got me off the ship. Now I need to find Team Somber and stop them! But I can't let the leaders catch me."),
     _("Uncharted Island")},
    {_("A Ranger's First Assignment"),
     _("Ranger Chief Ravine has asked me to deliver an important package to the Ranger Institute at Ivy River. I can't let him down!"),
     _("Ranger Institute")},
    {_("Fieldwork: Mega Evolution"),
     _("Scientists at the Ivy River Ranger Institute need specimens of different Pokémon to study their potential for Mega Evolution."),
     _("Ranger Institute")},
    {_("The Final Test"),
     _("This is the final test I need to complete to become a fully-fledged Pokémon Ranger. Can I track the mythical Pokémon?"),
     _("Ranger Institute")},
    {_("Help the Mayor!"),
     _("The mayor of Pelluca City has asked me to deal with the Qwilsquad! I need to find a way into their hideout by the riverbank."),
     _("Pelluca City")},
    {_("Save the Citizens!"),
     _("The city is flooded and some citizens are drowning! Leader Jania gave me the HM Surf. I need to save the drowning citizens!"),
     _("Pelluca City")},
};

static const struct CormoriaQuestEntry sQuests[CORMORIA_QUEST_COUNT] =
{
    {_("Lab Assistant"),
     _("From today, I'm going to be a Lab Assistant at the Tenebris Laboratory!"),
     _("I've done what I can back at the lab. Now it's time to head out and chase the dreamstones around Cormoria."),
     _("Tenebris Laboratory"), 0, 3},
    {_("Find the Dreamstone!"),
     _("I need to solve the mystery of the dreamstones around Cormoria."),
     _("A total failure! I couldn't stop Team Somber or find Professor Tenebris, and now I'm off the case. I guess that's it...?"),
     _("Mt. Ceram"), 3, 8},
    {_("Dreamstone Mysteries"),
     _("It doesn't matter if I'm off the case. I'll stop Team Somber, find Prof. Tenebris and solve the mystery of the dreamstones!"),
     _("I've solved the mystery of the dreamstones! Tenebris is back and Team Somber is done. All's well that ends well!"),
     _("Cormoria"), 11, 4},
    {_("Food Poisoning"),
     _("The Azurill in the house on Route 1 has food poisoning. She needs a Pecha Berry!"),
     _("Azurill has recovered and is happy again! A Pecha Berry a day keeps the doctor away."),
     _("Route 1"), 0, 0},
    {_("A Hiker's Treasure"),
     _("A conflict-averse hiker in Fennilahl Town has lost a Strange Rock in Route 2. Apparently a small pink Pokémon stole it!"),
     _("I got Breech his stone back...and found him a new companion! I hope to see him and Clefairy again soon."),
     _("Fennilahl Town"), 0, 0},
    {_("A Lost Skitty"),
     _("Someone's Skitty in Gastree City has gone missing! It probably climbed up a tree or something..."),
     _("Skitty is reunited with her trainer!"),
     _("Gastree City"), 0, 0},
    {_("Historical Preservation"),
     _("An archaeologist in Gastree City has asked me to find all ten ancient statues across Cormoria and scan them with the Archaeolens!"),
     _("I found all ten statues!"),
     _("Gastree City"), 0, 0},
    {_("Modern Matcha"),
     _("The lady at the Gastree Teahouse wants to craft a new tea blend. She needs 1 Revival Herb, 1 Energy Powder and 1 Shoal Salt."),
     _("The modern blend is done! But will it do well on the menu? Or will it be a flop? Silly question! It's a hit of course!"),
     _("Gastree City"), 0, 0},
    {_("Cyndaquil's New Move"),
     _("A trainer in Ceram Base Camp wants his Cyndaquil to learn...Acid Spray? Where can I get the TM for Acid Spray?"),
     _("I taught the Cyndaquil the move Acid Spray! I hope the trainer learns more about battling and they become strong together!"),
     _("Ceram Base Camp"), 0, 0},
    {_("Precious Pearls"),
     _("A rich lady in Galecrest City has had her pearls stolen! I'd better find the robber. Maybe I'll get a huge reward..."),
     _("I found the robber and returned the pearls! But the horrid lady charged me money for being late! Is that why she's rich?"),
     _("Galecrest City"), 0, 0},
    {_("Love Is Sacrifice"),
     _("A down-on-his-luck man in Galecrest City wants to do something special for his wife. He wants to gift her a Blue Flute!"),
     _("With the Blue Flute (and Jigglypuff), the house is singing! Hard times come and go, but love and music remain!"),
     _("Galecrest City"), 0, 0},
    {_("Malevolent Masterpiece"),
     _("A (self-proclaimed) famous artist in Silversun City wants Black Sludge to create the perfect shade of black paint."),
     _("The Black Sludge created the perfect shade of black paint! Maybe this artist is really a maestro after all."),
     _("Silversun City"), 0, 0},
    {_("I Can't Find My Wife!"),
     _("A man in the Silversun Sewers has gotten separated from his wife. If I don't hurry, she might get attacked by the Sewer Scourge!"),
     _("Husband and wife have been successfully reunited. They also got a Furfrou to help. All's well that ends well!"),
     _("Silversun Sewers"), 0, 0},
    {_("Career Crisis"),
     _("A fisherman on Route 6 wants to become a chef! He wants to sample some of Pelluca's famous Apple Pie and try to recreate it!"),
     _("The fisherman enjoyed the Apple Pie! Thank you for the Trolling Rod and all the best!"),
     _("Route 6"), 0, 0},
    {_("Pokémon Ranger Badge"),
     _("My path towards becoming a fully-fledged Pokémon Ranger!"),
     _("I cleared the interview, delivered the package, helped the scientists and now I'm a fully-fledged Pokémon Ranger!"),
     _("Ranger Institute"), 15, 3},
    {_("Pelluca's Leadership Tussle"),
     _("Pelluca City is in trouble! The mayor and the Qwilsquad Gang leader are tussling for power and the city is suffering!"),
     _("The mayor and the Qwilsquad boss have agreed to cooperate and develop the city they both love. All the best!"),
     _("Pelluca City"), 18, 2},
    {_("A Chef's Icy Troubles"),
     _("The refrigerators at the Pelluca Restaurant are broken and their ingredients are going bad. I need to bring them a Nevermelt Ice."),
     _("The Nevermelt Ice can keep the ingredients cool...until they get a Rotom Fridge."),
     _("Pelluca Restaurant"), 0, 0},
    {_("The Healers Need Help!"),
     _("A Chansey is lost inside Mt Mirroh! They've always healed me when I needed it, and now it's my turn to repay them!"),
     _("Chansey is reunited with the nurse and they're off to Winterlily Hollow! I hope their situation improves soon..."),
     _("Mt. Mirroh"), 0, 0},
    {_("Percy's Gone Missing!"),
     _("The Rivetshore Construction CEO's beloved Percy has gone missing! I need to find it. Did it fall for a prank, perhaps?"),
     _("Percy and the CEO are reunited! I thought I'd get money, but this rare flute is even cooler!"),
     _("Rivetshore City"), 0, 0},
    {_("Mean Old Grandma"),
     _("Two brothers in Rivetshore City want to game but their grandma won't buy them a console! Just like my younger days."),
     _("The brothers love their new PocketBoy! I hope they don't get addicted to it... Maybe I should play just one round."),
     _("Rivetshore City"), 0, 0},
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
    StringCopy(buffer, _("Cormoria Quests "));
    ConvertIntToDecimalStringN(gStringVar1, sQuestCount, STR_CONV_MODE_LEFT_ALIGN, 2);
    StringAppend(buffer, gStringVar1);
    StringAppend(buffer, _("/"));
    ConvertIntToDecimalStringN(gStringVar1, CORMORIA_QUEST_COUNT, STR_CONV_MODE_LEFT_ALIGN, 2);
    StringAppend(buffer, gStringVar1);
    PrintText(sWindowIds[0], FONT_NORMAL, buffer, 4, 0);
    StringCopy(buffer, sFilterNames[sFilter]);
    if (sAlphabetical)
        StringAppend(buffer, _(" A-Z"));
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
            StringCopy(rowText, index == sCursor ? _("> ") : _("  "));
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
            StringCopy(rowText, index == sCursor ? _("> ") : _("  "));
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
            StringAppend(buffer, _("\n"));
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
        StringAppend(buffer, _("\n"));
        StringAppend(buffer, description);
        WrapJournalText(buffer, 39);
        PrintText(sWindowIds[2], FONT_SMALL_NARROW, buffer, 4, 2);
        if (ReadQuestBit(questId, CORMORIA_QUEST_FAVORITE))
            PrintText(sWindowIds[2], FONT_SMALL_NARROW, _("Favorite"), 164, 44);
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
