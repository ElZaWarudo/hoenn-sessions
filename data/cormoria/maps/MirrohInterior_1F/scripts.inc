# 1 "data//maps/MirrohInterior_1F/scripts.pory"

# 2 "data//maps/MirrohInterior_1F/scripts.pory"
    .set Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_NURSE, 1
# 3 "data//maps/MirrohInterior_1F/scripts.pory"
    .set Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_ROBBER, 2
# 4 "data//maps/MirrohInterior_1F/scripts.pory"
    .set Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH1, 24
# 5 "data//maps/MirrohInterior_1F/scripts.pory"
    .set Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH2, 25
# 6 "data//maps/MirrohInterior_1F/scripts.pory"
    .set Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH3, 26        
# 7 "data//maps/MirrohInterior_1F/scripts.pory"
    .set Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CHANSEY_LAST, 32
# 8 "data//maps/MirrohInterior_1F/scripts.pory"
    .set Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1, 29
# 9 "data//maps/MirrohInterior_1F/scripts.pory"
    .set Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT2, 17

Cormoria_MirrohInterior_1F_MapScripts::
# 13 "data//maps/MirrohInterior_1F/scripts.pory"
	map_script MAP_SCRIPT_ON_FRAME_TABLE, Cormoria_MirrohInterior_1F_MapScripts_MAP_SCRIPT_ON_FRAME_TABLE
	.byte 0

Cormoria_MirrohInterior_1F_MapScripts_MAP_SCRIPT_ON_FRAME_TABLE:
# 14 "data//maps/MirrohInterior_1F/scripts.pory"
	map_script_2 VAR_TEMP_0, 0, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End
	.2byte 0


Cormoria_MirrohInterior_1F_EventScript_Nurse::
# 36 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_Nurse_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_1F_EventScript_Chansey::
# 41 "data//maps/MirrohInterior_1F/scripts.pory"
	playmoncry SPECIES_CHANSEY, CRY_MODE_NORMAL
# 42 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_Chansey_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger0::
# 47 "data//maps/MirrohInterior_1F/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROH_NURSE_QUEST_START, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger0_2
# 48 "data//maps/MirrohInterior_1F/scripts.pory"
	lockall
# 49 "data//maps/MirrohInterior_1F/scripts.pory"
	call Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation
# 51 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger0_Movement_0
# 52 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 53 "data//maps/MirrohInterior_1F/scripts.pory"
	goto Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Main
	return

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger0_2:
	end


Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger1::
# 56 "data//maps/MirrohInterior_1F/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROH_NURSE_QUEST_START, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger1_2
# 57 "data//maps/MirrohInterior_1F/scripts.pory"
	lockall
# 58 "data//maps/MirrohInterior_1F/scripts.pory"
	call Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation
# 60 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger1_Movement_0
# 61 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 62 "data//maps/MirrohInterior_1F/scripts.pory"
	goto Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Main
	return

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger1_2:
	end


Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger2::
# 65 "data//maps/MirrohInterior_1F/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROH_NURSE_QUEST_START, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger2_2
# 66 "data//maps/MirrohInterior_1F/scripts.pory"
	lockall
# 67 "data//maps/MirrohInterior_1F/scripts.pory"
	call Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation
# 69 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger2_Movement_0
# 70 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 71 "data//maps/MirrohInterior_1F/scripts.pory"
	goto Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Main
	return

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger2_2:
	end


Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation::
# 80 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_NURSE, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation_Movement_0
# 81 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 82 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 20
# 84 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation_Text_0
# 85 "data//maps/MirrohInterior_1F/scripts.pory"
	release
	return


Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Main::
# 89 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Main_Text_0
# 90 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 92 "data//maps/MirrohInterior_1F/scripts.pory"
	startquest QUEST_MIRROH_NURSE_CHANSEY
# 93 "data//maps/MirrohInterior_1F/scripts.pory"
	setvar Cormoria_VAR_MIRROH_NURSE_CHANSEY_QUEST, 1
