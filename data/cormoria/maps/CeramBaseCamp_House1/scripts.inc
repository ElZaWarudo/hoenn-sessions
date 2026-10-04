Cormoria_CeramBaseCamp_House1_MapScripts::
	.byte 0


Cormoria_CeramBaseCamp_House1_NPC::
# 5 "data//maps/CeramBaseCamp_House1/scripts.pory"
	goto_if_set Cormoria_FLAG_CERAMBASECAMP_ALTARIA_FEATHER, Cormoria_CeramBaseCamp_House1_NPC_2
# 32 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_NPC_Text_3
# 33 "data//maps/CeramBaseCamp_House1/scripts.pory"
	release
Cormoria_CeramBaseCamp_House1_NPC_1:
	end

Cormoria_CeramBaseCamp_House1_NPC_2:
# 6 "data//maps/CeramBaseCamp_House1/scripts.pory"
	checkitem ITEM_PRETTY_FEATHER
# 7 "data//maps/CeramBaseCamp_House1/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_CeramBaseCamp_House1_NPC_5
# 28 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_NPC_Text_3
# 29 "data//maps/CeramBaseCamp_House1/scripts.pory"
	release
	goto Cormoria_CeramBaseCamp_House1_NPC_1

Cormoria_CeramBaseCamp_House1_NPC_5:
# 8 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_NPC_Text_0, MSGBOX_YESNO
# 10 "data//maps/CeramBaseCamp_House1/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_CeramBaseCamp_House1_NPC_8
# 22 "data//maps/CeramBaseCamp_House1/scripts.pory"
	release
	end

Cormoria_CeramBaseCamp_House1_NPC_8:
# 11 "data//maps/CeramBaseCamp_House1/scripts.pory"
	release
# 12 "data//maps/CeramBaseCamp_House1/scripts.pory"
	delay 16
# 13 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_NPC_Text_1
# 14 "data//maps/CeramBaseCamp_House1/scripts.pory"
	release
# 15 "data//maps/CeramBaseCamp_House1/scripts.pory"
	delay 16
# 16 "data//maps/CeramBaseCamp_House1/scripts.pory"
	applymovement 1, Cormoria_CeramBaseCamp_House1_NPC_Movement_0
# 17 "data//maps/CeramBaseCamp_House1/scripts.pory"
	waitmovement 0
# 18 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_NPC_Text_2
# 19 "data//maps/CeramBaseCamp_House1/scripts.pory"
	release
	end


Cormoria_CeramBaseCamp_House1_Pokemon::
# 40 "data//maps/CeramBaseCamp_House1/scripts.pory"
	goto_if_set Cormoria_FLAG_CERAMBASECAMP_ALTARIA_FEATHER, Cormoria_CeramBaseCamp_House1_Pokemon_2
# 46 "data//maps/CeramBaseCamp_House1/scripts.pory"
	lockall
# 47 "data//maps/CeramBaseCamp_House1/scripts.pory"
	faceplayer
# 48 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_Pokemon_Text_1
# 49 "data//maps/CeramBaseCamp_House1/scripts.pory"
	release
# 50 "data//maps/CeramBaseCamp_House1/scripts.pory"
	applymovement 2, Cormoria_CeramBaseCamp_House1_Pokemon_Movement_0
# 51 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_Pokemon_Text_2
# 52 "data//maps/CeramBaseCamp_House1/scripts.pory"
	giveitem ITEM_PRETTY_FEATHER
# 53 "data//maps/CeramBaseCamp_House1/scripts.pory"
	release
# 54 "data//maps/CeramBaseCamp_House1/scripts.pory"
	delay 30
# 56 "data//maps/CeramBaseCamp_House1/scripts.pory"
	applymovement 2, Cormoria_CeramBaseCamp_House1_Pokemon_Movement_1
# 57 "data//maps/CeramBaseCamp_House1/scripts.pory"
	waitmovement 2
