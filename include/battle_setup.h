#ifndef GUARD_BATTLE_SETUP_H
#define GUARD_BATTLE_SETUP_H

#include "battle_transition.h"
#include "gym_leader_rematch.h"

#define REMATCHES_COUNT 5

struct Pokemon;

struct RematchTrainer
{
    u16 trainerIds[REMATCHES_COUNT];
    u16 mapGroup;
    u16 mapNum;
};

/*
the layout of the first byte can be confusing here
isDoubleBattle is the least lsb. msb is in the mode.
*/
typedef union PACKED TrainerBattleParameter
{
    struct PACKED _TrainerBattleParameter
    {
        u8 isDoubleBattle:1;
        u8 isRematch:1;
        u8 playMusicA:1;
        u8 playMusicB:1;
        u8 mode:4;
        u8 objEventLocalIdA;
        u16 opponentA;
        u8 *introTextA;
        u8 *defeatTextA;
        u8 *battleScriptRetAddrA;
        u8 objEventLocalIdB;
        u16 opponentB;
        u8 *introTextB;
        u8 *defeatTextB;
        u8 *battleScriptRetAddrB;
        u8 *victoryText;
        u8 *cannotBattleText;
        u8 rivalBattleFlags;
    } params;
    u8 data[sizeof(struct _TrainerBattleParameter)];
} TrainerBattleParameter;

extern const struct RematchTrainer gRematchTable[REMATCH_TABLE_ENTRIES];

extern TrainerBattleParameter gTrainerBattleParameter;
extern u16 gPartnerTrainerId;

#define TRAINER_BATTLE_PARAM gTrainerBattleParameter.params

void BattleSetup_StartWildBattle(void);
void BattleSetup_StartDoubleWildBattle(void);
void BattleSetup_StartBattlePikeWildBattle(void);
void BattleSetup_StartRoamerBattle(void);
void StartWallyTutorialBattle(void);
void BattleSetup_StartScriptedWildBattle(void);
void BattleSetup_StartScriptedDoubleWildBattle(void);
void BattleSetup_StartLatiBattle(void);
void BattleSetup_StartLegendaryBattle(void);
void StartGroudonKyogreBattle(void);
void StartRegiBattle(void);
enum BattleEnvironments BattleSetup_GetEnvironmentId(void);
enum BattleTransition GetWildBattleTransition(void);
enum BattleTransition GetTrainerBattleTransition(void);
enum BattleTransition GetSpecialBattleTransition(enum BattleTransitionGroup id);
void ChooseStarter(void);
void ResetTrainerOpponentIds(void);
void SetMapVarsToTrainerA(void);
void SetMapVarsToTrainerB(void);
const u8 *BattleSetup_ConfigureTrainerBattle(const u8 *data);
const u8* BattleSetup_ConfigureFacilityTrainerBattle(u8 facility, const u8* scriptEndPtr);
void ConfigureAndSetUpOneTrainerBattle(u8 trainerObjEventId, const u8 *trainerScript);
void ConfigureTwoTrainersBattle(u8 trainerObjEventId, const u8 *trainerScript);
void SetUpTwoTrainersBattle(void);
bool32 GetTrainerFlagFromScriptPointer(const u8 *data);
bool32 GetRematchFromScriptPointer(const u8 *data);
void SetTrainerFacingDirection(void);
u8 GetTrainerBattleMode(void);
bool8 GetTrainerFlag(void);
bool8 HasTrainerBeenFought(u16 trainerId);
void SetTrainerFlag(u16 trainerId);
void ClearTrainerFlag(u16 trainerId);
void ToggleTrainerFlag(u16 trainerId);
/* dotrainerbattle entry. An eligible co-op encounter parks the trainer script
 * here (context stopped, controls locked) until CoopBattleConsent_Poll starts
 * either the co-op battle or BattleSetup_StartVanillaTrainerBattle. */
