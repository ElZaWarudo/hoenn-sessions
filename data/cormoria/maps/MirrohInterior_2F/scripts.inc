Cormoria_MirrohInterior_2F_MapScripts::
# 2 "data//maps/MirrohInterior_2F/scripts.pory"
	map_script MAP_SCRIPT_ON_RESUME, Cormoria_MirrohInterior_2F_SetHoleWarp
	.byte 0


Cormoria_MirrohInterior_2F_SetHoleWarp::
# 6 "data//maps/MirrohInterior_2F/scripts.pory"
	setstepcallback STEP_CB_CRACKED_FLOOR
# 7 "data//maps/MirrohInterior_2F/scripts.pory"
	setholewarp MAP_CORMORIA_MIRROH_INTERIOR_B1F
	end


Cormoria_MirrohInterior_2F_CavePainting_WalkingWake::
# 28 "data//maps/MirrohInterior_2F/scripts.pory"
	lockall
# 31 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_0
# 32 "data//maps/MirrohInterior_2F/scripts.pory"
	release
# 33 "data//maps/MirrohInterior_2F/scripts.pory"
	delay 8
# 35 "data//maps/MirrohInterior_2F/scripts.pory"
	checkitem ITEM_ARCHAEOLENS_2
# 36 "data//maps/MirrohInterior_2F/scripts.pory"
	compare VAR_RESULT, TRUE
	goto_if_eq Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_2
# 70 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_5
# 71 "data//maps/MirrohInterior_2F/scripts.pory"
	playse SE_CLICK
# 72 "data//maps/MirrohInterior_2F/scripts.pory"
	delay 8
# 73 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_2
# 74 "data//maps/MirrohInterior_2F/scripts.pory"
	release
# 75 "data//maps/MirrohInterior_2F/scripts.pory"
	delay 8
# 76 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_3
# 77 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_1:
# 80 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	end

Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_2:
# 38 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_1
# 39 "data//maps/MirrohInterior_2F/scripts.pory"
	playse SE_CLICK
# 40 "data//maps/MirrohInterior_2F/scripts.pory"
	delay 8
# 41 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_2
# 42 "data//maps/MirrohInterior_2F/scripts.pory"
	release
# 43 "data//maps/MirrohInterior_2F/scripts.pory"
	delay 8
# 44 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_3
# 45 "data//maps/MirrohInterior_2F/scripts.pory"
	release
# 46 "data//maps/MirrohInterior_2F/scripts.pory"
	delay 8
# 48 "data//maps/MirrohInterior_2F/scripts.pory"
	goto_if_unset Cormoria_FLAG_ROARING_MOON, Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_5
	goto Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_1

Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_5:
# 49 "data//maps/MirrohInterior_2F/scripts.pory"
	setvar VAR_0x8004, 2
# 50 "data//maps/MirrohInterior_2F/scripts.pory"
	setvar VAR_0x8005, 2
# 51 "data//maps/MirrohInterior_2F/scripts.pory"
	setvar VAR_0x8006, 30
# 52 "data//maps/MirrohInterior_2F/scripts.pory"
	setvar VAR_0x8007, 5
# 53 "data//maps/MirrohInterior_2F/scripts.pory"
	special ShakeCamera
# 54 "data//maps/MirrohInterior_2F/scripts.pory"
	waitstate
# 55 "data//maps/MirrohInterior_2F/scripts.pory"
	release
# 57 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_4
# 58 "data//maps/MirrohInterior_2F/scripts.pory"
	delay 8
# 60 "data//maps/MirrohInterior_2F/scripts.pory"
	setflag Cormoria_FLAG_ROARING_MOON
# 62 "data//maps/MirrohInterior_2F/scripts.pory"
	setwildbattle SPECIES_ROARING_MOON, 50
# 63 "data//maps/MirrohInterior_2F/scripts.pory"
	dowildbattle
# 65 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	end


Cormoria_MirrohInterior_2F_SkiierA::
# 90 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_SkiierA_Text_0
# 91 "data//maps/MirrohInterior_2F/scripts.pory"
	release
	end


Cormoria_MirrohInterior_2F_TriathleteA::
# 95 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_TriathleteA_Text_0
# 96 "data//maps/MirrohInterior_2F/scripts.pory"
	release
	end


Cormoria_MirrohInterior_2F_Voltorb::
# 100 "data//maps/MirrohInterior_2F/scripts.pory"
	playmoncry SPECIES_VOLTORB, CRY_MODE_NORMAL
# 101 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Voltorb_Text_0
# 102 "data//maps/MirrohInterior_2F/scripts.pory"
	release
	end


Cormoria_MirrohInterior_2F_Sandshrew::
# 106 "data//maps/MirrohInterior_2F/scripts.pory"
	playmoncry SPECIES_SANDSHREW, CRY_MODE_NORMAL
# 107 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Sandshrew_Text_0
# 108 "data//maps/MirrohInterior_2F/scripts.pory"
	release
	end


Cormoria_MirrohInterior_2F_SkiierB::
# 114 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_SkiierB_Text_0
# 115 "data//maps/MirrohInterior_2F/scripts.pory"
	release
	end


