Cormoria_Route5_MapScripts::
# 2 "data//maps/Route5/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_Route5_OnTransition
	.byte 0


Cormoria_Route5_OnTransition::
# 6 "data//maps/Route5/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_LYNCH_COMPLETE, Cormoria_Route5_OnTransition_2
# 7 "data//maps/Route5/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_LYNCH_SETUP, Cormoria_Route5_OnTransition_4
	return

Cormoria_Route5_OnTransition_2:
	end

Cormoria_Route5_OnTransition_4:
# 8 "data//maps/Route5/scripts.pory"
	setobjectxyperm 18, 9, 8
# 9 "data//maps/Route5/scripts.pory"
	setobjectxyperm 19, 8, 8
	end


Cormoria_Route5_SaveGame_Trigger::
# 15 "data//maps/Route5/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_LYNCH_COMPLETE, Cormoria_Route5_SaveGame_Trigger_2
# 17 "data//maps/Route5/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_LYNCH_SETUP, Cormoria_Route5_SaveGame_Trigger_4
	return

Cormoria_Route5_SaveGame_Trigger_2:
	end

Cormoria_Route5_SaveGame_Trigger_4:
# 18 "data//maps/Route5/scripts.pory"
	setvar VAR_TEMP_E, 1
# 19 "data//maps/Route5/scripts.pory"
	call Cormoria_Common_EventScript_SaveGameBeforeBattle
# 20 "data//maps/Route5/scripts.pory"
	releaseall
	end


Cormoria_Route5_Lynch_Battle_Trigger::
# 26 "data//maps/Route5/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_LYNCH_COMPLETE, Cormoria_Route5_Lynch_Battle_Trigger_2
# 28 "data//maps/Route5/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_LYNCH_SETUP, Cormoria_Route5_Lynch_Battle_Trigger_4
	return

Cormoria_Route5_Lynch_Battle_Trigger_2:
	end

Cormoria_Route5_Lynch_Battle_Trigger_4:
# 29 "data//maps/Route5/scripts.pory"
	special SpawnCameraObject
# 30 "data//maps/Route5/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Route5_Lynch_Battle_Trigger_Movement_0
# 31 "data//maps/Route5/scripts.pory"
	applymovement OBJ_EVENT_ID_CAMERA, Cormoria_Route5_Lynch_Battle_Trigger_Movement_0
# 32 "data//maps/Route5/scripts.pory"
	waitmovement 0
# 33 "data//maps/Route5/scripts.pory"
	special RemoveCameraObject
# 35 "data//maps/Route5/scripts.pory"
	msgbox_delay Cormoria_Route5_Lynch_Battle_Trigger_Text_0, Cormoria_Route5_Lynch_Battle_Trigger_Text_1
# 37 "data//maps/Route5/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_CHAMPION_LYNCH, Cormoria_Route5_Lynch_Battle_Trigger_Text_2
# 39 "data//maps/Route5/scripts.pory"
	msgbox_delay Cormoria_Route5_Lynch_Battle_Trigger_Text_0, Cormoria_Route5_Lynch_Battle_Trigger_Text_3
# 43 "data//maps/Route5/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Route5_Lynch_Battle_Trigger_Movement_1
# 44 "data//maps/Route5/scripts.pory"
	applymovement 18, Cormoria_Route5_Lynch_Battle_Trigger_Movement_2
# 45 "data//maps/Route5/scripts.pory"
	applymovement 19, Cormoria_Route5_Lynch_Battle_Trigger_Movement_3
# 46 "data//maps/Route5/scripts.pory"
	waitmovement 0
# 48 "data//maps/Route5/scripts.pory"
	removeobject 18
# 49 "data//maps/Route5/scripts.pory"
	removeobject 19
# 56 "data//maps/Route5/scripts.pory"
	setflag Cormoria_FLAG_CHAMPION_LYNCH_COMPLETE
# 57 "data//maps/Route5/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_CHAMPION_LYNCH
# 59 "data//maps/Route5/scripts.pory"
	releaseall
	end


Cormoria_Route5_NPC_A::
# 65 "data//maps/Route5/scripts.pory"
	lockall
# 66 "data//maps/Route5/scripts.pory"
	faceplayer
# 67 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Route5_NPC_A_Text_0
# 68 "data//maps/Route5/scripts.pory"
	releaseall
	end


