#include "global.h"
#include "constants/characters.h"
#include "johto/rival.h"
#include "johto/save.h"
#include "load_save.h"
#include "main.h"
#include "naming_screen.h"
#include "overworld.h"
#include "script.h"

const u8 gJohtoRivalNameMarker[] = _("{RIVAL}");
static const u8 sDefaultRivalName[] = _("SILVER");

EWRAM_DATA static u8 sJohtoRivalNameBuffer[JOHTO_RIVAL_NAME_SIZE] = {0};

static bool8 ValidateRivalName(const u8 *name, u8 *length)
{
    u8 i;

    if (name == NULL)
        return FALSE;

    for (i = 0; i < JOHTO_RIVAL_NAME_SIZE; i++)
    {
        if (name[i] == EOS)
        {
            if (i == 0)
                return FALSE;
            if (length != NULL)
                *length = i;
            return TRUE;
        }
        if (name[i] == EXT_CTRL_CODE_BEGIN || name[i] == PLACEHOLDER_BEGIN)
            return FALSE;
    }
    return FALSE;
}

static struct JohtoSaveV1 *GetCurrentRivalSave(void)
{
    return &gSaveblock1.johto;
}

const u8 *JohtoRival_GetName(void)
{
    const struct JohtoSaveV1 *save = GetCurrentRivalSave();

    if (JohtoSave_Validate(save) && ValidateRivalName(save->rival_name, NULL))
        return save->rival_name;
    return sDefaultRivalName;
}

bool8 JohtoRival_SetName(const u8 *name)
{
    struct JohtoSaveV1 *save = GetCurrentRivalSave();
    u8 copy[JOHTO_RIVAL_NAME_SIZE];
    u8 length;

    if (!JohtoSave_Validate(save) || !ValidateRivalName(name, &length))
        return FALSE;

    memset(copy, 0, sizeof(copy));
    memcpy(copy, name, length + 1);
    memset(save->rival_name, 0, sizeof(save->rival_name));
    memcpy(save->rival_name, copy, sizeof(copy));
    return JohtoSave_Seal(save);
}

const u8 *JohtoRival_ResolveTrainerName(const u8 *name)
{
    if (name != NULL && name[0] == PLACEHOLDER_BEGIN
        && name[1] == PLACEHOLDER_ID_RIVAL && name[2] == EOS)
        return JohtoRival_GetName();
    return name;
}

static void JohtoRival_NameScreenCallback(void)
{
    (void)JohtoRival_SetName(sJohtoRivalNameBuffer);
    SetMainCallback2(CB2_ReturnToFieldContinueScriptPlayMapMusic);
}

void Johto_NameRival(void)
{
    u8 i;
    const u8 *name;

    Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE | SCREFF_HARDWARE);
    name = JohtoRival_GetName();
    memset(sJohtoRivalNameBuffer, 0, sizeof(sJohtoRivalNameBuffer));
    for (i = 0; i < JOHTO_RIVAL_NAME_SIZE; i++)
    {
        sJohtoRivalNameBuffer[i] = name[i];
        if (name[i] == EOS)
            break;
    }
    DoNamingScreen(NAMING_SCREEN_RIVAL,
                   sJohtoRivalNameBuffer,
                   gSaveBlock2Ptr->playerGender,
                   0,
                   0,
                   JohtoRival_NameScreenCallback);
}