Cormoria_MirrohInterior_2F_TriathleteB::
# 119 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_TriathleteB_Text_0
# 120 "data//maps/MirrohInterior_2F/scripts.pory"
	release
	end


Cormoria_MirrohInterior_2F_Lycanroc::
# 124 "data//maps/MirrohInterior_2F/scripts.pory"
	playmoncry SPECIES_LYCANROC, CRY_MODE_NORMAL
# 125 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Lycanroc_Text_0
# 126 "data//maps/MirrohInterior_2F/scripts.pory"
	release
	end


Cormoria_MirrohInterior_2F_Snover::
# 130 "data//maps/MirrohInterior_2F/scripts.pory"
	playmoncry SPECIES_SNOVER, CRY_MODE_NORMAL
# 131 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Snover_Text_0
# 132 "data//maps/MirrohInterior_2F/scripts.pory"
	release
	end


Cormoria_MirrohInterior_2F_Trainer_A::
# 146 "data//maps/MirrohInterior_2F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT2_A, Cormoria_MirrohInterior_2F_Trainer_A_Text_0, Cormoria_MirrohInterior_2F_Trainer_A_Text_1
# 147 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Trainer_A_Text_2
# 148 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_2F_Trainer_B::
# 152 "data//maps/MirrohInterior_2F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT2_B, Cormoria_MirrohInterior_2F_Trainer_B_Text_0, Cormoria_MirrohInterior_2F_Trainer_B_Text_1
# 153 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Trainer_B_Text_2
# 154 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_2F_Trainer_C::
# 158 "data//maps/MirrohInterior_2F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT2_C, Cormoria_MirrohInterior_2F_Trainer_C_Text_0, Cormoria_MirrohInterior_2F_Trainer_C_Text_1
# 159 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Trainer_C_Text_2
# 160 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_2F_Trainer_D::
# 164 "data//maps/MirrohInterior_2F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT2_D, Cormoria_MirrohInterior_2F_Trainer_D_Text_0, Cormoria_MirrohInterior_2F_Trainer_D_Text_1
# 165 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Trainer_D_Text_2
# 166 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_2F_Trainer_E::
# 174 "data//maps/MirrohInterior_2F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT2_E, Cormoria_MirrohInterior_2F_Trainer_E_Text_0, Cormoria_MirrohInterior_2F_Trainer_E_Text_1
# 175 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Trainer_E_Text_2
# 176 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_2F_Trainer_F::
# 180 "data//maps/MirrohInterior_2F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT2_F, Cormoria_MirrohInterior_2F_Trainer_F_Text_0, Cormoria_MirrohInterior_2F_Trainer_F_Text_1
# 181 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Trainer_F_Text_2
# 182 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_2F_Trainer_G::
# 187 "data//maps/MirrohInterior_2F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT2_G, Cormoria_MirrohInterior_2F_Trainer_G_Text_0, Cormoria_MirrohInterior_2F_Trainer_G_Text_1
# 188 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_Trainer_G_Text_2
# 189 "data//maps/MirrohInterior_2F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_2F_NPC_A::
# 209 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_NPC_A_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_2F_NPC_B::
# 214 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_NPC_B_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_2F_NPC_C::
# 219 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_NPC_C_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_2F_NPC_D::
# 224 "data//maps/MirrohInterior_2F/scripts.pory"
	msgbox Cormoria_MirrohInterior_2F_NPC_D_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_0:
# 31 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "It's an ancient cave painting...$"

Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_1:
# 38 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Scanning the ancient painting\n"
	.string "with the Archaeolens 2.0...$"

Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_2:
# 41 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Done!$"

Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_3:
# 44 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "It's a never before seen Pokémon! It\n"
	.string "looks reptilian, but also has feathers.\p"
	.string "Terrifying!$"

Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_4:
# 57 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "What was that?\p"
	.string "...\p"
	.string "The Archaeolens 2.0 is overheating!\n"
	.string "What's happening?$"

Cormoria_MirrohInterior_2F_CavePainting_WalkingWake_Text_5:
# 70 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Scanning the ancient painting\n"
	.string "with the Archaeolens...$"

Cormoria_MirrohInterior_2F_SkiierA_Text_0:
# 90 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "How dare you! Skiing is the best\n"
	.string "endurance sport!$"

Cormoria_MirrohInterior_2F_TriathleteA_Text_0:
# 95 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Are you crazy? Running is the best\n"
	.string "endurance sport!$"

Cormoria_MirrohInterior_2F_Voltorb_Text_0:
# 101 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Voltorb is fighting to prove that\n"
	.string "running is better!$"

Cormoria_MirrohInterior_2F_Sandshrew_Text_0:
# 107 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Sandshrew refuses to believe anything\n"
	.string "but skiing is the best!$"

Cormoria_MirrohInterior_2F_SkiierB_Text_0:
# 114 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Why don't you runners get it?\n"
	.string "Skiing is just superior!$"

