Cormoria_Championship_Corridor_MapScripts::
	.byte 0


Cormoria_Championship_Corridor_Register::
# 4 "data//maps/Championship_Corridor/scripts.pory"
	lockall
# 6 "data//maps/Championship_Corridor/scripts.pory"
	applymovement 1, Cormoria_Championship_Corridor_Register_Movement_0
# 8 "data//maps/Championship_Corridor/scripts.pory"
	compare Cormoria_VAR_CHAMP_STAGE, 1
	goto_if_eq Cormoria_Championship_Corridor_Register_2
Cormoria_Championship_Corridor_Register_1:
# 12 "data//maps/Championship_Corridor/scripts.pory"
	compare Cormoria_VAR_CHAMP_STAGE, 2
	goto_if_eq Cormoria_Championship_Corridor_Register_5
Cormoria_Championship_Corridor_Register_4:
# 16 "data//maps/Championship_Corridor/scripts.pory"
	compare Cormoria_VAR_CHAMP_STAGE, 3
	goto_if_eq Cormoria_Championship_Corridor_Register_8
Cormoria_Championship_Corridor_Register_7:
# 20 "data//maps/Championship_Corridor/scripts.pory"
	compare Cormoria_VAR_CHAMP_STAGE, 4
	goto_if_eq Cormoria_Championship_Corridor_Register_11
Cormoria_Championship_Corridor_Register_10:
# 25 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Register_Text_4, MSGBOX_POKENAV
# 26 "data//maps/Championship_Corridor/scripts.pory"
	dynmultichoice 10, 8, TRUE, 3, 0, DYN_MULTICHOICE_CB_NONE, Cormoria_Championship_Corridor_Register_Text_5, Cormoria_Championship_Corridor_Register_Text_6
# 28 "data//maps/Championship_Corridor/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_Championship_Corridor_Register_14
# 34 "data//maps/Championship_Corridor/scripts.pory"
	setvar Cormoria_VAR_CHAMP_STAGE, 0
# 35 "data//maps/Championship_Corridor/scripts.pory"
	special HealPlayerParty
# 36 "data//maps/Championship_Corridor/scripts.pory"
	warp MAP_CORMORIA_CHAMPIONSHIP, 40, 38
# 48 "data//maps/Championship_Corridor/scripts.pory"
	releaseall
	end

Cormoria_Championship_Corridor_Register_2:
# 9 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Register_Text_0
# 10 "data//maps/Championship_Corridor/scripts.pory"
	release
	goto Cormoria_Championship_Corridor_Register_1

Cormoria_Championship_Corridor_Register_5:
# 13 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Register_Text_1
# 14 "data//maps/Championship_Corridor/scripts.pory"
	release
	goto Cormoria_Championship_Corridor_Register_4

Cormoria_Championship_Corridor_Register_8:
# 17 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Register_Text_2
# 18 "data//maps/Championship_Corridor/scripts.pory"
	release
	goto Cormoria_Championship_Corridor_Register_7

Cormoria_Championship_Corridor_Register_11:
# 21 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Register_Text_3
# 22 "data//maps/Championship_Corridor/scripts.pory"
	release
	goto Cormoria_Championship_Corridor_Register_10

Cormoria_Championship_Corridor_Register_14:
# 29 "data//maps/Championship_Corridor/scripts.pory"
	releaseall
	end


Cormoria_Championship_Corridor_Register_Nurse::
# 53 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Register_Nurse_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_Exit_Nurse::
# 58 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Exit_Nurse_Text_0, MSGBOX_YESNO
# 59 "data//maps/Championship_Corridor/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_Championship_Corridor_Exit_Nurse_2
Cormoria_Championship_Corridor_Exit_Nurse_1:
	end

Cormoria_Championship_Corridor_Exit_Nurse_2:
# 60 "data//maps/Championship_Corridor/scripts.pory"
	special StartCredits
	goto Cormoria_Championship_Corridor_Exit_Nurse_1


Cormoria_Championship_Corridor_NPC1::
# 73 "data//maps/Championship_Corridor/scripts.pory"
	lockall
