Cormoria_Championship_R1_Lobby_MapScripts::
	.byte 0


Cormoria_Championship_Lobby_Teala::
# 4 "data//maps/Championship_R1_Lobby/scripts.pory"
	lockall
# 5 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_Lobby_Teala_Text_0, MSGBOX_YESNO
# 6 "data//maps/Championship_R1_Lobby/scripts.pory"
	release
# 7 "data//maps/Championship_R1_Lobby/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_Championship_Lobby_Teala_2
Cormoria_Championship_Lobby_Teala_1:
# 22 "data//maps/Championship_R1_Lobby/scripts.pory"
	releaseall
	end

Cormoria_Championship_Lobby_Teala_2:
# 9 "data//maps/Championship_R1_Lobby/scripts.pory"
	applymovement 2, Cormoria_Championship_Lobby_Teala_Movement_0
# 10 "data//maps/Championship_R1_Lobby/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Championship_Lobby_Teala_Movement_1
# 11 "data//maps/Championship_R1_Lobby/scripts.pory"
	waitmovement 0
# 12 "data//maps/Championship_R1_Lobby/scripts.pory"
	delay 8
# 15 "data//maps/Championship_R1_Lobby/scripts.pory"
	compare Cormoria_VAR_CHAMP_STAGE, 2
	goto_if_eq Cormoria_Championship_Lobby_Teala_5
Cormoria_Championship_Lobby_Teala_4:
# 16 "data//maps/Championship_R1_Lobby/scripts.pory"
	compare Cormoria_VAR_CHAMP_STAGE, 3
	goto_if_eq Cormoria_Championship_Lobby_Teala_8
Cormoria_Championship_Lobby_Teala_7:
# 17 "data//maps/Championship_R1_Lobby/scripts.pory"
	compare Cormoria_VAR_CHAMP_STAGE, 4
	goto_if_eq Cormoria_Championship_Lobby_Teala_11
Cormoria_Championship_Lobby_Teala_10:
# 18 "data//maps/Championship_R1_Lobby/scripts.pory"
	compare Cormoria_VAR_CHAMP_STAGE, 5
	goto_if_eq Cormoria_Championship_Lobby_Teala_13
	goto Cormoria_Championship_Lobby_Teala_1

Cormoria_Championship_Lobby_Teala_5:
# 15 "data//maps/Championship_R1_Lobby/scripts.pory"
	warp MAP_CORMORIA_CHAMPIONSHIP_R2, 4, 8
	goto Cormoria_Championship_Lobby_Teala_4

Cormoria_Championship_Lobby_Teala_8:
# 16 "data//maps/Championship_R1_Lobby/scripts.pory"
	warp MAP_CORMORIA_CHAMPIONSHIP_R3, 4, 8
	goto Cormoria_Championship_Lobby_Teala_7

Cormoria_Championship_Lobby_Teala_11:
# 17 "data//maps/Championship_R1_Lobby/scripts.pory"
	warp MAP_CORMORIA_CHAMPIONSHIP_R4, 4, 8
	goto Cormoria_Championship_Lobby_Teala_10

Cormoria_Championship_Lobby_Teala_13:
# 18 "data//maps/Championship_R1_Lobby/scripts.pory"
	warp MAP_CORMORIA_CHAMPIONSHIP_R5, 4, 9
	goto Cormoria_Championship_Lobby_Teala_1


Cormoria_Championship_Lobby_Nurse::
# 28 "data//maps/Championship_R1_Lobby/scripts.pory"
	lockall
# 30 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_Lobby_Nurse_Text_0
# 31 "data//maps/Championship_R1_Lobby/scripts.pory"
	call Cormoria_Common_EventScript_OutOfCenterPartyHeal
# 32 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_Lobby_Nurse_Text_1
# 34 "data//maps/Championship_R1_Lobby/scripts.pory"
	releaseall
	end


Cormoria_Championship_R1_Lobby_NPC1::
# 51 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_R1_Lobby_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_R1_Lobby_Mon1::
# 56 "data//maps/Championship_R1_Lobby/scripts.pory"
	playmoncry SPECIES_TOXAPEX, CRY_MODE_NORMAL
# 57 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_R1_Lobby_Mon1_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_R1_Lobby_NPC2::
# 63 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_R1_Lobby_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_Championship_R1_Lobby_Mon2::
# 68 "data//maps/Championship_R1_Lobby/scripts.pory"
	lockall
# 69 "data//maps/Championship_R1_Lobby/scripts.pory"
	playmoncry SPECIES_DELPHOX, CRY_MODE_NORMAL
# 70 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_R1_Lobby_Mon2_Text_0
# 71 "data//maps/Championship_R1_Lobby/scripts.pory"
	releaseall
	end


Cormoria_Championship_R1_Lobby_NPC3::
# 76 "data//maps/Championship_R1_Lobby/scripts.pory"
	lockall
# 77 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_R1_Lobby_NPC3_Text_0
# 78 "data//maps/Championship_R1_Lobby/scripts.pory"
	releaseall
	end


Cormoria_Championship_R1_Lobby_Mon3::
# 83 "data//maps/Championship_R1_Lobby/scripts.pory"
	lockall
# 84 "data//maps/Championship_R1_Lobby/scripts.pory"
	playmoncry SPECIES_FROSMOTH, CRY_MODE_NORMAL
# 85 "data//maps/Championship_R1_Lobby/scripts.pory"
	msgbox Cormoria_Championship_R1_Lobby_Mon3_Text_0
# 86 "data//maps/Championship_R1_Lobby/scripts.pory"
	releaseall
	end


# 9 "data//maps/Championship_R1_Lobby/scripts.pory"
Cormoria_Championship_Lobby_Teala_Movement_0:
# 9 "data//maps/Championship_R1_Lobby/scripts.pory"
	walk_right
# 9 "data//maps/Championship_R1_Lobby/scripts.pory"
	face_left
	step_end

# 10 "data//maps/Championship_R1_Lobby/scripts.pory"
Cormoria_Championship_Lobby_Teala_Movement_1:
# 10 "data//maps/Championship_R1_Lobby/scripts.pory"
	walk_up
# 10 "data//maps/Championship_R1_Lobby/scripts.pory"
	walk_up
	step_end

Cormoria_Championship_Lobby_Teala_Text_0:
# 5 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "Are you ready to proceed to the next\n"
	.string "round?$"

Cormoria_Championship_Lobby_Nurse_Text_0:
# 30 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "Congraluations on your victory,\n"
	.string "trainer!\p"
	.string "I'll now heal your Pokémon.$"

Cormoria_Championship_Lobby_Nurse_Text_1:
# 32 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "All the best!$"

Cormoria_Championship_R1_Lobby_NPC1_Text_0:
# 51 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "The first round was so hard!\n"
	.string "But Toxapex and I made it through!$"

Cormoria_Championship_R1_Lobby_Mon1_Text_0:
# 57 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "Toxapex seems pumped up!$"

Cormoria_Championship_R1_Lobby_NPC2_Text_0:
# 63 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "I'm not too old to win!\n"
	.string "Delphox and I have still got it!$"

Cormoria_Championship_R1_Lobby_Mon2_Text_0:
# 70 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "Delphox's eyes are closed.\n"
	.string "It's visualising victory...$"

Cormoria_Championship_R1_Lobby_NPC3_Text_0:
# 77 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "...after the hail comes up, you Quiver\n"
	.string "Dance twice, got it?$"

Cormoria_Championship_R1_Lobby_Mon3_Text_0:
# 85 "data//maps/Championship_R1_Lobby/scripts.pory"
	.string "Frosmoth is nodding its head.$"
