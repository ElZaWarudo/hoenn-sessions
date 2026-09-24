# 1 "data//maps/Route3/scripts.pory"

# 2 "data//maps/Route3/scripts.pory"
    .set Cormoria_maps_Route3_scripts_LOCALID_SKARM, 12

Cormoria_Route3_MapScripts::
	.byte 0


Cormoria_Route3_TrainerBattle_Kim::
# 9 "data//maps/Route3/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_ROUTE3_KIM, Cormoria_Route3_TrainerBattle_Kim_Text_0, Cormoria_Route3_TrainerBattle_Kim_Text_1
# 10 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_TrainerBattle_Kim_Text_2
# 11 "data//maps/Route3/scripts.pory"
	release
	end


Cormoria_Route3_TrainerBattle_Ronnie::
# 18 "data//maps/Route3/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_ROUTE3_RONNIE, Cormoria_Route3_TrainerBattle_Ronnie_Text_0, Cormoria_Route3_TrainerBattle_Ronnie_Text_1
# 19 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_TrainerBattle_Ronnie_Text_2
# 20 "data//maps/Route3/scripts.pory"
	release
	end


Cormoria_Route3_TrainerBattle_Jana::
# 27 "data//maps/Route3/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_ROUTE3_JANA, Cormoria_Route3_TrainerBattle_Jana_Text_0, Cormoria_Route3_TrainerBattle_Jana_Text_1
# 28 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_TrainerBattle_Jana_Text_2
# 29 "data//maps/Route3/scripts.pory"
	release
	end


Cormoria_Route3_TrainerBattle_Chara::
# 36 "data//maps/Route3/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_ROUTE3_CHARA, Cormoria_Route3_TrainerBattle_Chara_Text_0, Cormoria_Route3_TrainerBattle_Chara_Text_1
# 37 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_TrainerBattle_Chara_Text_2
# 38 "data//maps/Route3/scripts.pory"
	release
	end


Cormoria_Route3_NPC1::
# 54 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_Route3_NPC2::
# 59 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_Route3_Skarmory_Trigger::
# 71 "data//maps/Route3/scripts.pory"
	goto_if_set Cormoria_FLAG_ROUTE3_SKARMORY_WILD, Cormoria_Route3_Skarmory_Trigger_2
# 72 "data//maps/Route3/scripts.pory"
	lockall
# 73 "data//maps/Route3/scripts.pory"
	faceplayer
# 74 "data//maps/Route3/scripts.pory"
	playmoncry SPECIES_SKARMORY, CRY_MODE_NORMAL
# 75 "data//maps/Route3/scripts.pory"
	applymovement Cormoria_maps_Route3_scripts_LOCALID_SKARM, Cormoria_Route3_Skarmory_Trigger_Movement_0
# 76 "data//maps/Route3/scripts.pory"
	waitmovement 0
# 77 "data//maps/Route3/scripts.pory"
	delay 16
# 78 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_Skarmory_Trigger_Text_0
# 80 "data//maps/Route3/scripts.pory"
	setvar VAR_TEMP_0, 1
# 82 "data//maps/Route3/scripts.pory"
	releaseall
	end

Cormoria_Route3_Skarmory_Trigger_2:
	end


Cormoria_Route3_Skarmory_Battle::
# 87 "data//maps/Route3/scripts.pory"
	lockall
# 88 "data//maps/Route3/scripts.pory"
	faceplayer
# 89 "data//maps/Route3/scripts.pory"
	playmoncry SPECIES_SKARMORY, CRY_MODE_NORMAL
# 90 "data//maps/Route3/scripts.pory"
	applymovement Cormoria_maps_Route3_scripts_LOCALID_SKARM, Cormoria_Route3_Skarmory_Trigger_Movement_0
# 91 "data//maps/Route3/scripts.pory"
	waitmovement 0
# 92 "data//maps/Route3/scripts.pory"
	delay 16
# 93 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_Skarmory_Battle_Text_0
# 94 "data//maps/Route3/scripts.pory"
	release
# 96 "data//maps/Route3/scripts.pory"
	setwildbattle SPECIES_SKARMORY, 12
# 97 "data//maps/Route3/scripts.pory"
	dowildbattle
