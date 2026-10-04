# 1 "data//maps/MtMirrohExterior/scripts.pory"

# 2 "data//maps/MtMirrohExterior/scripts.pory"
    .set Cormoria_maps_MtMirrohExterior_scripts_LOCALID_BREECH, 5
# 3 "data//maps/MtMirrohExterior/scripts.pory"
    .set Cormoria_maps_MtMirrohExterior_scripts_LOCALID_CLEF, 6

Cormoria_MtMirrohExterior_MapScripts::
	.byte 0


Cormoria_MtMirrohExterior_BreechTrigger::
# 22 "data//maps/MtMirrohExterior/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROH_EXTERIOR_BREECH, Cormoria_MtMirrohExterior_BreechTrigger_2
# 23 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 25 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtMirrohExterior_BreechTrigger_Movement_0
# 26 "data//maps/MtMirrohExterior/scripts.pory"
	waitmovement 0
# 27 "data//maps/MtMirrohExterior/scripts.pory"
	speakername Cormoria_MtMirrohExterior_BreechTrigger_Text_0
# 28 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_BreechTrigger_Text_1
# 29 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 30 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement Cormoria_maps_MtMirrohExterior_scripts_LOCALID_CLEF, Cormoria_MtMirrohExterior_BreechTrigger_Movement_1
# 31 "data//maps/MtMirrohExterior/scripts.pory"
	waitmovement 0
# 32 "data//maps/MtMirrohExterior/scripts.pory"
	playmoncry SPECIES_CLEFABLE, CRY_MODE_NORMAL
# 33 "data//maps/MtMirrohExterior/scripts.pory"
	delay 24
# 34 "data//maps/MtMirrohExterior/scripts.pory"
	speakername Cormoria_MtMirrohExterior_BreechTrigger_Text_2
# 35 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_BreechTrigger_Text_3
# 36 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 37 "data//maps/MtMirrohExterior/scripts.pory"
	delay 30
# 40 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement Cormoria_maps_MtMirrohExterior_scripts_LOCALID_BREECH, Cormoria_MtMirrohExterior_BreechTrigger_Movement_2
# 41 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement Cormoria_maps_MtMirrohExterior_scripts_LOCALID_CLEF, Cormoria_MtMirrohExterior_BreechTrigger_Movement_2
# 42 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtMirrohExterior_BreechTrigger_Movement_3
# 43 "data//maps/MtMirrohExterior/scripts.pory"
	waitmovement 0
# 44 "data//maps/MtMirrohExterior/scripts.pory"
	delay 30
# 45 "data//maps/MtMirrohExterior/scripts.pory"
	speakername Cormoria_MtMirrohExterior_BreechTrigger_Text_0
# 46 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_BreechTrigger_Text_4
# 47 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 48 "data//maps/MtMirrohExterior/scripts.pory"
	delay 30
# 49 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement Cormoria_maps_MtMirrohExterior_scripts_LOCALID_BREECH, Cormoria_MtMirrohExterior_BreechTrigger_Movement_2
# 50 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement Cormoria_maps_MtMirrohExterior_scripts_LOCALID_CLEF, Cormoria_MtMirrohExterior_BreechTrigger_Movement_2
# 51 "data//maps/MtMirrohExterior/scripts.pory"
	waitmovement 0
# 52 "data//maps/MtMirrohExterior/scripts.pory"
	delay 30
# 53 "data//maps/MtMirrohExterior/scripts.pory"
	speakername Cormoria_MtMirrohExterior_BreechTrigger_Text_0
# 54 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_BreechTrigger_Text_5
# 55 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 56 "data//maps/MtMirrohExterior/scripts.pory"
	delay 60
# 57 "data//maps/MtMirrohExterior/scripts.pory"
	speakername Cormoria_MtMirrohExterior_BreechTrigger_Text_0
# 58 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_BreechTrigger_Text_6
# 59 "data//maps/MtMirrohExterior/scripts.pory"
	goto Cormoria_MtMirrohExterior_Breech_Reward
	return

Cormoria_MtMirrohExterior_BreechTrigger_2:
	end


Cormoria_MtMirrohExterior_Breech_Reward::
# 63 "data//maps/MtMirrohExterior/scripts.pory"
	message Cormoria_MtMirrohExterior_Breech_Reward_Text_0
