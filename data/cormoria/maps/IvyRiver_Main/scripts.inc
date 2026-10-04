# 1 "data//maps/IvyRiver_Main/scripts.pory"

# 2 "data//maps/IvyRiver_Main/scripts.pory"

# 3 "data//maps/IvyRiver_Main/scripts.pory"
    .set Cormoria_maps_IvyRiver_Main_scripts_LOCALID_BRO, 5
# 4 "data//maps/IvyRiver_Main/scripts.pory"
    .set Cormoria_maps_IvyRiver_Main_scripts_LOCALID_ZA, 6
# 5 "data//maps/IvyRiver_Main/scripts.pory"
    .set Cormoria_maps_IvyRiver_Main_scripts_LOCALID_F1, 7
# 6 "data//maps/IvyRiver_Main/scripts.pory"
    .set Cormoria_maps_IvyRiver_Main_scripts_LOCALID_F2, 8
# 7 "data//maps/IvyRiver_Main/scripts.pory"
    .set Cormoria_maps_IvyRiver_Main_scripts_LOCALID_F3, 9
# 8 "data//maps/IvyRiver_Main/scripts.pory"
    .set Cormoria_maps_IvyRiver_Main_scripts_LOCALID_SHIP, 11

Cormoria_IvyRiver_Main_MapScripts::
	.byte 0


Cormoria_IvyRiver_Trainer_A::
# 22 "data//maps/IvyRiver_Main/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_IVY_RIVER_A, Cormoria_IvyRiver_Trainer_A_Text_0, Cormoria_IvyRiver_Trainer_A_Text_1
# 28 "data//maps/IvyRiver_Main/scripts.pory"
	msgbox Cormoria_IvyRiver_Trainer_A_Text_2
# 29 "data//maps/IvyRiver_Main/scripts.pory"
	release
	end


Cormoria_IvyRiver_Trainer_B::
# 35 "data//maps/IvyRiver_Main/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_IVY_RIVER_B, Cormoria_IvyRiver_Trainer_B_Text_0, Cormoria_IvyRiver_Trainer_B_Text_1
# 41 "data//maps/IvyRiver_Main/scripts.pory"
	msgbox Cormoria_IvyRiver_Trainer_B_Text_2
# 42 "data//maps/IvyRiver_Main/scripts.pory"
	release
	end


Cormoria_IvyRiver_Trainer_C::
# 48 "data//maps/IvyRiver_Main/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_IVY_RIVER_C, Cormoria_IvyRiver_Trainer_C_Text_0, Cormoria_IvyRiver_Trainer_C_Text_1
# 54 "data//maps/IvyRiver_Main/scripts.pory"
	msgbox Cormoria_IvyRiver_Trainer_C_Text_2
# 55 "data//maps/IvyRiver_Main/scripts.pory"
	release
	end


Cormoria_IvyRiver_Trainer_D::
# 60 "data//maps/IvyRiver_Main/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_IVY_RIVER_D, Cormoria_IvyRiver_Trainer_D_Text_0, Cormoria_IvyRiver_Trainer_D_Text_1
# 66 "data//maps/IvyRiver_Main/scripts.pory"
	msgbox Cormoria_IvyRiver_Trainer_D_Text_2
# 67 "data//maps/IvyRiver_Main/scripts.pory"
	release
	end


Cormoria_IvyRiver_Trainer_D_Pinsir::
# 71 "data//maps/IvyRiver_Main/scripts.pory"
	playmoncry SPECIES_PINSIR, CRY_MODE_NORMAL
# 72 "data//maps/IvyRiver_Main/scripts.pory"
	msgbox Cormoria_IvyRiver_Trainer_D_Pinsir_Text_0, MSGBOX_NPC
	end


Cormoria_IvyRiver_NPC1::
# 78 "data//maps/IvyRiver_Main/scripts.pory"
	msgbox Cormoria_IvyRiver_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_IvyRiver_NPC2::
# 82 "data//maps/IvyRiver_Main/scripts.pory"
	msgbox Cormoria_IvyRiver_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_IvyRiver_Trainer_A_Text_0:
# 24 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Are you collecting rare Pokémon too?$"

Cormoria_IvyRiver_Trainer_A_Text_1:
# 25 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Wow, your Pokémon are rare!$"

Cormoria_IvyRiver_Trainer_A_Text_2:
# 28 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Because Ivy River is untouched by\n"
	.string "people, rare Pokémon can thrive.$"

Cormoria_IvyRiver_Trainer_B_Text_0:
# 37 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Swimming in this river is only for the\n"
	.string "toughest swimmers like me.\p"
	.string "I don't even need the HM Flip Turn!$"

Cormoria_IvyRiver_Trainer_B_Text_1:
# 38 "data//maps/IvyRiver_Main/scripts.pory"
	.string "I bet I could beat you if you weren't\n"
	.string "surfing on your Pokémon!$"

Cormoria_IvyRiver_Trainer_B_Text_2:
# 41 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Swimming is the best full-body\n"
	.string "exercise!$"

Cormoria_IvyRiver_Trainer_C_Text_0:
# 50 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Deep in the river lurks an unknown\n"
	.string "malaise...keep your eyes wide open or\l"
	.string "it'll eat your mind!$"

Cormoria_IvyRiver_Trainer_C_Text_1:
# 51 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Your mind can't be eaten!$"

Cormoria_IvyRiver_Trainer_C_Text_2:
# 54 "data//maps/IvyRiver_Main/scripts.pory"
	.string "To be honest with you, there's no\n"
	.string "malaise. There are strong currents, and\l"
	.string "that's about it.$"

Cormoria_IvyRiver_Trainer_D_Text_0:
# 62 "data//maps/IvyRiver_Main/scripts.pory"
	.string "You're the trainer who beat me back in\n"
	.string "Vilethorn Woods!\p"
	.string "I wanted to become the first BUG type\n"
	.string "Gym Leader but guess what - I've\l"
	.string "changed my dream. Now I'm going to be\l"
	.string "the first BUG type Pokémon Champion!$"

Cormoria_IvyRiver_Trainer_D_Text_1:
# 63 "data//maps/IvyRiver_Main/scripts.pory"
	.string "How could I lose to you twice?$"

Cormoria_IvyRiver_Trainer_D_Text_2:
# 66 "data//maps/IvyRiver_Main/scripts.pory"
	.string "I lost to you today, but I'll beat you at\n"
	.string "the Pokémon Championship!$"

Cormoria_IvyRiver_Trainer_D_Pinsir_Text_0:
# 72 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Pinsir looks grumpy it lost.$"

Cormoria_IvyRiver_NPC1_Text_0:
# 78 "data//maps/IvyRiver_Main/scripts.pory"
	.string "You can't navigate the strong currents\n"
	.string "here without HM Flip Turn.\p"
	.string "Oh, you don't have it?\p"
	.string "Too bad.$"

Cormoria_IvyRiver_NPC2_Text_0:
# 82 "data//maps/IvyRiver_Main/scripts.pory"
	.string "Most people you'll meet here are either\n"
	.string "collecting rare Pokémon...or lost!\p"
	.string "Like me...$"
