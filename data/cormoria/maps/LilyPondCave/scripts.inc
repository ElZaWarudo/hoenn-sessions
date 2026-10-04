Cormoria_LilyPondCave_MapScripts::
# 2 "data//maps/LilyPondCave/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_LilyPondCave_OnTransition
	.byte 0


Cormoria_LilyPondCave_OnTransition::
# 6 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_COMPLETE, Cormoria_LilyPondCave_OnTransition_2
# 8 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_SETUP, Cormoria_LilyPondCave_OnTransition_4
	return

Cormoria_LilyPondCave_OnTransition_2:
	end

Cormoria_LilyPondCave_OnTransition_4:
# 9 "data//maps/LilyPondCave/scripts.pory"
	setobjectxyperm 8, 13, 29
# 10 "data//maps/LilyPondCave/scripts.pory"
	setobjectxyperm 9, 12, 29
	end


Cormoria_LilyPondCave_SaveGame_Trigger::
# 16 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_COMPLETE, Cormoria_LilyPondCave_SaveGame_Trigger_2
# 18 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_SETUP, Cormoria_LilyPondCave_SaveGame_Trigger_4
	return

Cormoria_LilyPondCave_SaveGame_Trigger_2:
	end

Cormoria_LilyPondCave_SaveGame_Trigger_4:
# 19 "data//maps/LilyPondCave/scripts.pory"
	setvar VAR_TEMP_E, 1
# 20 "data//maps/LilyPondCave/scripts.pory"
	call Cormoria_Common_EventScript_SaveGameBeforeBattle
# 21 "data//maps/LilyPondCave/scripts.pory"
	releaseall
	end


Cormoria_LilyPondCave_Martha_Battle_Trigger0::
# 27 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_COMPLETE, Cormoria_LilyPondCave_Martha_Battle_Trigger0_2
# 29 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_SETUP, Cormoria_LilyPondCave_Martha_Battle_Trigger0_4
	return

Cormoria_LilyPondCave_Martha_Battle_Trigger0_2:
	end

Cormoria_LilyPondCave_Martha_Battle_Trigger0_4:
# 30 "data//maps/LilyPondCave/scripts.pory"
	special SpawnCameraObject
# 31 "data//maps/LilyPondCave/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_LilyPondCave_Martha_Battle_Trigger0_Movement_0
# 32 "data//maps/LilyPondCave/scripts.pory"
	applymovement OBJ_EVENT_ID_CAMERA, Cormoria_LilyPondCave_Martha_Battle_Trigger0_Movement_1
# 33 "data//maps/LilyPondCave/scripts.pory"
	waitmovement 0
# 34 "data//maps/LilyPondCave/scripts.pory"
	special RemoveCameraObject
# 36 "data//maps/LilyPondCave/scripts.pory"
	goto Cormoria_LilyPondCave_Martha_Battle
	return


Cormoria_LilyPondCave_Martha_Battle_Trigger1::
# 40 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_COMPLETE, Cormoria_LilyPondCave_Martha_Battle_Trigger1_2
# 42 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_SETUP, Cormoria_LilyPondCave_Martha_Battle_Trigger1_4
	return

Cormoria_LilyPondCave_Martha_Battle_Trigger1_2:
	end

Cormoria_LilyPondCave_Martha_Battle_Trigger1_4:
# 43 "data//maps/LilyPondCave/scripts.pory"
	special SpawnCameraObject
# 44 "data//maps/LilyPondCave/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_LilyPondCave_Martha_Battle_Trigger1_Movement_0
# 45 "data//maps/LilyPondCave/scripts.pory"
	applymovement OBJ_EVENT_ID_CAMERA, Cormoria_LilyPondCave_Martha_Battle_Trigger0_Movement_1
# 46 "data//maps/LilyPondCave/scripts.pory"
	waitmovement 0
# 47 "data//maps/LilyPondCave/scripts.pory"
	special RemoveCameraObject
# 49 "data//maps/LilyPondCave/scripts.pory"
	goto Cormoria_LilyPondCave_Martha_Battle
	return


Cormoria_LilyPondCave_Martha_Battle_Trigger2::
# 54 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_COMPLETE, Cormoria_LilyPondCave_Martha_Battle_Trigger2_2
# 56 "data//maps/LilyPondCave/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMPION_MARTHA_SETUP, Cormoria_LilyPondCave_Martha_Battle_Trigger2_4
	return

Cormoria_LilyPondCave_Martha_Battle_Trigger2_2:
	end

Cormoria_LilyPondCave_Martha_Battle_Trigger2_4:
# 57 "data//maps/LilyPondCave/scripts.pory"
	special SpawnCameraObject
# 58 "data//maps/LilyPondCave/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_LilyPondCave_Martha_Battle_Trigger2_Movement_0
# 59 "data//maps/LilyPondCave/scripts.pory"
	applymovement OBJ_EVENT_ID_CAMERA, Cormoria_LilyPondCave_Martha_Battle_Trigger0_Movement_1
# 60 "data//maps/LilyPondCave/scripts.pory"
	waitmovement 0
# 61 "data//maps/LilyPondCave/scripts.pory"
	special RemoveCameraObject
# 63 "data//maps/LilyPondCave/scripts.pory"
	goto Cormoria_LilyPondCave_Martha_Battle
	return


Cormoria_LilyPondCave_Martha_Battle::
# 68 "data//maps/LilyPondCave/scripts.pory"
	msgbox_delay Cormoria_LilyPondCave_Martha_Battle_Text_0, Cormoria_LilyPondCave_Martha_Battle_Text_1
