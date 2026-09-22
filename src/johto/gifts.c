#include "global.h"
#include "event_data.h"
#include "daycare.h"
#include "johto/gifts.h"
#include "mail.h"
#include "pokemon.h"
#include "random.h"
#include "script.h"
#include "string_util.h"

enum JohtoNamedGiftId
{
    JOHTO_NAMED_GIFT_KENYA = 1,
    JOHTO_NAMED_GIFT_SHUCKIE,
    JOHTO_NAMED_GIFT_BILL_EEVEE,
    JOHTO_NAMED_GIFT_DRATINI,
};

static const u8 sKenyaNickname[] = _("KENYA");
static const u8 sKenyaOtName[] = _("RUDY");
static const u8 sShuckieNickname[] = _("SHUCKIE");
static const u8 sShuckieOtName[] = _("KIRK");
static const u8 sBillOtName[] = _("BILL");

static const u16 sKenyaMailWords[MAIL_WORDS_COUNT] =
{
    EC_WORD_YUP,
    EC_WORD_MAIL,
    EC_WORD_TIME,
    EC_WORD_TAKE,
    EC_WORD_THIS,
    EC_WORD_POKEMON,
    EC_WORD_DON_T,
    EC_WORD_LOSE,
    EC_WORD_IT,
};

static const enum Species sOddEggSpecies[] =
{
    SPECIES_NONE,
    SPECIES_PICHU,
    SPECIES_CLEFFA,
    SPECIES_IGGLYBUFF,
    SPECIES_TYROGUE,
    SPECIES_SMOOCHUM,
    SPECIES_ELEKID,
    SPECIES_MAGBY,
};

struct NamedGift
{
    enum Species species;
    u8 level;
    enum Item item;
    u32 otId;
    const u8 *nickname;
    const u8 *otName;
};

static bool8 GetNamedGift(u16 giftId, struct NamedGift *gift)
{
    switch (giftId)
    {
    case JOHTO_NAMED_GIFT_KENYA:
        *gift = (struct NamedGift) {
            .species = SPECIES_SPEAROW,
            .level = 20,
            .item = ITEM_RETRO_MAIL,
            .otId = 61225,
            .nickname = sKenyaNickname,
            .otName = sKenyaOtName,
        };
        return TRUE;
    case JOHTO_NAMED_GIFT_SHUCKIE:
        *gift = (struct NamedGift) {
            .species = SPECIES_SHUCKLE,
            .level = 20,
            .item = ITEM_BERRY_JUICE,
            .otId = 4336,
            .nickname = sShuckieNickname,
            .otName = sShuckieOtName,
        };
        return TRUE;
    case JOHTO_NAMED_GIFT_BILL_EEVEE:
        *gift = (struct NamedGift) {
            .species = SPECIES_EEVEE,
            .level = 20,
            .item = ITEM_NONE,
            .otId = 5231,
            .nickname = NULL,
            .otName = sBillOtName,
        };
        return TRUE;
    case JOHTO_NAMED_GIFT_DRATINI:
        *gift = (struct NamedGift) {
            .species = SPECIES_DRATINI,
            .level = 15,
            .item = ITEM_NONE,
            .otId = 0,
            .nickname = NULL,
            .otName = NULL,
        };
        return TRUE;
    default:
        return FALSE;
    }
}

static s8 FindEmptyPartySlot(void)
{
    u8 i;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        if (GetMonData(&gPlayerParty[i], MON_DATA_SPECIES) == SPECIES_NONE)
            return i;
    }
    return -1;
}

static bool8 HasEmptyPartyMailSlot(void)
{
    u8 i;

    for (i = 0; i < PARTY_SIZE; i++)
    {
        if (gSaveBlock1Ptr->mail[i].itemId == ITEM_NONE)
            return TRUE;
    }
    return FALSE;
}

static void SetMailTrainerId(struct Mail *mail, u32 trainerId)
{
    u8 i;

    for (i = 0; i < TRAINER_ID_LENGTH; i++)
        mail->trainerId[i] = trainerId >> (i * 8);
}

static void PrepareKenyaMail(struct Mail *mail)
{
    ClearMail(mail);
    memcpy(mail->words, sKenyaMailWords, sizeof(sKenyaMailWords));
    StringCopy(mail->playerName, sKenyaOtName);
    SetMailTrainerId(mail, 61225);
    mail->species = SPECIES_SPEAROW;
    mail->itemId = ITEM_RETRO_MAIL;
}