# 58 "data//maps/CeramBaseCamp_House1/scripts.pory"
	delay 24
# 59 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_Pokemon_Text_3, MSGBOX_NPC
# 60 "data//maps/CeramBaseCamp_House1/scripts.pory"
	setflag Cormoria_FLAG_CERAMBASECAMP_ALTARIA_FEATHER
# 61 "data//maps/CeramBaseCamp_House1/scripts.pory"
	releaseall
	return

Cormoria_CeramBaseCamp_House1_Pokemon_2:
# 41 "data//maps/CeramBaseCamp_House1/scripts.pory"
	msgbox Cormoria_CeramBaseCamp_House1_Pokemon_Text_0, MSGBOX_NPC
	end


# 16 "data//maps/CeramBaseCamp_House1/scripts.pory"
Cormoria_CeramBaseCamp_House1_NPC_Movement_0:
# 16 "data//maps/CeramBaseCamp_House1/scripts.pory"
	emote_exclamation_mark
# 16 "data//maps/CeramBaseCamp_House1/scripts.pory"
	jump_in_place_right
	step_end

# 50 "data//maps/CeramBaseCamp_House1/scripts.pory"
Cormoria_CeramBaseCamp_House1_Pokemon_Movement_0:
# 50 "data//maps/CeramBaseCamp_House1/scripts.pory"
	emote_exclamation_mark
	step_end

# 56 "data//maps/CeramBaseCamp_House1/scripts.pory"
Cormoria_CeramBaseCamp_House1_Pokemon_Movement_1:
# 56 "data//maps/CeramBaseCamp_House1/scripts.pory"
	jump_in_place_left
# 56 "data//maps/CeramBaseCamp_House1/scripts.pory"
	emote_exclamation_mark
	step_end

Cormoria_CeramBaseCamp_House1_NPC_Text_0:
# 8 "data//maps/CeramBaseCamp_House1/scripts.pory"
	.string "She's fallen asleep on her chair...such\n"
	.string "peace can only come with clarity.\p"
	.string "Tickle her nose with a Pretty Feather?$"

Cormoria_CeramBaseCamp_House1_NPC_Text_1:
# 13 "data//maps/CeramBaseCamp_House1/scripts.pory"
	.string "...\p"
	.string "...\p"
	.string "...$"

Cormoria_CeramBaseCamp_House1_NPC_Text_2:
# 18 "data//maps/CeramBaseCamp_House1/scripts.pory"
	.string "-achoo!-\p"
	.string "...\p"
	.string "...\p"
	.string "...she's asleep again.$"

Cormoria_CeramBaseCamp_House1_NPC_Text_3:
# 28 "data//maps/CeramBaseCamp_House1/scripts.pory"
	.string "She's fallen asleep on her chair...such\n"
	.string "peace can only come with clarity.$"

Cormoria_CeramBaseCamp_House1_Pokemon_Text_0:
# 41 "data//maps/CeramBaseCamp_House1/scripts.pory"
	.string "Altaria is dancing! It looks so joyous.\n"
	.string "Better not distract it!$"

Cormoria_CeramBaseCamp_House1_Pokemon_Text_1:
# 48 "data//maps/CeramBaseCamp_House1/scripts.pory"
	.string "Altaria is dancing! What rhythm is this?\p"
	.string "...\p"
	.string "It's so elegant...$"

Cormoria_CeramBaseCamp_House1_Pokemon_Text_2:
# 51 "data//maps/CeramBaseCamp_House1/scripts.pory"
	.string "Altaria comes closer and ruffles one of\n"
	.string "its glorious wings...\p"
	.string "...\p"
	.string "...a feather floats gently in your palm.$"

Cormoria_CeramBaseCamp_House1_Pokemon_Text_3:
# 59 "data//maps/CeramBaseCamp_House1/scripts.pory"
	.string "Altaria nods towards the lady sleeping\n"
	.string "in the chair...what does it want?$"