Cormoria_Route5_NPC_B::
# 73 "data//maps/Route5/scripts.pory"
	lockall
# 74 "data//maps/Route5/scripts.pory"
	faceplayer
# 75 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Route5_NPC_B_Text_0
# 76 "data//maps/Route5/scripts.pory"
	releaseall
	end


Cormoria_Route5_NPC_C::
# 81 "data//maps/Route5/scripts.pory"
	lockall
# 82 "data//maps/Route5/scripts.pory"
	faceplayer
# 83 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Route5_NPC_C_Text_0
# 84 "data//maps/Route5/scripts.pory"
	releaseall
	end


Cormoria_Route5_Berger::
# 93 "data//maps/Route5/scripts.pory"
	lockall
# 94 "data//maps/Route5/scripts.pory"
	speakername Cormoria_Route5_Berger_Text_0
# 95 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Route5_Berger_Text_1
# 96 "data//maps/Route5/scripts.pory"
	faceplayer
# 97 "data//maps/Route5/scripts.pory"
	speakername Cormoria_Route5_Berger_Text_0
# 98 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Route5_Berger_Text_2
# 99 "data//maps/Route5/scripts.pory"
	release
# 100 "data//maps/Route5/scripts.pory"
	applymovement 16, Cormoria_Route5_Berger_Movement_0
# 101 "data//maps/Route5/scripts.pory"
	waitmovement 0
# 102 "data//maps/Route5/scripts.pory"
	releaseall
	end


Cormoria_Route5_Bergmite::
# 107 "data//maps/Route5/scripts.pory"
	playmoncry SPECIES_BERGMITE, CRY_MODE_NORMAL
# 108 "data//maps/Route5/scripts.pory"
	waitmoncry
# 109 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Route5_Bergmite_Text_0, MSGBOX_NPC
	end


Cormoria_Route5_Greta_Trigger0::
# 117 "data//maps/Route5/scripts.pory"
	goto_if_set Cormoria_FLAG_GRETA_ROUTE5, Cormoria_Route5_Greta_Trigger0_2
# 119 "data//maps/Route5/scripts.pory"
	playbgm MUS_ENCOUNTER_MAY, FALSE
# 120 "data//maps/Route5/scripts.pory"
	lockall
# 121 "data//maps/Route5/scripts.pory"
	applymovement 7, Cormoria_Route5_Greta_Trigger0_Movement_0
# 122 "data//maps/Route5/scripts.pory"
	waitmovement 0
# 124 "data//maps/Route5/scripts.pory"
	goto Cormoria_Route5_Greta_Main
	return

Cormoria_Route5_Greta_Trigger0_2:
	end


Cormoria_Route5_Greta_Trigger1::
# 128 "data//maps/Route5/scripts.pory"
	goto_if_set Cormoria_FLAG_GRETA_ROUTE5, Cormoria_Route5_Greta_Trigger1_2
# 130 "data//maps/Route5/scripts.pory"
	playbgm MUS_ENCOUNTER_MAY, FALSE
# 131 "data//maps/Route5/scripts.pory"
	lockall
# 132 "data//maps/Route5/scripts.pory"
	applymovement 7, Cormoria_Route5_Greta_Trigger1_Movement_0
# 133 "data//maps/Route5/scripts.pory"
	waitmovement 0
# 135 "data//maps/Route5/scripts.pory"
	goto Cormoria_Route5_Greta_Main
	return

Cormoria_Route5_Greta_Trigger1_2:
	end


Cormoria_Route5_Greta_Main::
# 140 "data//maps/Route5/scripts.pory"
	speakername Cormoria_Route5_Greta_Main_Text_0
# 141 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Route5_Greta_Main_Text_1
# 142 "data//maps/Route5/scripts.pory"
	release
# 143 "data//maps/Route5/scripts.pory"
	delay 20
# 144 "data//maps/Route5/scripts.pory"
	speakername Cormoria_Route5_Greta_Main_Text_0
# 145 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Common_Text_Greta_Comecome
# 146 "data//maps/Route5/scripts.pory"
	release
# 147 "data//maps/Route5/scripts.pory"
	delay 20
# 149 "data//maps/Route5/scripts.pory"
	goto Cormoria_Route5_Greta_ShowItemsForSale
	return


