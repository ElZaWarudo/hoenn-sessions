#include "global.h"
#include "event_object_movement.h"
#include "test/test.h"
#include "palette.h"
#include "constants/event_objects.h"

TEST("Johto object graphics append without changing host IDs")
{
    EXPECT(OBJ_EVENT_GFX_JOHTO_SILVER > OBJ_EVENT_GFX_SS_ANNE);
    EXPECT_EQ(OBJ_EVENT_GFX_JOHTO_SHINY_GYARADOS, OBJ_EVENT_GFX_JOHTO_SILVER + 34);
    EXPECT_EQ(OBJ_EVENT_GFX_JOHTO_SHARED_BEAUTY, OBJ_EVENT_GFX_JOHTO_SHINY_GYARADOS + 1);
    EXPECT_EQ(OBJ_EVENT_GFX_JOHTO_SHARED_YOUNGSTER, OBJ_EVENT_GFX_JOHTO_SHARED_BEAUTY + 48);
    EXPECT_EQ(NUM_OBJ_EVENT_GFX, OBJ_EVENT_GFX_JOHTO_SHARED_YOUNGSTER + 1);
    EXPECT_EQ(OBJ_EVENT_GFX_VARS, NUM_OBJ_EVENT_GFX + 1);
}

TEST("Johto object graphics expose all 35 public records")
{
    static const u16 ids[] = {
        OBJ_EVENT_GFX_JOHTO_SILVER, OBJ_EVENT_GFX_JOHTO_SUPER_NERD,
        OBJ_EVENT_GFX_JOHTO_KIMONO_GIRL, OBJ_EVENT_GFX_JOHTO_SLOWPOKE_NO_TAIL,
        OBJ_EVENT_GFX_JOHTO_KURT, OBJ_EVENT_GFX_JOHTO_BATTLE_GIRL,
        OBJ_EVENT_GFX_JOHTO_SAGE, OBJ_EVENT_GFX_JOHTO_ATTENDANT,
        OBJ_EVENT_GFX_JOHTO_EUSINE, OBJ_EVENT_GFX_JOHTO_ENGINEER,
        OBJ_EVENT_GFX_JOHTO_FIREBREATHER, OBJ_EVENT_GFX_JOHTO_JUGGLER,
        OBJ_EVENT_GFX_JOHTO_LEGENDARY_SHADOW, OBJ_EVENT_GFX_JOHTO_WHIRLPOOL,
        OBJ_EVENT_GFX_JOHTO_ARCHER, OBJ_EVENT_GFX_JOHTO_SCIENTIST_M,
        OBJ_EVENT_GFX_JOHTO_PROF_ELM, OBJ_EVENT_GFX_JOHTO_SCIENTIST_F,
        OBJ_EVENT_GFX_JOHTO_NURSE_CHANSEY, OBJ_EVENT_GFX_JOHTO_FALKNER,
        OBJ_EVENT_GFX_JOHTO_BUGSY, OBJ_EVENT_GFX_JOHTO_BURGLAR,
        OBJ_EVENT_GFX_JOHTO_WHITNEY, OBJ_EVENT_GFX_JOHTO_ATTENDANT_M,
        OBJ_EVENT_GFX_JOHTO_TRAIN_FRONT, OBJ_EVENT_GFX_JOHTO_PROTON,
        OBJ_EVENT_GFX_JOHTO_ARIANA, OBJ_EVENT_GFX_JOHTO_PETREL,
        OBJ_EVENT_GFX_JOHTO_MORTY, OBJ_EVENT_GFX_JOHTO_JASMINE,
        OBJ_EVENT_GFX_JOHTO_CHUCK, OBJ_EVENT_GFX_JOHTO_PRYCE,
        OBJ_EVENT_GFX_JOHTO_CLAIR, OBJ_EVENT_GFX_JOHTO_JANINE,
        OBJ_EVENT_GFX_JOHTO_SHINY_GYARADOS,
    };
    u32 i;
    EXPECT_EQ(ARRAY_COUNT(ids), 35);
    for (i = 0; i < ARRAY_COUNT(ids); i++)
    {
        const struct ObjectEventGraphicsInfo *info = GetObjectEventGraphicsInfo(ids[i]);
        EXPECT(info != NULL);
        EXPECT(info->images != NULL);
        EXPECT(info->anims != NULL);
        EXPECT(info->paletteTag >= 0x1200 && info->paletteTag <= 0x121A);
    }
}