# 94 "data//maps/MirrohInterior_1F/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_NURSE_QUEST_START
# 95 "data//maps/MirrohInterior_1F/scripts.pory"
	releaseall
	end


Cormoria_MirrohInterior_1F_EventScript_NurseQuest_MissingChansey::
# 99 "data//maps/MirrohInterior_1F/scripts.pory"
	lockall
# 100 "data//maps/MirrohInterior_1F/scripts.pory"
	faceplayer
# 101 "data//maps/MirrohInterior_1F/scripts.pory"
	playmoncry SPECIES_CHANSEY, CRY_MODE_NORMAL
# 102 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_MissingChansey_Text_0
# 103 "data//maps/MirrohInterior_1F/scripts.pory"
	call Cormoria_Common_EventScript_OutOfCenterPartyHeal
# 104 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_MissingChansey_Text_1
# 105 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 106 "data//maps/MirrohInterior_1F/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_NURSE_QUEST_CHANSEYGET
# 107 "data//maps/MirrohInterior_1F/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_MIRROH_LAST_CHANSEY
# 108 "data//maps/MirrohInterior_1F/scripts.pory"
	setvar Cormoria_VAR_MIRROH_NURSE_CHANSEY_QUEST, 2
# 109 "data//maps/MirrohInterior_1F/scripts.pory"
	warp MAP_CORMORIA_MIRROH_INTERIOR_1F, 42, 46
	return


Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End::
# 112 "data//maps/MirrohInterior_1F/scripts.pory"
	compare Cormoria_VAR_MIRROH_NURSE_CHANSEY_QUEST, 2
	goto_if_eq Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_2
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_1:
	end

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_2:
# 114 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_0
# 115 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_NURSE, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation_Movement_0
# 116 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 119 "data//maps/MirrohInterior_1F/scripts.pory"
	playmoncry SPECIES_CHANSEY, CRY_MODE_NORMAL
# 120 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH1, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_1
# 121 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH2, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_1
# 122 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH3, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_1
# 123 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 20
# 126 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH1, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_2
# 127 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH2, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_3
# 128 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH3, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_4
# 129 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH1
# 130 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 20
# 132 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_0
# 133 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 134 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_NURSE, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_5
# 135 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 136 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_1
# 137 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 138 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 139 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CHANSEY_LAST, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_6
# 140 "data//maps/MirrohInterior_1F/scripts.pory"
	playmoncry SPECIES_CHANSEY, CRY_MODE_NORMAL
# 141 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_2
# 143 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 144 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 145 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_NURSE, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_7
# 146 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 147 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_3
# 148 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 149 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_8
# 150 "data//maps/MirrohInterior_1F/scripts.pory"
	playmoncry SPECIES_CHANSEY, CRY_MODE_NORMAL
# 151 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CHANSEY_LAST, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_9
# 152 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 153 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 154 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_4
# 155 "data//maps/MirrohInterior_1F/scripts.pory"
	giveitem ITEM_EVIOLITE
# 156 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_5
# 157 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 158 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 159 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_0
# 160 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 161 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 162 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_6
# 163 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 166 "data//maps/MirrohInterior_1F/scripts.pory"
	fadescreen FADE_TO_BLACK
# 167 "data//maps/MirrohInterior_1F/scripts.pory"
	removeobject Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH1
# 168 "data//maps/MirrohInterior_1F/scripts.pory"
	removeobject Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH2
# 169 "data//maps/MirrohInterior_1F/scripts.pory"
	removeobject Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CH3
# 170 "data//maps/MirrohInterior_1F/scripts.pory"
	removeobject Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_CHANSEY_LAST
# 171 "data//maps/MirrohInterior_1F/scripts.pory"
	removeobject Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_NURSE
# 172 "data//maps/MirrohInterior_1F/scripts.pory"
	fadescreen FADE_FROM_BLACK
