Cormoria_AncientCeram_MapScripts::
# 2 "data//maps/AncientCeram/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_AncientCeram_OnTransition_ExitReady
	.byte 0


Cormoria_AncientCeram_OnTransition_ExitReady::
# 7 "data//maps/AncientCeram/scripts.pory"
	setrespawn Cormoria_HEAL_LOCATION_ANCIENT_CERAM
# 9 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 3
	goto_if_eq Cormoria_AncientCeram_OnTransition_ExitReady_2
Cormoria_AncientCeram_OnTransition_ExitReady_1:
	end

Cormoria_AncientCeram_OnTransition_ExitReady_2:
# 10 "data//maps/AncientCeram/scripts.pory"
	setobjectxyperm 1, 28, 22
# 11 "data//maps/AncientCeram/scripts.pory"
	setobjectmovementtype 1, MOVEMENT_TYPE_FACE_LEFT
# 12 "data//maps/AncientCeram/scripts.pory"
	setobjectmovementtype OBJ_EVENT_ID_PLAYER, MOVEMENT_TYPE_FACE_LEFT
	goto Cormoria_AncientCeram_OnTransition_ExitReady_1


Cormoria_AncientCeram_EventScript_Gabrielle::
# 19 "data//maps/AncientCeram/scripts.pory"
	lockall
# 20 "data//maps/AncientCeram/scripts.pory"
	faceplayer
# 23 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 0
	goto_if_eq Cormoria_AncientCeram_EventScript_Gabrielle_2
Cormoria_AncientCeram_EventScript_Gabrielle_1:
# 32 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 1
	goto_if_eq Cormoria_AncientCeram_EventScript_Gabrielle_5
Cormoria_AncientCeram_EventScript_Gabrielle_4:
# 38 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 3
	goto_if_eq Cormoria_AncientCeram_EventScript_Gabrielle_8
Cormoria_AncientCeram_EventScript_Gabrielle_7:
# 44 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 2
	goto_if_eq Cormoria_AncientCeram_EventScript_Gabrielle_11
Cormoria_AncientCeram_EventScript_Gabrielle_10:
# 52 "data//maps/AncientCeram/scripts.pory"
	releaseall
	end

Cormoria_AncientCeram_EventScript_Gabrielle_2:
# 24 "data//maps/AncientCeram/scripts.pory"
	speakername Cormoria_AncientCeram_EventScript_Gabrielle_Text_0
# 25 "data//maps/AncientCeram/scripts.pory"
	msgbox Cormoria_AncientCeram_EventScript_Gabrielle_Text_1
# 26 "data//maps/AncientCeram/scripts.pory"
	setvar Cormoria_VAR_ANCIENT_GAB_ESCAPE, 1
# 28 "data//maps/AncientCeram/scripts.pory"
	completesubquest QUEST_DREAMSTONE_MYSTERIES, SUB_QUEST_2
	goto Cormoria_AncientCeram_EventScript_Gabrielle_1

Cormoria_AncientCeram_EventScript_Gabrielle_5:
# 33 "data//maps/AncientCeram/scripts.pory"
	speakername Cormoria_AncientCeram_EventScript_Gabrielle_Text_0
# 34 "data//maps/AncientCeram/scripts.pory"
	msgbox Cormoria_AncientCeram_EventScript_Gabrielle_Text_2
	goto Cormoria_AncientCeram_EventScript_Gabrielle_4

Cormoria_AncientCeram_EventScript_Gabrielle_8:
# 39 "data//maps/AncientCeram/scripts.pory"
	speakername Cormoria_AncientCeram_EventScript_Gabrielle_Text_0
# 40 "data//maps/AncientCeram/scripts.pory"
	msgbox Cormoria_AncientCeram_EventScript_Gabrielle_Text_3
	goto Cormoria_AncientCeram_EventScript_Gabrielle_7

Cormoria_AncientCeram_EventScript_Gabrielle_11:
# 45 "data//maps/AncientCeram/scripts.pory"
	speakername Cormoria_AncientCeram_EventScript_Gabrielle_Text_0
# 46 "data//maps/AncientCeram/scripts.pory"
	msgbox Cormoria_AncientCeram_EventScript_Gabrielle_Text_4
# 47 "data//maps/AncientCeram/scripts.pory"
	setvar Cormoria_VAR_ANCIENT_GAB_ESCAPE, 3
# 50 "data//maps/AncientCeram/scripts.pory"
	warp MAP_CORMORIA_ANCIENT_CERAM, 28, 23
	goto Cormoria_AncientCeram_EventScript_Gabrielle_10


Cormoria_AncientCeram_EventScript_Exit::
# 60 "data//maps/AncientCeram/scripts.pory"
	lockall