TEST("Johto object graphics preserve representative dimensions and real Whirlpool frames")
{
    const struct ObjectEventGraphicsInfo *silver = GetObjectEventGraphicsInfo(OBJ_EVENT_GFX_JOHTO_SILVER);
    const struct ObjectEventGraphicsInfo *slowpoke = GetObjectEventGraphicsInfo(OBJ_EVENT_GFX_JOHTO_SLOWPOKE_NO_TAIL);
    const struct ObjectEventGraphicsInfo *whirlpool = GetObjectEventGraphicsInfo(OBJ_EVENT_GFX_JOHTO_WHIRLPOOL);
    const struct ObjectEventGraphicsInfo *train = GetObjectEventGraphicsInfo(OBJ_EVENT_GFX_JOHTO_TRAIN_FRONT);
    EXPECT_EQ(silver->width, 16);
    EXPECT_EQ(silver->height, 32);
    EXPECT_EQ(silver->size, 256);
    EXPECT_EQ(slowpoke->width, 32);
    EXPECT_EQ(slowpoke->height, 32);
    EXPECT_EQ(slowpoke->size, 512);
    EXPECT_EQ(whirlpool->width, 64);
    EXPECT_EQ(whirlpool->height, 64);
    EXPECT_EQ(whirlpool->size, 2048);
    EXPECT_EQ((u32)whirlpool->shadowSize, SHADOW_SIZE_NONE);
    EXPECT_EQ((u32)train->inanimate, TRUE);
    EXPECT_EQ((u32)train->tracks, TRACKS_NONE);
    EXPECT_EQ(whirlpool->images[4].size, whirlpool->size);
}