# 175 "data//maps/MirrohInterior_1F/scripts.pory"
	completequest QUEST_MIRROH_NURSE_CHANSEY
# 176 "data//maps/MirrohInterior_1F/scripts.pory"
	setvar Cormoria_VAR_MIRROH_NURSE_CHANSEY_QUEST, 3
# 177 "data//maps/MirrohInterior_1F/scripts.pory"
	release
	goto Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_1


Cormoria_MirrohInterior_1F_Trainer_A::
# 200 "data//maps/MirrohInterior_1F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT_A, Cormoria_MirrohInterior_1F_Trainer_A_Text_0, Cormoria_MirrohInterior_1F_Trainer_A_Text_1
# 201 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_Trainer_A_Text_2
# 202 "data//maps/MirrohInterior_1F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_1F_Trainer_B::
# 207 "data//maps/MirrohInterior_1F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT_B, Cormoria_MirrohInterior_1F_Trainer_B_Text_0, Cormoria_MirrohInterior_1F_Trainer_B_Text_1
# 208 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_Trainer_B_Text_2
# 209 "data//maps/MirrohInterior_1F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_1F_Trainer_C::
# 214 "data//maps/MirrohInterior_1F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT_C, Cormoria_MirrohInterior_1F_Trainer_C_Text_0, Cormoria_MirrohInterior_1F_Trainer_C_Text_1
# 215 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_Trainer_C_Text_2
# 216 "data//maps/MirrohInterior_1F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_1F_Trainer_D::
# 221 "data//maps/MirrohInterior_1F/scripts.pory"
	lockall
# 222 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_Trainer_D_Text_0
# 223 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 224 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_ROBBER, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_9
# 225 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 226 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 20
# 227 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_Trainer_D_Text_1
# 229 "data//maps/MirrohInterior_1F/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_MIRROHINT_D, Cormoria_MirrohInterior_1F_Trainer_D_Text_2
# 230 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_Trainer_D_Text_3
# 231 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 232 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohInterior_1F_Trainer_D_Movement_0
# 233 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_ROBBER, Cormoria_MirrohInterior_1F_Trainer_D_Movement_1
# 234 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 235 "data//maps/MirrohInterior_1F/scripts.pory"
	removeobject Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_ROBBER
# 237 "data//maps/MirrohInterior_1F/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_ROBBER
# 238 "data//maps/MirrohInterior_1F/scripts.pory"
	releaseall
	end


Cormoria_MirrohInterior_1F_Trainer_E::
# 244 "data//maps/MirrohInterior_1F/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MIRROHINT_D, Cormoria_MirrohInterior_1F_Trainer_E_Text_0, Cormoria_MirrohInterior_1F_Trainer_E_Text_1
# 245 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_Trainer_E_Text_2
# 246 "data//maps/MirrohInterior_1F/scripts.pory"
	releaseall
	return


Cormoria_MirrohInterior_1F_NPC_A::
# 267 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_NPC_A_Text_0, MSGBOX_NPC
# 268 "data//maps/MirrohInterior_1F/scripts.pory"
	releaseall
	end


Cormoria_MirrohInterior_1F_NPC_B::
# 273 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_NPC_B_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_1F_NPC_C::
# 278 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_NPC_C_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_1F_NPC_D::
# 284 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_NPC_D_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_1F_NPC_E::
# 290 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_NPC_D_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_1F_Pineco::
# 294 "data//maps/MirrohInterior_1F/scripts.pory"
	playmoncry SPECIES_PINECO, CRY_MODE_NORMAL
# 295 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_Pineco_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_Bergmite_A::
# 308 "data//maps/MirrohInterior_1F/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROH_BERGMITE_A, Cormoria_MirrohInterior_Bergmite_A_2
# 313 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_Bergmite_A_Text_1
# 314 "data//maps/MirrohInterior_1F/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_BERGMITE_A
# 315 "data//maps/MirrohInterior_1F/scripts.pory"
	setwildbattle SPECIES_BERGMITE, 28