Cormoria_Route5_Greta_GottaGo::
# 155 "data//maps/Route5/scripts.pory"
	release
# 156 "data//maps/Route5/scripts.pory"
	applymovement 7, Cormoria_Route5_Greta_GottaGo_Movement_0
# 157 "data//maps/Route5/scripts.pory"
	waitmovement 0
# 158 "data//maps/Route5/scripts.pory"
	speakername Cormoria_Route5_Greta_Main_Text_0
# 159 "data//maps/Route5/scripts.pory"
	msgbox Cormoria_Route5_Greta_GottaGo_Text_0
# 160 "data//maps/Route5/scripts.pory"
	release
# 161 "data//maps/Route5/scripts.pory"
	applymovement 7, Cormoria_Route5_Greta_GottaGo_Movement_1
# 162 "data//maps/Route5/scripts.pory"
	waitmovement 0
# 163 "data//maps/Route5/scripts.pory"
	removeobject 7
# 164 "data//maps/Route5/scripts.pory"
	setflag Cormoria_FLAG_GRETA_ROUTE5
# 165 "data//maps/Route5/scripts.pory"
	fadedefaultbgm
# 167 "data//maps/Route5/scripts.pory"
	releaseall
	end


# 30 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Lynch_Battle_Trigger_Movement_0:
# 30 "data//maps/Route5/scripts.pory"
	walk_up
	step_end

# 43 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Lynch_Battle_Trigger_Movement_1:
# 43 "data//maps/Route5/scripts.pory"
	walk_down
# 43 "data//maps/Route5/scripts.pory"
	walk_down
# 43 "data//maps/Route5/scripts.pory"
	walk_left
# 43 "data//maps/Route5/scripts.pory"
	walk_left
# 43 "data//maps/Route5/scripts.pory"
	face_right
	step_end

# 44 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Lynch_Battle_Trigger_Movement_2:
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_right
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
# 44 "data//maps/Route5/scripts.pory"
	walk_down
	step_end

# 45 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Lynch_Battle_Trigger_Movement_3:
# 45 "data//maps/Route5/scripts.pory"
	walk_right
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_right
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
# 45 "data//maps/Route5/scripts.pory"
	walk_down
	step_end

# 100 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Berger_Movement_0:
# 100 "data//maps/Route5/scripts.pory"
	face_down
	step_end

# 121 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Greta_Trigger0_Movement_0:
# 121 "data//maps/Route5/scripts.pory"
	walk_right
# 121 "data//maps/Route5/scripts.pory"
	walk_right
# 121 "data//maps/Route5/scripts.pory"
	walk_right
# 121 "data//maps/Route5/scripts.pory"
	walk_right
# 121 "data//maps/Route5/scripts.pory"
	walk_right
# 121 "data//maps/Route5/scripts.pory"
	walk_right
# 121 "data//maps/Route5/scripts.pory"
	walk_right
# 121 "data//maps/Route5/scripts.pory"
	walk_up
# 121 "data//maps/Route5/scripts.pory"
	face_right
	step_end

# 132 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Greta_Trigger1_Movement_0:
# 132 "data//maps/Route5/scripts.pory"
	walk_right
# 132 "data//maps/Route5/scripts.pory"
	walk_right
# 132 "data//maps/Route5/scripts.pory"
	walk_right
# 132 "data//maps/Route5/scripts.pory"
	walk_right
# 132 "data//maps/Route5/scripts.pory"
	walk_right
# 132 "data//maps/Route5/scripts.pory"
	walk_right
# 132 "data//maps/Route5/scripts.pory"
	walk_right
	step_end

# 156 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Greta_GottaGo_Movement_0:
# 156 "data//maps/Route5/scripts.pory"
	emote_exclamation_mark
	step_end

# 161 "data//maps/Route5/scripts.pory"
Cormoria_Route5_Greta_GottaGo_Movement_1:
# 161 "data//maps/Route5/scripts.pory"
	walk_left
# 161 "data//maps/Route5/scripts.pory"
	walk_left
# 161 "data//maps/Route5/scripts.pory"
	walk_left
# 161 "data//maps/Route5/scripts.pory"
	walk_left
# 161 "data//maps/Route5/scripts.pory"
	walk_left