// Fixed oracle captured from the pinned donor source ledger and JASC palettes.
// This fixture is independent of the production importer and its output manifest.
TEST("Johto object graphics register exact source palettes and bounded animation frames")
{
    static const struct {
        u16 id, tag;
        u8 frames, animations;
        u16 colors[16];
    } cases[] = {
        {OBJ_EVENT_GFX_JOHTO_SILVER, 0x1216, 9, 28, {0x4EF3, 0x104C, 0x2539, 0x1C74, 0x3DFC, 0x5F7F, 0x210F, 0x4F1E, 0x0000, 0x3A7B, 0x30CA, 0x3508, 0x20A4, 0x208A, 0x2865}},
        {OBJ_EVENT_GFX_JOHTO_SUPER_NERD, 0x120D, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x5A9F, 0x3DBA, 0x2911, 0x7712, 0x660C, 0x24E7, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_KIMONO_GIRL, 0x1209, 9, 28, {0x4EF3, 0x1D05, 0x6FBE, 0x25BE, 0x0000, 0x210F, 0x7B9C, 0x418A, 0x1084, 0x677E, 0x15A7, 0x135C, 0x73BF, 0x10BE, 0x18C9, 0x1247}},
        {OBJ_EVENT_GFX_JOHTO_SLOWPOKE_NO_TAIL, 0x1217, 6, 24, {0x6B5A, 0x7FFF, 0x673A, 0x47DF, 0x333F, 0x229E, 0x1133, 0x7C1F, 0x111E, 0x1091, 0x318D, 0x429F, 0x321F, 0x111E, 0x00F1, 0x0842}},
        {OBJ_EVENT_GFX_JOHTO_KURT, 0x120D, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x5A9F, 0x3DBA, 0x2911, 0x7712, 0x660C, 0x24E7, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_BATTLE_GIRL, 0x120E, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x32B9, 0x21CF, 0x0CE7, 0x25BC, 0x14F2, 0x004A, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_SAGE, 0x1213, 9, 28, {0x530E, 0x2911, 0x210F, 0x3A5B, 0x0000, 0x4AFE, 0x7FFF, 0x5B5F, 0x24E7, 0x2D29, 0x660C, 0x7712}},
        {OBJ_EVENT_GFX_JOHTO_ATTENDANT, 0x120E, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x32B9, 0x21CF, 0x0CE7, 0x25BC, 0x14F2, 0x004A, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_EUSINE, 0x1205, 9, 28, {0x4EF3, 0x2108, 0x251B, 0x2D29, 0x7FBD, 0x0000, 0x44CF, 0x6318, 0x1DD1, 0x30CB, 0x3A7B, 0x090B, 0x2676, 0x4E73, 0x4F1E, 0x2889}},
        {OBJ_EVENT_GFX_JOHTO_ENGINEER, 0x120B, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x277F, 0x12BA, 0x0190, 0x7634, 0x5D4D, 0x30E8, 0x21DD, 0x1D15, 0x18C9, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_FIREBREATHER, 0x120D, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x5A9F, 0x3DBA, 0x2911, 0x7712, 0x660C, 0x24E7, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_JUGGLER, 0x120C, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x22FB, 0x1214, 0x114A, 0x1B30, 0x0227, 0x0904, 0x5E5B, 0x4134, 0x208A, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_LEGENDARY_SHADOW, 0x1218, 9, 28, {0x61E0, 0x7FFF, 0x779B, 0x7337, 0x5E93, 0x4E31, 0x3D8C, 0x3528, 0x3198, 0x3515, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x72C9}},
        {OBJ_EVENT_GFX_JOHTO_WHIRLPOOL, 0x1219, 5, 20, {0x03FF, 0x7B99, 0x7F56, 0x76B1, 0x5508, 0x6DEC, 0x652A, 0x4CE8, 0x7BDD, 0x7779, 0x6208, 0x7F33, 0x7ED0, 0x6E6A, 0x6E09, 0x728C}},
        {OBJ_EVENT_GFX_JOHTO_ARCHER, 0x1210, 9, 28, {0x4EF3, 0x24C6, 0x097B, 0x773A, 0x6695, 0x358C, 0x7FBD, 0x0000, 0x210F, 0x5F5F, 0x66EE, 0x2529, 0x3A7B, 0x522B, 0x5F7F, 0x4F1E}},
        {OBJ_EVENT_GFX_JOHTO_SCIENTIST_M, 0x120E, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x32B9, 0x21CF, 0x0CE7, 0x25BC, 0x14F2, 0x004A, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_PROF_ELM, 0x1204, 9, 28, {0x4EF3, 0x18E9, 0x296D, 0x2A15, 0x0000, 0x5F7F, 0x4AFE, 0x210E, 0x3A5B, 0x10C7, 0x4E52, 0x7B9C, 0x6B17, 0x7FFF, 0x520F, 0x21CF}},
        {OBJ_EVENT_GFX_JOHTO_SCIENTIST_F, 0x1214, 9, 28, {0x0000, 0x0000, 0x24A5, 0x3129, 0x5631, 0x5F5F, 0x210F, 0x3A7B, 0x3E9B, 0x4F1E, 0x3659, 0x39AA, 0x6EF7, 0x3DAC, 0x0821, 0x7FBD}},
        {OBJ_EVENT_GFX_JOHTO_NURSE_CHANSEY, 0x120D, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x5A9F, 0x3DBA, 0x2911, 0x7712, 0x660C, 0x24E7, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_FALKNER, 0x1206, 9, 28, {0x4EF3, 0x6A75, 0x62B0, 0x1488, 0x4DE5, 0x46DC, 0x0000, 0x49C9, 0x4124, 0x34E3, 0x7B9C, 0x3A7B, 0x4F1E, 0x3639, 0x573E, 0x210E}},
        {OBJ_EVENT_GFX_JOHTO_BUGSY, 0x1200, 9, 28, {0x4EF3, 0x30A8, 0x0000, 0x6191, 0x7256, 0x4AFE, 0x3A5B, 0x5B5F, 0x1148, 0x164A, 0x1679, 0x4354, 0x0227, 0x7B9C, 0x4A31}},
        {OBJ_EVENT_GFX_JOHTO_BURGLAR, 0x120E, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x32B9, 0x21CF, 0x0CE7, 0x25BC, 0x14F2, 0x004A, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_WHITNEY, 0x121A, 9, 28, {0x4AF3, 0x62BF, 0x1CAB, 0x4DE9, 0x2508, 0x7FFF, 0x323A, 0x2F1E, 0x561F, 0x0000, 0x3A5B, 0x1D0F, 0x5B5F, 0x6B18, 0x05F5, 0x3518}},
        {OBJ_EVENT_GFX_JOHTO_ATTENDANT_M, 0x120E, 9, 28, {0x530E, 0x5B5F, 0x4AFE, 0x3A5B, 0x210F, 0x32B9, 0x21CF, 0x0CE7, 0x25BC, 0x14F2, 0x004A, 0x6B18, 0x4A31, 0x2D29, 0x7FFF, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_TRAIN_FRONT, 0x1201, 1, 1, {0x5757, 0x354A, 0x3DEF, 0x52D6, 0x7FFD, 0x2298, 0x36DA, 0x4AF9, 0x61C6, 0x7EEF, 0x739B, 0x3629, 0x3AF0, 0x299C, 0x31FF, 0x4652}},
        {OBJ_EVENT_GFX_JOHTO_PROTON, 0x1210, 9, 28, {0x4EF3, 0x24C6, 0x097B, 0x773A, 0x6695, 0x358C, 0x7FBD, 0x0000, 0x210F, 0x5F5F, 0x66EE, 0x2529, 0x3A7B, 0x522B, 0x5F7F, 0x4F1E}},
        {OBJ_EVENT_GFX_JOHTO_ARIANA, 0x1211, 9, 28, {0x4EF3, 0x188A, 0x1492, 0x1898, 0x3A7B, 0x0000, 0x4F1E, 0x210F, 0x1ADE, 0x2529, 0x6695, 0x7FBD, 0x18C9, 0x097B, 0x358C, 0x773A}},
        {OBJ_EVENT_GFX_JOHTO_PETREL, 0x1212, 9, 28, {0x0380, 0x1CC6, 0x2929, 0x210F, 0x773A, 0x5192, 0x358C, 0x26B9, 0x097B, 0x5B5F, 0x4AFE, 0x5F7F, 0x6695, 0x4F1E, 0x6657, 0x3A5B}},
        {OBJ_EVENT_GFX_JOHTO_MORTY, 0x120A, 9, 28, {0x0320, 0x2D29, 0x2C87, 0x4E52, 0x05F5, 0x47BF, 0x133E, 0x48EC, 0x5B5F, 0x20D5, 0x4AFE, 0x5F7F, 0x0000, 0x4F1E, 0x210F, 0x0421}},
        {OBJ_EVENT_GFX_JOHTO_JASMINE, 0x1208, 9, 28, {0x4EF3, 0x296F, 0x6393, 0x45AF, 0x10A7, 0x159A, 0x7FFF, 0x4AFE, 0x210F, 0x4298, 0x468C, 0x263D, 0x5F7F, 0x5B5F, 0x66F7, 0x0000}},
        {OBJ_EVENT_GFX_JOHTO_CHUCK, 0x1202, 9, 28, {0x6FE5, 0x28CC, 0x18E9, 0x5F7F, 0x0000, 0x4E52, 0x210F, 0x0421, 0x254C, 0x46DC, 0x18E9, 0x3A5B, 0x571E, 0x4AFE, 0x323A, 0x5B5F}},
        {OBJ_EVENT_GFX_JOHTO_PRYCE, 0x120F, 9, 28, {0x4EF3, 0x4A10, 0x7B9C, 0x62F7, 0x0000, 0x4F1E, 0x5F7F, 0x5B3E, 0x210F, 0x323A, 0x3A7B, 0x30E4, 0x4DA9, 0x3DEF}},
        {OBJ_EVENT_GFX_JOHTO_CLAIR, 0x1203, 9, 28, {0x4EF3, 0x24E4, 0x49E6, 0x66A9, 0x131D, 0x6EEC, 0x0000, 0x3A7B, 0x5F7F, 0x4F1E, 0x210F, 0x01B7, 0x2488, 0x2529, 0x18A5, 0x18C9}},
        {OBJ_EVENT_GFX_JOHTO_JANINE, 0x1207, 9, 28, {0x4EF3, 0x318C, 0x6A73, 0x1884, 0x554F, 0x3A7B, 0x0E57, 0x44B1, 0x3D0C, 0x5F7F, 0x18CD, 0x0000, 0x693A, 0x210F, 0x5B3E, 0x28A7}},
        {OBJ_EVENT_GFX_JOHTO_SHINY_GYARADOS, 0x1215, 6, 24, {0x4E66, 0x0000, 0x000D, 0x7FBD, 0x151F, 0x28B9, 0x737B, 0x3E56, 0x0079, 0x1A36, 0x0214, 0x1191, 0x373F, 0x0211, 0x3517, 0x4DDB}},
    };
    u16 savedPalette[16];
    u32 i, a, c;
    for (c = 0; c < 16; c++)
        savedPalette[c] = gPlttBufferUnfaded[OBJ_PLTT_ID(15) + c];
    for (i = 0; i < ARRAY_COUNT(cases); i++)
    {
        const struct ObjectEventGraphicsInfo *info = GetObjectEventGraphicsInfo(cases[i].id);
        EXPECT_EQ(info->paletteTag, cases[i].tag);
        PatchObjectPalette(cases[i].tag, 15);
        for (c = 0; c < 16; c++)
        {
            if (gPlttBufferUnfaded[OBJ_PLTT_ID(15) + c] != cases[i].colors[c])
                Test_MgbaPrintf("Palette mismatch: case %u, color %u", i, c);
            EXPECT_EQ(gPlttBufferUnfaded[OBJ_PLTT_ID(15) + c], cases[i].colors[c]);
        }
        for (c = 0; c < cases[i].frames; c++)
        {
            EXPECT(info->images[c].data != NULL);
            EXPECT_EQ(info->images[c].size, info->size);
        }
        for (a = 0; a < cases[i].animations; a++)
        {
            bool32 terminated = FALSE;
            for (c = 0; c < 32; c++)
            {
                const union AnimCmd *command = &info->anims[a][c];
                if (command->type == -1 || command->type == -2)
                {
                    if (command->type == -2)
                        EXPECT(command->jump.target <= c);
                    terminated = TRUE;
                    break;
                }
                EXPECT(command->type >= 0 || command->type == -3);
                if (command->type >= 0)
                    EXPECT(command->frame.imageValue < cases[i].frames);
            }
            EXPECT(terminated);
        }
    }
    LoadPalette(savedPalette, OBJ_PLTT_ID(15), sizeof(savedPalette));
}
