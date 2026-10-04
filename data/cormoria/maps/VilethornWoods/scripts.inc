# 1 "data//maps/VilethornWoods/scripts.pory"

# 2 "data//maps/VilethornWoods/scripts.pory"
    .set Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, 17
# 3 "data//maps/VilethornWoods/scripts.pory"
    .set Cormoria_maps_VilethornWoods_scripts_LOCALID_PROF, 16
# 4 "data//maps/VilethornWoods/scripts.pory"
    .set Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS1, 18
# 5 "data//maps/VilethornWoods/scripts.pory"
    .set Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS2, 19
# 6 "data//maps/VilethornWoods/scripts.pory"
    .set Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS3, 20
# 7 "data//maps/VilethornWoods/scripts.pory"
    .set Cormoria_maps_VilethornWoods_scripts_LOCALID_CELEBI, 31

Cormoria_VilethornWoods_MapScripts::
# 11 "data//maps/VilethornWoods/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_VilethornWoods_MapScripts_MAP_SCRIPT_ON_TRANSITION
	.byte 0

Cormoria_VilethornWoods_MapScripts_MAP_SCRIPT_ON_TRANSITION:
# 12 "data//maps/VilethornWoods/scripts.pory"
	goto Cormoria_VilethornWoods_SetLayout
	return


Cormoria_VilethornWoods_SetLayout::
# 16 "data//maps/VilethornWoods/scripts.pory"
	compare Cormoria_VAR_CELEBI, 2
	goto_if_eq Cormoria_VilethornWoods_SetLayout_1
	return

Cormoria_VilethornWoods_SetLayout_1:
# 17 "data//maps/VilethornWoods/scripts.pory"
	setmaplayoutindex LAYOUT_CORMORIA_VILETHORN_MANSION
	return


Cormoria_VilethornWoods_EventScript_NurseDemo::
# 23 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_NurseDemo_Text_0, MSGBOX_NPC
	end


Cormoria_VilethornWoods_NurseJoy_OnRoute::
# 28 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 29 "data//maps/VilethornWoods/scripts.pory"
	faceplayer
# 30 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NurseJoy_OnRoute_Text_0
# 31 "data//maps/VilethornWoods/scripts.pory"
	call Cormoria_Common_EventScript_NurseJoyOnRoute
# 32 "data//maps/VilethornWoods/scripts.pory"
	delay 30
# 33 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Ralts_FirstTrigger::
# 51 "data//maps/VilethornWoods/scripts.pory"
	goto_if_set Cormoria_FLAG_HIDE_VILETHORN_RALTS_1, Cormoria_VilethornWoods_Ralts_FirstTrigger_2
# 52 "data//maps/VilethornWoods/scripts.pory"
	goto_if_set Cormoria_FLAG_VILETHORN_RALTS_FINISHED, Cormoria_VilethornWoods_Ralts_FirstTrigger_5
# 54 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 55 "data//maps/VilethornWoods/scripts.pory"
	playmoncry SPECIES_RALTS, CRY_MODE_NORMAL
# 56 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS1, Cormoria_VilethornWoods_Ralts_FirstTrigger_Movement_0
# 57 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 58 "data//maps/VilethornWoods/scripts.pory"
	delay 24
# 59 "data//maps/VilethornWoods/scripts.pory"
	playse SE_WARP_OUT
# 60 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS1, Cormoria_VilethornWoods_Ralts_FirstTrigger_Movement_1
# 61 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 65 "data//maps/VilethornWoods/scripts.pory"
	removeobject Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS1
# 66 "data//maps/VilethornWoods/scripts.pory"
	setflag Cormoria_FLAG_HIDE_VILETHORN_RALTS_1
# 67 "data//maps/VilethornWoods/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_VILETHORN_RALTS_2
# 71 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Ralts_FirstTrigger_Text_0
# 72 "data//maps/VilethornWoods/scripts.pory"
	release
# 74 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end

Cormoria_VilethornWoods_Ralts_FirstTrigger_2:
	end

Cormoria_VilethornWoods_Ralts_FirstTrigger_5:
	end


Cormoria_VilethornWoods_Ralts_SecondTrigger::
# 79 "data//maps/VilethornWoods/scripts.pory"
	goto_if_set Cormoria_FLAG_HIDE_VILETHORN_RALTS_2, Cormoria_VilethornWoods_Ralts_SecondTrigger_2
# 80 "data//maps/VilethornWoods/scripts.pory"
	goto_if_set Cormoria_FLAG_VILETHORN_RALTS_FINISHED, Cormoria_VilethornWoods_Ralts_SecondTrigger_5
# 82 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 83 "data//maps/VilethornWoods/scripts.pory"
	playmoncry SPECIES_RALTS, CRY_MODE_NORMAL
# 84 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS2, Cormoria_VilethornWoods_Ralts_FirstTrigger_Movement_0
# 85 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 86 "data//maps/VilethornWoods/scripts.pory"
	delay 24
# 87 "data//maps/VilethornWoods/scripts.pory"
	playse SE_WARP_OUT