# 74 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_NPC1_Text_0
# 75 "data//maps/Championship_Corridor/scripts.pory"
	releaseall
	end


Cormoria_Championship_Corridor_Mon1::
# 80 "data//maps/Championship_Corridor/scripts.pory"
	playmoncry SPECIES_EELEKTROSS, CRY_MODE_NORMAL
# 81 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Mon1_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_NPC2::
# 87 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_Mon2::
# 92 "data//maps/Championship_Corridor/scripts.pory"
	playmoncry SPECIES_EXCADRILL, CRY_MODE_NORMAL
# 93 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Mon2_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_NPC3::
# 99 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_NPC3_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_Mon3::
# 104 "data//maps/Championship_Corridor/scripts.pory"
	playmoncry SPECIES_ROSERADE, CRY_MODE_NORMAL
# 105 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Mon3_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_NPC4::
# 111 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_NPC4_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_Mon4::
# 116 "data//maps/Championship_Corridor/scripts.pory"
	playmoncry SPECIES_TOXTRICITY, CRY_MODE_NORMAL
# 117 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Mon4_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_NPC5::
# 124 "data//maps/Championship_Corridor/scripts.pory"
	lockall
# 125 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_NPC5_Text_0
# 126 "data//maps/Championship_Corridor/scripts.pory"
	releaseall
	end


Cormoria_Championship_Corridor_Mon5::
# 131 "data//maps/Championship_Corridor/scripts.pory"
	playmoncry SPECIES_DRAGALGE, CRY_MODE_NORMAL
# 132 "data//maps/Championship_Corridor/scripts.pory"
	msgbox Cormoria_Championship_Corridor_Mon5_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_Corridor_TV::
# 137 "data//maps/Championship_Corridor/scripts.pory"
	playse SE_APPLAUSE
# 138 "data//maps/Championship_Corridor/scripts.pory"
	msgbox_delay Cormoria_Championship_Corridor_TV_Text_0, Cormoria_Championship_Corridor_TV_Text_1
	end


# 6 "data//maps/Championship_Corridor/scripts.pory"
Cormoria_Championship_Corridor_Register_Movement_0:
# 6 "data//maps/Championship_Corridor/scripts.pory"
	face_left
	step_end

Cormoria_Championship_Corridor_Register_Text_0:
# 9 "data//maps/Championship_Corridor/scripts.pory"
	.string "With your defeat to Chara, you have\n"
	.string "been eliminated from the Cormoria\l"
	.string "Pokémon Championship.\p"
	.string "Your final result is: Top 32.\p"
	.string "Congratulations, {PLAYER}!\p"
	.string "Despite not winning the tournament,\n"
	.string "you and your Pokémon have proved\l"
	.string "yourselves as strong trainers.$"

Cormoria_Championship_Corridor_Register_Text_1:
# 13 "data//maps/Championship_Corridor/scripts.pory"
	.string "With your defeat to Stanja, you have\n"
	.string "been eliminated from the Cormoria\l"
	.string "Pokémon Championship.\p"
	.string "Your final result is: Top 16.\p"
	.string "Congratulations, {PLAYER}!\p"
	.string "Despite not winning the tournament,\n"
	.string "you and your Pokémon have proved\l"
	.string "yourselves as strong trainers.$"

Cormoria_Championship_Corridor_Register_Text_2:
# 17 "data//maps/Championship_Corridor/scripts.pory"
	.string "With your defeat to Anaya, you have\n"
	.string "been eliminated from the Cormoria\l"
	.string "Pokémon Championship.\p"
	.string "Your final result is: Quarter-Finalist.\p"
	.string "Congratulations, {PLAYER}!\p"
	.string "Despite not winning the tournament,\n"
	.string "you and your Pokémon have proved\l"
	.string "yourselves as strong trainers.$"