# 70 "data//maps/LilyPondCave/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_CHAMPION_MARTHA, Cormoria_LilyPondCave_Martha_Battle_Text_2
# 72 "data//maps/LilyPondCave/scripts.pory"
	msgbox_delay Cormoria_LilyPondCave_Martha_Battle_Text_0, Cormoria_LilyPondCave_Martha_Battle_Text_3
# 75 "data//maps/LilyPondCave/scripts.pory"
	applymovement 8, Cormoria_LilyPondCave_Martha_Battle_Movement_0
# 76 "data//maps/LilyPondCave/scripts.pory"
	applymovement 9, Cormoria_LilyPondCave_Martha_Battle_Movement_1
# 77 "data//maps/LilyPondCave/scripts.pory"
	waitmovement 0
# 79 "data//maps/LilyPondCave/scripts.pory"
	removeobject 8
# 80 "data//maps/LilyPondCave/scripts.pory"
	removeobject 9
# 82 "data//maps/LilyPondCave/scripts.pory"
	playse SE_EXIT
# 83 "data//maps/LilyPondCave/scripts.pory"
	waitse
# 85 "data//maps/LilyPondCave/scripts.pory"
	setflag Cormoria_FLAG_CHAMPION_MARTHA_COMPLETE
# 86 "data//maps/LilyPondCave/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_MIRROHBASECAMP_HISTORIAN
# 87 "data//maps/LilyPondCave/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_CHAMPION_MARTHA
# 89 "data//maps/LilyPondCave/scripts.pory"
	releaseall
	end


# 31 "data//maps/LilyPondCave/scripts.pory"
Cormoria_LilyPondCave_Martha_Battle_Trigger0_Movement_0:
# 31 "data//maps/LilyPondCave/scripts.pory"
	walk_up
# 31 "data//maps/LilyPondCave/scripts.pory"
	walk_right
# 31 "data//maps/LilyPondCave/scripts.pory"
	face_up
	step_end

# 32 "data//maps/LilyPondCave/scripts.pory"
Cormoria_LilyPondCave_Martha_Battle_Trigger0_Movement_1:
# 32 "data//maps/LilyPondCave/scripts.pory"
	walk_up
# 32 "data//maps/LilyPondCave/scripts.pory"
	walk_up
	step_end

# 44 "data//maps/LilyPondCave/scripts.pory"
Cormoria_LilyPondCave_Martha_Battle_Trigger1_Movement_0:
# 44 "data//maps/LilyPondCave/scripts.pory"
	walk_up
	step_end

# 58 "data//maps/LilyPondCave/scripts.pory"
Cormoria_LilyPondCave_Martha_Battle_Trigger2_Movement_0:
# 58 "data//maps/LilyPondCave/scripts.pory"
	walk_up
# 58 "data//maps/LilyPondCave/scripts.pory"
	walk_left
# 58 "data//maps/LilyPondCave/scripts.pory"
	face_up
	step_end

# 75 "data//maps/LilyPondCave/scripts.pory"
Cormoria_LilyPondCave_Martha_Battle_Movement_0:
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_right
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_left
# 75 "data//maps/LilyPondCave/scripts.pory"
	walk_down
	step_end

# 76 "data//maps/LilyPondCave/scripts.pory"
Cormoria_LilyPondCave_Martha_Battle_Movement_1:
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_right
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_right
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_down
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_left
# 76 "data//maps/LilyPondCave/scripts.pory"
	walk_down
	step_end

Cormoria_LilyPondCave_Martha_Battle_Text_0:
# 68 "data//maps/LilyPondCave/scripts.pory"
	.string "Martha$"

Cormoria_LilyPondCave_Martha_Battle_Text_1:
# 68 "data//maps/LilyPondCave/scripts.pory"
	.string "Terrible, you are, making someone my age\n"
	.string "travel so far.\p"
	.string "But I suppose I owe the Fifth Cormoria\n"
	.string "Champion a fitting welcome.\p"
	.string "You know, back when I won the inagural\n"
	.string "Pokemon Championship, Cormoria was\l"
	.string "much younger...and much wilder!\l"
	.string "Trainers competed furiously for the\l"
	.string "prestige of being the inagural title!\p"
	.string "But of course, there were many losers\n"
	.string "and only one victor.\p"
	.string "I have fond memories of those days,\n"
	.string "{PLAYER}. I wish I could show you the\l"
	.string "Cormoria that used to be!\p"
	.string "Haha! Not Ancient Cormoria!\n"
	.string "Just my era.\p"
	.string "But these old bones haven't been\n"
	.string "sitting idle, you know? I've been\l"
	.string "keeping up with the times. Professor\l"
	.string "Tenebris gifted me a wonderful new\l"
	.string "Pokemon from the past!\p"
	.string "...\p"
	.string "It's time {PLAYER}.\p"
	.string "This is your final battle.\p"
	.string "Leave no regrets!$"

Cormoria_LilyPondCave_Martha_Battle_Text_2:
# 70 "data//maps/LilyPondCave/scripts.pory"
	.string "Oh my! A spectacular battle!$"

Cormoria_LilyPondCave_Martha_Battle_Text_3:
# 72 "data//maps/LilyPondCave/scripts.pory"
	.string "After I became the First Cormoria\n"
	.string "Champion, I tucked away my Trainer Card\l"
	.string "and pursued a career as a historian. All\l"
	.string "the other Champions did the same. I\l"
	.string "suppose that's what makes us Champions\l"
	.string "after all - we don't stop, for there are\l"
	.string "always new peaks to scale!\p"
	.string "Your journey never truly ends,\n"
	.string "{PLAYER}. Find your next peak and\l"
	.string "scale it!$"