# 64 "data//maps/MtMirrohExterior/scripts.pory"
	waitmessage
# 65 "data//maps/MtMirrohExterior/scripts.pory"
	dynmultichoice 0, 0, TRUE, 2, 0, DYN_MULTICHOICE_CB_NONE, Cormoria_MtMirrohExterior_Breech_Reward_Text_1, Cormoria_MtMirrohExterior_Breech_Reward_Text_2
# 66 "data//maps/MtMirrohExterior/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_MtMirrohExterior_Breech_Reward_2
Cormoria_MtMirrohExterior_Breech_Reward_1:
# 67 "data//maps/MtMirrohExterior/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_MtMirrohExterior_Breech_Reward_5
Cormoria_MtMirrohExterior_Breech_Reward_4:
# 68 "data//maps/MtMirrohExterior/scripts.pory"
	goto Cormoria_MtMirrohExterior_Breech_Reward_Give
	return

Cormoria_MtMirrohExterior_Breech_Reward_2:
# 66 "data//maps/MtMirrohExterior/scripts.pory"
	setvar VAR_0x8006, ITEM_ASSAULT_VEST
	goto Cormoria_MtMirrohExterior_Breech_Reward_1

Cormoria_MtMirrohExterior_Breech_Reward_5:
# 67 "data//maps/MtMirrohExterior/scripts.pory"
	setvar VAR_0x8006, ITEM_ROCKY_HELMET
	goto Cormoria_MtMirrohExterior_Breech_Reward_4


Cormoria_MtMirrohExterior_Breech_Reward_Give::
# 72 "data//maps/MtMirrohExterior/scripts.pory"
	showitemdescription
# 73 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_Breech_Reward_Give_Text_0, MSGBOX_YESNO
# 74 "data//maps/MtMirrohExterior/scripts.pory"
	hideitemdescription
# 75 "data//maps/MtMirrohExterior/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_MtMirrohExterior_Breech_Reward_Give_1
# 76 "data//maps/MtMirrohExterior/scripts.pory"
	goto Cormoria_MtMirrohExterior_Breech_Reward
	return

Cormoria_MtMirrohExterior_Breech_Reward_Give_1:
# 75 "data//maps/MtMirrohExterior/scripts.pory"
	giveitem VAR_0x8006
# 75 "data//maps/MtMirrohExterior/scripts.pory"
	goto Cormoria_MtMirrohExterior_Breech_Reward_End
	return


Cormoria_MtMirrohExterior_Breech_Reward_End::
# 80 "data//maps/MtMirrohExterior/scripts.pory"
	speakername Cormoria_MtMirrohExterior_BreechTrigger_Text_0
# 81 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_Breech_Reward_End_Text_0
# 82 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 83 "data//maps/MtMirrohExterior/scripts.pory"
	delay 16
# 84 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement SPECIES_CLEFABLE, Cormoria_MtMirrohExterior_Breech_Reward_End_Movement_0
# 85 "data//maps/MtMirrohExterior/scripts.pory"
	waitmovement 0
# 86 "data//maps/MtMirrohExterior/scripts.pory"
	delay 16
# 87 "data//maps/MtMirrohExterior/scripts.pory"
	playmoncry SPECIES_CLEFABLE, CRY_MODE_NORMAL
# 88 "data//maps/MtMirrohExterior/scripts.pory"
	delay 8
# 89 "data//maps/MtMirrohExterior/scripts.pory"
	speakername Cormoria_MtMirrohExterior_BreechTrigger_Text_2
# 90 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_BreechTrigger_Text_3
# 91 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 92 "data//maps/MtMirrohExterior/scripts.pory"
	delay 24
# 95 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtMirrohExterior_Breech_Reward_End_Movement_1
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement Cormoria_maps_MtMirrohExterior_scripts_LOCALID_BREECH, Cormoria_MtMirrohExterior_Breech_Reward_End_Movement_2
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement Cormoria_maps_MtMirrohExterior_scripts_LOCALID_CLEF, Cormoria_MtMirrohExterior_Breech_Reward_End_Movement_3
# 98 "data//maps/MtMirrohExterior/scripts.pory"
	waitmovement 0