# 316 "data//maps/MirrohInterior_1F/scripts.pory"
	dowildbattle
	end

Cormoria_MirrohInterior_Bergmite_A_2:
# 309 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_Bergmite_A_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_Bergmite_B::
# 321 "data//maps/MirrohInterior_1F/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROH_BERGMITE_B, Cormoria_MirrohInterior_Bergmite_B_2
# 326 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_Bergmite_A_Text_1
# 327 "data//maps/MirrohInterior_1F/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_BERGMITE_B
# 328 "data//maps/MirrohInterior_1F/scripts.pory"
	setwildbattle SPECIES_BERGMITE, 28
# 329 "data//maps/MirrohInterior_1F/scripts.pory"
	dowildbattle
	end

Cormoria_MirrohInterior_Bergmite_B_2:
# 322 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_Bergmite_A_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_Bergmite_C::
# 334 "data//maps/MirrohInterior_1F/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROH_BERGMITE_C, Cormoria_MirrohInterior_Bergmite_C_2
# 339 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_Bergmite_A_Text_1
# 340 "data//maps/MirrohInterior_1F/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_BERGMITE_C
# 341 "data//maps/MirrohInterior_1F/scripts.pory"
	setwildbattle SPECIES_BERGMITE, 28
# 342 "data//maps/MirrohInterior_1F/scripts.pory"
	dowildbattle
	end

Cormoria_MirrohInterior_Bergmite_C_2:
# 335 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_Bergmite_A_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohInterior_1F_SomberGrunts_Trigger::
# 367 "data//maps/MirrohInterior_1F/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROH_INT_SOMBER_GRUNTS, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_2
# 369 "data//maps/MirrohInterior_1F/scripts.pory"
	lockall
# 370 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_0
# 371 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 372 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 373 "data//maps/MirrohInterior_1F/scripts.pory"
	speakername Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_0
# 374 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_1
# 375 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 376 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT2, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_1
# 377 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 378 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 379 "data//maps/MirrohInterior_1F/scripts.pory"
	speakername Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_2
# 380 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_3
# 381 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 382 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 24
# 384 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation_Movement_0
# 385 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT2, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation_Movement_0
# 386 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 387 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 24
# 389 "data//maps/MirrohInterior_1F/scripts.pory"
	speakername Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_0
# 390 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_4
# 391 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 393 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_2
# 394 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT2, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_2
# 395 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 396 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 24
# 398 "data//maps/MirrohInterior_1F/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_MIRROHINT_E, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_5
# 400 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1, Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation_Movement_0
# 401 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 402 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 403 "data//maps/MirrohInterior_1F/scripts.pory"
	speakername Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_0
# 404 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_6
# 405 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 406 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 8
# 407 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT2, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_3
# 408 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 409 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 410 "data//maps/MirrohInterior_1F/scripts.pory"
	speakername Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_2
# 411 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_7
# 412 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 413 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 24
# 415 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_4
# 416 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT2, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_5
# 417 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 418 "data//maps/MirrohInterior_1F/scripts.pory"
	delay 16
# 419 "data//maps/MirrohInterior_1F/scripts.pory"
	speakername Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_0
# 420 "data//maps/MirrohInterior_1F/scripts.pory"
	msgbox Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_8
# 421 "data//maps/MirrohInterior_1F/scripts.pory"
	release
# 424 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_6
# 425 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT2, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_6
# 426 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 427 "data//maps/MirrohInterior_1F/scripts.pory"
	removeobject Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT2
# 428 "data//maps/MirrohInterior_1F/scripts.pory"
	applymovement Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1, Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_7
# 429 "data//maps/MirrohInterior_1F/scripts.pory"
	waitmovement 0
# 430 "data//maps/MirrohInterior_1F/scripts.pory"
	removeobject Cormoria_maps_MirrohInterior_1F_scripts_LOCALID_GRUNT1
