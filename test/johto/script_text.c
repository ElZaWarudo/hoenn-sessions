#include "global.h"
#include "constants/characters.h"
#include "event_data.h"
#include "load_save.h"
#include "pokemon.h"
#include "script.h"
#include "string_util.h"
#include "constants/vars.h"
#include "johto/script_text.h"
#include "test/test.h"

static const u8 sSentinel[] = _("UNCHANGED");
static const u8 sLeaf[] = _("Leaf");
static const u8 sFireMouse[] = _("Fire Mouse");
static const u8 sBigJaw[] = _("Big Jaw");

static void ResetStrings(void)
{
    StringCopy(gStringVar1, sSentinel);
    StringCopy(gStringVar2, sSentinel);
    StringCopy(gStringVar3, sSentinel);
}

static void BufferCategory(u8 destination, u16 operand)
{
    struct ScriptContext ctx;
    u8 payload[] = {destination, operand, operand >> 8};
    InitScriptContext(&ctx, NULL, NULL);
    ctx.scriptPtr = payload;
    Script_JohtoBufferMonCategory(&ctx);
    EXPECT_EQ(ctx.scriptPtr, payload + sizeof(payload));
}

TEST("Johto category operands copy three bare starter categories without changing other buffers")
{
    static const u16 species[] = {SPECIES_CHIKORITA, SPECIES_CYNDAQUIL, SPECIES_TOTODILE};
    const u8 *expected[] = {sLeaf, sFireMouse, sBigJaw};
    u8 *buffers[] = {gStringVar1, gStringVar2, gStringVar3};
    u32 i, j, variable;
    SetSaveBlocksPointers(0);
    InitEventData();
    for (variable = 0; variable < 2; variable++)
        for (i = 0; i < ARRAY_COUNT(species); i++)
        {
            ResetStrings();
            VarSet(VAR_TEMP_2, species[i]);
            BufferCategory(i, variable ? VAR_TEMP_2 : species[i]);
            for (j = 0; j < ARRAY_COUNT(buffers); j++)
                EXPECT_EQ(StringCompare(buffers[j], j == i ? expected[i] : sSentinel), 0);
            EXPECT_EQ(VarGet(VAR_TEMP_2), species[i]);
        }
}

TEST("Johto category rejects invalid destination and species while consuming operands")
{
    u32 i;
    static const u16 invalid[] = {SPECIES_NONE, NUM_SPECIES, 0xFFFF};
    SetSaveBlocksPointers(0);
    InitEventData();
    ResetStrings();
    BufferCategory(3, SPECIES_CHIKORITA);
    BufferCategory(255, SPECIES_TOTODILE);
    EXPECT_EQ(StringCompare(gStringVar1, sSentinel), 0);
    EXPECT_EQ(StringCompare(gStringVar2, sSentinel), 0);
    EXPECT_EQ(StringCompare(gStringVar3, sSentinel), 0);
    for (i = 0; i < ARRAY_COUNT(invalid); i++)
    {
        ResetStrings();
        VarSet(VAR_TEMP_2, invalid[i]);
        BufferCategory(1, VAR_TEMP_2);
        EXPECT_EQ(gStringVar2[0], EOS);
        EXPECT_EQ(StringCompare(gStringVar1, sSentinel), 0);
        EXPECT_EQ(StringCompare(gStringVar3, sSentinel), 0);
        EXPECT_EQ(VarGet(VAR_TEMP_2), invalid[i]);
    }
}

TEST("Johto category native is callable during effect analysis without saved mutations")
{
    u8 program[9];
    u32 i, pointer = (uintptr_t)Script_JohtoBufferMonCategory | 0x0A000000;
    SetSaveBlocksPointers(0);
    InitEventData();
    VarSet(VAR_TEMP_2, SPECIES_TOTODILE);
    ResetStrings();
    program[0] = 0x23; // Instrumented callnative, matching the macro ABI.
    for (i = 0; i < 4; i++)
        program[i + 1] = pointer >> (i * 8);
    program[5] = 2;
    program[6] = VAR_TEMP_2 & 0xFF;
    program[7] = VAR_TEMP_2 >> 8;
    program[8] = 0x02; // end
    EXPECT(!RunScriptImmediatelyUntilEffect(SCREFF_V1 | SCREFF_SAVE | SCREFF_HARDWARE, program, NULL));
    EXPECT_EQ(StringCompare(gStringVar3, sBigJaw), 0);
    EXPECT_EQ(StringCompare(gStringVar1, sSentinel), 0);
    EXPECT_EQ(StringCompare(gStringVar2, sSentinel), 0);
    EXPECT_EQ(VarGet(VAR_TEMP_2), SPECIES_TOTODILE);
}