# 99 "data//maps/MtMirrohExterior/scripts.pory"
	removeobject Cormoria_maps_MtMirrohExterior_scripts_LOCALID_BREECH
# 100 "data//maps/MtMirrohExterior/scripts.pory"
	removeobject Cormoria_maps_MtMirrohExterior_scripts_LOCALID_CLEF
# 102 "data//maps/MtMirrohExterior/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_EXTERIOR_BREECH
# 103 "data//maps/MtMirrohExterior/scripts.pory"
	setflag Cormoria_FLAG_DETECTIVE_ACAD_BREECH
# 105 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_Trainer_A::
# 113 "data//maps/MtMirrohExterior/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHEXT_A, Cormoria_MtMirrohExterior_Trainer_A_Text_0, Cormoria_MtMirrohExterior_Trainer_A_Text_1
# 114 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_Trainer_A_Text_2
# 115 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_Trainer_B::
# 119 "data//maps/MtMirrohExterior/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHEXT_B, Cormoria_MtMirrohExterior_Trainer_B_Text_0, Cormoria_MtMirrohExterior_Trainer_B_Text_1
# 120 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_Trainer_B_Text_2
# 121 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_Trainer_C::
# 125 "data//maps/MtMirrohExterior/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHEXT_C, Cormoria_MtMirrohExterior_Trainer_C_Text_0, Cormoria_MtMirrohExterior_Trainer_C_Text_1
# 126 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_Trainer_C_Text_2
# 127 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_NPC_A::
# 160 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 161 "data//maps/MtMirrohExterior/scripts.pory"
	faceplayer
# 162 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_A_Text_0
# 163 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_NPC_B::
# 167 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 168 "data//maps/MtMirrohExterior/scripts.pory"
	faceplayer
# 169 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_B_Text_0
# 170 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_NPC_C::
# 174 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 175 "data//maps/MtMirrohExterior/scripts.pory"
	faceplayer
# 176 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_C_Text_0
# 177 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_NPC_D::
# 183 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 184 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_D_Text_0
# 185 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 186 "data//maps/MtMirrohExterior/scripts.pory"
	faceplayer
# 187 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement 9, Cormoria_MtMirrohExterior_NPC_D_Movement_0
# 188 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_D_Text_1
# 189 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 190 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement 9, Cormoria_MtMirrohExterior_BreechTrigger_Movement_3
# 191 "data//maps/MtMirrohExterior/scripts.pory"
	waitmovement 0
# 192 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_Conkeldurr::
# 196 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 197 "data//maps/MtMirrohExterior/scripts.pory"
	playmoncry SPECIES_CONKELDURR, CRY_MODE_NORMAL
# 198 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_Conkeldurr_Text_0
# 199 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_NPC_E::
# 205 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 206 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_E_Text_0
# 207 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 208 "data//maps/MtMirrohExterior/scripts.pory"
	faceplayer
# 209 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement 10, Cormoria_MtMirrohExterior_NPC_D_Movement_0
# 210 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_D_Text_1
# 211 "data//maps/MtMirrohExterior/scripts.pory"
	release
# 212 "data//maps/MtMirrohExterior/scripts.pory"
	applymovement 10, Cormoria_MtMirrohExterior_NPC_E_Movement_0
# 213 "data//maps/MtMirrohExterior/scripts.pory"
	waitmovement 0
# 214 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_Toxicroak::
# 218 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 219 "data//maps/MtMirrohExterior/scripts.pory"
	playmoncry SPECIES_TOXICROAK, CRY_MODE_NORMAL
# 220 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_Toxicroak_Text_0
# 221 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_NPC_F::
# 227 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 228 "data//maps/MtMirrohExterior/scripts.pory"
	faceplayer
# 229 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_F_Text_0
# 230 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_NPC_G::
# 235 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 236 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_NPC_G_Text_0
# 237 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


Cormoria_MtMirrohExterior_Medicham::
# 242 "data//maps/MtMirrohExterior/scripts.pory"
	lockall
# 243 "data//maps/MtMirrohExterior/scripts.pory"
	msgbox Cormoria_MtMirrohExterior_Medicham_Text_0
# 244 "data//maps/MtMirrohExterior/scripts.pory"
	releaseall
	end


# 25 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_BreechTrigger_Movement_0:
# 25 "data//maps/MtMirrohExterior/scripts.pory"
	walk_up
	step_end

