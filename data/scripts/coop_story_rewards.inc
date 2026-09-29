@ Co-op story battle wins on the PARTNER's ROM (A9).
@
@ The requester's ROM runs the trainer's own post-battle script. The partner's
@ ROM has none, so when the partner is at the same story point of a GRANT
@ battle (sCoopStoryBattles in src/coop/trainer_rewards.c) CoopTrainerRewards_Apply
@ runs CoopStory_EventScript_Grant<Battle> at once (RunScriptImmediately). Each
@ one holds exactly the flags, vars and specials the vanilla post-battle script
@ in data/maps/<Map>/scripts.inc sets, in its order; a `removeobject` there is
@ its object's hide flag here. The item a script gives with `giveitem` is
@ given in C (the table's item), so a full bag behaves as in vanilla.
@
@ Left out on purpose, because they only animate the requester's scene: the
@ rival walking, biking or flying away, Scott and Archie walking in and out,
@ the submarine leaving, fades and music. The partner's map scripts rebuild
@ the rest from the flags and vars on its next load; objects the grant hid
@ that are on the partner's screen are removed by the notice
@ (CoopTrainerRewards_SyncStoryObjects), as the map would not spawn them.

@ Route103_EventScript_RivalEnd.
CoopStory_EventScript_GrantRivalRoute103::
	setflag FLAG_HIDE_ROUTE_103_RIVAL
	setvar VAR_BIRCH_LAB_STATE, 4
	clearflag FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_RIVAL
	setflag FLAG_DEFEATED_RIVAL_ROUTE103
	setvar VAR_OLDALE_RIVAL_STATE, 1
	clearflag FLAG_HIDE_OLDALE_TOWN_RIVAL
	end

@ Route110_EventScript_<Rival>Defeated through RivalExit (ITEM_DOWSING_MACHINE).
CoopStory_EventScript_GrantRivalRoute110::
	setflag FLAG_HIDE_ROUTE_110_RIVAL
	setflag FLAG_HIDE_ROUTE_110_RIVAL_ON_BIKE
	setvar VAR_ROUTE110_STATE, 1
	end

@ Route119_EventScript_Defeated<Rival>, GiveFlyHM and RivalExitScottArrive
@ (ITEM_HM_FLY; the flag is set whether or not the bag had room, as vanilla).
CoopStory_EventScript_GrantRivalRoute119::
	setflag FLAG_RECEIVED_HM_FLY
	setflag FLAG_HIDE_ROUTE_119_RIVAL
	setflag FLAG_HIDE_ROUTE_119_RIVAL_ON_BIKE
	setvar VAR_ROUTE119_STATE, 1
	addvar VAR_SCOTT_STATE, 1
	setflag FLAG_HIDE_ROUTE_119_SCOTT
	end

@ LilycoveCity_EventScript_Defeated<Rival> and RivalFlyAway. The bedroom
@ flags follow the PARTNER's rival, as its own script would.
CoopStory_EventScript_GrantRivalLilycove::
	checkplayergender
	call_if_eq VAR_RESULT, MALE, CoopStory_EventScript_ShowMayAtHome
	call_if_eq VAR_RESULT, FEMALE, CoopStory_EventScript_ShowBrendanAtHome
	setflag FLAG_HIDE_LILYCOVE_CITY_RIVAL
	setflag FLAG_MET_RIVAL_LILYCOVE
	end

CoopStory_EventScript_ShowMayAtHome::
	clearflag FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_RIVAL_BEDROOM
	clearflag FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_2F_PICHU_DOLL
	return

CoopStory_EventScript_ShowBrendanAtHome::
	clearflag FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM
	clearflag FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F_SWABLU_DOLL
	return

@ MauvilleCity_EventScript_DefeatedWally.
CoopStory_EventScript_GrantWallyMauville::
	setflag FLAG_HIDE_MAUVILLE_CITY_WALLY
	setflag FLAG_HIDE_MAUVILLE_CITY_WALLYS_UNCLE
	clearflag FLAG_HIDE_VERDANTURF_TOWN_WANDAS_HOUSE_WALLY
	clearflag FLAG_HIDE_VERDANTURF_TOWN_WANDAS_HOUSE_WALLYS_UNCLE
	setflag FLAG_DEFEATED_WALLY_MAUVILLE
	setvar VAR_WALLY_CALL_STEP_COUNTER, 0
	setflag FLAG_ENABLE_FIRST_WALLY_POKENAV_CALL
	addvar VAR_SCOTT_STATE, 1
	setflag FLAG_HIDE_MAUVILLE_CITY_SCOTT
	end

@ VictoryRoad_1F_EventScript_WallyBattleWon. The requester's trigger side
@ (VAR_0x8008) only picks where Wally waits; the partner's Wally waits at
@ the first spot (VictoryRoad_1F_OnTransition).
CoopStory_EventScript_GrantWallyVictoryRoad::
	clearflag FLAG_HIDE_VICTORY_ROAD_ENTRANCE_WALLY
	setflag FLAG_DEFEATED_WALLY_VICTORY_ROAD
	setvar VAR_VICTORY_ROAD_1F_STATE, 1
	end

@ JaggedPass_EventScript_MagmaHideoutGuard after the battle.
CoopStory_EventScript_GrantGruntJaggedPass::
	setflag FLAG_BEAT_MAGMA_GRUNT_JAGGED_PASS
	end

@ AquaHideout_B2F_EventScript_SubmarineEscape.
CoopStory_EventScript_GrantMattAquaHideout::
	setflag FLAG_HIDE_AQUA_HIDEOUT_B2F_SUBMARINE_SHADOW
	setflag FLAG_TEAM_AQUA_ESCAPED_IN_SUBMARINE
	setflag FLAG_HIDE_LILYCOVE_CITY_AQUA_GRUNTS
	end

@ MtChimney_EventScript_Maxie after the battle.
CoopStory_EventScript_GrantMaxieMtChimney::
	setflag FLAG_HIDE_MT_CHIMNEY_TEAM_MAGMA
	setflag FLAG_HIDE_MT_CHIMNEY_TEAM_AQUA
	setflag FLAG_DEFEATED_EVIL_TEAM_MT_CHIMNEY
	clearflag FLAG_HIDE_FALLARBOR_HOUSE_PROF_COZMO
	setflag FLAG_HIDE_METEOR_FALLS_1F_1R_COZMO
	clearflag FLAG_HIDE_MT_CHIMNEY_LAVA_COOKIE_LADY
	end

@ EverGrandeCity_<Member>Room_EventScript_Defeated. The door is opened by the
@ notice (PokemonLeague_EliteFour_SetAdvanceToNextRoomMetatiles) when the
@ partner stands in that room, as the room's OnLoad does from the flag.
CoopStory_EventScript_GrantSidney::
	setflag FLAG_DEFEATED_ELITE_4_SIDNEY
	end

CoopStory_EventScript_GrantPhoebe::
	setflag FLAG_DEFEATED_ELITE_4_PHOEBE
	end

CoopStory_EventScript_GrantGlacia::
	setflag FLAG_DEFEATED_ELITE_4_GLACIA
	end

CoopStory_EventScript_GrantDrake::
	setvar VAR_0x8004, FANCOUNTER_DEFEATED_DRAKE
	special Script_TryGainNewFanFromCounter
	setflag FLAG_DEFEATED_ELITE_4_DRAKE
	end

@ MeteorFalls_StevensCave_EventScript_Steven after the battle.
CoopStory_EventScript_GrantStevenMeteorFalls::
	setflag FLAG_DEFEATED_METEOR_FALLS_STEVEN
	end

@ The partner's presentation only; every grant is already applied.
CoopStory_EventScript_Notice::
	lockall
	callnative CoopTrainerRewards_SyncStoryObjects
	callnative CoopTrainerRewards_LoadStoryNotice
	call_if_eq VAR_0x8004, TRUE, PokemonLeague_EliteFour_SetAdvanceToNextRoomMetatiles
	msgbox CoopStory_Text_PartnerStory, MSGBOX_DEFAULT
	call_if_eq VAR_0x8005, TRUE, EventScript_ObtainItemMessage
	releaseall
	end

CoopStory_Text_PartnerStory:
	.string "Your partner's victory carried your\n"
	.string "story forward, too!$"