# 88 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS2, Cormoria_VilethornWoods_Ralts_FirstTrigger_Movement_1
# 89 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 93 "data//maps/VilethornWoods/scripts.pory"
	removeobject Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS2
# 94 "data//maps/VilethornWoods/scripts.pory"
	setflag Cormoria_FLAG_HIDE_VILETHORN_RALTS_2
# 95 "data//maps/VilethornWoods/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_VILETHORN_RALTS_3
# 99 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Ralts_SecondTrigger_Text_0
# 100 "data//maps/VilethornWoods/scripts.pory"
	release
# 102 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end

Cormoria_VilethornWoods_Ralts_SecondTrigger_2:
	end

Cormoria_VilethornWoods_Ralts_SecondTrigger_5:
	end


Cormoria_VilethornWoods_Ralts::
# 108 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 109 "data//maps/VilethornWoods/scripts.pory"
	playmoncry SPECIES_RALTS, CRY_MODE_NORMAL
# 110 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS3, Cormoria_VilethornWoods_Ralts_Movement_0
# 111 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 112 "data//maps/VilethornWoods/scripts.pory"
	faceplayer
# 113 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 114 "data//maps/VilethornWoods/scripts.pory"
	delay 16
# 115 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Ralts_Text_0
# 116 "data//maps/VilethornWoods/scripts.pory"
	release
# 118 "data//maps/VilethornWoods/scripts.pory"
	setwildbattle SPECIES_RALTS, 18
# 119 "data//maps/VilethornWoods/scripts.pory"
	dowildbattle
# 121 "data//maps/VilethornWoods/scripts.pory"
	setflag Cormoria_FLAG_HIDE_VILETHORN_RALTS_3
# 122 "data//maps/VilethornWoods/scripts.pory"
	setflag Cormoria_FLAG_VILETHORN_RALTS_FINISHED
# 123 "data//maps/VilethornWoods/scripts.pory"
	removeobject Cormoria_maps_VilethornWoods_scripts_LOCALID_RALTS3
# 125 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_EventScript_TriggerProf::
# 150 "data//maps/VilethornWoods/scripts.pory"
	goto_if_set Cormoria_FLAG_VILETHORN_PROF, Cormoria_VilethornWoods_EventScript_TriggerProf_2
# 154 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 157 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, Cormoria_VilethornWoods_Ralts_Movement_0
# 158 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 159 "data//maps/VilethornWoods/scripts.pory"
	delay 16
# 160 "data//maps/VilethornWoods/scripts.pory"
	playse SE_POKENAV_CALL
# 161 "data//maps/VilethornWoods/scripts.pory"
	waitse
# 162 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_EventScript_TriggerProf_Text_0
# 163 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_1
# 164 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_2
# 165 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_EventScript_TriggerProf_Text_0
# 166 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_3
# 167 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_2
# 168 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_EventScript_TriggerProf_Text_0
# 169 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_4
# 170 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_2
# 171 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_EventScript_TriggerProf_Text_0
# 172 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_5
# 173 "data//maps/VilethornWoods/scripts.pory"
	release
# 174 "data//maps/VilethornWoods/scripts.pory"
	playse SE_POKENAV_HANG_UP
# 175 "data//maps/VilethornWoods/scripts.pory"
	waitse
# 176 "data//maps/VilethornWoods/scripts.pory"
	delay 16
# 179 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_PROF, Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_0
# 180 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_0
# 181 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 182 "data//maps/VilethornWoods/scripts.pory"
	delay 16
# 183 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_1
# 184 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 185 "data//maps/VilethornWoods/scripts.pory"
	delay 36
# 186 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_2
# 187 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 188 "data//maps/VilethornWoods/scripts.pory"
	delay 36
# 189 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_3
# 190 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 191 "data//maps/VilethornWoods/scripts.pory"
	delay 36
# 192 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_EventScript_TriggerProf_Text_0
# 193 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_6
# 194 "data//maps/VilethornWoods/scripts.pory"
	release
# 195 "data//maps/VilethornWoods/scripts.pory"
	delay 16
# 196 "data//maps/VilethornWoods/scripts.pory"
	applymovement Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_0
# 197 "data//maps/VilethornWoods/scripts.pory"
	waitmovement 0
# 199 "data//maps/VilethornWoods/scripts.pory"
	setobjectxyperm Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, 0, 0
# 200 "data//maps/VilethornWoods/scripts.pory"
	setobjectxyperm Cormoria_maps_VilethornWoods_scripts_LOCALID_PROF, 1, 0
# 201 "data//maps/VilethornWoods/scripts.pory"
	setobjectxy Cormoria_maps_VilethornWoods_scripts_LOCALID_GRUNT, 0, 0
# 202 "data//maps/VilethornWoods/scripts.pory"
	setobjectxy Cormoria_maps_VilethornWoods_scripts_LOCALID_PROF, 1, 0
# 204 "data//maps/VilethornWoods/scripts.pory"
	setflag Cormoria_FLAG_VILETHORN_PROF
# 205 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	return

Cormoria_VilethornWoods_EventScript_TriggerProf_2:
	end