# 30 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_BreechTrigger_Movement_1:
# 30 "data//maps/MtMirrohExterior/scripts.pory"
	jump_in_place_up
	step_end

# 40 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_BreechTrigger_Movement_2:
# 40 "data//maps/MtMirrohExterior/scripts.pory"
	face_left
# 40 "data//maps/MtMirrohExterior/scripts.pory"
	emote_exclamation_mark
	step_end

# 42 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_BreechTrigger_Movement_3:
# 42 "data//maps/MtMirrohExterior/scripts.pory"
	face_right
	step_end

# 84 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_Breech_Reward_End_Movement_0:
# 84 "data//maps/MtMirrohExterior/scripts.pory"
	jump_in_place_left
	step_end

# 95 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_Breech_Reward_End_Movement_1:
# 95 "data//maps/MtMirrohExterior/scripts.pory"
	walk_left
# 95 "data//maps/MtMirrohExterior/scripts.pory"
	face_right
	step_end

# 96 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_Breech_Reward_End_Movement_2:
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_left
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_down
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_down
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_down
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 96 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
	step_end

# 97 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_Breech_Reward_End_Movement_3:
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_left
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_left
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_down
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_down
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_down
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
# 97 "data//maps/MtMirrohExterior/scripts.pory"
	walk_right
	step_end

# 187 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_NPC_D_Movement_0:
# 187 "data//maps/MtMirrohExterior/scripts.pory"
	emote_exclamation_mark
	step_end

# 212 "data//maps/MtMirrohExterior/scripts.pory"
Cormoria_MtMirrohExterior_NPC_E_Movement_0:
# 212 "data//maps/MtMirrohExterior/scripts.pory"
	face_left
	step_end

Cormoria_MtMirrohExterior_BreechTrigger_Text_0:
# 27 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Breech$"

Cormoria_MtMirrohExterior_BreechTrigger_Text_1:
# 28 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Clefable, did you see the Team Somber\n"
	.string "grunts too?$"

Cormoria_MtMirrohExterior_BreechTrigger_Text_2:
# 34 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Clefable$"

Cormoria_MtMirrohExterior_BreechTrigger_Text_3:
# 35 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Clee!!!$"

Cormoria_MtMirrohExterior_BreechTrigger_Text_4:
# 46 "data//maps/MtMirrohExterior/scripts.pory"
	.string "{PLAYER}! It's you! Fancy seeing you\n"
	.string "here. How are you?\p"
	.string "...\p"
	.string "So you're chasing Team Somber. I admire\n"
	.string "you, {PLAYER}. Clefable and I just saw\l"
	.string "a couple of them enter Mt. Mirroh. They\l"
	.string "left this drill here and blocked the\l"
	.string "path with these boulders. I wonder what\l"
	.string "they're using all this equipment for?$"

Cormoria_MtMirrohExterior_BreechTrigger_Text_5:
# 54 "data//maps/MtMirrohExterior/scripts.pory"
	.string "I see...so they must be using all this\n"
	.string "equipment to build and carry this\l"
	.string "Dreamalyser thing you're talking about.\l"
	.string "How scary!$"

Cormoria_MtMirrohExterior_BreechTrigger_Text_6:
# 58 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Clefable and I just made our way down\n"
	.string "Mt. Mirroh. We're going to take a break\l"
	.string "from the mountains.\p"
	.string "Where are we going next?\p"
	.string "Rivetshore City! We're going to enjoy\n"
	.string "the sea breeze for a while.$"

Cormoria_MtMirrohExterior_Breech_Reward_Text_0:
# 63 "data//maps/MtMirrohExterior/scripts.pory"
	.string "We don't need these any more, so would\n"
	.string "you like one?$"

Cormoria_MtMirrohExterior_Breech_Reward_Text_1:
# 65 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Assault Vest$"

Cormoria_MtMirrohExterior_Breech_Reward_Text_2:
# 65 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Rocky Helmet$"

Cormoria_MtMirrohExterior_Breech_Reward_Give_Text_0:
# 73 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Are you sure?$"

Cormoria_MtMirrohExterior_Breech_Reward_End_Text_0:
# 81 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Maybe we'll see you in Rivetshore City!$"

