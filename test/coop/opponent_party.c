#include "global.h"
#include "battle.h"
#include "battle_main.h"
#include "battle_script_commands.h"
#include "coop/battle_runtime.h"
#include "coop/trainer_rewards.h"
#include "data.h"
#include "malloc.h"
#include "pokemon.h"
#include "random.h"
#include "string_util.h"
#include "constants/battle.h"
#include "constants/characters.h"
#include "constants/maps.h"
#include "constants/trainers.h"
#include "test/test.h"

/* A6: in a co-op battle a single-mon trainer fields a second mon at the
 * same level, built only from data both ROMs share (trainer data and the
 * manifest's battle ID and seed). */

#define COOP_OPPONENT_FLAGS (BATTLE_TYPE_TRAINER | BATTLE_TYPE_MULTI \
                             | BATTLE_TYPE_INGAME_PARTNER | BATTLE_TYPE_DOUBLE)

static const u8 sNameAlice[] = _("ALICE");
static const u8 sNameBob[] = _("BOB");

/* The test build has its own trainer table, so the trainers are built here
 * the way trainerproc emits them (Calvin, Fredrick, a four-mon leader and a
 * four-mon hiker with the same party). */
static const struct TrainerMon sSingleParty[] =
{
    {
        .species = SPECIES_POOCHYENA,
        .gender = TRAINER_MON_RANDOM_GENDER,
        .iv = TRAINER_PARTY_IVS(0, 0, 0, 0, 0, 0),
        .lvl = 5,
        .ball = POKEBALL_COUNT,
        .nature = NATURE_HARDY,
        .dynamaxLevel = MAX_DYNAMAX_LEVEL,
    },
};

static const struct TrainerMon sPairParty[] =
{
    { .species = SPECIES_MAKUHITA, .iv = TRAINER_PARTY_IVS(12, 12, 12, 12, 12, 12), .lvl = 30,
      .ball = POKEBALL_COUNT, .nature = NATURE_HARDY, .dynamaxLevel = MAX_DYNAMAX_LEVEL },
    { .species = SPECIES_MACHOKE, .iv = TRAINER_PARTY_IVS(12, 12, 12, 12, 12, 12), .lvl = 30,
      .ball = POKEBALL_COUNT, .nature = NATURE_HARDY, .dynamaxLevel = MAX_DYNAMAX_LEVEL },
};

static const struct TrainerMon sLeaderParty[] =
{
    { .species = SPECIES_GEODUDE, .lvl = 12, .ball = POKEBALL_COUNT, .nature = NATURE_HARDY, .dynamaxLevel = MAX_DYNAMAX_LEVEL },
    { .species = SPECIES_GEODUDE, .lvl = 12, .ball = POKEBALL_COUNT, .nature = NATURE_HARDY, .dynamaxLevel = MAX_DYNAMAX_LEVEL },
    { .species = SPECIES_NOSEPASS, .lvl = 15, .ball = POKEBALL_COUNT, .nature = NATURE_HARDY, .dynamaxLevel = MAX_DYNAMAX_LEVEL },
    { .species = SPECIES_ONIX, .lvl = 14, .ball = POKEBALL_COUNT, .nature = NATURE_HARDY, .dynamaxLevel = MAX_DYNAMAX_LEVEL },
};

static const struct Trainer sSingleTrainer =
{
    .trainerName = _("CALVIN"),
    .trainerClass = TRAINER_CLASS_YOUNGSTER,
    .gender = TRAINER_GENDER_MALE,
    .battleType = TRAINER_BATTLE_TYPE_SINGLES,
    .party = sSingleParty,
    .partySize = ARRAY_COUNT(sSingleParty),
};

static const struct Trainer sPairTrainer =
{
    .trainerName = _("FREDRICK"),
    .trainerClass = TRAINER_CLASS_EXPERT,
    .gender = TRAINER_GENDER_MALE,
    .battleType = TRAINER_BATTLE_TYPE_SINGLES,
    .party = sPairParty,
    .partySize = ARRAY_COUNT(sPairParty),
};