Cormoria_VilethornWoods_Trainer_A::
# 216 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_A, Cormoria_VilethornWoods_Trainer_A_Text_0, Cormoria_VilethornWoods_Trainer_A_Text_1
# 217 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_A_Text_2
# 218 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Trainer_B::
# 223 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_B, Cormoria_VilethornWoods_Trainer_B_Text_0, Cormoria_VilethornWoods_Trainer_B_Text_1
# 224 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_B_Text_2
# 225 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Trainer_C::
# 230 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_C, Cormoria_VilethornWoods_Trainer_C_Text_0, Cormoria_VilethornWoods_Trainer_C_Text_1
# 231 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_C_Text_2
# 232 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Trainer_D::
# 237 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_D, Cormoria_VilethornWoods_Trainer_D_Text_0, Cormoria_VilethornWoods_Trainer_D_Text_1
# 238 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_D_Text_2
# 239 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Trainer_E::
# 244 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_E, Cormoria_VilethornWoods_Trainer_E_Text_0, Cormoria_VilethornWoods_Trainer_E_Text_1
# 245 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_E_Text_2
# 246 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Trainer_F1::
# 251 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 252 "data//maps/VilethornWoods/scripts.pory"
	faceplayer
# 253 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_F, Cormoria_VilethornWoods_Trainer_F1_Text_0, Cormoria_VilethornWoods_Trainer_F1_Text_1
# 254 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_F1_Text_2
# 255 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Trainer_F2::
# 260 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 261 "data//maps/VilethornWoods/scripts.pory"
	faceplayer
# 262 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_F, Cormoria_VilethornWoods_Trainer_F1_Text_0, Cormoria_VilethornWoods_Trainer_F1_Text_1
# 263 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_F1_Text_2
# 264 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Trainer_G::
# 269 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_G, Cormoria_VilethornWoods_Trainer_G_Text_0, Cormoria_VilethornWoods_Trainer_G_Text_1
# 270 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_G_Text_2
# 271 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_Trainer_H::
# 276 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VILETHORN_H, Cormoria_VilethornWoods_Trainer_H_Text_0, Cormoria_VilethornWoods_Trainer_H_Text_1
# 277 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Trainer_H_Text_2
# 278 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


Cormoria_VilethornWoods_NPC1::
# 285 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_VilethornWoods_NPC2::
# 291 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 292 "data//maps/VilethornWoods/scripts.pory"
	faceplayer
# 294 "data//maps/VilethornWoods/scripts.pory"
	goto_if_set Cormoria_FLAG_VILETHORN_NPC_MUSHROOMS, Cormoria_VilethornWoods_NPC2_2
# 298 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_NPC2_Text_0
# 299 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC2_Text_2, MSGBOX_YESNO
# 301 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornWoods_NPC2_5
# 315 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_NPC2_Text_0
# 316 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC2_Text_5
Cormoria_VilethornWoods_NPC2_1:
# 319 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end

Cormoria_VilethornWoods_NPC2_2:
# 295 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_NPC2_Text_0
# 296 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC2_Text_1
	goto Cormoria_VilethornWoods_NPC2_1

Cormoria_VilethornWoods_NPC2_5:
	checkitem ITEM_BIG_MUSHROOM
# 0 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_ne Cormoria_VilethornWoods_NPC2_8
# 311 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_NPC2_Text_0
# 312 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC2_Text_4
	goto Cormoria_VilethornWoods_NPC2_1

Cormoria_VilethornWoods_NPC2_8:
# 304 "data//maps/VilethornWoods/scripts.pory"
	removeitem ITEM_BIG_MUSHROOM
# 305 "data//maps/VilethornWoods/scripts.pory"
	speakername Cormoria_VilethornWoods_NPC2_Text_0
# 306 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC2_Text_3
# 308 "data//maps/VilethornWoods/scripts.pory"
	goto Cormoria_VilethornWoods_Cook_Reward
	goto Cormoria_VilethornWoods_NPC2_1


Cormoria_VilethornWoods_Cook_Reward::
# 324 "data//maps/VilethornWoods/scripts.pory"
	message Cormoria_VilethornWoods_Cook_Reward_Text_0
# 325 "data//maps/VilethornWoods/scripts.pory"
	waitmessage
# 326 "data//maps/VilethornWoods/scripts.pory"
	dynmultichoice 0, 0, TRUE, 4, 0, DYN_MULTICHOICE_CB_NONE, Cormoria_VilethornWoods_Cook_Reward_Text_1, Cormoria_VilethornWoods_Cook_Reward_Text_2
# 327 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_VilethornWoods_Cook_Reward_2
Cormoria_VilethornWoods_Cook_Reward_1:
# 328 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornWoods_Cook_Reward_5
Cormoria_VilethornWoods_Cook_Reward_4:
# 329 "data//maps/VilethornWoods/scripts.pory"
	goto Cormoria_VilethornWoods_Cook_Reward_Give
	return

Cormoria_VilethornWoods_Cook_Reward_2:
# 327 "data//maps/VilethornWoods/scripts.pory"
	setvar VAR_0x8006, ITEM_TWISTEDSPOON
	goto Cormoria_VilethornWoods_Cook_Reward_1