Cormoria_MtMirrohExterior_Trainer_A_Text_0:
# 113 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Once you jump over that ledge over\n"
	.string "there, you can't come back. So let me\l"
	.string "battle you before you go!$"

Cormoria_MtMirrohExterior_Trainer_A_Text_1:
# 113 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Okay, you can go!$"

Cormoria_MtMirrohExterior_Trainer_A_Text_2:
# 114 "data//maps/MtMirrohExterior/scripts.pory"
	.string "I'm training here so that I can safely\n"
	.string "go back to the Pokémon Center if I need\l"
	.string "to.$"

Cormoria_MtMirrohExterior_Trainer_B_Text_0:
# 119 "data//maps/MtMirrohExterior/scripts.pory"
	.string "I know what you're eyeing! That's why I\n"
	.string "planted myself here - hahaha!$"

Cormoria_MtMirrohExterior_Trainer_B_Text_1:
# 119 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Aw I lost!$"

Cormoria_MtMirrohExterior_Trainer_B_Text_2:
# 120 "data//maps/MtMirrohExterior/scripts.pory"
	.string "I love the mountains! There are so many\n"
	.string "good hiding spots.$"

Cormoria_MtMirrohExterior_Trainer_C_Text_0:
# 125 "data//maps/MtMirrohExterior/scripts.pory"
	.string "I'd rather be skiing on these slopes but\n"
	.string "I don't mind a battle!$"

Cormoria_MtMirrohExterior_Trainer_C_Text_1:
# 125 "data//maps/MtMirrohExterior/scripts.pory"
	.string "I definitely prefer skiing!$"

Cormoria_MtMirrohExterior_Trainer_C_Text_2:
# 126 "data//maps/MtMirrohExterior/scripts.pory"
	.string "You won't get slopes like these in all of\n"
	.string "Cormoria!$"

Cormoria_MtMirrohExterior_NPC_A_Text_0:
# 162 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Be careful! There are some nasty types\n"
	.string "around. They're wearing dark suits and\l"
	.string "harrassing everyone they see. They\l"
	.string "even threatened me - a senior citizen!$"

Cormoria_MtMirrohExterior_NPC_B_Text_0:
# 169 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Higher up, the weather becomes too\n"
	.string "harsh to climb, so to reach Winterlily\l"
	.string "Hollow, you have to cross the caverns\l"
	.string "inside the mountain.$"

Cormoria_MtMirrohExterior_NPC_C_Text_0:
# 176 "data//maps/MtMirrohExterior/scripts.pory"
	.string "The Vivillon from the Garden Shop take\n"
	.string "good care of the flowers, which is why\l"
	.string "they grow even in such cold weather!$"

Cormoria_MtMirrohExterior_NPC_D_Text_0:
# 184 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Hang in there Conkeldurr!$"

Cormoria_MtMirrohExterior_NPC_D_Text_1:
# 188 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Don't disturb us - we're training!$"

Cormoria_MtMirrohExterior_Conkeldurr_Text_0:
# 198 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Durr!!!!\p"
	.string "Conkeldurr's been poisoned, but it's\n"
	.string "still going strong!$"

Cormoria_MtMirrohExterior_NPC_E_Text_0:
# 206 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Finish this, Toxicroak!$"

Cormoria_MtMirrohExterior_Toxicroak_Text_0:
# 220 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Croooaaakk!!!!\p"
	.string "It's hammering Conkeldurr with Posion\n"
	.string "Jabs!$"

Cormoria_MtMirrohExterior_NPC_F_Text_0:
# 229 "data//maps/MtMirrohExterior/scripts.pory"
	.string "Someone told me you could buy Flame\n"
	.string "Orbs here, but it turned out to be a\l"
	.string "garden supply store!\p"
	.string "I heard you can get them in Rivetshore\n"
	.string "City though.$"

Cormoria_MtMirrohExterior_NPC_G_Text_0:
# 236 "data//maps/MtMirrohExterior/scripts.pory"
	.string "She's meditating with her\n"
	.string "Medicham...best not disturb her.$"

Cormoria_MtMirrohExterior_Medicham_Text_0:
# 243 "data//maps/MtMirrohExterior/scripts.pory"
	.string "She's meditating with her trainer...best\n"
	.string "not disturb her.$"
