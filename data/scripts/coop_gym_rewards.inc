@ Co-op gym wins on the PARTNER's ROM (A8).
@
@ The requester's ROM runs the leader's own post-battle script. The partner's
@ ROM has no such script, so when the partner is at the same story point (it
@ lacks this badge and holds every earlier one) CoopTrainerRewards_Apply
@ gives it the same things:
@
@ - CoopGym_EventScript_Grant<Gym> is run at once (RunScriptImmediately) and
@   holds every flag, var, gym-trainer flag and special of the vanilla
@   <Leader>Defeated script in data/maps/<Gym>/scripts.inc, in its order;
@ - the TM and its FLAG_RECEIVED_TM_* are given in C (sHoennGyms), exactly
@   like the vanilla Give<TM> path: a full bag leaves the flag clear, so the
@   leader hands the TM over when talked to later;
@ - the leader's trainer flag, match-call registration and prize are paid in
@   C, as CB2_EndTrainerBattle would.
@
@ Left out on purpose, because they only redraw the current map or play a
@ cutscene with the requester's leader: Dewford's flash animation, the
@ Mauville puzzle reset (MauvilleGymDeactivatePuzzle / DrawWholeMapView),
@ Petalburg's door metatiles and Wally's father walking the player out of
@ the gym (the warp to Petalburg City). The map scripts restore the first
@ three from the flags on the next load; Petalburg City's frame script plays
@ the walk to Wally's house when the partner steps out of the gym
@ (VAR_PETALBURG_CITY_STATE = 4). The leaders' match-call registration
@ messages are skipped; their FLAG_ENABLE_*_MATCH_CALL flags are set.
@
@ CoopGym_EventScript_Notice<Gym> is the partner's presentation only: the
@ vanilla "received the badge" text and fanfare, then the TM message.

CoopGym_EventScript_GrantRustboro::
	setflag FLAG_DEFEATED_RUSTBORO_GYM
	setflag FLAG_BADGE01_GET
	setvar VAR_RUSTBORO_CITY_STATE, 1
	addvar VAR_PETALBURG_GYM_STATE, 1
	setvar VAR_0x8008, 1
	call Common_EventScript_SetGymTrainers
	call_if_eq VAR_PETALBURG_GYM_STATE, 6, Common_EventScript_ReadyPetalburgGymForBattle
	end

CoopGym_EventScript_GrantDewford::
	setflag FLAG_DEFEATED_DEWFORD_GYM
	setflag FLAG_BADGE02_GET
	addvar VAR_PETALBURG_GYM_STATE, 1
	call_if_eq VAR_PETALBURG_GYM_STATE, 6, Common_EventScript_ReadyPetalburgGymForBattle
	setvar VAR_0x8008, 2
	call Common_EventScript_SetGymTrainers
	setflag FLAG_ENABLE_BRAWLY_MATCH_CALL
	setvar VAR_ROXANNE_CALL_STEP_COUNTER, 0
	setflag FLAG_ENABLE_ROXANNE_FIRST_CALL
	end

CoopGym_EventScript_GrantMauville::
	setvar VAR_SLATEPORT_OUTSIDE_MUSEUM_STATE, 3
	clearflag FLAG_HIDE_VERDANTURF_TOWN_SCOTT
	setflag FLAG_DEFEATED_MAUVILLE_GYM
	setflag FLAG_BADGE03_GET
	addvar VAR_PETALBURG_GYM_STATE, 1
	call_if_eq VAR_PETALBURG_GYM_STATE, 6, Common_EventScript_ReadyPetalburgGymForBattle
	setvar VAR_0x8008, 3
	call Common_EventScript_SetGymTrainers
	setflag FLAG_ENABLE_WATTSON_MATCH_CALL
	end

CoopGym_EventScript_GrantLavaridge::
	setflag FLAG_WHITEOUT_TO_LAVARIDGE
	setflag FLAG_DEFEATED_LAVARIDGE_GYM
	setflag FLAG_BADGE04_GET
	addvar VAR_PETALBURG_GYM_STATE, 1
	call_if_eq VAR_PETALBURG_GYM_STATE, 6, Common_EventScript_ReadyPetalburgGymForBattle
	setvar VAR_0x8008, 4
	call Common_EventScript_SetGymTrainers
	setflag FLAG_HIDE_VERDANTURF_TOWN_WANDAS_HOUSE_WALLY
	setvar VAR_LAVARIDGE_TOWN_STATE, 1
	setflag FLAG_ENABLE_FLANNERY_MATCH_CALL
	end

@ PetalburgCity_Gym_EventScript_NormanBattle after trainerbattle_no_intro,
@ through the end of PetalburgCity_Gym_EventScript_WallysDadArrives.
CoopGym_EventScript_GrantPetalburg::
	setflag FLAG_DEFEATED_PETALBURG_GYM
	setvar VAR_PETALBURG_GYM_STATE, 7
	setflag FLAG_BADGE05_GET
	special ResetHealLocationFromDewford
	call EventScript_HideMrBriney
	setflag FLAG_HIDE_MAUVILLE_GYM_WATTSON
	clearflag FLAG_HIDE_MAUVILLE_CITY_WATTSON
	setvar VAR_0x8008, 5
	call Common_EventScript_SetGymTrainers
	clearflag FLAG_HIDE_DEWFORD_HALL_SLUDGE_BOMB_MAN
	@ removeobject LOCALID_PETALBURG_GYM_WALLYS_DAD sets his hide flag.
	setflag FLAG_HIDE_PETALBURG_GYM_WALLYS_DAD
	setvar VAR_PETALBURG_CITY_STATE, 4
	clearflag FLAG_HIDE_PETALBURG_CITY_WALLYS_DAD
	end