# 432 "data//maps/MirrohInterior_1F/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_INT_SOMBER_GRUNTS
# 434 "data//maps/MirrohInterior_1F/scripts.pory"
	releaseall
	end

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_2:
	end


# 51 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger0_Movement_0:
# 51 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 51 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 51 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 51 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 51 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 51 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 51 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
	step_end

# 60 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger1_Movement_0:
# 60 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 60 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 60 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 60 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 60 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 60 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
	step_end

# 69 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Trigger2_Movement_0:
# 69 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 69 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 69 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 69 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 69 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
	step_end

# 80 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation_Movement_0:
# 80 "data//maps/MirrohInterior_1F/scripts.pory"
	jump_in_place_right
# 80 "data//maps/MirrohInterior_1F/scripts.pory"
	emote_exclamation_mark
	step_end

# 114 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_0:
# 114 "data//maps/MirrohInterior_1F/scripts.pory"
	face_left
	step_end

# 120 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_1:
# 120 "data//maps/MirrohInterior_1F/scripts.pory"
	emote_exclamation_mark
	step_end

# 126 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_2:
# 126 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 126 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_right
# 126 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 126 "data//maps/MirrohInterior_1F/scripts.pory"
	face_left
	step_end

# 127 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_3:
# 127 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_right
# 127 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_right
# 127 "data//maps/MirrohInterior_1F/scripts.pory"
	face_down
	step_end

# 128 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_4:
# 128 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_right
# 128 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_up
# 128 "data//maps/MirrohInterior_1F/scripts.pory"
	face_down
	step_end

# 134 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_5:
# 134 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_up
# 134 "data//maps/MirrohInterior_1F/scripts.pory"
	face_right
	step_end

# 139 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_6:
# 139 "data//maps/MirrohInterior_1F/scripts.pory"
	jump_in_place_right
	step_end

# 145 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_7:
# 145 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 145 "data//maps/MirrohInterior_1F/scripts.pory"
	face_right
	step_end

# 149 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_8:
# 149 "data//maps/MirrohInterior_1F/scripts.pory"
	face_up
	step_end

# 151 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Movement_9:
# 151 "data//maps/MirrohInterior_1F/scripts.pory"
	jump_in_place_down
# 151 "data//maps/MirrohInterior_1F/scripts.pory"
	emote_exclamation_mark
	step_end

# 232 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_Trainer_D_Movement_0:
# 232 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 232 "data//maps/MirrohInterior_1F/scripts.pory"
	face_down
	step_end

# 233 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_Trainer_D_Movement_1:
# 233 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 233 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 233 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 233 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 233 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 233 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 233 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
	step_end

# 370 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_0:
# 370 "data//maps/MirrohInterior_1F/scripts.pory"
	jump_in_place_down
	step_end

# 376 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_1:
# 376 "data//maps/MirrohInterior_1F/scripts.pory"
	jump_in_place_up
	step_end

# 393 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_2:
# 393 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_right
	step_end

# 407 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_3:
# 407 "data//maps/MirrohInterior_1F/scripts.pory"
	emote_question_mark
	step_end

# 415 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_4:
# 415 "data//maps/MirrohInterior_1F/scripts.pory"
	emote_exclamation_mark
# 415 "data//maps/MirrohInterior_1F/scripts.pory"
	face_down
	step_end

# 416 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_5:
# 416 "data//maps/MirrohInterior_1F/scripts.pory"
	emote_exclamation_mark
# 416 "data//maps/MirrohInterior_1F/scripts.pory"
	face_up
	step_end

# 424 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_6:
# 424 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 424 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 424 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 424 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_left
# 424 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 424 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
# 424 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
	step_end

# 428 "data//maps/MirrohInterior_1F/scripts.pory"
Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Movement_7:
# 428 "data//maps/MirrohInterior_1F/scripts.pory"
	walk_down
	step_end