void BattleSetup_StartTrainerBattle(void);
/* The unmodified single-player trainer battle start. It reads the trainer
 * parameters and approaching-trainer count left by the script, so a deferred
 * call behaves exactly like the original dotrainerbattle. */
void BattleSetup_StartVanillaTrainerBattle(void);
/* Starts the server-authorized co-op trainer battle path. The caller must
 * have a validated trainer manifest and a complete peer party snapshot. */
bool8 BattleSetup_StartCoopTrainerBattle(void);
/* Starts a released friendly battle against the grouped partner. team is
 * this ROM's picked team (the snapshot it sent); the partner's team comes
 * from the peer snapshot. The whole party is restored afterwards. */
bool8 BattleSetup_StartCoopFriendlyBattle(const struct Pokemon *team, u8 count);
/* special BattleSetup_StartRematchBattle: the same co-op hook as
 * BattleSetup_StartTrainerBattle, for the match-call rematch modes. */
void BattleSetup_StartRematchBattle(void);
/* The unmodified rematch start, used directly or as the co-op fallback. */
void BattleSetup_StartVanillaRematchBattle(void);
bool8 IsRematchBattleMode(u8 mode);
/* TRUE when trainerId is a later gRematchTable entry (CALVIN_2 ... 5,
 * ROXANNE_2 ... 5), with the table's first-battle trainer in *baseTrainerId. */
bool8 BattleSetup_GetRematchBaseTrainer(u16 trainerId, u16 *baseTrainerId);
/* Exactly what a vanilla rematch win records for this trainer: match-call
 * registration, the trainer flag and the cleared "wants a rematch" state. */
void BattleSetup_ApplyCoopRematchWin(u16 trainerId);
/* The vanilla end-of-battle match-call registration for this trainer. */
void BattleSetup_RegisterTrainerInMatchCall(u16 trainerId);
void ShowTrainerIntroSpeech(void);
const u8 *BattleSetup_GetScriptAddrAfterBattle(void);
const u8 *BattleSetup_GetTrainerPostBattleScript(void);
void ShowTrainerCantBattleSpeech(void);
void PlayTrainerEncounterMusic(void);
const u8 *GetTrainerALoseText(void);
const u8 *GetTrainerBLoseText(void);
const u8 *GetTrainerWonSpeech(void);
void UpdateRematchIfDefeated(s32 rematchTableId);
void ClearCurrentTrainerWantRematchVsSeeker(void);
void IncrementRematchStepCounter(void);
void TryUpdateRandomTrainerRematches(u16 mapGroup, u16 mapNum);
bool32 DoesSomeoneWantRematchIn(u16 mapGroup, u16 mapNum);
bool32 IsRematchTrainerIn(u16 mapGroup, u16 mapNum);
u16 GetLastBeatenRematchTrainerId(u16 trainerId);
bool8 ShouldTryRematchBattle(void);
bool8 ShouldTryRematchBattleForTrainerId(u16 trainerId);
bool8 IsTrainerReadyForRematch(void);
void ShouldTryGetTrainerScript(void);
u16 CountMaxPossibleRematch(u16 trainerId);
u16 CountBattledRematchTeams(u16 trainerId);
void TrainerBattleLoadArgs(const u8 *data);
void TrainerBattleLoadArgsTrainerA(const u8 *data);
void TrainerBattleLoadArgsTrainerB(const u8 *data);
void TrainerBattleLoadArgsSecondTrainer(const u8 *data);
void InitTrainerBattleParameter(void);

void DoStandardWildBattle_Debug(void);
void BattleSetup_StartTrainerBattle_Debug(void);
s32 TrainerIdToRematchTableId(const struct RematchTrainer *table, u16 trainerId);
s32 FirstBattleTrainerIdToRematchTableId(const struct RematchTrainer *table, u16 trainerId);
u16 GetRematchTrainerIdFromTable(const struct RematchTrainer *table, u16 firstBattleTrainerId);
u8 GetRivalBattleFlags(void);

#endif // GUARD_BATTLE_SETUP_H
