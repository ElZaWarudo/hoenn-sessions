#include "global.h"
#include "constants/species.h"
#include "event_data.h"
#include "johto/quest_party.h"
#include "mail.h"
#include "pokemon.h"
#include "pokemon_storage_system.h"
#include "script.h"
#include "string_util.h"

static const u8 sKenyaNickname[] = _("KENYA");
static const u8 sShuckieNickname[] = _("SHUCKIE");

static bool8 IsEgg(struct Pokemon *mon)
{
    return GetMonData(mon, MON_DATA_IS_EGG, NULL);
}

static bool8 HasOtherNonEggMon(u16 excludedSlot)
{
    u16 i;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        struct Pokemon *mon = &gPlayerParty[i];
        if (i != excludedSlot
         && GetMonData(mon, MON_DATA_SPECIES, NULL) != SPECIES_NONE
         && !IsEgg(mon))
            return TRUE;
    }
    return FALSE;
}

static bool8 HasSafeMail(struct Pokemon *mon)
{
    u16 heldItem = GetMonData(mon, MON_DATA_HELD_ITEM, NULL);
    u8 mailId = GetMonData(mon, MON_DATA_MAIL, NULL);
    u32 i;

    if (!ItemIsMail(heldItem))
        return TRUE;

    if (!MonHasMail(mon) || mailId >= PARTY_SIZE)
        return FALSE;
    for (i = 0; i < PARTY_SIZE; i++)
    {
        struct Pokemon *other = &gPlayerParty[i];
        if (other != mon && GetMonData(other, MON_DATA_SPECIES) != SPECIES_NONE
         && MonHasMail(other) && GetMonData(other, MON_DATA_MAIL) == mailId)
            return FALSE;
    }
    return gSaveBlock1Ptr->mail[mailId].itemId == heldItem;
}

static bool8 HasExpectedNickname(struct Pokemon *mon, const u8 *expected)
{
    u8 nickname[POKEMON_NAME_LENGTH + 1];

    GetMonData(mon, MON_DATA_NICKNAME, nickname);
    return StringCompare(nickname, expected) == 0;
}

static bool8 IsNamedGiftMatch(u16 giftId, struct Pokemon *mon)
{
    u16 species = GetMonData(mon, MON_DATA_SPECIES, NULL);
    const u8 *nickname;

    if (giftId == JOHTO_QUEST_NAMED_GIFT_KENYA)
    {
        if (species != SPECIES_SPEAROW && species != SPECIES_FEAROW)
            return FALSE;
        nickname = sKenyaNickname;
    }
    else if (giftId == JOHTO_QUEST_NAMED_GIFT_SHUCKIE)
    {
        if (species != SPECIES_SHUCKLE)
            return FALSE;
        nickname = sShuckieNickname;
    }
    else
    {
        return FALSE;
    }

    return HasExpectedNickname(mon, nickname);
}

static bool8 IsBaobaSpecies(u8 checkId, u16 species)
{
    switch (checkId)
    {
    case 1:
        return species == SPECIES_CACNEA
            || species == SPECIES_LOTAD
            || species == SPECIES_MAKUHITA
            || species == SPECIES_LOMBRE
            || species == SPECIES_TRAPINCH
            || species == SPECIES_BELDUM
            || species == SPECIES_VIBRAVA;
    case 2:
        return species == SPECIES_TROPIUS
            || species == SPECIES_CHIMECHO
            || species == SPECIES_ABSOL
            || species == SPECIES_CASTFORM;
    case 3:
        return species == SPECIES_BARBOACH
            || species == SPECIES_WHISCASH
            || species == SPECIES_MEDITITE
            || species == SPECIES_NUMEL
            || species == SPECIES_BALTOY
            || species == SPECIES_ABSOL
            || species == SPECIES_MEDICHAM
            || species == SPECIES_CAMERUPT;
    case 4:
        return species == SPECIES_WHISMUR
            || species == SPECIES_NOSEPASS
            || species == SPECIES_BAGON
            || species == SPECIES_RELICANTH
            || species == SPECIES_FEEBAS;
    default:
        return FALSE;
    }
}

void Script_JohtoRemoveNamedMon(struct ScriptContext *ctx)
{
    u16 giftId = ScriptReadHalfword(ctx);
    u16 i;

    Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE);
    gSpecialVar_Result = JOHTO_QUEST_RESULT_NAMED_CANT_GIVE;

    if (giftId != JOHTO_QUEST_NAMED_GIFT_KENYA
     && giftId != JOHTO_QUEST_NAMED_GIFT_SHUCKIE)
        return;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        struct Pokemon *mon = &gPlayerParty[i];
        u16 species = GetMonData(mon, MON_DATA_SPECIES, NULL);

        if (species == SPECIES_NONE || IsEgg(mon) || !IsNamedGiftMatch(giftId, mon))
            continue;

        if (giftId == JOHTO_QUEST_NAMED_GIFT_KENYA)
        {
            if (!ItemIsMail(GetMonData(mon, MON_DATA_HELD_ITEM, NULL)) || !MonHasMail(mon))
                return;
        }
        else if (GetMonData(mon, MON_DATA_FRIENDSHIP, NULL) > 200)
        {
            gSpecialVar_Result = JOHTO_QUEST_RESULT_SHUCKIE_TOO_FRIENDLY;
            return;
        }

        if (!HasOtherNonEggMon(i) || !HasSafeMail(mon))
            return;

        if (MonHasMail(mon))
            TakeMailFromMon(mon);
        ZeroMonData(mon);
        CompactPartySlots();
        CalculatePlayerPartyCount();
        gSpecialVar_Result = JOHTO_QUEST_RESULT_NAMED_GIVEN;
        return;
    }
}

void Script_JohtoRemoveGenericMon(struct ScriptContext *ctx)
{
    u16 targetSpecies = ScriptReadHalfword(ctx);
    u16 monIndex = VarGet(VAR_0x8004);
    struct Pokemon *mon;
    u16 species;
    u16 level;

    Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE);
    gSpecialVar_Result = JOHTO_QUEST_RESULT_FAILURE;

    if (targetSpecies != SPECIES_MAGIKARP || monIndex >= PARTY_SIZE)
        return;

    mon = &gPlayerParty[monIndex];
    species = GetMonData(mon, MON_DATA_SPECIES, NULL);
    if (species != targetSpecies || species == SPECIES_NONE || IsEgg(mon)
     || !HasOtherNonEggMon(monIndex) || !HasSafeMail(mon))
        return;

    level = GetMonData(mon, MON_DATA_LEVEL, NULL);
    if (MonHasMail(mon))
        TakeMailFromMon(mon);
    ZeroMonData(mon);
    CompactPartySlots();
    CalculatePlayerPartyCount();
    gSpecialVar_Result = level == 100
        ? JOHTO_QUEST_RESULT_MAGIKARP_LEVEL_100
        : JOHTO_QUEST_RESULT_MAGIKARP;
}

void Script_JohtoBaobaCheckMon(struct ScriptContext *ctx)
{
    u8 checkId = ScriptReadByte(ctx);
    u16 monIndex = VarGet(VAR_0x8004);
    u16 species;

    Script_RequestEffects(SCREFF_V1);
    gSpecialVar_Result = FALSE;

    if (checkId < 1 || checkId > 4 || monIndex >= PARTY_SIZE)
        return;

    species = GetMonData(&gPlayerParty[monIndex], MON_DATA_SPECIES, NULL);
    if (species == SPECIES_NONE || IsEgg(&gPlayerParty[monIndex]))
        return;

    gSpecialVar_Result = IsBaobaSpecies(checkId, species);
}