static const struct Trainer sLongTrainer =
{
    .trainerName = _("HIKER"),
    .trainerClass = TRAINER_CLASS_HIKER,
    .gender = TRAINER_GENDER_MALE,
    .battleType = TRAINER_BATTLE_TYPE_SINGLES,
    .party = sLeaderParty,
    .partySize = ARRAY_COUNT(sLeaderParty),
};

static const struct Trainer sLeaderTrainer =
{
    .trainerName = _("ROXANNE"),
    .trainerClass = TRAINER_CLASS_LEADER,
    .gender = TRAINER_GENDER_FEMALE,
    .battleType = TRAINER_BATTLE_TYPE_SINGLES,
    .party = sLeaderParty,
    .partySize = ARRAY_COUNT(sLeaderParty),
};

/* What differs between the two members' ROMs: player name and gender, the
 * map they stand on, and (outside the battle) the RNG state. */
struct LocalSaveFixture
{
    u8 name[PLAYER_NAME_LENGTH + 1];
    u8 gender;
    s8 mapGroup;
    s8 mapNum;
    rng_value_t rng;
};

static void SaveLocal(struct LocalSaveFixture *saved)
{
    memcpy(saved->name, gSaveBlock2Ptr->playerName, sizeof(saved->name));
    saved->gender = gSaveBlock2Ptr->playerGender;
    saved->mapGroup = gSaveBlock1Ptr->location.mapGroup;
    saved->mapNum = gSaveBlock1Ptr->location.mapNum;
    saved->rng = gRngValue;
}

static void RestoreLocal(const struct LocalSaveFixture *saved)
{
    memcpy(gSaveBlock2Ptr->playerName, saved->name, sizeof(saved->name));
    gSaveBlock2Ptr->playerGender = saved->gender;
    gSaveBlock1Ptr->location.mapGroup = saved->mapGroup;
    gSaveBlock1Ptr->location.mapNum = saved->mapNum;
    gRngValue = saved->rng;
}

static void SetLocal(const u8 *name, u8 gender, u16 map)
{
    memset(gSaveBlock2Ptr->playerName, EOS, PLAYER_NAME_LENGTH + 1);
    StringCopy(gSaveBlock2Ptr->playerName, name);
    gSaveBlock2Ptr->playerGender = gender;
    gSaveBlock1Ptr->location.mapGroup = MAP_GROUP(map);
    gSaveBlock1Ptr->location.mapNum = MAP_NUM(map);
}