# 100 "data//maps/Route3/scripts.pory"
	specialvar VAR_RESULT, GetBattleOutcome
# 101 "data//maps/Route3/scripts.pory"
	compare VAR_RESULT, B_OUTCOME_WON
	goto_if_eq Cormoria_Route3_Skarmory_Battle_2
Cormoria_Route3_Skarmory_Battle_1:
# 106 "data//maps/Route3/scripts.pory"
	compare VAR_RESULT, B_OUTCOME_CAUGHT
	goto_if_eq Cormoria_Route3_Skarmory_Battle_5
Cormoria_Route3_Skarmory_Battle_4:
# 111 "data//maps/Route3/scripts.pory"
	compare VAR_RESULT, B_OUTCOME_RAN
	goto_if_eq Cormoria_Route3_Skarmory_Battle_8
Cormoria_Route3_Skarmory_Battle_7:
# 116 "data//maps/Route3/scripts.pory"
	releaseall
	end

Cormoria_Route3_Skarmory_Battle_2:
# 102 "data//maps/Route3/scripts.pory"
	removeobject Cormoria_maps_Route3_scripts_LOCALID_SKARM
# 103 "data//maps/Route3/scripts.pory"
	setflag Cormoria_FLAG_ROUTE3_SKARMORY_WILD
# 104 "data//maps/Route3/scripts.pory"
	releaseall
	goto Cormoria_Route3_Skarmory_Battle_1

Cormoria_Route3_Skarmory_Battle_5:
# 107 "data//maps/Route3/scripts.pory"
	removeobject Cormoria_maps_Route3_scripts_LOCALID_SKARM
# 108 "data//maps/Route3/scripts.pory"
	setflag Cormoria_FLAG_ROUTE3_SKARMORY_WILD
# 109 "data//maps/Route3/scripts.pory"
	releaseall
	goto Cormoria_Route3_Skarmory_Battle_4

Cormoria_Route3_Skarmory_Battle_8:
# 112 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_Skarmory_Battle_Text_1
# 113 "data//maps/Route3/scripts.pory"
	release
	goto Cormoria_Route3_Skarmory_Battle_7


Cormoria_Route3_Ext_TrainerBattle_A::
# 135 "data//maps/Route3/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_ROUTE3_EXT_A, Cormoria_Route3_Ext_TrainerBattle_A_Text_0, Cormoria_Route3_Ext_TrainerBattle_A_Text_1
# 136 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_Ext_TrainerBattle_A_Text_2
# 137 "data//maps/Route3/scripts.pory"
	release
	end


Cormoria_Route3_Ext_TrainerBattle_B::
# 141 "data//maps/Route3/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_ROUTE3_EXT_B, Cormoria_Route3_Ext_TrainerBattle_B_Text_0, Cormoria_Route3_Ext_TrainerBattle_B_Text_1, Cormoria_Route3_Ext_DoubleBattle_ItemGive
# 142 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_Ext_TrainerBattle_B_Text_2
# 143 "data//maps/Route3/scripts.pory"
	release
	end


Cormoria_Route3_Ext_DoubleBattle_ItemGive::
# 147 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_Ext_DoubleBattle_ItemGive_Text_0
# 148 "data//maps/Route3/scripts.pory"
	goto Cormoria_Route3_Ext_DoubleBattle_Reward
	return


Cormoria_Route3_Ext_DoubleBattle_Reward::
# 152 "data//maps/Route3/scripts.pory"
	message Cormoria_Route3_Ext_DoubleBattle_Reward_Text_0
# 153 "data//maps/Route3/scripts.pory"
	waitmessage
# 154 "data//maps/Route3/scripts.pory"
	dynmultichoice 0, 0, TRUE, 2, 0, DYN_MULTICHOICE_CB_NONE, Cormoria_Route3_Ext_DoubleBattle_Reward_Text_1, Cormoria_Route3_Ext_DoubleBattle_Reward_Text_2
# 155 "data//maps/Route3/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_Route3_Ext_DoubleBattle_Reward_2
Cormoria_Route3_Ext_DoubleBattle_Reward_1:
# 156 "data//maps/Route3/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_Route3_Ext_DoubleBattle_Reward_5
Cormoria_Route3_Ext_DoubleBattle_Reward_4:
# 157 "data//maps/Route3/scripts.pory"
	goto Cormoria_Route3_Ext_DoubleBattle_Reward_Give
	return