void Script_JohtoGiveNamedMon(struct ScriptContext *ctx)
{
    struct NamedGift gift;
    struct Pokemon mon;
    struct Mail mail;
    s8 partySlot;
    u16 giftId = ScriptReadHalfword(ctx);
    u32 personality;
    bool32 isShiny;

    Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE);
    gSpecialVar_Result = MON_CANT_GIVE;

    if (!GetNamedGift(giftId, &gift))
        return;

    partySlot = FindEmptyPartySlot();
    if (partySlot < 0)
        return;
    if (giftId == JOHTO_NAMED_GIFT_KENYA && !HasEmptyPartyMailSlot())
        return;

    if (giftId == JOHTO_NAMED_GIFT_DRATINI)
    {
        personality = GetMonPersonality(gift.species, MON_GENDER_RANDOM, NATURE_ADAMANT, RANDOM_UNOWN_LETTER);
        CreateMonWithIVs(&mon, gift.species, gift.level, personality, OTID_STRUCT_PLAYER_ID, USE_RANDOM_IVS);
        isShiny = TRUE;
        SetMonData(&mon, MON_DATA_IS_SHINY, &isShiny);
        SetMonMoveSlot(&mon, MOVE_EXTREME_SPEED, 0);
    }
    else
    {
        personality = Random32();
        CreateMonWithIVs(&mon, gift.species, gift.level, personality, OTID_STRUCT_PRESET(gift.otId), USE_RANDOM_IVS);
        if (gift.nickname != NULL)
            SetMonData(&mon, MON_DATA_NICKNAME, gift.nickname);
        SetMonData(&mon, MON_DATA_OT_NAME, gift.otName);
        SetMonData(&mon, MON_DATA_HELD_ITEM, &gift.item);
    }

    if (giftId == JOHTO_NAMED_GIFT_KENYA)
    {
        PrepareKenyaMail(&mail);
        if (GiveMailToMon(&mon, &mail) == MAIL_NONE)
            return;
    }

    CalculateMonStats(&mon);
    gSpecialVar_Result = GiveScriptedMonToPlayer(&mon, partySlot);
}

void Script_JohtoGiveOddEgg(struct ScriptContext *ctx)
{
    struct Pokemon mon;
    s8 partySlot;
    u16 which = VarGet(ScriptReadHalfword(ctx));
    enum Species species;
    bool32 isShiny;
    u8 friendship;

    Script_RequestEffects(SCREFF_V1 | SCREFF_SAVE);
    gSpecialVar_Result = MON_CANT_GIVE;

    if (which == 0 || which >= ARRAY_COUNT(sOddEggSpecies))
        return;
    species = sOddEggSpecies[which];
    partySlot = FindEmptyPartySlot();
    if (partySlot < 0)
        return;

    CreateEgg(&mon, species, TRUE);
    isShiny = RandomPercentage(RNG_NONE, 14);
    SetMonData(&mon, MON_DATA_IS_SHINY, &isShiny);
    friendship = min(gSpeciesInfo[species].eggCycles, 2);
    SetMonData(&mon, MON_DATA_FRIENDSHIP, &friendship);
    SetMonMoveSlot(&mon, MOVE_DIZZY_PUNCH, 1);
    CalculateMonStats(&mon);

    /* GiveScriptedMonToPlayer marks both dex flags, which is incorrect for an
     * unhatched egg. Match ScriptGiveEgg while retaining party-only behavior. */
    CopyMon(&gPlayerParty[partySlot], &mon, sizeof(mon));
    CalculatePlayerPartyCount();
    gSpecialVar_Result = MON_GIVEN_TO_PARTY;
}

void Script_JohtoMarkWildShiny(struct ScriptContext *ctx)
{
    bool32 isShiny = TRUE;

    (void)ctx;
    Script_RequestEffects(SCREFF_V1);
    if (GetMonData(&gEnemyParty[0], MON_DATA_SPECIES) == SPECIES_NONE)
        return;
    SetMonData(&gEnemyParty[0], MON_DATA_IS_SHINY, &isShiny);
}
