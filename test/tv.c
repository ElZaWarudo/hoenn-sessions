#include "global.h"
#include "tv.h"
#include "constants/region_map_sections.h"
#include "test/test.h"

TEST("TV show location keeps wide sections in its existing record")
{
    static const u8 kinds[] = {
        TVSHOW_SMART_SHOPPER,
        TVSHOW_POKEMON_TODAY_FAILED,
        TVSHOW_WORLD_OF_MASTERS,
        TVSHOW_TODAYS_RIVAL_TRAINER,
        TVSHOW_TREASURE_INVESTIGATORS,
        TVSHOW_BREAKING_NEWS,
    };
    static const u16 sections[] = {249, 250, 255, 256, 300};
    TVShow show;
    TVShow copied;
    u32 i;
    u32 j;

    for (i = 0; i < ARRAY_COUNT(kinds); i++)
    {
        for (j = 0; j < ARRAY_COUNT(sections); j++)
        {
            memset(&show, 0, sizeof(show));
            show.common.kind = kinds[i];
            EXPECT(TVShow_SetMapSection(&show, sections[j]));
            EXPECT_EQ(TVShow_GetMapSection(&show), sections[j]);
            EXPECT_EQ(((u8 *)&show)[0x1B], sections[j] >> 8);

            // The complete record is what save compaction and mixing copy.
            copied = show;
            EXPECT_EQ(TVShow_GetMapSection(&copied), sections[j]);
        }
    }

    memset(&show, 0, sizeof(show));
    EXPECT(!TVShow_SetMapSection(&show, 300));
    EXPECT_EQ(TVShow_GetMapSection(&show), MAPSEC_NONE);
    EXPECT_EQ(((u8 *)&show)[0x1B], 0);
}

TEST("TV Gabby and Ty location keeps its high bits in the existing save record")
{
    struct GabbyAndTyData data = {0};

    data.playerLostAMon2 = TRUE;
    EXPECT(GabbyAndTy_SetMapSection(&data, 300));
    EXPECT_EQ(GabbyAndTy_GetMapSection(&data), 300);
    EXPECT_EQ((u8)data.mapnumHi, 1);
    EXPECT(data.playerLostAMon2);
    EXPECT(GabbyAndTy_SetMapSection(&data, 4095));
    EXPECT_EQ(GabbyAndTy_GetMapSection(&data), 4095);
    EXPECT(!GabbyAndTy_SetMapSection(&data, 4096));
    EXPECT_EQ(GabbyAndTy_GetMapSection(&data), 4095);
    EXPECT(!GabbyAndTy_SetMapSection(NULL, 300));
    EXPECT_EQ(GabbyAndTy_GetMapSection(NULL), MAPSEC_NONE);
}

TEST("TV Ruby record mixing rejects every show with a foreign map section")
{
    static EWRAM_DATA TVShow shows[TV_SHOWS_COUNT];
    static const u8 kinds[] = {
        TVSHOW_SMART_SHOPPER,
        TVSHOW_POKEMON_TODAY_FAILED,
        TVSHOW_WORLD_OF_MASTERS,
        TVSHOW_TODAYS_RIVAL_TRAINER,
        TVSHOW_TREASURE_INVESTIGATORS,
        TVSHOW_BREAKING_NEWS,
    };
    u32 i;

    for (i = 0; i < ARRAY_COUNT(kinds); i++)
    {
        memset(shows, 0, sizeof(shows));
        shows[0].common.kind = kinds[i];
        EXPECT(TVShow_SetMapSection(&shows[0], 300));
        SanitizeTVShowLocationsForRuby(shows);
        EXPECT_EQ(shows[0].common.kind, TVSHOW_OFF_AIR);

        shows[0].common.kind = kinds[i];
        EXPECT(TVShow_SetMapSection(&shows[0], MAPSEC_LITTLEROOT_TOWN));
        SanitizeTVShowLocationsForRuby(shows);
        EXPECT_EQ(shows[0].common.kind, kinds[i]);
    }
}