Cormoria_Route3_Ext_DoubleBattle_Reward_2:
# 155 "data//maps/Route3/scripts.pory"
	setvar VAR_0x8006, ITEM_BLACK_BELT
	goto Cormoria_Route3_Ext_DoubleBattle_Reward_1

Cormoria_Route3_Ext_DoubleBattle_Reward_5:
# 156 "data//maps/Route3/scripts.pory"
	setvar VAR_0x8006, ITEM_SHARP_BEAK
	goto Cormoria_Route3_Ext_DoubleBattle_Reward_4


Cormoria_Route3_Ext_DoubleBattle_Reward_Give::
# 161 "data//maps/Route3/scripts.pory"
	showitemdescription
# 162 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_Ext_DoubleBattle_Reward_Give_Text_0, MSGBOX_YESNO
# 163 "data//maps/Route3/scripts.pory"
	hideitemdescription
# 164 "data//maps/Route3/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_Route3_Ext_DoubleBattle_Reward_Give_1
# 165 "data//maps/Route3/scripts.pory"
	goto Cormoria_Route3_Ext_DoubleBattle_Reward
	return

Cormoria_Route3_Ext_DoubleBattle_Reward_Give_1:
# 164 "data//maps/Route3/scripts.pory"
	giveitem VAR_0x8006
# 164 "data//maps/Route3/scripts.pory"
	goto Cormoria_Route3_Ext_DoubleBattle_Reward_End
	return


Cormoria_Route3_Ext_DoubleBattle_Reward_End::
# 169 "data//maps/Route3/scripts.pory"
	msgbox Cormoria_Route3_Ext_DoubleBattle_Reward_End_Text_0
# 170 "data//maps/Route3/scripts.pory"
	releaseall
	end


# 75 "data//maps/Route3/scripts.pory"
Cormoria_Route3_Skarmory_Trigger_Movement_0:
# 75 "data//maps/Route3/scripts.pory"
	emote_exclamation_mark
# 75 "data//maps/Route3/scripts.pory"
	jump_in_place_left
	step_end

Cormoria_Route3_TrainerBattle_Kim_Text_0:
# 9 "data//maps/Route3/scripts.pory"
	.string "Mom gifted my sister and me a cool\n"
	.string "FLYING-type Pokémon!$"

Cormoria_Route3_TrainerBattle_Kim_Text_1:
# 9 "data//maps/Route3/scripts.pory"
	.string "My sister will beat you!$"

Cormoria_Route3_TrainerBattle_Kim_Text_2:
# 10 "data//maps/Route3/scripts.pory"
	.string "My sister is up ahead, near the Gastree\n"
	.string "City exit. She's super strong!$"

Cormoria_Route3_TrainerBattle_Ronnie_Text_0:
# 18 "data//maps/Route3/scripts.pory"
	.string "I'm on my way to Gastree City to\n"
	.string "challenge the Pokémon Gym, but I'm not\l"
	.string "sure I can win. Can I practice with you?$"

Cormoria_Route3_TrainerBattle_Ronnie_Text_1:
# 18 "data//maps/Route3/scripts.pory"
	.string "Looks like I need more practice!$"

Cormoria_Route3_TrainerBattle_Ronnie_Text_2:
# 19 "data//maps/Route3/scripts.pory"
	.string "I heard that the Gastree City Gym\n"
	.string "Leader uses ROCK type Pokémon.$"

Cormoria_Route3_TrainerBattle_Jana_Text_0:
# 27 "data//maps/Route3/scripts.pory"
	.string "I'm a Pokémon Breeder! All Breeders\n"
	.string "specialise in one Pokémon variety, such\l"
	.string "as dog, cat, or even bear Pokémon!\p"
	.string "Can you guess my specialty?$"

Cormoria_Route3_TrainerBattle_Jana_Text_1:
# 27 "data//maps/Route3/scripts.pory"
	.string "You guessed it - I love foxes!$"

Cormoria_Route3_TrainerBattle_Jana_Text_2:
# 28 "data//maps/Route3/scripts.pory"
	.string "My Pokémon are my most cherished\n"
	.string "partners!$"