# 161 "data//maps/Route5/scripts.pory"
	walk_left
# 161 "data//maps/Route5/scripts.pory"
	walk_left
# 161 "data//maps/Route5/scripts.pory"
	walk_left
	step_end

Cormoria_Route5_Lynch_Battle_Trigger_Text_0:
# 35 "data//maps/Route5/scripts.pory"
	.string "Lynch$"

Cormoria_Route5_Lynch_Battle_Trigger_Text_1:
# 35 "data//maps/Route5/scripts.pory"
	.string "My fellow ingredient hunter...and now\n"
	.string "Fifth Cormoria Champion!\p"
	.string "Ravine told me about your duel with him!\n"
	.string "I've never seen him in such a mood - you\l"
	.string "must've really lit him on fire!\p"
	.string "I was never much of a battler, you\n"
	.string "know? I'd always had one foot in the\l"
	.string "kitchen. But my Pokemon and I grew\l"
	.string "stronger and stronger, and we won the\l"
	.string "Championship!\p"
	.string "A great time! The best of my life!\p"
	.string "Sigh...how time flies.\p"
	.string "You'll be old in the blink of an eye too,\n"
	.string "{PLAYER}! But while you're young, why\l"
	.string "don't you try to light the same fire\l"
	.string "inside me as well?$"

Cormoria_Route5_Lynch_Battle_Trigger_Text_2:
# 37 "data//maps/Route5/scripts.pory"
	.string "Hydrapple! We actually lost!$"

Cormoria_Route5_Lynch_Battle_Trigger_Text_3:
# 39 "data//maps/Route5/scripts.pory"
	.string "Do you know why we all asked you to\n"
	.string "come to such offbeat locations?\p"
	.string "No?\p"
	.string "Imagine the press, the frenzy! Two\n"
	.string "Champions colliding! The entire region\l"
	.string "would descend upon us! And what of the\l"
	.string "property damage? Our Pokemon are far\l"
	.string "too strong to be let loose inside a\l"
	.string "building.\p"
	.string "No, this is much better.\p"
	.string "Out in nature with nobody to disturb us.\n"
	.string "Just the two of us, our partners, and\l"
	.string "all our skills and experience.\p"
	.string "And it really was lovely, don't you\n"
	.string "think?$"

Cormoria_Route5_NPC_A_Text_0:
# 67 "data//maps/Route5/scripts.pory"
	.string "My head is spinning because of the fog\n"
	.string "in Vilethorn Woods! I need some rest.$"

Cormoria_Route5_NPC_B_Text_0:
# 75 "data//maps/Route5/scripts.pory"
	.string "I saw a rough-looking fellow in a dark\n"
	.string "outfit run past me. What's going on?$"

Cormoria_Route5_NPC_C_Text_0:
# 83 "data//maps/Route5/scripts.pory"
	.string "I wish they'd clear those boulders in\n"
	.string "Vilethorn Woods! It's blocking the\l"
	.string "path back to Galecrest City.$"

Cormoria_Route5_Berger_Text_0:
# 94 "data//maps/Route5/scripts.pory"
	.string "Berger$"

Cormoria_Route5_Berger_Text_1:
# 95 "data//maps/Route5/scripts.pory"
	.string "C'mon Bergmite! What's the matter?\n"
	.string "Don't be so scared of Vilethorn Woods!$"

Cormoria_Route5_Berger_Text_2:
# 98 "data//maps/Route5/scripts.pory"
	.string "We're trying to reach Silversun City\n"
	.string "but Bergmite's being a scaredy-cat!$"

Cormoria_Route5_Bergmite_Text_0:
# 109 "data//maps/Route5/scripts.pory"
	.string "Bergmite is shivering!$"

Cormoria_Route5_Greta_Main_Text_0:
# 140 "data//maps/Route5/scripts.pory"
	.string "Greta$"

Cormoria_Route5_Greta_Main_Text_1:
# 141 "data//maps/Route5/scripts.pory"
	.string "It's you! The wandering saviour from Mt.\n"
	.string "Ceram. How wonderful! A new customer.$"

Cormoria_Route5_Greta_GottaGo_Text_0:
# 159 "data//maps/Route5/scripts.pory"
	.string "Would you look at the time! I've gotta\n"
	.string "go places. See you around, {PLAYER}!$"