Cormoria_VilethornWoods_Cook_Reward_5:
# 328 "data//maps/VilethornWoods/scripts.pory"
	setvar VAR_0x8006, ITEM_ROSE_INCENSE
	goto Cormoria_VilethornWoods_Cook_Reward_4


Cormoria_VilethornWoods_Cook_Reward_Give::
# 333 "data//maps/VilethornWoods/scripts.pory"
	showitemdescription
# 334 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Cook_Reward_Give_Text_0, MSGBOX_YESNO
# 335 "data//maps/VilethornWoods/scripts.pory"
	hideitemdescription
# 336 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornWoods_Cook_Reward_Give_1
# 337 "data//maps/VilethornWoods/scripts.pory"
	goto Cormoria_VilethornWoods_Cook_Reward
	return

Cormoria_VilethornWoods_Cook_Reward_Give_1:
# 336 "data//maps/VilethornWoods/scripts.pory"
	giveitem VAR_0x8006
# 336 "data//maps/VilethornWoods/scripts.pory"
	goto Cormoria_VilethornWoods_Cook_RewardEnd
	return


Cormoria_VilethornWoods_Cook_RewardEnd::
# 341 "data//maps/VilethornWoods/scripts.pory"
	setflag Cormoria_FLAG_VILETHORN_NPC_MUSHROOMS
# 342 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Cook_RewardEnd_Text_0, MSGBOX_NPC
	end


Cormoria_VilethornWoods_NPC3::
# 350 "data//maps/VilethornWoods/scripts.pory"
	goto_if_set Cormoria_FLAG_VILETHORN_NPC_ITEM, Cormoria_VilethornWoods_NPC3_2
# 353 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 354 "data//maps/VilethornWoods/scripts.pory"
	faceplayer
# 355 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC3_Text_1, MSGBOX_YESNO
# 356 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornWoods_NPC3_5
# 365 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC3_Text_5, MSGBOX_NPC
Cormoria_VilethornWoods_NPC3_1:
	end

Cormoria_VilethornWoods_NPC3_2:
# 351 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC3_Text_0, MSGBOX_NPC
	goto Cormoria_VilethornWoods_NPC3_1

Cormoria_VilethornWoods_NPC3_5:
# 357 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC3_Text_2
# 358 "data//maps/VilethornWoods/scripts.pory"
	release
# 359 "data//maps/VilethornWoods/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_VILETHORN_I, Cormoria_VilethornWoods_NPC3_Text_3
# 360 "data//maps/VilethornWoods/scripts.pory"
	giveitem ITEM_BIG_ROOT
# 361 "data//maps/VilethornWoods/scripts.pory"
	release
# 362 "data//maps/VilethornWoods/scripts.pory"
	setflag Cormoria_FLAG_VILETHORN_NPC_ITEM
# 363 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_NPC3_Text_4, MSGBOX_NPC
	goto Cormoria_VilethornWoods_NPC3_1


Cormoria_VilethornWoods_Celebi_Seed::
# 378 "data//maps/VilethornWoods/scripts.pory"
	compare Cormoria_VAR_CELEBI, 1
	goto_if_eq Cormoria_VilethornWoods_Celebi_Seed_2
# 422 "data//maps/VilethornWoods/scripts.pory"
	compare Cormoria_VAR_CELEBI, 0
	goto_if_eq Cormoria_VilethornWoods_Celebi_Seed_5
# 444 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end

Cormoria_VilethornWoods_Celebi_Seed_2:
# 379 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Celebi_Seed_Text_0, MSGBOX_NPC
# 381 "data//maps/VilethornWoods/scripts.pory"
	checkitem ITEM_TIME_WATER
# 383 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornWoods_Celebi_Seed_8
Cormoria_VilethornWoods_Celebi_Seed_7:
# 417 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end

Cormoria_VilethornWoods_Celebi_Seed_5:
# 424 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Celebi_Seed_Text_4, MSGBOX_NPC
# 426 "data//maps/VilethornWoods/scripts.pory"
	checkitem ITEM_TIME_SEED
# 428 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornWoods_Celebi_Seed_11
Cormoria_VilethornWoods_Celebi_Seed_10:
# 441 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end

Cormoria_VilethornWoods_Celebi_Seed_8:
# 384 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Celebi_Seed_Text_1, MSGBOX_YESNO
# 385 "data//maps/VilethornWoods/scripts.pory"
	release
# 387 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornWoods_Celebi_Seed_13
	goto Cormoria_VilethornWoods_Celebi_Seed_7

Cormoria_VilethornWoods_Celebi_Seed_11:
# 429 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Celebi_Seed_Text_5, MSGBOX_YESNO
# 430 "data//maps/VilethornWoods/scripts.pory"
	release
# 431 "data//maps/VilethornWoods/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornWoods_Celebi_Seed_15
	goto Cormoria_VilethornWoods_Celebi_Seed_10

Cormoria_VilethornWoods_Celebi_Seed_13:
# 388 "data//maps/VilethornWoods/scripts.pory"
	setvar Cormoria_VAR_CELEBI, 2