Cormoria_Route3_TrainerBattle_Chara_Text_0:
# 36 "data//maps/Route3/scripts.pory"
	.string "Mom gifted my sister and me a cool\n"
	.string "FLYING-type Pokémon!\p"
	.string "...\p"
	.string "You met her and beat her?\p"
	.string "Well this is a revenge match then!$"

Cormoria_Route3_TrainerBattle_Chara_Text_1:
# 36 "data//maps/Route3/scripts.pory"
	.string "Aw I lost!$"

Cormoria_Route3_TrainerBattle_Chara_Text_2:
# 37 "data//maps/Route3/scripts.pory"
	.string "We love our Swablu!$"

Cormoria_Route3_NPC1_Text_0:
# 54 "data//maps/Route3/scripts.pory"
	.string "I love the high-altitude winds!\n"
	.string "I feel one with the hills.$"

Cormoria_Route3_NPC2_Text_0:
# 59 "data//maps/Route3/scripts.pory"
	.string "I was supposed to go to a marathon, but\n"
	.string "I don't think this is where it starts.$"

Cormoria_Route3_Skarmory_Trigger_Text_0:
# 78 "data//maps/Route3/scripts.pory"
	.string "Skarmory roars defiantly at you.\n"
	.string "Could be dangerous to approach.$"

Cormoria_Route3_Skarmory_Battle_Text_0:
# 93 "data//maps/Route3/scripts.pory"
	.string "The wild Skarmory roars and attacks!$"

Cormoria_Route3_Skarmory_Battle_Text_1:
# 112 "data//maps/Route3/scripts.pory"
	.string "The Skarmory preens its steely\n"
	.string "feathers victoriously.\n"
	.string "Better not disturb it again!$"

Cormoria_Route3_Ext_TrainerBattle_A_Text_0:
# 135 "data//maps/Route3/scripts.pory"
	.string "My buddy and I train here every day!\n"
	.string "Let me show you how strong we've\l"
	.string "become!$"

Cormoria_Route3_Ext_TrainerBattle_A_Text_1:
# 135 "data//maps/Route3/scripts.pory"
	.string "No way! Against the two of us!$"

Cormoria_Route3_Ext_TrainerBattle_A_Text_2:
# 136 "data//maps/Route3/scripts.pory"
	.string "We're going to keep training together\n"
	.string "until we become the strongest!$"

Cormoria_Route3_Ext_TrainerBattle_B_Text_0:
# 141 "data//maps/Route3/scripts.pory"
	.string "You wanna take on me and my buddy\n"
	.string "together? When we fight side by side,\l"
	.string "our strength multiplies!$"

Cormoria_Route3_Ext_TrainerBattle_B_Text_1:
# 141 "data//maps/Route3/scripts.pory"
	.string "How? It was two-on-one!$"

Cormoria_Route3_Ext_TrainerBattle_B_Text_2:
# 142 "data//maps/Route3/scripts.pory"
	.string "You need good friends if you want to\n"
	.string "reach the top!$"

Cormoria_Route3_Ext_DoubleBattle_ItemGive_Text_0:
# 147 "data//maps/Route3/scripts.pory"
	.string "What a battle! You beat the two of us\n"
	.string "together. You're really strong - strong\l"
	.string "enough to be our third buddy!$"

Cormoria_Route3_Ext_DoubleBattle_Reward_Text_0:
# 152 "data//maps/Route3/scripts.pory"
	.string "Take one of these - you gotta have one\n"
	.string "as our third buddy!$"

Cormoria_Route3_Ext_DoubleBattle_Reward_Text_1:
# 154 "data//maps/Route3/scripts.pory"
	.string "Black Belt$"

Cormoria_Route3_Ext_DoubleBattle_Reward_Text_2:
# 154 "data//maps/Route3/scripts.pory"
	.string "Sharp Beak$"

Cormoria_Route3_Ext_DoubleBattle_Reward_Give_Text_0:
# 162 "data//maps/Route3/scripts.pory"
	.string "Are you sure?$"

Cormoria_Route3_Ext_DoubleBattle_Reward_End_Text_0:
# 169 "data//maps/Route3/scripts.pory"
	.string "Come train with us again sometime!$"