Cormoria_MirrohInterior_1F_EventScript_Nurse_Text_0:
# 36 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Have you found Chansey yet? We're all\n"
	.string "really worried!$"

Cormoria_MirrohInterior_1F_EventScript_Chansey_Text_0:
# 42 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Chansey looks stressed!\n"
	.string "Must be worried about its friend.$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Exclamation_Text_0:
# 84 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Hey! Are you a trainer? I need your\n"
	.string "help!$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_Main_Text_0:
# 89 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "I was taking these Chanseys up to\n"
	.string "Winterlily Hollow to help the Nurse Joy\l"
	.string "there, but there was a cave-in and one\l"
	.string "of the Chanseys got lost!\p"
	.string "We can't go through these caves alone -\n"
	.string "the Pokémon are too fierce.\p"
	.string "Please find Chansey for us!$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_MissingChansey_Text_0:
# 102 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "seyy!!!\p"
	.string "Chansey looks so relieved...\n"
	.string "It's about to cry!\p"
	.string "It must have been really scared,\n"
	.string "stuck all alone in here.\p"
	.string "...\p"
	.string "Chansey stretches out a hand...$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_MissingChansey_Text_1:
# 104 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Thank you Chansey!\p"
	.string "You don't need to worry any more.\n"
	.string "Let's go back to Nurse Joy!$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_0:
# 132 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Chansey!$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_1:
# 136 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Oh my! Chansey! Are you alright?$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_2:
# 141 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Seyyy!!!$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_3:
# 147 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Thank you trainer! You saved Chansey!\p"
	.string "This group of people calling themselves\n"
	.string "Team Somber caused these cave-ins!\p"
	.string "They came in here with big machinery\n"
	.string "and collapsed parts of the cavern.\p"
	.string "They seemed so scary!\n"
	.string "I hope we don't run into them.\p"
	.string "We're all in your debt, {PLAYER}.\n"
	.string "There's not much in the way of a reward\l"
	.string "that I can...oh?$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_4:
# 154 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "seeyyy!!\p"
	.string "Chansey pulls something out of its\n"
	.string "pocket and hands it to you!$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_5:
# 156 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Thank you Chansey!$"

Cormoria_MirrohInterior_1F_EventScript_NurseQuest_End_Text_6:
# 162 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "We should go to Winterlily Hollow now.\n"
	.string "The nurse must be waiting for us.\p"
	.string "Thank you again, {PLAYER} and we wish\n"
	.string "you all the best in your journey.\p"
	.string "Drop by the Pokémon Center any time!$"

Cormoria_MirrohInterior_1F_Trainer_A_Text_0:
# 200 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "I'm looking for a Bergmite but why I\n"
	.string "can't find any?$"

Cormoria_MirrohInterior_1F_Trainer_A_Text_1:
# 200 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "If I had a Bergmite I would've won!$"

Cormoria_MirrohInterior_1F_Trainer_A_Text_2:
# 201 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Maybe the Bergmite hide in the rocks or\n"
	.string "something...$"

Cormoria_MirrohInterior_1F_Trainer_B_Text_0:
# 207 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Nope! No way! That item's mine!$"

Cormoria_MirrohInterior_1F_Trainer_B_Text_1:
# 207 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Fine, you can have it...$"

Cormoria_MirrohInterior_1F_Trainer_B_Text_2:
# 208 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Travelers often leave rare items inside\n"
	.string "caves and mountains.$"

Cormoria_MirrohInterior_1F_Trainer_C_Text_0:
# 214 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "I may switch to ICE type Pokémon after\n"
	.string "seeing the Pokémon here!$"

Cormoria_MirrohInterior_1F_Trainer_C_Text_1:
# 214 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "I should just use ROCK types!$"

Cormoria_MirrohInterior_1F_Trainer_C_Text_2:
# 215 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "If you go deeper into the mountain,\n"
	.string "you'll find some rare ICE type Pokémon!$"