# 389 "data//maps/VilethornWoods/scripts.pory"
	removeitem ITEM_TIME_SEED
# 391 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Celebi_Seed_Text_2, MSGBOX_SIGN
# 392 "data//maps/VilethornWoods/scripts.pory"
	release
# 393 "data//maps/VilethornWoods/scripts.pory"
	delay 32
# 394 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_2, MSGBOX_SIGN
# 395 "data//maps/VilethornWoods/scripts.pory"
	delay 32
# 396 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_2, MSGBOX_SIGN
# 397 "data//maps/VilethornWoods/scripts.pory"
	delay 32
# 398 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_EventScript_TriggerProf_Text_2, MSGBOX_SIGN
# 399 "data//maps/VilethornWoods/scripts.pory"
	delay 32
# 401 "data//maps/VilethornWoods/scripts.pory"
	message Cormoria_VilethornWoods_Celebi_Seed_Text_3
# 403 "data//maps/VilethornWoods/scripts.pory"
	setvar VAR_0x8004, 2
# 404 "data//maps/VilethornWoods/scripts.pory"
	setvar VAR_0x8005, 2
# 405 "data//maps/VilethornWoods/scripts.pory"
	setvar VAR_0x8006, 30
# 406 "data//maps/VilethornWoods/scripts.pory"
	setvar VAR_0x8007, 5
# 407 "data//maps/VilethornWoods/scripts.pory"
	special ShakeCamera
# 408 "data//maps/VilethornWoods/scripts.pory"
	delay 20
# 409 "data//maps/VilethornWoods/scripts.pory"
	waitmessage
# 411 "data//maps/VilethornWoods/scripts.pory"
	getplayerxy VAR_TEMP_2, VAR_TEMP_3
# 412 "data//maps/VilethornWoods/scripts.pory"
	warp MAP_CORMORIA_VILETHORN_WOODS, VAR_TEMP_2, VAR_TEMP_3
# 413 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	goto Cormoria_VilethornWoods_Celebi_Seed_7

Cormoria_VilethornWoods_Celebi_Seed_15:
# 432 "data//maps/VilethornWoods/scripts.pory"
	removeitem ITEM_TIME_SEED
# 433 "data//maps/VilethornWoods/scripts.pory"
	setvar Cormoria_VAR_CELEBI, 1
# 434 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Celebi_Seed_Text_6, MSGBOX_SIGN
# 435 "data//maps/VilethornWoods/scripts.pory"
	release
# 436 "data//maps/VilethornWoods/scripts.pory"
	delay 16
# 437 "data//maps/VilethornWoods/scripts.pory"
	msgbox Cormoria_VilethornWoods_Celebi_Seed_Text_7, MSGBOX_SIGN
	goto Cormoria_VilethornWoods_Celebi_Seed_10


Cormoria_VilethornWoods_Celebi::
# 450 "data//maps/VilethornWoods/scripts.pory"
	lockall
# 452 "data//maps/VilethornWoods/scripts.pory"
	special SaveGame
# 453 "data//maps/VilethornWoods/scripts.pory"
	waitstate
# 454 "data//maps/VilethornWoods/scripts.pory"
	release
# 457 "data//maps/VilethornWoods/scripts.pory"
	playmoncry SPECIES_CELEBI, CRY_MODE_NORMAL
# 458 "data//maps/VilethornWoods/scripts.pory"
	waitmoncry
# 459 "data//maps/VilethornWoods/scripts.pory"
	setflag Cormoria_FLAG_CELEBI
# 461 "data//maps/VilethornWoods/scripts.pory"
	setwildbattle SPECIES_CELEBI, 50
# 462 "data//maps/VilethornWoods/scripts.pory"
	dowildbattle
# 463 "data//maps/VilethornWoods/scripts.pory"
	removeobject Cormoria_maps_VilethornWoods_scripts_LOCALID_CELEBI
# 465 "data//maps/VilethornWoods/scripts.pory"
	releaseall
	end


# 56 "data//maps/VilethornWoods/scripts.pory"
Cormoria_VilethornWoods_Ralts_FirstTrigger_Movement_0:
# 56 "data//maps/VilethornWoods/scripts.pory"
	face_up
# 56 "data//maps/VilethornWoods/scripts.pory"
	emote_exclamation_mark
	step_end

# 60 "data//maps/VilethornWoods/scripts.pory"
Cormoria_VilethornWoods_Ralts_FirstTrigger_Movement_1:
# 60 "data//maps/VilethornWoods/scripts.pory"
	fly_up
	step_end

# 110 "data//maps/VilethornWoods/scripts.pory"
Cormoria_VilethornWoods_Ralts_Movement_0:
# 110 "data//maps/VilethornWoods/scripts.pory"
	emote_exclamation_mark
	step_end

# 179 "data//maps/VilethornWoods/scripts.pory"
Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_0:
# 179 "data//maps/VilethornWoods/scripts.pory"
	walk_up
# 179 "data//maps/VilethornWoods/scripts.pory"
	walk_up