# 63 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 0
	goto_if_eq Cormoria_AncientCeram_EventScript_Exit_2
Cormoria_AncientCeram_EventScript_Exit_1:
# 66 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 1
	goto_if_eq Cormoria_AncientCeram_EventScript_Exit_5
Cormoria_AncientCeram_EventScript_Exit_4:
# 69 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 2
	goto_if_eq Cormoria_AncientCeram_EventScript_Exit_8
Cormoria_AncientCeram_EventScript_Exit_7:
# 72 "data//maps/AncientCeram/scripts.pory"
	compare Cormoria_VAR_ANCIENT_GAB_ESCAPE, 3
	goto_if_eq Cormoria_AncientCeram_EventScript_Exit_11
Cormoria_AncientCeram_EventScript_Exit_10:
# 91 "data//maps/AncientCeram/scripts.pory"
	releaseall
	return

Cormoria_AncientCeram_EventScript_Exit_2:
# 64 "data//maps/AncientCeram/scripts.pory"
	setvar Cormoria_VAR_ANCIENT_GAB_ESCAPE, 2
	goto Cormoria_AncientCeram_EventScript_Exit_1

Cormoria_AncientCeram_EventScript_Exit_5:
# 67 "data//maps/AncientCeram/scripts.pory"
	setvar Cormoria_VAR_ANCIENT_GAB_ESCAPE, 2
	goto Cormoria_AncientCeram_EventScript_Exit_4

Cormoria_AncientCeram_EventScript_Exit_8:
# 70 "data//maps/AncientCeram/scripts.pory"
	msgbox Cormoria_AncientCeram_EventScript_Exit_Text_0
	goto Cormoria_AncientCeram_EventScript_Exit_7

Cormoria_AncientCeram_EventScript_Exit_11:
# 74 "data//maps/AncientCeram/scripts.pory"
	setvar VAR_0x8004, 2
# 75 "data//maps/AncientCeram/scripts.pory"
	setvar VAR_0x8005, 2
# 76 "data//maps/AncientCeram/scripts.pory"
	setvar VAR_0x8006, 30
# 77 "data//maps/AncientCeram/scripts.pory"
	setvar VAR_0x8007, 5
# 78 "data//maps/AncientCeram/scripts.pory"
	special ShakeCamera
# 79 "data//maps/AncientCeram/scripts.pory"
	waitstate
# 80 "data//maps/AncientCeram/scripts.pory"
	release
# 83 "data//maps/AncientCeram/scripts.pory"
	setflag Cormoria_FLAG_ANCIENT_FIRST_TIME
# 84 "data//maps/AncientCeram/scripts.pory"
	setvar Cormoria_VAR_ANCIENT_GAB_ESCAPE, 4
# 85 "data//maps/AncientCeram/scripts.pory"
	setrespawn Cormoria_HEAL_LOCATION_GALECREST_CITY
# 88 "data//maps/AncientCeram/scripts.pory"
	warp MAP_CORMORIA_GALECREST_CITY, 53, 30
	goto Cormoria_AncientCeram_EventScript_Exit_10


Cormoria_AncientCeram_EventScript_Gabrielle_Text_0:
# 24 "data//maps/AncientCeram/scripts.pory"
	.string "Gabrielle$"

Cormoria_AncientCeram_EventScript_Gabrielle_Text_1:
# 25 "data//maps/AncientCeram/scripts.pory"
	.string "What just happened? Where are we?\p"
	.string "...\p"
	.string "Ouch! I can't move my leg!\p"
	.string "{PLAYER}, p-please...you have to help\n"
	.string "me. I don't know what's going on and\l"
	.string "where we are. And the boss just...he\l"
	.string "just left me!!\p"
	.string "...\p"
	.string "I can't get up.$"

Cormoria_AncientCeram_EventScript_Gabrielle_Text_2:
# 34 "data//maps/AncientCeram/scripts.pory"
	.string "If you find a way out of this place,\n"
	.string "please take me with you!$"

Cormoria_AncientCeram_EventScript_Gabrielle_Text_3:
# 40 "data//maps/AncientCeram/scripts.pory"
	.string "Shall we go? Touch the dreamstone\n"
	.string "whenever you want to leave.$"

Cormoria_AncientCeram_EventScript_Gabrielle_Text_4:
# 46 "data//maps/AncientCeram/scripts.pory"
	.string "You found another dreamstone?\n"
	.string "Okay, let's get out of here.$"

Cormoria_AncientCeram_EventScript_Exit_Text_0:
# 70 "data//maps/AncientCeram/scripts.pory"
	.string "I can't leave Gabrielle behind!$"