static void MakeManifest(u8 *manifest, u8 idByte, u8 seedByte, u8 slot)
{
    u8 i;

    memset(manifest, 0, COOP_BATTLE_MANIFEST_SIZE);
    for (i = 0; i < COOP_BATTLE_ID_SIZE; i++)
        manifest[i] = idByte + i;
    for (i = 18; i < 50; i++)
        manifest[i] = seedByte ^ (i * 7);
    manifest[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_TRAINER;
    manifest[COOP_BATTLE_MANIFEST_MEMBER_SLOT_OFFSET] = slot;
}

static u8 CountMons(struct Pokemon *party)
{
    u8 i;
    u8 count = 0;

    for (i = 0; i < PARTY_SIZE; i++)
        if (GetMonData(&party[i], MON_DATA_SPECIES) != SPECIES_NONE)
            count++;
    return count;
}

TEST("Cloud Coop opponent seed hashes only bytes both ROMs share")
{
    u8 member0[COOP_BATTLE_MANIFEST_SIZE];
    u8 member1[COOP_BATTLE_MANIFEST_SIZE];
    u8 other[COOP_BATTLE_MANIFEST_SIZE];
    u32 seed;
    u32 seed0 = 0;

    MakeManifest(member0, 0x40, 0x11, 0);
    MakeManifest(member1, 0x40, 0x11, 1);
    /* Each ROM sees its own member slot; the turn bytes are not shared
     * either. Neither may move the seed. */
    member1[16] = 3;
    member1[50] = 0x99;
    EXPECT_EQ(CoopBattleRuntime_DeriveOpponentSeed(member0, TRAINER_CALVIN_1),
              CoopBattleRuntime_DeriveOpponentSeed(member1, TRAINER_CALVIN_1));

    MakeManifest(other, 0x41, 0x11, 0);
    EXPECT_NE(CoopBattleRuntime_DeriveOpponentSeed(member0, TRAINER_CALVIN_1),
              CoopBattleRuntime_DeriveOpponentSeed(other, TRAINER_CALVIN_1));
    MakeManifest(other, 0x40, 0x12, 0);
    EXPECT_NE(CoopBattleRuntime_DeriveOpponentSeed(member0, TRAINER_CALVIN_1),
              CoopBattleRuntime_DeriveOpponentSeed(other, TRAINER_CALVIN_1));
    EXPECT_NE(CoopBattleRuntime_DeriveOpponentSeed(member0, TRAINER_CALVIN_1),
              CoopBattleRuntime_DeriveOpponentSeed(member0, TRAINER_BILLY));

    /* The runtime reads the same bytes from the received manifest. */
    CoopBattleRuntime_Init();
    EXPECT(!CoopBattleRuntime_GetOpponentSeed(TRAINER_CALVIN_1, &seed));
    CoopBattleRuntime_OnSessionReady(61);
    member0[COOP_BATTLE_MANIFEST_KIND_OFFSET] = COOP_BATTLE_MANIFEST_KIND_FRIENDLY;
    member0[COOP_BATTLE_MANIFEST_RULES_OFFSET] = COOP_BATTLE_FRIENDLY_SINGLES;
    member0[COOP_BATTLE_MANIFEST_RULES_OFFSET + 2] = 1;
    EXPECT_EQ(CoopBattleRuntime_ReceiveManifest(member0, sizeof(member0)),
              COOP_BATTLE_INBOUND_ACCEPTED);
    EXPECT(CoopBattleRuntime_GetOpponentSeed(TRAINER_CALVIN_1, &seed0));
    EXPECT_EQ(seed0, CoopBattleRuntime_DeriveOpponentSeed(member1, TRAINER_CALVIN_1));
    CoopBattleRuntime_Init();
}

TEST("Cloud Coop single-mon trainer fields a second mon at the same level")
{
    struct Pokemon *party = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE];
    struct LocalSaveFixture saved;
    const struct Trainer *calvin = &sSingleTrainer;

    SaveLocal(&saved);
    EXPECT_EQ((u32)calvin->partySize, 1);
    MakeManifest(manifest, 0x20, 0x33, 0);
    SeedRng(0x1234);
    CreateCoopTrainerParty(party, calvin, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest, TRAINER_CALVIN_1));
    EXPECT_EQ(CountMons(party), 2);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_SPECIES), SPECIES_POOCHYENA);
    EXPECT_EQ(GetMonData(&party[1], MON_DATA_SPECIES), SPECIES_POOCHYENA);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_LEVEL), 5);
    EXPECT_EQ(GetMonData(&party[1], MON_DATA_LEVEL), 5);
    EXPECT_NE(GetMonData(&party[1], MON_DATA_HP), 0);
    EXPECT_NE(GetMonData(&party[0], MON_DATA_PERSONALITY),
              GetMonData(&party[1], MON_DATA_PERSONALITY));
    /* The trainer data's IVs apply to the added mon too. */
    EXPECT_EQ(GetMonData(&party[1], MON_DATA_ATK_IV), 0);
    Free(party);
    RestoreLocal(&saved);
}