Cormoria_MirrohInterior_1F_Trainer_D_Text_0:
# 222 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Hehehe - that Chansey in the corner is\n"
	.string "mine! I just need to figure out how to\l"
	.string "clear these rocks out of the way...$"

Cormoria_MirrohInterior_1F_Trainer_D_Text_1:
# 227 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Who're you? Mind your own business, kid.\p"
	.string "...\p"
	.string "You're going to return Chansey to Nurse\n"
	.string "Joy by the entrance?\p"
	.string "Hah! Fat chance. Ever heard about\n"
	.string "finders keepers? Well I'll teach you\l"
	.string "since nobody else did!$"

Cormoria_MirrohInterior_1F_Trainer_D_Text_2:
# 229 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "No! My new Chansey!$"

Cormoria_MirrohInterior_1F_Trainer_D_Text_3:
# 230 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Fine! You can go give that Chansey back\n"
	.string "to Nurse Joy! I'm outta here!$"

Cormoria_MirrohInterior_1F_Trainer_E_Text_0:
# 244 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "I can't get through to Winterlily Hollow\n"
	.string "from this side. It's so frustrating!$"

Cormoria_MirrohInterior_1F_Trainer_E_Text_1:
# 244 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "You're also frustrating!$"

Cormoria_MirrohInterior_1F_Trainer_E_Text_2:
# 245 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "I mean why even build a tunnel then?$"

Cormoria_MirrohInterior_1F_NPC_A_Text_0:
# 267 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "I'm trekking to Winterlily Hollow! But my\n"
	.string "GRASS type Pokémon can't deal with all\l"
	.string "the ICE type Pokémon around here...$"

Cormoria_MirrohInterior_1F_NPC_B_Text_0:
# 273 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Winterlily Hollow is past the ledge\n"
	.string "above.\n"
	.string "But how do I cross the ledge?$"

Cormoria_MirrohInterior_1F_NPC_C_Text_0:
# 278 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Some dangerous looking people passed\n"
	.string "through here earlier with loads of\l"
	.string "heavy machinery. All that commotion\l"
	.string "caused a bunch of rocks to cave in!$"

Cormoria_MirrohInterior_1F_NPC_D_Text_0:
# 284 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Past this is the trek up to the\n"
	.string "mountain peak. The weather is harsh!$"

Cormoria_MirrohInterior_1F_Pineco_Text_0:
# 295 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Pineco looks cold!$"

Cormoria_MirrohInterior_Bergmite_A_Text_0:
# 309 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "There's nothing here.$"

Cormoria_MirrohInterior_Bergmite_A_Text_1:
# 313 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "{PLAYER} searched the rock...\p"
	.string "...but it wasn't a rock!\n"
	.string "It was a wild Pokémon!$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_0:
# 373 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Grunt A$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_1:
# 374 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Did you move all the equipment? The\n"
	.string "boss is gonna be angry - we gotta get\l"
	.string "the Dreamalyser up and running quick!$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_2:
# 379 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Grunt B$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_3:
# 380 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Would you relax? I'm not Gabrielle - of\n"
	.string "course I moved everything! Caused some\l"
	.string "cave-ins, but that's not our problem!\p"
	.string "Haha!$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_4:
# 390 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Hey! Didn't your parents tell you it was\n"
	.string "rude to eavesdrop?$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_5:
# 398 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Argh! My Pokémon are so weak!$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_6:
# 404 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Argh! Stupid Mime Jr! You're useless!$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_7:
# 411 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Hey - aren't you that lab assistant who\n"
	.string "beat Melea back in Silversun City?$"

Cormoria_MirrohInterior_1F_SomberGrunts_Trigger_Text_8:
# 420 "data//maps/MirrohInterior_1F/scripts.pory"
	.string "Yeah - the same one who beat Moxie up\n"
	.string "in Mt. Ceram too!\p"
	.string "The bosses will want to know this.\n"
	.string "Let's go!$"