CoopGym_EventScript_GrantFortree::
	setflag FLAG_DEFEATED_FORTREE_GYM
	setflag FLAG_BADGE06_GET
	setvar VAR_0x8008, 6
	call Common_EventScript_SetGymTrainers
	setflag FLAG_ENABLE_WINONA_MATCH_CALL
	setvar VAR_SCOTT_FORTREE_CALL_STEP_COUNTER, 0
	setflag FLAG_SCOTT_CALL_FORTREE_GYM
	end

CoopGym_EventScript_GrantMossdeep::
	setflag FLAG_DEFEATED_MOSSDEEP_GYM
	setflag FLAG_BADGE07_GET
	setflag FLAG_HIDE_AQUA_HIDEOUT_GRUNTS
	clearflag FLAG_HIDE_SLATEPORT_CITY_STERNS_SHIPYARD_MR_BRINEY
	clearflag FLAG_UNKNOWN_0x393
	setvar VAR_0x8008, 7
	call Common_EventScript_SetGymTrainers
	clearflag FLAG_HIDE_SLATEPORT_CITY_HARBOR_PATRONS
	clearflag FLAG_HIDE_MOSSDEEP_CITY_TEAM_MAGMA
	clearflag FLAG_HIDE_MOSSDEEP_CITY_SPACE_CENTER_1F_TEAM_MAGMA
	clearflag FLAG_HIDE_MOSSDEEP_CITY_SPACE_CENTER_2F_TEAM_MAGMA
	clearflag FLAG_HIDE_MOSSDEEP_CITY_SPACE_CENTER_2F_STEVEN
	setvar VAR_MOSSDEEP_CITY_STATE, 1
	setflag FLAG_HIDE_MOSSDEEP_CITY_SPACE_CENTER_1F_STEVEN
	setvar VAR_MOSSDEEP_SPACE_CENTER_STATE, 1
	setflag FLAG_ENABLE_TATE_AND_LIZA_MATCH_CALL
	end

CoopGym_EventScript_GrantSootopolis::
	setflag FLAG_DEFEATED_SOOTOPOLIS_GYM
	setflag FLAG_BADGE08_GET
	setflag FLAG_HIDE_SOOTOPOLIS_CITY_RESIDENTS
	setflag FLAG_HIDE_SOOTOPOLIS_CITY_STEVEN
	setflag FLAG_HIDE_SOOTOPOLIS_CITY_WALLACE
	setvar VAR_SOOTOPOLIS_CITY_STATE, 6
	clearflag FLAG_HIDE_SOOTOPOLIS_CITY_MAN_1
	setvar VAR_0x8008, 8
	call Common_EventScript_SetGymTrainers
	setflag FLAG_ENABLE_JUAN_MATCH_CALL
	end

CoopGym_EventScript_NoticeRustboro::
	lockall
	message RustboroCity_Gym_Text_ReceivedStoneBadge
	goto CoopGym_EventScript_Notice

CoopGym_EventScript_NoticeDewford::
	lockall
	message DewfordTown_Gym_Text_ReceivedKnuckleBadge
	goto CoopGym_EventScript_Notice

CoopGym_EventScript_NoticeMauville::
	lockall
	message MauvilleCity_Gym_Text_ReceivedDynamoBadge
	goto CoopGym_EventScript_Notice

CoopGym_EventScript_NoticeLavaridge::
	lockall
	message LavaridgeTown_Gym_1F_Text_ReceivedHeatBadge
	goto CoopGym_EventScript_Notice

CoopGym_EventScript_NoticePetalburg::
	lockall
	message PetalburgCity_Gym_Text_ReceivedBalanceBadge
	goto CoopGym_EventScript_Notice

CoopGym_EventScript_NoticeFortree::
	lockall
	message FortreeCity_Gym_Text_ReceivedFeatherBadge
	goto CoopGym_EventScript_Notice

CoopGym_EventScript_NoticeMossdeep::
	lockall
	message MossdeepCity_Gym_Text_ReceivedMindBadge
	goto CoopGym_EventScript_Notice

CoopGym_EventScript_NoticeSootopolis::
	lockall
	message SootopolisCity_Gym_1F_Text_ReceivedRainBadge
	goto CoopGym_EventScript_Notice

CoopGym_EventScript_Notice::
	waitmessage
	call Common_EventScript_PlayGymBadgeFanfare
	waitbuttonpress
	callnative CoopTrainerRewards_LoadGymNoticeItem
	goto_if_eq VAR_0x8007, FALSE, CoopGym_EventScript_NoticeBagFull
	call EventScript_ObtainItemMessage
	releaseall
	end

CoopGym_EventScript_NoticeBagFull::
	msgbox gText_TooBadBagIsFull, MSGBOX_DEFAULT
	releaseall
	end