Cormoria_MirrohInterior_2F_TriathleteB_Text_0:
# 119 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "The snow has addled your brains!\n"
	.string "Clear it out with a good run!$"

Cormoria_MirrohInterior_2F_Lycanroc_Text_0:
# 125 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Lycanroc is fighting...but only\n"
	.string "half-heartedly.\p"
	.string "Perhaps it wants to try skiing once?$"

Cormoria_MirrohInterior_2F_Snover_Text_0:
# 131 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Snover won't back down...but isn't\n"
	.string "showing much energy.\p"
	.string "Maybe it's looking to try new things?$"

Cormoria_MirrohInterior_2F_Trainer_A_Text_0:
# 146 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Deep in the depths of Mt. Mirroh, I\n"
	.string "sense psychic energy!$"

Cormoria_MirrohInterior_2F_Trainer_A_Text_1:
# 146 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Nope, there's no psychic energy!$"

Cormoria_MirrohInterior_2F_Trainer_A_Text_2:
# 147 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "I suppose there's psychic energy\n"
	.string "everywhere. It's like happiness - a\l"
	.string "state of mind.$"

Cormoria_MirrohInterior_2F_Trainer_B_Text_0:
# 152 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Why are you here? Are you trying to\n"
	.string "steal my Pokémon like those Team\l"
	.string "Somber grunts?$"

Cormoria_MirrohInterior_2F_Trainer_B_Text_1:
# 152 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "I guess you're not!$"

Cormoria_MirrohInterior_2F_Trainer_B_Text_2:
# 153 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "When I was on the upper floor, some\n"
	.string "Team Somber grunts tried to take my\l"
	.string "Pokémon. But I beat one of them and ran\l"
	.string "away from the other!$"

Cormoria_MirrohInterior_2F_Trainer_C_Text_0:
# 158 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "So what if it does? Running needs more\n"
	.string "endurance than skiing!\p"
	.string "Hey, trainer!\n"
	.string "You decide which one is better!$"

Cormoria_MirrohInterior_2F_Trainer_C_Text_1:
# 158 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Even my endurance didn't help!$"

Cormoria_MirrohInterior_2F_Trainer_C_Text_2:
# 159 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "I don't know why we argue when the\n"
	.string "bottom line is that all sports are fun.$"

Cormoria_MirrohInterior_2F_Trainer_D_Text_0:
# 164 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Why don't runners get it? Skiing takes\n"
	.string "more technique than running!$"

Cormoria_MirrohInterior_2F_Trainer_D_Text_1:
# 164 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Even my technique didn't help!$"

Cormoria_MirrohInterior_2F_Trainer_D_Text_2:
# 165 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "To be frank, if I didn't have access to a\n"
	.string "ski slope, I'd be running myself.$"

Cormoria_MirrohInterior_2F_Trainer_E_Text_0:
# 174 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Didn't I battle you before in Route 3?\n"
	.string "You beat me and my sister!$"

Cormoria_MirrohInterior_2F_Trainer_E_Text_1:
# 174 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "You win again!$"

Cormoria_MirrohInterior_2F_Trainer_E_Text_2:
# 175 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "I'm going to collect all 8 gym badges!\n"
	.string "But I only have 5 so far.$"

Cormoria_MirrohInterior_2F_Trainer_F_Text_0:
# 180 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Check out my new friends!$"

Cormoria_MirrohInterior_2F_Trainer_F_Text_1:
# 180 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Don't hurt them!$"

Cormoria_MirrohInterior_2F_Trainer_F_Text_2:
# 181 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "I'm a huge Pokefan! They're my best\n"
	.string "friends! But I have human friends too.$"

Cormoria_MirrohInterior_2F_Trainer_G_Text_0:
# 187 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "I can't wait to get out of this cave and\n"
	.string "collect my 6th gym badge at Winterlily\l"
	.string "Hollow. Yep, that's right! I'm not your\l"
	.string "average trainer - I'm on my way to\l"
	.string "become the Pokémon Champion!$"

Cormoria_MirrohInterior_2F_Trainer_G_Text_1:
# 187 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "I've still got a long way to go!$"

Cormoria_MirrohInterior_2F_Trainer_G_Text_2:
# 188 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "Four more badges to go...whew, this gym\n"
	.string "challenge is exhausting!$"

Cormoria_MirrohInterior_2F_NPC_A_Text_0:
# 209 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "On the way to Winterlily Hollow? I live\n"
	.string "there. It's a little further ahead, to\l"
	.string "the right and up, past the icy floor.$"

Cormoria_MirrohInterior_2F_NPC_B_Text_0:
# 214 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "My husband is an avid history buff.\n"
	.string "He's down there right now, examining\l"
	.string "some old cave paintings.$"

Cormoria_MirrohInterior_2F_NPC_C_Text_0:
# 219 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "What a fearsome Pokémon! One must be\n"
	.string "grateful it's not alive today.$"

Cormoria_MirrohInterior_2F_NPC_D_Text_0:
# 224 "data//maps/MirrohInterior_2F/scripts.pory"
	.string "This cave is so exhausting! I can't wait\n"
	.string "to be on the other side.$"