TEST("Cloud Coop second opponent mon is byte-identical on both ROMs")
{
    struct Pokemon *member0 = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    struct Pokemon *member1 = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    struct Pokemon *vanilla = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    u8 manifest0[COOP_BATTLE_MANIFEST_SIZE];
    u8 manifest1[COOP_BATTLE_MANIFEST_SIZE];
    struct LocalSaveFixture saved;
    rng_value_t afterCoop;
    const struct Trainer *calvin = &sSingleTrainer;

    SaveLocal(&saved);
    MakeManifest(manifest0, 0x50, 0x0A, 0);
    MakeManifest(manifest1, 0x50, 0x0A, 1);

    /* Same battle, two ROMs with different players, maps and member slots.
     * The battle RNG is seeded from the manifest on both. */
    SetLocal(sNameAlice, MALE, MAP_ROUTE102);
    SeedRng(0xC0FFEE);
    CreateCoopTrainerParty(member0, calvin, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest0, TRAINER_CALVIN_1));
    afterCoop = gRngValue;
    SetLocal(sNameBob, FEMALE, MAP_ROUTE103);
    SeedRng(0xC0FFEE);
    CreateCoopTrainerParty(member1, calvin, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest1, TRAINER_CALVIN_1));
    EXPECT_EQ(memcmp(member0, member1, PARTY_SIZE * sizeof(struct Pokemon)), 0);

    /* The added mon neither reads nor advances the battle RNG: the stream
     * continues exactly as after the vanilla one-mon party. */
    SeedRng(0xC0FFEE);
    CreateNPCTrainerPartyFromTrainer(vanilla, calvin, TRUE, COOP_OPPONENT_FLAGS);
    EXPECT_EQ(memcmp(&afterCoop, &gRngValue, sizeof(gRngValue)), 0);
    SeedRng(0xBAD5EED);
    CreateCoopTrainerParty(member1, calvin, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest1, TRAINER_CALVIN_1));
    EXPECT_EQ(memcmp(&member0[1], &member1[1], sizeof(member0[1])), 0);
    Free(vanilla);
    Free(member1);
    Free(member0);
    RestoreLocal(&saved);
}

TEST("Cloud Coop opponent mons record no met location through the V2 codec")
{
    struct Pokemon *party = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE];
    struct LocalSaveFixture saved;
    const struct Trainer *calvin = &sSingleTrainer;
    u16 location;
    u32 i;

    SaveLocal(&saved);
    MakeManifest(manifest, 0x70, 0x0C, 0);
    SetLocal(sNameAlice, MALE, MAP_ROUTE102);
    SeedRng(5);
    CreateCoopTrainerParty(party, calvin, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest, TRAINER_CALVIN_1));
    EXPECT_EQ(CountMons(party), 2);
    for (i = 0; i < 2; i++)
    {
        /* Never the local map section, and never MAPSEC_NONE truncated to a
         * byte (which is a real section): V2 "none" is the low byte 250 with
         * clear marker bits. */
        EXPECT(GetBoxMonMetLocationV2(&party[i].box, &location));
        EXPECT_EQ(location, MET_LOCATION_V2_NONE);
        EXPECT_EQ(GetMonData(&party[i], MON_DATA_MET_LOCATION), 250);
        EXPECT_NE(GetMonData(&party[i], MON_DATA_MET_LOCATION), (u8)MAPSEC_NONE);
    }
    Free(party);
    RestoreLocal(&saved);
}

TEST("Cloud Coop second opponent mon changes with the battle ID")
{
    struct Pokemon *first = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    struct Pokemon *second = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE];
    struct LocalSaveFixture saved;
    const struct Trainer *calvin = &sSingleTrainer;

    SaveLocal(&saved);
    MakeManifest(manifest, 0x60, 0x0B, 0);
    SeedRng(77);
    CreateCoopTrainerParty(first, calvin, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest, TRAINER_CALVIN_1));
    MakeManifest(manifest, 0x61, 0x0B, 0);
    SeedRng(77);
    CreateCoopTrainerParty(second, calvin, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest, TRAINER_CALVIN_1));
    EXPECT_EQ(memcmp(&first[0], &second[0], sizeof(first[0])), 0);
    EXPECT_NE(GetMonData(&first[1], MON_DATA_PERSONALITY),
              GetMonData(&second[1], MON_DATA_PERSONALITY));
    EXPECT_EQ(GetMonData(&second[1], MON_DATA_LEVEL), 5);
    Free(second);
    Free(first);
    RestoreLocal(&saved);
}

