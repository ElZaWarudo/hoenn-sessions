Cormoria_CarabrueTown_MapScripts::
# 2 "data//maps/CarabrueTown/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_CarabrueTown_MapScripts_MAP_SCRIPT_ON_TRANSITION
# 6 "data//maps/CarabrueTown/scripts.pory"
	map_script MAP_SCRIPT_ON_FRAME_TABLE, Cormoria_CarabrueTown_MapScripts_MAP_SCRIPT_ON_FRAME_TABLE
	.byte 0

Cormoria_CarabrueTown_MapScripts_MAP_SCRIPT_ON_TRANSITION:
# 3 "data//maps/CarabrueTown/scripts.pory"
	setflag Cormoria_FLAG_VISITED_CARABRUE_TOWN
	return

Cormoria_CarabrueTown_MapScripts_MAP_SCRIPT_ON_FRAME_TABLE:
# 7 "data//maps/CarabrueTown/scripts.pory"
	map_script_2 VAR_TEMP_0, 0, Cormoria_CarabrueTown_PostGame
	.2byte 0


Cormoria_CarabrueTown_PostGame::
# 14 "data//maps/CarabrueTown/scripts.pory"
	setvar VAR_TEMP_0, 1
# 15 "data//maps/CarabrueTown/scripts.pory"
	goto_if_set Cormoria_FLAG_SKIP_POSTGAME_MSG, Cormoria_CarabrueTown_PostGame_2
# 16 "data//maps/CarabrueTown/scripts.pory"
	goto_if_set Cormoria_FLAG_SYS_GAME_CLEAR, Cormoria_CarabrueTown_PostGame_4
	return

Cormoria_CarabrueTown_PostGame_2:
# 15 "data//maps/CarabrueTown/scripts.pory"
	releaseall
	end

Cormoria_CarabrueTown_PostGame_4:
# 18 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_PostGame_Text_0, MSGBOX_POKENAV
# 20 "data//maps/CarabrueTown/scripts.pory"
	setflag Cormoria_FLAG_SKIP_POSTGAME_MSG
# 21 "data//maps/CarabrueTown/scripts.pory"
	releaseall
	end


Cormoria_CarabrueTown_EventScript_PoliceRoadBlock::
# 30 "data//maps/CarabrueTown/scripts.pory"
	lockall
# 32 "data//maps/CarabrueTown/scripts.pory"
	goto_if_unset Cormoria_FLAG_UNUSED_0x020, Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_2
# 36 "data//maps/CarabrueTown/scripts.pory"
	speakername Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_0
# 37 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_2
# 38 "data//maps/CarabrueTown/scripts.pory"
	release
# 39 "data//maps/CarabrueTown/scripts.pory"
	applymovement 1, Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Movement_0
# 40 "data//maps/CarabrueTown/scripts.pory"
	applymovement 2, Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Movement_0
# 41 "data//maps/CarabrueTown/scripts.pory"
	delay 20
# 42 "data//maps/CarabrueTown/scripts.pory"
	speakername Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_0
# 43 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_3
# 44 "data//maps/CarabrueTown/scripts.pory"
	release
# 46 "data//maps/CarabrueTown/scripts.pory"
	giveitem ITEM_EXP_SHARE
# 47 "data//maps/CarabrueTown/scripts.pory"
	speakername Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_0
# 48 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_4
# 49 "data//maps/CarabrueTown/scripts.pory"
	release
# 50 "data//maps/CarabrueTown/scripts.pory"
	fadescreenswapbuffers FADE_TO_BLACK
# 52 "data//maps/CarabrueTown/scripts.pory"
	removeobject 1
# 53 "data//maps/CarabrueTown/scripts.pory"
	removeobject 2
# 54 "data//maps/CarabrueTown/scripts.pory"
	playse SE_EXIT
# 55 "data//maps/CarabrueTown/scripts.pory"
	delay 16
# 56 "data//maps/CarabrueTown/scripts.pory"
	fadescreenswapbuffers FADE_FROM_BLACK
# 58 "data//maps/CarabrueTown/scripts.pory"
	removeitem ITEM_LAB_WELCOMEPACKAGE
# 59 "data//maps/CarabrueTown/scripts.pory"
	setflag Cormoria_FLAG_CARABRUE_POLICE
Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_1:
# 62 "data//maps/CarabrueTown/scripts.pory"
	releaseall
	return

Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_2:
# 33 "data//maps/CarabrueTown/scripts.pory"
	speakername Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_0
# 34 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_1
	goto Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_1


Cormoria_CarabrueTown_Sign_ChampionHouse::
# 70 "data//maps/CarabrueTown/scripts.pory"
	goto_if_set Cormoria_FLAG_SYS_GAME_CLEAR, Cormoria_CarabrueTown_Sign_ChampionHouse_2
# 80 "data//maps/CarabrueTown/scripts.pory"
	lockall
# 81 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_Sign_ChampionHouse_Text_1
# 82 "data//maps/CarabrueTown/scripts.pory"
	releaseall
	end