# 179 "data//maps/VilethornWoods/scripts.pory"
	walk_up
# 179 "data//maps/VilethornWoods/scripts.pory"
	walk_up
# 179 "data//maps/VilethornWoods/scripts.pory"
	walk_up
# 179 "data//maps/VilethornWoods/scripts.pory"
	walk_up
	step_end

# 183 "data//maps/VilethornWoods/scripts.pory"
Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_1:
# 183 "data//maps/VilethornWoods/scripts.pory"
	walk_down
# 183 "data//maps/VilethornWoods/scripts.pory"
	walk_down
# 183 "data//maps/VilethornWoods/scripts.pory"
	walk_down
# 183 "data//maps/VilethornWoods/scripts.pory"
	walk_down
# 183 "data//maps/VilethornWoods/scripts.pory"
	walk_down
# 183 "data//maps/VilethornWoods/scripts.pory"
	walk_down
# 183 "data//maps/VilethornWoods/scripts.pory"
	emote_question_mark
	step_end

# 186 "data//maps/VilethornWoods/scripts.pory"
Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_2:
# 186 "data//maps/VilethornWoods/scripts.pory"
	face_left
	step_end

# 189 "data//maps/VilethornWoods/scripts.pory"
Cormoria_VilethornWoods_EventScript_TriggerProf_Movement_3:
# 189 "data//maps/VilethornWoods/scripts.pory"
	face_right
	step_end

Cormoria_VilethornWoods_EventScript_NurseDemo_Text_0:
# 23 "data//maps/VilethornWoods/scripts.pory"
	.string "This area is closed for the demo.$"

Cormoria_VilethornWoods_NurseJoy_OnRoute_Text_0:
# 30 "data//maps/VilethornWoods/scripts.pory"
	.string "These woods are so creepy!\n"
	.string "I'm so glad Chansey is here with me.$"

Cormoria_VilethornWoods_Ralts_FirstTrigger_Text_0:
# 71 "data//maps/VilethornWoods/scripts.pory"
	.string "Ralts teleported away!\n"
	.string "It can't have gone far...$"

Cormoria_VilethornWoods_Ralts_SecondTrigger_Text_0:
# 99 "data//maps/VilethornWoods/scripts.pory"
	.string "Ralts teleported away again! It must be\n"
	.string "somewhere in this accursed forest...$"

Cormoria_VilethornWoods_Ralts_Text_0:
# 115 "data//maps/VilethornWoods/scripts.pory"
	.string "Aha! Ralts can't escape this time!$"

Cormoria_VilethornWoods_EventScript_TriggerProf_Text_0:
# 162 "data//maps/VilethornWoods/scripts.pory"
	.string "Grunt$"

Cormoria_VilethornWoods_EventScript_TriggerProf_Text_1:
# 163 "data//maps/VilethornWoods/scripts.pory"
	.string "...\p"
	.string "...yes boss. We have the Professor.$"

Cormoria_VilethornWoods_EventScript_TriggerProf_Text_2:
# 164 "data//maps/VilethornWoods/scripts.pory"
	.string "...$"

Cormoria_VilethornWoods_EventScript_TriggerProf_Text_3:
# 166 "data//maps/VilethornWoods/scripts.pory"
	.string "No, nobody saw us, boss.$"

Cormoria_VilethornWoods_EventScript_TriggerProf_Text_4:
# 169 "data//maps/VilethornWoods/scripts.pory"
	.string "Yes boss, I'm positive.$"

Cormoria_VilethornWoods_EventScript_TriggerProf_Text_5:
# 172 "data//maps/VilethornWoods/scripts.pory"
	.string "...okay, we'll head to HQ right away.$"

Cormoria_VilethornWoods_EventScript_TriggerProf_Text_6:
# 193 "data//maps/VilethornWoods/scripts.pory"
	.string "Must be a wild Impidimp or something...$"

Cormoria_VilethornWoods_Trainer_A_Text_0:
# 216 "data//maps/VilethornWoods/scripts.pory"
	.string "PSYCHIC type Pokémon are my natural\n"
	.string "enemies! That's why I'm training here.$"

Cormoria_VilethornWoods_Trainer_A_Text_1:
# 216 "data//maps/VilethornWoods/scripts.pory"
	.string "I'd rather fight PSYCHIC types than you!$"

Cormoria_VilethornWoods_Trainer_A_Text_2:
# 217 "data//maps/VilethornWoods/scripts.pory"
	.string "I like training but I don't want to go\n"
	.string "too deep into this forest...$"

Cormoria_VilethornWoods_Trainer_B_Text_0:
# 223 "data//maps/VilethornWoods/scripts.pory"
	.string "The psychic currents in the air will lead\n"
	.string "me to victory!$"

Cormoria_VilethornWoods_Trainer_B_Text_1:
# 223 "data//maps/VilethornWoods/scripts.pory"
	.string "The currents betrayed me!$"

Cormoria_VilethornWoods_Trainer_B_Text_2:
# 224 "data//maps/VilethornWoods/scripts.pory"
	.string "Did the psychic energy betray me?\n"
	.string "Were you chosen by them?$"