static const struct TrainerMon sPoolParty[] =
{
    { .species = SPECIES_ZIGZAGOON, .lvl = 10, .ball = POKEBALL_COUNT, .nature = NATURE_HARDY },
    { .species = SPECIES_WURMPLE, .lvl = 3, .ball = POKEBALL_COUNT, .nature = NATURE_HARDY },
    { .species = SPECIES_TAILLOW, .lvl = 7, .ball = POKEBALL_COUNT, .nature = NATURE_HARDY },
    { .species = SPECIES_LOTAD, .lvl = 12, .ball = POKEBALL_COUNT, .nature = NATURE_HARDY },
};

static const struct Trainer sPoolTrainer =
{
    .trainerName = _("POOL"),
    .trainerClass = TRAINER_CLASS_YOUNGSTER,
    .gender = TRAINER_GENDER_MALE,
    .battleType = TRAINER_BATTLE_TYPE_SINGLES,
    .party = sPoolParty,
    .partySize = 1,
    .poolSize = ARRAY_COUNT(sPoolParty),
};

static bool8 IsPoolSpecies(u32 species, u8 *level)
{
    u8 i;

    for (i = 0; i < ARRAY_COUNT(sPoolParty); i++)
    {
        if (sPoolParty[i].species == species)
        {
            *level = sPoolParty[i].lvl;
            return TRUE;
        }
    }
    return FALSE;
}

TEST("Cloud Coop pool trainer draws its second mon from the rest of its pool")
{
    struct Pokemon *party = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    u8 manifest[COOP_BATTLE_MANIFEST_SIZE];
    struct LocalSaveFixture saved;
    u32 seenSecond = 0;
    u32 seed;
    u8 firstLevel = 0;
    u8 secondLevel = 0;
    u8 i;

    SaveLocal(&saved);
    for (i = 0; i < 16; i++)
    {
        u32 firstSpecies;
        u32 secondSpecies;

        MakeManifest(manifest, 0x70 + i, i, 0);
        seed = CoopBattleRuntime_DeriveOpponentSeed(manifest, TRAINER_CALVIN_1);
        SeedRng(1000 + i);
        CreateCoopTrainerParty(party, &sPoolTrainer, COOP_OPPONENT_FLAGS, seed);
        EXPECT_EQ(CountMons(party), 2);
        firstSpecies = GetMonData(&party[0], MON_DATA_SPECIES);
        secondSpecies = GetMonData(&party[1], MON_DATA_SPECIES);
        EXPECT(IsPoolSpecies(firstSpecies, &firstLevel));
        EXPECT(IsPoolSpecies(secondSpecies, &secondLevel));
        EXPECT_NE(firstSpecies, secondSpecies);
        /* The first mon keeps its own level; the added one matches it. */
        EXPECT_EQ(GetMonData(&party[0], MON_DATA_LEVEL), firstLevel);
        EXPECT_EQ(GetMonData(&party[1], MON_DATA_LEVEL), firstLevel);
        EXPECT_NE(GetMonData(&party[1], MON_DATA_HP), 0);
        seenSecond |= 1u << (secondSpecies & 31);
    }
    /* The pick is seeded, not fixed. */
    EXPECT_NE(seenSecond & (seenSecond - 1), 0);
    Free(party);
    RestoreLocal(&saved);
}

/* A co-op party equals the vanilla one built with the same half-team rule
 * (up to OT data, which CreateCoopTrainerParty takes from the trainer). */