Cormoria_CarabrueTown_Sign_ChampionHouse_2:
# 71 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_Sign_ChampionHouse_Text_0, MSGBOX_YESNO
# 72 "data//maps/CarabrueTown/scripts.pory"
	release
# 73 "data//maps/CarabrueTown/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_CarabrueTown_Sign_ChampionHouse_5
# 74 "data//maps/CarabrueTown/scripts.pory"
	playse SE_DOOR
# 75 "data//maps/CarabrueTown/scripts.pory"
	warp MAP_CORMORIA_THE_OTHER_HOUSE, 2, 8
# 76 "data//maps/CarabrueTown/scripts.pory"
	releaseall
	end

Cormoria_CarabrueTown_Sign_ChampionHouse_5:
	end


Cormoria_CarabrueTown_EventScript_NPC1::
# 87 "data//maps/CarabrueTown/scripts.pory"
	goto_if_set Cormoria_FLAG_CARABRUE_POLICE, Cormoria_CarabrueTown_EventScript_NPC1_2
# 90 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_EventScript_NPC1_Text_1, MSGBOX_NPC
Cormoria_CarabrueTown_EventScript_NPC1_1:
	end

Cormoria_CarabrueTown_EventScript_NPC1_2:
# 88 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_EventScript_NPC1_Text_0, MSGBOX_NPC
	goto Cormoria_CarabrueTown_EventScript_NPC1_1


Cormoria_CarabrueTown_EventScript_NPC2::
# 96 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_EventScript_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_CarabrueTown_Chikorita::
# 101 "data//maps/CarabrueTown/scripts.pory"
	playmoncry SPECIES_CHIKORITA, CRY_MODE_NORMAL
# 102 "data//maps/CarabrueTown/scripts.pory"
	msgbox Cormoria_CarabrueTown_Chikorita_Text_0, MSGBOX_NPC
	end


# 39 "data//maps/CarabrueTown/scripts.pory"
Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Movement_0:
# 39 "data//maps/CarabrueTown/scripts.pory"
	emote_exclamation_mark
	step_end

Cormoria_CarabrueTown_PostGame_Text_0:
# 18 "data//maps/CarabrueTown/scripts.pory"
	.string "Thank you for playing Pokémon\n"
	.string "Dreamstone Mysteries and\l"
	.string "congratulations on becoming\l"
	.string "the Champion of Cormoria!\p"
	.string "But tell me - is a Champion truly a\n"
	.string "Champion unless they've explored\l"
	.string "all of Cormoria?\p"
	.string "Why don't you try completing\n"
	.string "the side-quests and catching\l"
	.string "the 4 Mythical Pokémon available?$"

Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_0:
# 33 "data//maps/CarabrueTown/scripts.pory"
	.string "Detective$"

Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_1:
# 34 "data//maps/CarabrueTown/scripts.pory"
	.string "Sorry, this road is temporarily closed.\n"
	.string "We're investigating Professor Tenebris'\l"
	.string "disappearance.$"

Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_2:
# 37 "data//maps/CarabrueTown/scripts.pory"
	.string "Is this the evidence from the lab?\n"
	.string "Thanks, kid. We'll take it from here.\p"
	.string "...$"

Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_3:
# 43 "data//maps/CarabrueTown/scripts.pory"
	.string "Hold on - this isn't evidence!$"

Cormoria_CarabrueTown_EventScript_PoliceRoadBlock_Text_4:
# 48 "data//maps/CarabrueTown/scripts.pory"
	.string "What's the matter with you guys?\p"
	.string "Can't keep it together without the\n"
	.string "Professor around to help you?\p"
	.string "Well thanks anyway, kid. See you.$"

Cormoria_CarabrueTown_Sign_ChampionHouse_Text_0:
# 71 "data//maps/CarabrueTown/scripts.pory"
	.string "There's no lock. Open the door?$"

Cormoria_CarabrueTown_Sign_ChampionHouse_Text_1:
# 81 "data//maps/CarabrueTown/scripts.pory"
	.string "This house is locked.$"

Cormoria_CarabrueTown_EventScript_NPC1_Text_0:
# 88 "data//maps/CarabrueTown/scripts.pory"
	.string "Looks like the police went away. I hope\n"
	.string "they got the evidence they needed!$"

Cormoria_CarabrueTown_EventScript_NPC1_Text_1:
# 90 "data//maps/CarabrueTown/scripts.pory"
	.string "Psst - {PLAYER}! See those\n"
	.string "detectives standing over there?\p"
	.string "They came all the way from Fennilahl\n"
	.string "Town to investigate the Professor's\l"
	.string "disappearance. They're blocking access\l"
	.string "to Route 1 while they investigate.$"

Cormoria_CarabrueTown_EventScript_NPC2_Text_0:
# 96 "data//maps/CarabrueTown/scripts.pory"
	.string "My Chikorita and I love how cozy\n"
	.string "Carabrue Town is! That's because it's\l"
	.string "nestled deep in the hills of Cormoria.$"

Cormoria_CarabrueTown_Chikorita_Text_0:
# 102 "data//maps/CarabrueTown/scripts.pory"
	.string "Chikorita is happy!$"