Cormoria_VilethornWoods_Trainer_C_Text_0:
# 230 "data//maps/VilethornWoods/scripts.pory"
	.string "The sweet, delicious fog envelops\n"
	.string "us...we are one with the mindwaves!$"

Cormoria_VilethornWoods_Trainer_C_Text_1:
# 230 "data//maps/VilethornWoods/scripts.pory"
	.string "You're not one with the mindwaves!$"

Cormoria_VilethornWoods_Trainer_C_Text_2:
# 231 "data//maps/VilethornWoods/scripts.pory"
	.string "I want to go deeper...sink deeper...\n"
	.string "escape my real life...$"

Cormoria_VilethornWoods_Trainer_D_Text_0:
# 237 "data//maps/VilethornWoods/scripts.pory"
	.string "I'm a Pokémon Breeder. We all specialise\n"
	.string "in different Pokémon forms.\p"
	.string "I'm a horse breeding expert!$"

Cormoria_VilethornWoods_Trainer_D_Text_1:
# 237 "data//maps/VilethornWoods/scripts.pory"
	.string "Well fought!$"

Cormoria_VilethornWoods_Trainer_D_Text_2:
# 238 "data//maps/VilethornWoods/scripts.pory"
	.string "I need to raise my horses to become\n"
	.string "stronger. Maybe witamins will help...$"

Cormoria_VilethornWoods_Trainer_E_Text_0:
# 244 "data//maps/VilethornWoods/scripts.pory"
	.string "Eek! These woods are creepy! I was\n"
	.string "supposed to go to Gloria's theater\l"
	.string "studio in Silversun City...but I'm lost!$"

Cormoria_VilethornWoods_Trainer_E_Text_1:
# 244 "data//maps/VilethornWoods/scripts.pory"
	.string "You creep me out too!$"

Cormoria_VilethornWoods_Trainer_E_Text_2:
# 245 "data//maps/VilethornWoods/scripts.pory"
	.string "I've been stuck here for hours. I need\n"
	.string "to get out!$"

Cormoria_VilethornWoods_Trainer_F1_Text_0:
# 253 "data//maps/VilethornWoods/scripts.pory"
	.string "We're on our way to Silversun City\n"
	.string "to interview the great Gloria!\l"
	.string "Get out of our way!$"

Cormoria_VilethornWoods_Trainer_F1_Text_1:
# 253 "data//maps/VilethornWoods/scripts.pory"
	.string "We lost!$"

Cormoria_VilethornWoods_Trainer_F1_Text_2:
# 254 "data//maps/VilethornWoods/scripts.pory"
	.string "If we can get an exclusive interview\n"
	.string "with her, it'll be the scoop of the week!$"

Cormoria_VilethornWoods_Trainer_G_Text_0:
# 269 "data//maps/VilethornWoods/scripts.pory"
	.string "I'm not like the others! I'm one of\n"
	.string "the best bug catchers around.\p"
	.string "I'm going to become the first BUG-type\n"
	.string "Pokémon Champion one day!$"

Cormoria_VilethornWoods_Trainer_G_Text_1:
# 269 "data//maps/VilethornWoods/scripts.pory"
	.string "Are you a Gym Leader or something?$"

Cormoria_VilethornWoods_Trainer_G_Text_2:
# 270 "data//maps/VilethornWoods/scripts.pory"
	.string "As a Gym Leader to-be, my team needs to\n"
	.string "be able to handle a variety of threats.$"

Cormoria_VilethornWoods_Trainer_H_Text_0:
# 276 "data//maps/VilethornWoods/scripts.pory"
	.string "I wonder what's inside this house...\n"
	.string "maybe there's some loot!$"

Cormoria_VilethornWoods_Trainer_H_Text_1:
# 276 "data//maps/VilethornWoods/scripts.pory"
	.string "No loot for me!$"

Cormoria_VilethornWoods_Trainer_H_Text_2:
# 277 "data//maps/VilethornWoods/scripts.pory"
	.string "Fine, I'll just stand here and look\n"
	.string "while you loot everything...$"

Cormoria_VilethornWoods_NPC1_Text_0:
# 285 "data//maps/VilethornWoods/scripts.pory"
	.string "The Morelull here in Vilethorn Woods\n"
	.string "release spores when in danger.\l"
	.string "That's what creates this fog.$"

Cormoria_VilethornWoods_NPC2_Text_0:
# 295 "data//maps/VilethornWoods/scripts.pory"
	.string "Chef$"

Cormoria_VilethornWoods_NPC2_Text_1:
# 296 "data//maps/VilethornWoods/scripts.pory"
	.string "These mushrooms are really rare, you\n"
	.string "know?$"

Cormoria_VilethornWoods_NPC2_Text_2:
# 299 "data//maps/VilethornWoods/scripts.pory"
	.string "You again! Fancy seeing you here.\n"
	.string "I'm a traveling chef! While I usually\l"
	.string "cook at the Pelluca Restaurant,\l"
	.string "I'm here now to collect mushrooms to\l"
	.string "create new recipes...\l"
	.string "but I can only find Tiny Mushrooms!\p"
	.string "Say, why don't you help me?\n"
	.string "If you can get me a Big Mushroom,\l"
	.string "I can give you something in exchange!\p"
	.string "Have you got a Big Mushroom?$"