static void ExpectCoopPartyMatchesVanilla(const struct Trainer *trainer, bool32 halfTeam)
{
    struct Pokemon *coop = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    struct Pokemon *vanilla = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    u8 expected = halfTeam && trainer->partySize > PARTY_SIZE / 2 ? PARTY_SIZE / 2
                                                                  : trainer->partySize;
    u32 i;
    u32 j;

    SeedRng(4242);
    CreateNPCTrainerPartyFromTrainer(vanilla, trainer, halfTeam, COOP_OPPONENT_FLAGS);
    SeedRng(4242);
    CreateCoopTrainerParty(coop, trainer, COOP_OPPONENT_FLAGS, 0xDEADBEEF);
    EXPECT_EQ(CountMons(vanilla), expected);
    EXPECT_EQ(CountMons(coop), expected);
    for (i = 0; i < PARTY_SIZE; i++)
    {
        EXPECT_EQ(GetMonData(&coop[i], MON_DATA_SPECIES), GetMonData(&vanilla[i], MON_DATA_SPECIES));
        EXPECT_EQ(GetMonData(&coop[i], MON_DATA_LEVEL), GetMonData(&vanilla[i], MON_DATA_LEVEL));
        EXPECT_EQ(GetMonData(&coop[i], MON_DATA_PERSONALITY), GetMonData(&vanilla[i], MON_DATA_PERSONALITY));
        EXPECT_EQ(GetMonData(&coop[i], MON_DATA_OT_ID), GetMonData(&vanilla[i], MON_DATA_OT_ID));
        EXPECT_EQ(GetMonData(&coop[i], MON_DATA_HELD_ITEM), GetMonData(&vanilla[i], MON_DATA_HELD_ITEM));
        EXPECT_EQ(GetMonData(&coop[i], MON_DATA_IVS), GetMonData(&vanilla[i], MON_DATA_IVS));
        EXPECT_EQ(GetMonData(&coop[i], MON_DATA_MAX_HP), GetMonData(&vanilla[i], MON_DATA_MAX_HP));
        for (j = 0; j < MAX_MON_MOVES; j++)
            EXPECT_EQ(GetMonData(&coop[i], MON_DATA_MOVE1 + j), GetMonData(&vanilla[i], MON_DATA_MOVE1 + j));
    }
    Free(vanilla);
    Free(coop);
}

TEST("Cloud Coop trainers with two or more mons keep their co-op party")
{
    struct LocalSaveFixture saved;

    SaveLocal(&saved);
    ExpectCoopPartyMatchesVanilla(&sPairTrainer, TRUE);
    /* Longer parties keep the three-mon cap; gym leaders do not. */
    ExpectCoopPartyMatchesVanilla(&sLongTrainer, TRUE);
    ExpectCoopPartyMatchesVanilla(&sLeaderTrainer, FALSE);
    RestoreLocal(&saved);
}

TEST("Cloud Coop gym leader fields its full team, byte-identical on both ROMs")
{
    struct Pokemon *member0 = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    struct Pokemon *member1 = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    u8 manifest0[COOP_BATTLE_MANIFEST_SIZE];
    u8 manifest1[COOP_BATTLE_MANIFEST_SIZE];
    struct LocalSaveFixture saved;
    u8 otName[PLAYER_NAME_LENGTH + 1];

    SaveLocal(&saved);
    EXPECT_GT((u32)sLeaderTrainer.partySize, PARTY_SIZE / 2);
    MakeManifest(manifest0, 0x30, 0x44, 0);
    MakeManifest(manifest1, 0x30, 0x44, 1);
    SetLocal(sNameAlice, MALE, MAP_RUSTBORO_CITY_GYM);
    SeedRng(0x600D);
    CreateCoopTrainerParty(member0, &sLeaderTrainer, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest0, TRAINER_ROXANNE_1));
    SetLocal(sNameBob, FEMALE, MAP_ROUTE104);
    SeedRng(0x600D);
    CreateCoopTrainerParty(member1, &sLeaderTrainer, COOP_OPPONENT_FLAGS,
                           CoopBattleRuntime_DeriveOpponentSeed(manifest1, TRAINER_ROXANNE_1));
    EXPECT_EQ(CountMons(member0), sLeaderTrainer.partySize);
    EXPECT_EQ(memcmp(member0, member1, PARTY_SIZE * sizeof(struct Pokemon)), 0);
    /* Every mon, not only the first three, carries the leader as OT. */
    GetMonData(&member0[PARTY_SIZE / 2], MON_DATA_OT_NAME, otName);
    EXPECT_EQ(otName[0], sLeaderTrainer.trainerName[0]);
    EXPECT_EQ(GetMonData(&member0[PARTY_SIZE / 2], MON_DATA_SPECIES), SPECIES_ONIX);
    Free(member1);
    Free(member0);
    RestoreLocal(&saved);
}