Cormoria_Championship_Corridor_Register_Text_3:
# 21 "data//maps/Championship_Corridor/scripts.pory"
	.string "With your defeat to Gabrielle, you have\n"
	.string "been eliminated from the Cormoria\l"
	.string "Pokémon Championship.\p"
	.string "Your final result is: Semi-Finalist.\p"
	.string "Congratulations, {PLAYER}!\p"
	.string "Despite not winning the tournament,\n"
	.string "you and your Pokémon have proved\l"
	.string "yourselves as strong trainers.$"

Cormoria_Championship_Corridor_Register_Text_4:
# 25 "data//maps/Championship_Corridor/scripts.pory"
	.string "Are you ready to try again?$"

Cormoria_Championship_Corridor_Register_Text_5:
# 26 "data//maps/Championship_Corridor/scripts.pory"
	.string "I'm ready!$"

Cormoria_Championship_Corridor_Register_Text_6:
# 26 "data//maps/Championship_Corridor/scripts.pory"
	.string "Not yet$"

Cormoria_Championship_Corridor_Register_Nurse_Text_0:
# 53 "data//maps/Championship_Corridor/scripts.pory"
	.string "Whenever ready, please use the machine\n"
	.string "next to me.$"

Cormoria_Championship_Corridor_Exit_Nurse_Text_0:
# 58 "data//maps/Championship_Corridor/scripts.pory"
	.string "Would you like to return to the\n"
	.string "Championship hall?$"

Cormoria_Championship_Corridor_NPC1_Text_0:
# 74 "data//maps/Championship_Corridor/scripts.pory"
	.string "We could've been up there...$"

Cormoria_Championship_Corridor_Mon1_Text_0:
# 81 "data//maps/Championship_Corridor/scripts.pory"
	.string "Eelectross is in a daze...$"

Cormoria_Championship_Corridor_NPC2_Text_0:
# 87 "data//maps/Championship_Corridor/scripts.pory"
	.string "Sniff...I just can't believe it..\n"
	.string "...after all our training...$"

Cormoria_Championship_Corridor_Mon2_Text_0:
# 93 "data//maps/Championship_Corridor/scripts.pory"
	.string "Excadrill is trying to console its\n"
	.string "trainer, but it's clearly sad too.$"

Cormoria_Championship_Corridor_NPC3_Text_0:
# 99 "data//maps/Championship_Corridor/scripts.pory"
	.string "My first-round opponent was the Ace\n"
	.string "Rookie from Galecrest...\p"
	.string "Roserade and I never had a chance.\p"
	.string "It's fine...\p"
	.string "I just...sniff...$"

Cormoria_Championship_Corridor_Mon3_Text_0:
# 105 "data//maps/Championship_Corridor/scripts.pory"
	.string "Roserade's petals look a little singed...$"

Cormoria_Championship_Corridor_NPC4_Text_0:
# 111 "data//maps/Championship_Corridor/scripts.pory"
	.string "...\p"
	.string "Toxtricity and I trained for years...\n"
	.string "...it was over so fast...$"

Cormoria_Championship_Corridor_Mon4_Text_0:
# 117 "data//maps/Championship_Corridor/scripts.pory"
	.string "Toxtricity looks dejected...$"

Cormoria_Championship_Corridor_NPC5_Text_0:
# 125 "data//maps/Championship_Corridor/scripts.pory"
	.string "Listen up, Dragalge! We're not gonna\n"
	.string "lose next time, got it?\p"
	.string "We're going to win!\p"
	.string "...sniff...next time...$"

Cormoria_Championship_Corridor_Mon5_Text_0:
# 132 "data//maps/Championship_Corridor/scripts.pory"
	.string "Dragalge is trying to put on a brave\n"
	.string "face for its trainer.$"

Cormoria_Championship_Corridor_TV_Text_0:
# 138 "data//maps/Championship_Corridor/scripts.pory"
	.string "TV$"

Cormoria_Championship_Corridor_TV_Text_1:
# 138 "data//maps/Championship_Corridor/scripts.pory"
	.string "And the crowd goes wild as Kohla's\n"
	.string "Ceruledge knocks out another Pokémon!\l"
	.string "I must say, we haven't seen a trainer\l"
	.string "like this in years! The skill! The style!\p"
	.string "He sure is this years favourite!$"