Cormoria_VilethornWoods_NPC2_Text_3:
# 306 "data//maps/VilethornWoods/scripts.pory"
	.string "Perfect! This'll be a great addition to\n"
	.string "our pantry. Thank you! Now I must give\l"
	.string "you something in exchage.$"

Cormoria_VilethornWoods_NPC2_Text_4:
# 312 "data//maps/VilethornWoods/scripts.pory"
	.string "Hey, you don't have any!\p"
	.string "Are you trying to trick me? The Drowzee\n"
	.string "and Morelull doing enough of that!$"

Cormoria_VilethornWoods_NPC2_Text_5:
# 316 "data//maps/VilethornWoods/scripts.pory"
	.string "Come back when you do!$"

Cormoria_VilethornWoods_Cook_Reward_Text_0:
# 324 "data//maps/VilethornWoods/scripts.pory"
	.string "Perhaps a spoon from the restaurant?\n"
	.string "Or this incense?$"

Cormoria_VilethornWoods_Cook_Reward_Text_1:
# 326 "data//maps/VilethornWoods/scripts.pory"
	.string "Twisted Spoon$"

Cormoria_VilethornWoods_Cook_Reward_Text_2:
# 326 "data//maps/VilethornWoods/scripts.pory"
	.string "Rose Incense$"

Cormoria_VilethornWoods_Cook_Reward_Give_Text_0:
# 334 "data//maps/VilethornWoods/scripts.pory"
	.string "Are you sure?$"

Cormoria_VilethornWoods_Cook_RewardEnd_Text_0:
# 342 "data//maps/VilethornWoods/scripts.pory"
	.string "Maybe I'll see you somewhere else, since\n"
	.string "I travel all over Cormoria. And if you\l"
	.string "ever visit Pelluca City, be sure to drop\l"
	.string "by our restaurant!$"

Cormoria_VilethornWoods_NPC3_Text_0:
# 351 "data//maps/VilethornWoods/scripts.pory"
	.string "My foot hurts!$"

Cormoria_VilethornWoods_NPC3_Text_1:
# 355 "data//maps/VilethornWoods/scripts.pory"
	.string "Gah! I got lost in this stupid forest\n"
	.string "and tripped on a giant tree root.\l"
	.string "I got mad and ripped it out!\p"
	.string "Do you want it?$"

Cormoria_VilethornWoods_NPC3_Text_2:
# 357 "data//maps/VilethornWoods/scripts.pory"
	.string "Gah - I'm just so angry that you'll have\n"
	.string "to battle me for it!$"

Cormoria_VilethornWoods_NPC3_Text_3:
# 359 "data//maps/VilethornWoods/scripts.pory"
	.string "My anger is gone now!$"

Cormoria_VilethornWoods_NPC3_Text_4:
# 363 "data//maps/VilethornWoods/scripts.pory"
	.string "Good riddance, stupid root!\n"
	.string "I hope you find some use for it.$"

Cormoria_VilethornWoods_NPC3_Text_5:
# 365 "data//maps/VilethornWoods/scripts.pory"
	.string "Just as well. It's a root after all -\n"
	.string "what use would it have?$"

Cormoria_VilethornWoods_Celebi_Seed_Text_0:
# 379 "data//maps/VilethornWoods/scripts.pory"
	.string "A mystical seed has been planted in\n"
	.string "this soil.\p"
	.string "Maybe it needs some type of water to\n"
	.string "grow...$"

Cormoria_VilethornWoods_Celebi_Seed_Text_1:
# 384 "data//maps/VilethornWoods/scripts.pory"
	.string "Water with the Water of Time?$"

Cormoria_VilethornWoods_Celebi_Seed_Text_2:
# 391 "data//maps/VilethornWoods/scripts.pory"
	.string "{PLAYER} watered the fertile soil\n"
	.string "with the Water of Time.$"

Cormoria_VilethornWoods_Celebi_Seed_Text_3:
# 401 "data//maps/VilethornWoods/scripts.pory"
	.string "The trees shift...$"

Cormoria_VilethornWoods_Celebi_Seed_Text_4:
# 424 "data//maps/VilethornWoods/scripts.pory"
	.string "It's fertile soil...but not the same as\n"
	.string "berry tree soil.$"

Cormoria_VilethornWoods_Celebi_Seed_Text_5:
# 429 "data//maps/VilethornWoods/scripts.pory"
	.string "Plant a Seed of Time?$"

Cormoria_VilethornWoods_Celebi_Seed_Text_6:
# 434 "data//maps/VilethornWoods/scripts.pory"
	.string "{PLAYER} planted Seed of Time in the\n"
	.string "fertile soil.$"

Cormoria_VilethornWoods_Celebi_Seed_Text_7:
# 437 "data//maps/VilethornWoods/scripts.pory"
	.string "Maybe it needs some type of water to\n"
	.string "grow...$"