TEST("Cloud Coop second opponent mon never reaches a vanilla battle")
{
    struct Pokemon *party = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    struct LocalSaveFixture saved;
    u8 otName[PLAYER_NAME_LENGTH + 1];

    SaveLocal(&saved);
    SetLocal(sNameAlice, MALE, MAP_ROUTE102);
    EXPECT(!CoopBattleRuntime_IsEngineActive());
    SeedRng(9);
    CreateNPCTrainerPartyFromTrainer(party, &sSingleTrainer, FALSE, BATTLE_TYPE_TRAINER);
    EXPECT_EQ(CountMons(party), 1);
    EXPECT_EQ(GetMonData(&party[0], MON_DATA_LEVEL), 5);
    /* Vanilla trainer mons still carry the local player as OT. */
    GetMonData(&party[0], MON_DATA_OT_NAME, otName);
    EXPECT_EQ(memcmp(otName, gSaveBlock2Ptr->playerName, PLAYER_NAME_LENGTH), 0);
    SeedRng(9);
    CreateNPCTrainerPartyFromTrainer(party, &sPoolTrainer, FALSE, BATTLE_TYPE_TRAINER);
    EXPECT_EQ(CountMons(party), 1);
    Free(party);
    RestoreLocal(&saved);
}

TEST("Cloud Coop prize money ignores the added opponent mon")
{
    struct Pokemon *enemy = AllocZeroed(PARTY_SIZE * sizeof(struct Pokemon));
    struct LocalSaveFixture saved;
    u32 prize;
    u32 doubled;

    /* The prize reads the trainer's own data (its last mon's level), never
     * the battle party, so the added mon cannot change it. */
    SaveLocal(&saved);
    memcpy(enemy, gParties[B_TRAINER_1], PARTY_SIZE * sizeof(struct Pokemon));
    ZeroPartyMons(gParties[B_TRAINER_1]);
    prize = CoopTrainerRewards_GetPrizeMoney(TRAINER_CALVIN_1, 1);
    doubled = CoopTrainerRewards_GetPrizeMoney(TRAINER_CALVIN_1, 2);
    EXPECT_EQ(prize, GetTrainerPrizeMoney(TRAINER_CALVIN_1, 1,
                                          GetTrainerBattleType(TRAINER_CALVIN_1) == TRAINER_BATTLE_TYPE_DOUBLES));

    SeedRng(5);
    CreateCoopTrainerParty(gParties[B_TRAINER_1], &sSingleTrainer, COOP_OPPONENT_FLAGS, 0x5EED);
    EXPECT_EQ(CountMons(gParties[B_TRAINER_1]), 2);
    EXPECT_EQ(CoopTrainerRewards_GetPrizeMoney(TRAINER_CALVIN_1, 1), prize);
    EXPECT_EQ(CoopTrainerRewards_GetPrizeMoney(TRAINER_CALVIN_1, 2), doubled);
    memcpy(gParties[B_TRAINER_1], enemy, PARTY_SIZE * sizeof(struct Pokemon));
    Free(enemy);
    RestoreLocal(&saved);
}
