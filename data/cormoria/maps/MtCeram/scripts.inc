# 1 "data//maps/MtCeram/scripts.pory"

# 2 "data//maps/MtCeram/scripts.pory"
    .set Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA, 19
# 3 "data//maps/MtCeram/scripts.pory"
    .set Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT1, 20
# 4 "data//maps/MtCeram/scripts.pory"
    .set Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT2, 21
# 5 "data//maps/MtCeram/scripts.pory"
    .set Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT1, 10
# 6 "data//maps/MtCeram/scripts.pory"
    .set Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT2, 9
# 7 "data//maps/MtCeram/scripts.pory"
    .set Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, 22
# 8 "data//maps/MtCeram/scripts.pory"
    .set Cormoria_maps_MtCeram_scripts_LOCALID_GAB, 5

Cormoria_MtCeram_MapScripts::
	.byte 0


Cormoria_MtCeram_Charizard::
# 18 "data//maps/MtCeram/scripts.pory"
	lockall
# 20 "data//maps/MtCeram/scripts.pory"
	checkitem ITEM_POKE_FLUTE
# 22 "data//maps/MtCeram/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_MtCeram_Charizard_2
# 30 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Charizard_Text_1, MSGBOX_YESNO
# 31 "data//maps/MtCeram/scripts.pory"
	release
# 33 "data//maps/MtCeram/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_MtCeram_Charizard_5
# 73 "data//maps/MtCeram/scripts.pory"
	release
Cormoria_MtCeram_Charizard_4:
# 75 "data//maps/MtCeram/scripts.pory"
	releaseall
	end

Cormoria_MtCeram_Charizard_2:
# 23 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Charizard_Text_0
# 24 "data//maps/MtCeram/scripts.pory"
	releaseall
	end

Cormoria_MtCeram_Charizard_5:
# 34 "data//maps/MtCeram/scripts.pory"
	fadeoutbgm 1
# 35 "data//maps/MtCeram/scripts.pory"
	playse MUS_RG_POKE_FLUTE
# 36 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Charizard_Text_2
# 37 "data//maps/MtCeram/scripts.pory"
	delay 128
# 38 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Charizard_Text_3
# 39 "data//maps/MtCeram/scripts.pory"
	release
# 40 "data//maps/MtCeram/scripts.pory"
	applymovement 18, Cormoria_MtCeram_Charizard_Movement_0
# 41 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 42 "data//maps/MtCeram/scripts.pory"
	delay 30
# 43 "data//maps/MtCeram/scripts.pory"
	applymovement 18, Cormoria_MtCeram_Charizard_Movement_1
# 44 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 45 "data//maps/MtCeram/scripts.pory"
	delay 60
# 46 "data//maps/MtCeram/scripts.pory"
	applymovement 18, Cormoria_MtCeram_Charizard_Movement_2
# 47 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 48 "data//maps/MtCeram/scripts.pory"
	delay 60
# 49 "data//maps/MtCeram/scripts.pory"
	applymovement 18, Cormoria_MtCeram_Charizard_Movement_3
# 50 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 51 "data//maps/MtCeram/scripts.pory"
	delay 16
# 52 "data//maps/MtCeram/scripts.pory"
	playmoncry SPECIES_CHARIZARD, CRY_MODE_ENCOUNTER
# 53 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Charizard_Text_4
# 54 "data//maps/MtCeram/scripts.pory"
	setwildbattle SPECIES_CHARIZARD, 48
# 55 "data//maps/MtCeram/scripts.pory"
	dowildbattle
# 58 "data//maps/MtCeram/scripts.pory"
	specialvar VAR_RESULT, GetBattleOutcome
# 59 "data//maps/MtCeram/scripts.pory"
	compare VAR_RESULT, B_OUTCOME_WON
	goto_if_eq Cormoria_MtCeram_Charizard_9
Cormoria_MtCeram_Charizard_8:
# 63 "data//maps/MtCeram/scripts.pory"
	compare VAR_RESULT, B_OUTCOME_CAUGHT
	goto_if_eq Cormoria_MtCeram_Charizard_12
Cormoria_MtCeram_Charizard_11:
# 67 "data//maps/MtCeram/scripts.pory"
	compare VAR_RESULT, B_OUTCOME_RAN
	goto_if_eq Cormoria_MtCeram_Charizard_14
	goto Cormoria_MtCeram_Charizard_4

Cormoria_MtCeram_Charizard_9:
# 60 "data//maps/MtCeram/scripts.pory"
	removeobject 18
# 61 "data//maps/MtCeram/scripts.pory"
	setflag Cormoria_FLAG_CERAM_CHARIZARD
	goto Cormoria_MtCeram_Charizard_8

Cormoria_MtCeram_Charizard_12:
# 64 "data//maps/MtCeram/scripts.pory"
	removeobject 18
# 65 "data//maps/MtCeram/scripts.pory"
	setflag Cormoria_FLAG_CERAM_CHARIZARD
	goto Cormoria_MtCeram_Charizard_11

Cormoria_MtCeram_Charizard_14:
# 68 "data//maps/MtCeram/scripts.pory"
	applymovement 18, Cormoria_MtCeram_Charizard_Movement_4
# 69 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Charizard_Text_5
# 70 "data//maps/MtCeram/scripts.pory"
	release
	goto Cormoria_MtCeram_Charizard_4


Cormoria_MtCeram_Eavesdrop_Trigger0::
# 86 "data//maps/MtCeram/scripts.pory"
	goto_if_set Cormoria_FLAG_MT_CERAM_SOMBER_FOOT, Cormoria_MtCeram_Eavesdrop_Trigger0_2
# 87 "data//maps/MtCeram/scripts.pory"
	playbgm MUS_ENCOUNTER_MAGMA, FALSE
# 88 "data//maps/MtCeram/scripts.pory"
	lockall
# 89 "data//maps/MtCeram/scripts.pory"
	delay 16
# 90 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA, Cormoria_MtCeram_Eavesdrop_Trigger0_Movement_0
# 91 "data//maps/MtCeram/scripts.pory"
	delay 16
# 93 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 94 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Eavesdrop_Trigger0_Text_1
# 95 "data//maps/MtCeram/scripts.pory"
	release
# 97 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtCeram_Eavesdrop_Trigger0_Movement_1
# 98 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 100 "data//maps/MtCeram/scripts.pory"
	call Cormoria_MtCeram_EventScript_Eavesdrop
	end

Cormoria_MtCeram_Eavesdrop_Trigger0_2:
	end


Cormoria_MtCeram_Eavesdrop_Trigger1::
# 105 "data//maps/MtCeram/scripts.pory"
	goto_if_set Cormoria_FLAG_MT_CERAM_SOMBER_FOOT, Cormoria_MtCeram_Eavesdrop_Trigger1_2
# 106 "data//maps/MtCeram/scripts.pory"
	playbgm MUS_ENCOUNTER_MAGMA, FALSE
# 107 "data//maps/MtCeram/scripts.pory"
	lockall
# 108 "data//maps/MtCeram/scripts.pory"
	delay 16
# 109 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA, Cormoria_MtCeram_Eavesdrop_Trigger0_Movement_0
# 110 "data//maps/MtCeram/scripts.pory"
	delay 16
# 112 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 113 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Eavesdrop_Trigger0_Text_1
# 114 "data//maps/MtCeram/scripts.pory"
	release
# 116 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtCeram_Eavesdrop_Trigger1_Movement_0
# 117 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 119 "data//maps/MtCeram/scripts.pory"
	call Cormoria_MtCeram_EventScript_Eavesdrop
	end

Cormoria_MtCeram_Eavesdrop_Trigger1_2:
	end


Cormoria_MtCeram_Eavesdrop_Trigger2::
# 124 "data//maps/MtCeram/scripts.pory"
	goto_if_set Cormoria_FLAG_MT_CERAM_SOMBER_FOOT, Cormoria_MtCeram_Eavesdrop_Trigger2_2
# 125 "data//maps/MtCeram/scripts.pory"
	playbgm MUS_ENCOUNTER_MAGMA, FALSE
# 126 "data//maps/MtCeram/scripts.pory"
	lockall
# 127 "data//maps/MtCeram/scripts.pory"
	delay 16
# 128 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA, Cormoria_MtCeram_Eavesdrop_Trigger0_Movement_0
# 129 "data//maps/MtCeram/scripts.pory"
	delay 16
# 131 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 132 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Eavesdrop_Trigger0_Text_1
# 133 "data//maps/MtCeram/scripts.pory"
	release
# 135 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtCeram_Eavesdrop_Trigger2_Movement_0
# 136 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 138 "data//maps/MtCeram/scripts.pory"
	call Cormoria_MtCeram_EventScript_Eavesdrop
	end

Cormoria_MtCeram_Eavesdrop_Trigger2_2:
	end


Cormoria_MtCeram_Eavesdrop_Trigger3::
# 143 "data//maps/MtCeram/scripts.pory"
	goto_if_set Cormoria_FLAG_MT_CERAM_SOMBER_FOOT, Cormoria_MtCeram_Eavesdrop_Trigger3_2
# 144 "data//maps/MtCeram/scripts.pory"
	playbgm MUS_ENCOUNTER_MAGMA, FALSE
# 145 "data//maps/MtCeram/scripts.pory"
	lockall
# 146 "data//maps/MtCeram/scripts.pory"
	delay 16
# 147 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA, Cormoria_MtCeram_Eavesdrop_Trigger0_Movement_0
# 148 "data//maps/MtCeram/scripts.pory"
	delay 16
# 150 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 151 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_Eavesdrop_Trigger0_Text_1
# 152 "data//maps/MtCeram/scripts.pory"
	release
# 154 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtCeram_Eavesdrop_Trigger3_Movement_0
# 155 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 157 "data//maps/MtCeram/scripts.pory"
	call Cormoria_MtCeram_EventScript_Eavesdrop
	end

Cormoria_MtCeram_Eavesdrop_Trigger3_2:
	end


Cormoria_MtCeram_EventScript_Eavesdrop::
# 163 "data//maps/MtCeram/scripts.pory"
	lockall
# 164 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_0
# 165 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 166 "data//maps/MtCeram/scripts.pory"
	delay 16
# 168 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 169 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Eavesdrop_Text_0
# 170 "data//maps/MtCeram/scripts.pory"
	release
# 172 "data//maps/MtCeram/scripts.pory"
	special SpawnCameraObject
# 173 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_CAMERA, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_1
# 174 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 175 "data//maps/MtCeram/scripts.pory"
	special RemoveCameraObject
# 178 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT1, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_2
# 179 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 180 "data//maps/MtCeram/scripts.pory"
	delay 16
# 181 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 182 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Eavesdrop_Text_2
# 183 "data//maps/MtCeram/scripts.pory"
	release
# 184 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT2, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_3
# 185 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_3
# 186 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Eavesdrop_Text_4
# 187 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT1, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_2
# 188 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 189 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Eavesdrop_Text_5
# 190 "data//maps/MtCeram/scripts.pory"
	release
# 191 "data//maps/MtCeram/scripts.pory"
	delay 16
# 192 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT2, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_4
# 193 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 194 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_3
# 195 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Eavesdrop_Text_6
# 196 "data//maps/MtCeram/scripts.pory"
	release
# 197 "data//maps/MtCeram/scripts.pory"
	delay 8
# 198 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT1, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_2
# 199 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 200 "data//maps/MtCeram/scripts.pory"
	delay 16
# 201 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 202 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Eavesdrop_Text_7
# 203 "data//maps/MtCeram/scripts.pory"
	release
# 206 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT1, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_5
# 207 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT2, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_5
# 208 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 211 "data//maps/MtCeram/scripts.pory"
	removeobject Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT1
# 212 "data//maps/MtCeram/scripts.pory"
	removeobject Cormoria_maps_MtCeram_scripts_LOCALID_FOOTGRUNT2
# 213 "data//maps/MtCeram/scripts.pory"
	delay 8
# 216 "data//maps/MtCeram/scripts.pory"
	special SpawnCameraObject
# 217 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_CAMERA, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_6
# 218 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 219 "data//maps/MtCeram/scripts.pory"
	special RemoveCameraObject
# 221 "data//maps/MtCeram/scripts.pory"
	delay 8
# 224 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_7
# 225 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 226 "data//maps/MtCeram/scripts.pory"
	delay 16
# 227 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 228 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Eavesdrop_Text_8
# 229 "data//maps/MtCeram/scripts.pory"
	release
# 232 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_8
# 233 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 234 "data//maps/MtCeram/scripts.pory"
	removeobject Cormoria_maps_MtCeram_scripts_LOCALID_FOOTKOHLA
# 236 "data//maps/MtCeram/scripts.pory"
	fadedefaultbgm
# 238 "data//maps/MtCeram/scripts.pory"
	setflag Cormoria_FLAG_MT_CERAM_SOMBER_FOOT
# 241 "data//maps/MtCeram/scripts.pory"
	releaseall
	return


Cormoria_MtCeram_BeforePeak_Trigger0::
# 270 "data//maps/MtCeram/scripts.pory"
	goto_if_set Cormoria_FLAG_MT_CERAM_SOMBER_PEAK, Cormoria_MtCeram_BeforePeak_Trigger0_2
# 271 "data//maps/MtCeram/scripts.pory"
	playbgm MUS_ENCOUNTER_MAGMA, FALSE
# 272 "data//maps/MtCeram/scripts.pory"
	lockall
# 273 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtCeram_BeforePeak_Trigger0_Movement_0
# 274 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 275 "data//maps/MtCeram/scripts.pory"
	call Cormoria_MtCeram_EventScript_BeforePeakBattle
	end

Cormoria_MtCeram_BeforePeak_Trigger0_2:
	end


Cormoria_MtCeram_BeforePeak_Trigger1::
# 280 "data//maps/MtCeram/scripts.pory"
	goto_if_set Cormoria_FLAG_MT_CERAM_SOMBER_PEAK, Cormoria_MtCeram_BeforePeak_Trigger1_2
# 282 "data//maps/MtCeram/scripts.pory"
	lockall
# 283 "data//maps/MtCeram/scripts.pory"
	playbgm MUS_ENCOUNTER_MAGMA, FALSE
# 284 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtCeram_BeforePeak_Trigger1_Movement_0
# 285 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 286 "data//maps/MtCeram/scripts.pory"
	call Cormoria_MtCeram_EventScript_BeforePeakBattle
	end

Cormoria_MtCeram_BeforePeak_Trigger1_2:
	end


Cormoria_MtCeram_EventScript_BeforePeakBattle::
# 292 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_0
# 293 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 294 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 295 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_0
# 296 "data//maps/MtCeram/scripts.pory"
	release
# 297 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT1, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_3
# 298 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT2, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_3
# 299 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 300 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 301 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_1
# 302 "data//maps/MtCeram/scripts.pory"
	release
# 303 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, Cormoria_MtCeram_Charizard_Movement_0
# 304 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 305 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 306 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_2
# 307 "data//maps/MtCeram/scripts.pory"
	release
# 308 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT1, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_3
# 309 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT2, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_3
# 310 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 311 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_3
# 312 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_3
# 313 "data//maps/MtCeram/scripts.pory"
	release
# 314 "data//maps/MtCeram/scripts.pory"
	delay 16
# 316 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_1
# 317 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 318 "data//maps/MtCeram/scripts.pory"
	delay 16
# 319 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT1, Cormoria_MtCeram_Charizard_Movement_0
# 320 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT2, Cormoria_MtCeram_Charizard_Movement_0
# 321 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, Cormoria_MtCeram_Charizard_Movement_0
# 322 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 323 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_3
# 324 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_4
# 325 "data//maps/MtCeram/scripts.pory"
	release
# 326 "data//maps/MtCeram/scripts.pory"
	delay 16
# 327 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_2
# 328 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 329 "data//maps/MtCeram/scripts.pory"
	delay 16
# 331 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 332 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_5
# 333 "data//maps/MtCeram/scripts.pory"
	release
# 334 "data//maps/MtCeram/scripts.pory"
	delay 8
# 335 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, Cormoria_MtCeram_Charizard_Movement_4
# 336 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 337 "data//maps/MtCeram/scripts.pory"
	delay 8
# 338 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 339 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_6
# 340 "data//maps/MtCeram/scripts.pory"
	release
# 341 "data//maps/MtCeram/scripts.pory"
	fadeoutbgm 0
# 344 "data//maps/MtCeram/scripts.pory"
	fadescreenswapbuffers FADE_TO_BLACK
# 345 "data//maps/MtCeram/scripts.pory"
	playse SE_SUPER_EFFECTIVE
# 346 "data//maps/MtCeram/scripts.pory"
	waitse
# 347 "data//maps/MtCeram/scripts.pory"
	playmoncry SPECIES_CHARCADET, CRY_MODE_NORMAL
# 348 "data//maps/MtCeram/scripts.pory"
	waitmoncry
# 349 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 350 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_7
# 351 "data//maps/MtCeram/scripts.pory"
	release
# 352 "data//maps/MtCeram/scripts.pory"
	delay 8
# 353 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 354 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_8
# 355 "data//maps/MtCeram/scripts.pory"
	release
# 356 "data//maps/MtCeram/scripts.pory"
	fadescreenswapbuffers FADE_FROM_BLACK
# 357 "data//maps/MtCeram/scripts.pory"
	playbgm MUS_ENCOUNTER_MAGMA, FALSE
# 359 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_3
# 360 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 364 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0
# 365 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_9
# 366 "data//maps/MtCeram/scripts.pory"
	release
# 367 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA, Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_4
# 368 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 371 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT1, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_3
# 372 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT2, Cormoria_MtCeram_EventScript_Eavesdrop_Movement_3
# 373 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 374 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 375 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_10
# 376 "data//maps/MtCeram/scripts.pory"
	release
# 377 "data//maps/MtCeram/scripts.pory"
	applymovement Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT2, Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_5
# 378 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 379 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_3
# 380 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_11
# 381 "data//maps/MtCeram/scripts.pory"
	release
# 382 "data//maps/MtCeram/scripts.pory"
	delay 8
# 383 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 384 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_12
# 385 "data//maps/MtCeram/scripts.pory"
	release
# 387 "data//maps/MtCeram/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_6
# 388 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 391 "data//maps/MtCeram/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_MTCERAM_GRUNT_DOUBLE, Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_13
# 394 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 395 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_14
# 396 "data//maps/MtCeram/scripts.pory"
	release
# 397 "data//maps/MtCeram/scripts.pory"
	delay 8
# 398 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_3
# 399 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_15
# 400 "data//maps/MtCeram/scripts.pory"
	release
# 401 "data//maps/MtCeram/scripts.pory"
	delay 8
# 402 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Eavesdrop_Text_1
# 403 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_16
# 404 "data//maps/MtCeram/scripts.pory"
	release
# 407 "data//maps/MtCeram/scripts.pory"
	fadescreenswapbuffers FADE_TO_BLACK
# 408 "data//maps/MtCeram/scripts.pory"
	removeobject Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT1
# 409 "data//maps/MtCeram/scripts.pory"
	removeobject Cormoria_maps_MtCeram_scripts_LOCALID_TOPGRUNT2
# 410 "data//maps/MtCeram/scripts.pory"
	removeobject Cormoria_maps_MtCeram_scripts_LOCALID_GAB
# 411 "data//maps/MtCeram/scripts.pory"
	removeobject Cormoria_maps_MtCeram_scripts_LOCALID_TOPKOHLA
# 412 "data//maps/MtCeram/scripts.pory"
	playse SE_EXIT
# 413 "data//maps/MtCeram/scripts.pory"
	waitse
# 414 "data//maps/MtCeram/scripts.pory"
	fadescreenswapbuffers FADE_FROM_BLACK
# 416 "data//maps/MtCeram/scripts.pory"
	fadedefaultbgm
# 419 "data//maps/MtCeram/scripts.pory"
	setflag Cormoria_FLAG_MT_CERAM_SOMBER_PEAK
# 421 "data//maps/MtCeram/scripts.pory"
	releaseall
	end


Cormoria_MtCeram_TrainerBattle_A::
# 440 "data//maps/MtCeram/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MTCERAM_A, Cormoria_MtCeram_TrainerBattle_A_Text_0, Cormoria_MtCeram_TrainerBattle_A_Text_1
# 441 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_TrainerBattle_A_Text_2
# 442 "data//maps/MtCeram/scripts.pory"
	release
	end


Cormoria_MtCeram_TrainerBattle_B::
# 448 "data//maps/MtCeram/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MTCERAM_B, Cormoria_MtCeram_TrainerBattle_B_Text_0, Cormoria_MtCeram_TrainerBattle_B_Text_1
# 449 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_TrainerBattle_B_Text_2
# 450 "data//maps/MtCeram/scripts.pory"
	release
	end


Cormoria_MtCeram_TrainerBattle_C::
# 456 "data//maps/MtCeram/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MTCERAM_C, Cormoria_MtCeram_TrainerBattle_C_Text_0, Cormoria_MtCeram_TrainerBattle_C_Text_1
# 457 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_TrainerBattle_C_Text_2
# 458 "data//maps/MtCeram/scripts.pory"
	release
	end


Cormoria_MtCeram_TrainerBattle_D::
# 464 "data//maps/MtCeram/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MTCERAM_D, Cormoria_MtCeram_TrainerBattle_D_Text_0, Cormoria_MtCeram_TrainerBattle_D_Text_1
# 465 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_TrainerBattle_D_Text_2
# 466 "data//maps/MtCeram/scripts.pory"
	release
	end


Cormoria_MtCeram_TrainerBattle_GRUNT1::
# 471 "data//maps/MtCeram/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MTCERAM_GRUNT1, Cormoria_MtCeram_TrainerBattle_GRUNT1_Text_0, Cormoria_MtCeram_TrainerBattle_GRUNT1_Text_1
# 472 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_TrainerBattle_GRUNT1_Text_2
# 473 "data//maps/MtCeram/scripts.pory"
	release
	end


Cormoria_MtCeram_TrainerBattle_GRUNT2::
# 479 "data//maps/MtCeram/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_MTCERAM_GRUNT2, Cormoria_MtCeram_TrainerBattle_GRUNT2_Text_0, Cormoria_MtCeram_TrainerBattle_GRUNT2_Text_1
# 480 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_TrainerBattle_GRUNT2_Text_2
# 481 "data//maps/MtCeram/scripts.pory"
	release
	end


Cormoria_MtCeram_EventScript_NPC1::
# 503 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_MtCeram_EventScript_NPC2::
# 509 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_MtCeram_EventScript_NPC3::
# 515 "data//maps/MtCeram/scripts.pory"
	goto_if_set Cormoria_FLAG_MT_CERAM_FRESH_WATER, Cormoria_MtCeram_EventScript_NPC3_2
# 518 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_NPC3_Text_1, MSGBOX_NPC
# 519 "data//maps/MtCeram/scripts.pory"
	giveitem ITEM_FRESH_WATER
# 520 "data//maps/MtCeram/scripts.pory"
	release
# 521 "data//maps/MtCeram/scripts.pory"
	setflag Cormoria_FLAG_MT_CERAM_FRESH_WATER
Cormoria_MtCeram_EventScript_NPC3_1:
	end

Cormoria_MtCeram_EventScript_NPC3_2:
# 516 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_NPC3_Text_0, MSGBOX_NPC
	goto Cormoria_MtCeram_EventScript_NPC3_1


Cormoria_MtCeram_EventScript_NPCNurse::
# 530 "data//maps/MtCeram/scripts.pory"
	lockall
# 531 "data//maps/MtCeram/scripts.pory"
	faceplayer
# 533 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_NPCNurse_Text_0
# 534 "data//maps/MtCeram/scripts.pory"
	release
# 535 "data//maps/MtCeram/scripts.pory"
	playmoncry SPECIES_CHANSEY, CRY_MODE_NORMAL
# 536 "data//maps/MtCeram/scripts.pory"
	applymovement 24, Cormoria_MtCeram_EventScript_NPCNurse_Movement_0
# 537 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 538 "data//maps/MtCeram/scripts.pory"
	delay 24
# 539 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_NPCNurse_Text_1
# 540 "data//maps/MtCeram/scripts.pory"
	call Cormoria_Common_EventScript_NurseJoyOnRoute
# 542 "data//maps/MtCeram/scripts.pory"
	releaseall
	end


Cormoria_MtCeram_EventScript_Gabrielle::
# 547 "data//maps/MtCeram/scripts.pory"
	lockall
# 548 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Gabrielle_Text_0
# 549 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Gabrielle_Text_1
# 550 "data//maps/MtCeram/scripts.pory"
	release
# 551 "data//maps/MtCeram/scripts.pory"
	delay 30
# 552 "data//maps/MtCeram/scripts.pory"
	applymovement 5, Cormoria_MtCeram_Charizard_Movement_0
# 553 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 554 "data//maps/MtCeram/scripts.pory"
	release
# 555 "data//maps/MtCeram/scripts.pory"
	faceplayer
# 556 "data//maps/MtCeram/scripts.pory"
	delay 20
# 557 "data//maps/MtCeram/scripts.pory"
	speakername Cormoria_MtCeram_EventScript_Gabrielle_Text_0
# 558 "data//maps/MtCeram/scripts.pory"
	msgbox Cormoria_MtCeram_EventScript_Gabrielle_Text_2
# 559 "data//maps/MtCeram/scripts.pory"
	release
# 560 "data//maps/MtCeram/scripts.pory"
	delay 16
# 561 "data//maps/MtCeram/scripts.pory"
	applymovement 5, Cormoria_MtCeram_EventScript_Gabrielle_Movement_0
# 562 "data//maps/MtCeram/scripts.pory"
	waitmovement 0
# 563 "data//maps/MtCeram/scripts.pory"
	releaseall
	return


# 40 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Charizard_Movement_0:
# 40 "data//maps/MtCeram/scripts.pory"
	emote_exclamation_mark
	step_end

# 43 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Charizard_Movement_1:
# 43 "data//maps/MtCeram/scripts.pory"
	emote_question_mark
# 43 "data//maps/MtCeram/scripts.pory"
	face_left
	step_end

# 46 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Charizard_Movement_2:
# 46 "data//maps/MtCeram/scripts.pory"
	emote_question_mark
# 46 "data//maps/MtCeram/scripts.pory"
	face_right
	step_end

# 49 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Charizard_Movement_3:
# 49 "data//maps/MtCeram/scripts.pory"
	face_down
# 49 "data//maps/MtCeram/scripts.pory"
	emote_exclamation_mark
	step_end

# 68 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Charizard_Movement_4:
# 68 "data//maps/MtCeram/scripts.pory"
	face_up
	step_end

# 90 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Eavesdrop_Trigger0_Movement_0:
# 90 "data//maps/MtCeram/scripts.pory"
	emote_exclamation_mark
# 90 "data//maps/MtCeram/scripts.pory"
	face_down
	step_end

# 97 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Eavesdrop_Trigger0_Movement_1:
# 97 "data//maps/MtCeram/scripts.pory"
	walk_up
# 97 "data//maps/MtCeram/scripts.pory"
	walk_left
# 97 "data//maps/MtCeram/scripts.pory"
	walk_left
# 97 "data//maps/MtCeram/scripts.pory"
	walk_up
# 97 "data//maps/MtCeram/scripts.pory"
	face_right
	step_end

# 116 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Eavesdrop_Trigger1_Movement_0:
# 116 "data//maps/MtCeram/scripts.pory"
	walk_up
# 116 "data//maps/MtCeram/scripts.pory"
	walk_left
# 116 "data//maps/MtCeram/scripts.pory"
	walk_left
# 116 "data//maps/MtCeram/scripts.pory"
	walk_left
# 116 "data//maps/MtCeram/scripts.pory"
	walk_up
# 116 "data//maps/MtCeram/scripts.pory"
	face_right
	step_end

# 135 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Eavesdrop_Trigger2_Movement_0:
# 135 "data//maps/MtCeram/scripts.pory"
	walk_up
# 135 "data//maps/MtCeram/scripts.pory"
	walk_left
# 135 "data//maps/MtCeram/scripts.pory"
	walk_left
# 135 "data//maps/MtCeram/scripts.pory"
	walk_left
# 135 "data//maps/MtCeram/scripts.pory"
	walk_left
# 135 "data//maps/MtCeram/scripts.pory"
	walk_up
# 135 "data//maps/MtCeram/scripts.pory"
	face_right
	step_end

# 154 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_Eavesdrop_Trigger3_Movement_0:
# 154 "data//maps/MtCeram/scripts.pory"
	walk_up
# 154 "data//maps/MtCeram/scripts.pory"
	walk_left
# 154 "data//maps/MtCeram/scripts.pory"
	walk_left
# 154 "data//maps/MtCeram/scripts.pory"
	walk_left
# 154 "data//maps/MtCeram/scripts.pory"
	walk_left
# 154 "data//maps/MtCeram/scripts.pory"
	walk_left
# 154 "data//maps/MtCeram/scripts.pory"
	walk_up
# 154 "data//maps/MtCeram/scripts.pory"
	face_right
	step_end

# 164 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_0:
# 164 "data//maps/MtCeram/scripts.pory"
	face_right
	step_end

# 173 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_1:
# 173 "data//maps/MtCeram/scripts.pory"
	walk_right
# 173 "data//maps/MtCeram/scripts.pory"
	walk_right
# 173 "data//maps/MtCeram/scripts.pory"
	walk_up
# 173 "data//maps/MtCeram/scripts.pory"
	walk_up
	step_end

# 178 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_2:
# 178 "data//maps/MtCeram/scripts.pory"
	jump_in_place_right
	step_end

# 184 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_3:
# 184 "data//maps/MtCeram/scripts.pory"
	emote_question_mark
	step_end

# 192 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_4:
# 192 "data//maps/MtCeram/scripts.pory"
	jump_in_place_left
	step_end

# 206 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_5:
# 206 "data//maps/MtCeram/scripts.pory"
	walk_right
# 206 "data//maps/MtCeram/scripts.pory"
	walk_right
# 206 "data//maps/MtCeram/scripts.pory"
	walk_right
# 206 "data//maps/MtCeram/scripts.pory"
	walk_right
# 206 "data//maps/MtCeram/scripts.pory"
	walk_right
# 206 "data//maps/MtCeram/scripts.pory"
	walk_right
# 206 "data//maps/MtCeram/scripts.pory"
	walk_right
# 206 "data//maps/MtCeram/scripts.pory"
	walk_right
	step_end

# 217 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_6:
# 217 "data//maps/MtCeram/scripts.pory"
	walk_down
# 217 "data//maps/MtCeram/scripts.pory"
	walk_down
# 217 "data//maps/MtCeram/scripts.pory"
	walk_left
# 217 "data//maps/MtCeram/scripts.pory"
	walk_left
	step_end

# 224 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_7:
# 224 "data//maps/MtCeram/scripts.pory"
	emote_exclamation_mark
# 224 "data//maps/MtCeram/scripts.pory"
	face_left
	step_end

# 232 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Eavesdrop_Movement_8:
# 232 "data//maps/MtCeram/scripts.pory"
	walk_down
# 232 "data//maps/MtCeram/scripts.pory"
	walk_right
# 232 "data//maps/MtCeram/scripts.pory"
	walk_right
# 232 "data//maps/MtCeram/scripts.pory"
	walk_right
# 232 "data//maps/MtCeram/scripts.pory"
	walk_right
# 232 "data//maps/MtCeram/scripts.pory"
	walk_up
# 232 "data//maps/MtCeram/scripts.pory"
	walk_up
# 232 "data//maps/MtCeram/scripts.pory"
	walk_right
# 232 "data//maps/MtCeram/scripts.pory"
	walk_right
# 232 "data//maps/MtCeram/scripts.pory"
	walk_right
# 232 "data//maps/MtCeram/scripts.pory"
	walk_right
	step_end

# 273 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_BeforePeak_Trigger0_Movement_0:
# 273 "data//maps/MtCeram/scripts.pory"
	walk_up
# 273 "data//maps/MtCeram/scripts.pory"
	walk_up
# 273 "data//maps/MtCeram/scripts.pory"
	walk_up
# 273 "data//maps/MtCeram/scripts.pory"
	walk_left
# 273 "data//maps/MtCeram/scripts.pory"
	walk_left
# 273 "data//maps/MtCeram/scripts.pory"
	face_up
	step_end

# 284 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_BeforePeak_Trigger1_Movement_0:
# 284 "data//maps/MtCeram/scripts.pory"
	walk_up
# 284 "data//maps/MtCeram/scripts.pory"
	walk_up
# 284 "data//maps/MtCeram/scripts.pory"
	walk_up
# 284 "data//maps/MtCeram/scripts.pory"
	walk_left
# 284 "data//maps/MtCeram/scripts.pory"
	walk_left
# 284 "data//maps/MtCeram/scripts.pory"
	walk_left
# 284 "data//maps/MtCeram/scripts.pory"
	face_up
	step_end

# 292 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_0:
# 292 "data//maps/MtCeram/scripts.pory"
	jump_in_place_up
	step_end

# 316 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_1:
# 316 "data//maps/MtCeram/scripts.pory"
	face_down
	step_end

# 327 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_2:
# 327 "data//maps/MtCeram/scripts.pory"
	jump_in_place_down
	step_end

# 359 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_3:
# 359 "data//maps/MtCeram/scripts.pory"
	walk_down
	step_end

# 367 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_4:
# 367 "data//maps/MtCeram/scripts.pory"
	walk_down
# 367 "data//maps/MtCeram/scripts.pory"
	walk_right
# 367 "data//maps/MtCeram/scripts.pory"
	walk_right
# 367 "data//maps/MtCeram/scripts.pory"
	walk_down
# 367 "data//maps/MtCeram/scripts.pory"
	walk_down
# 367 "data//maps/MtCeram/scripts.pory"
	walk_down
# 367 "data//maps/MtCeram/scripts.pory"
	walk_down
# 367 "data//maps/MtCeram/scripts.pory"
	walk_down
	step_end

# 377 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_5:
# 377 "data//maps/MtCeram/scripts.pory"
	jump_in_place_down
# 377 "data//maps/MtCeram/scripts.pory"
	emote_exclamation_mark
	step_end

# 387 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_BeforePeakBattle_Movement_6:
# 387 "data//maps/MtCeram/scripts.pory"
	walk_up
	step_end

# 536 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_NPCNurse_Movement_0:
# 536 "data//maps/MtCeram/scripts.pory"
	jump_in_place_up
# 536 "data//maps/MtCeram/scripts.pory"
	emote_exclamation_mark
	step_end

# 561 "data//maps/MtCeram/scripts.pory"
Cormoria_MtCeram_EventScript_Gabrielle_Movement_0:
# 561 "data//maps/MtCeram/scripts.pory"
	face_left
	step_end

Cormoria_MtCeram_Charizard_Text_0:
# 23 "data//maps/MtCeram/scripts.pory"
	.string "It's asleep...$"

Cormoria_MtCeram_Charizard_Text_1:
# 30 "data//maps/MtCeram/scripts.pory"
	.string "It's asleep...use Poké Flute?$"

Cormoria_MtCeram_Charizard_Text_2:
# 36 "data//maps/MtCeram/scripts.pory"
	.string "{PLAYER} played the Poke Flute.$"

Cormoria_MtCeram_Charizard_Text_3:
# 38 "data//maps/MtCeram/scripts.pory"
	.string "...$"

Cormoria_MtCeram_Charizard_Text_4:
# 53 "data//maps/MtCeram/scripts.pory"
	.string "The Charizard is furious!$"

Cormoria_MtCeram_Charizard_Text_5:
# 69 "data//maps/MtCeram/scripts.pory"
	.string "The Charizard grunts and falls back\n"
	.string "asleep. Better not disturb it again!$"

Cormoria_MtCeram_Eavesdrop_Trigger0_Text_0:
# 93 "data//maps/MtCeram/scripts.pory"
	.string "Kohla$"

Cormoria_MtCeram_Eavesdrop_Trigger0_Text_1:
# 94 "data//maps/MtCeram/scripts.pory"
	.string "{PLAYER} - what are you doing?\n"
	.string "Come here, you idiot!$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_0:
# 169 "data//maps/MtCeram/scripts.pory"
	.string "I've been spying on those two Team\n"
	.string "Somber Grunts. Don't make a sound!$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_1:
# 181 "data//maps/MtCeram/scripts.pory"
	.string "Grunt A$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_2:
# 182 "data//maps/MtCeram/scripts.pory"
	.string "...boss told us to be EXTRA careful not\n"
	.string "to let anyone up the mountain.$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_3:
# 185 "data//maps/MtCeram/scripts.pory"
	.string "Grunt B$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_4:
# 186 "data//maps/MtCeram/scripts.pory"
	.string "But where's the boss? And Gabrielle?$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_5:
# 189 "data//maps/MtCeram/scripts.pory"
	.string "The boss found the dreamstone at the\n"
	.string "mountain peak! He's heading there now.\p"
	.string "As for Gabrielle...after her punishment\n"
	.string "for losing to that Tenebris Lab trainer\l"
	.string "twice...she's been a little down.$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_6:
# 195 "data//maps/MtCeram/scripts.pory"
	.string "Serves her right for losing! Team\n"
	.string "Somber doesn't need any losers!$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_7:
# 202 "data//maps/MtCeram/scripts.pory"
	.string "Yeah what a loser she is!\p"
	.string "Now lets go stand guard. We can't let\n"
	.string "anyone pass until the boss gets the\l"
	.string "dreamstone at the peak!$"

Cormoria_MtCeram_EventScript_Eavesdrop_Text_8:
# 228 "data//maps/MtCeram/scripts.pory"
	.string "Heard that? I've got to get that\n"
	.string "dreamstone before Team Somber!\p"
	.string "That's called responsibility.\n"
	.string "Something you wouldn't know about\p"
	.string "...\p"
	.string "What? You're coming too?\p"
	.string "Hah!\p"
	.string "We're not buddies, {PLAYER}. I'm the\n"
	.string "one and only Ace Rookie, and I've told\l"
	.string "you before - I'm strong enough alone.\p"
	.string "I don't need your help, chum!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_0:
# 295 "data//maps/MtCeram/scripts.pory"
	.string "That's enough of your mischief Team\n"
	.string "Somber! You're getting in the way of\l"
	.string "police business!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_1:
# 301 "data//maps/MtCeram/scripts.pory"
	.string "You're just a kid. Where's the police?$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_2:
# 306 "data//maps/MtCeram/scripts.pory"
	.string "I'm not a kid! I'm the Ace Rookie of the\n"
	.string "Detective Academy, and I'm not going\l"
	.string "to let you steal the dreamstone!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_3:
# 312 "data//maps/MtCeram/scripts.pory"
	.string "You alone? Against the two of us?$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_4:
# 324 "data//maps/MtCeram/scripts.pory"
	.string "Oh look - the help arrives.\n"
	.string "But you're both just kids! Hahaha!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_5:
# 332 "data//maps/MtCeram/scripts.pory"
	.string "{PLAYER}, I told you I don't need your\n"
	.string "help! You'll just get in the way.$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_6:
# 339 "data//maps/MtCeram/scripts.pory"
	.string "Now come on Team Somber! Bring it on!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_7:
# 350 "data//maps/MtCeram/scripts.pory"
	.string "No! Charcadet!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_8:
# 354 "data//maps/MtCeram/scripts.pory"
	.string "Your ace was a pushover, Ace Rookie!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_9:
# 365 "data//maps/MtCeram/scripts.pory"
	.string "My Pokémon...I have to go and heal\n"
	.string "them...I couldn't stop Team Somber...$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_10:
# 375 "data//maps/MtCeram/scripts.pory"
	.string "And you? You want a beating too?$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_11:
# 380 "data//maps/MtCeram/scripts.pory"
	.string "Wait a second...aren't you the trainer\n"
	.string "that beat Gabrielle?$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_12:
# 384 "data//maps/MtCeram/scripts.pory"
	.string "Hey - yeah! But Gabrielle's just a loser.\n"
	.string "But we're the real deal!\p"
	.string "Let's see what you've got!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_13:
# 391 "data//maps/MtCeram/scripts.pory"
	.string "Argh! You stupid kid!$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_14:
# 395 "data//maps/MtCeram/scripts.pory"
	.string "How could you beat us?$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_15:
# 399 "data//maps/MtCeram/scripts.pory"
	.string "We-we've got to tell the boss...$"

Cormoria_MtCeram_EventScript_BeforePeakBattle_Text_16:
# 403 "data//maps/MtCeram/scripts.pory"
	.string "Yeah...c'mon let's hurry.\p"
	.string "{PLAYER} - you may have beaten us\n"
	.string "but you can't stop Team Somber from\l"
	.string "achieving its goal!\p"
	.string "We're warning you - DON'T follow us up\n"
	.string "the mountain peak, or you'll be sorry!$"

Cormoria_MtCeram_TrainerBattle_A_Text_0:
# 440 "data//maps/MtCeram/scripts.pory"
	.string "These mountains are the perfect\n"
	.string "training spot. Don't you agree?$"

Cormoria_MtCeram_TrainerBattle_A_Text_1:
# 440 "data//maps/MtCeram/scripts.pory"
	.string "It's never enough!$"

Cormoria_MtCeram_TrainerBattle_A_Text_2:
# 441 "data//maps/MtCeram/scripts.pory"
	.string "Since it's so hot here, you sweat more.\n"
	.string "And sweat is good!$"

Cormoria_MtCeram_TrainerBattle_B_Text_0:
# 448 "data//maps/MtCeram/scripts.pory"
	.string "I'm a firefighter! I'm training against\n"
	.string "all the FIRE types here. You look like\l"
	.string "you could use a dowsing of water!$"

Cormoria_MtCeram_TrainerBattle_B_Text_1:
# 448 "data//maps/MtCeram/scripts.pory"
	.string "You can't be extinguished!$"

Cormoria_MtCeram_TrainerBattle_B_Text_2:
# 449 "data//maps/MtCeram/scripts.pory"
	.string "Fire safety starts with awareness.\n"
	.string "Stay safe, stay prepared!$"

Cormoria_MtCeram_TrainerBattle_C_Text_0:
# 456 "data//maps/MtCeram/scripts.pory"
	.string "My Sizzlepede loves preying on the\n"
	.string "Venipede around here!$"

Cormoria_MtCeram_TrainerBattle_C_Text_1:
# 456 "data//maps/MtCeram/scripts.pory"
	.string "You sizzle too much!$"

Cormoria_MtCeram_TrainerBattle_C_Text_2:
# 457 "data//maps/MtCeram/scripts.pory"
	.string "Sizzlepede and Venipede are sworn\n"
	.string "enemies, much like their evolutions.$"

Cormoria_MtCeram_TrainerBattle_D_Text_0:
# 464 "data//maps/MtCeram/scripts.pory"
	.string "I brought my sheep Pokémon here but\n"
	.string "their wool got singed in the heat!$"

Cormoria_MtCeram_TrainerBattle_D_Text_1:
# 464 "data//maps/MtCeram/scripts.pory"
	.string "My sheep and I should go back down!$"

Cormoria_MtCeram_TrainerBattle_D_Text_2:
# 465 "data//maps/MtCeram/scripts.pory"
	.string "Pokémon Breeders specialise in a single\n"
	.string "group. I specialise in rearing sheep!$"

Cormoria_MtCeram_TrainerBattle_GRUNT1_Text_0:
# 471 "data//maps/MtCeram/scripts.pory"
	.string "Hey, kid! Where'd you come from?\n"
	.string "You're not supposed to be here.\p"
	.string "Why?\p"
	.string "Stop asking questions and get lost!$"

Cormoria_MtCeram_TrainerBattle_GRUNT1_Text_1:
# 471 "data//maps/MtCeram/scripts.pory"
	.string "Fine! Don't get lost!$"

Cormoria_MtCeram_TrainerBattle_GRUNT1_Text_2:
# 472 "data//maps/MtCeram/scripts.pory"
	.string "Team Somber has control of this\n"
	.string "mountain now! Well...except for you.$"

Cormoria_MtCeram_TrainerBattle_GRUNT2_Text_0:
# 479 "data//maps/MtCeram/scripts.pory"
	.string "We're going to get the dreamstone and\n"
	.string "Team Somber will reward us!\p"
	.string "Wait, you're not a grunt!.\n"
	.string "Get out of here!$"

Cormoria_MtCeram_TrainerBattle_GRUNT2_Text_1:
# 479 "data//maps/MtCeram/scripts.pory"
	.string "I'm going to be punished for this!$"

Cormoria_MtCeram_TrainerBattle_GRUNT2_Text_2:
# 480 "data//maps/MtCeram/scripts.pory"
	.string "I love Team Somber! No wait...\n"
	.string "I love money!$"

Cormoria_MtCeram_EventScript_NPC1_Text_0:
# 503 "data//maps/MtCeram/scripts.pory"
	.string "If I jump over this ledge, I won't be\n"
	.string "able to return! So I do my jogging here.$"

Cormoria_MtCeram_EventScript_NPC2_Text_0:
# 509 "data//maps/MtCeram/scripts.pory"
	.string "If you're here, that means you've got\n"
	.string "two gym badges, right?\p"
	.string "To enter the Pokémon Championship,\n"
	.string "you'll need all eight badges.$"

Cormoria_MtCeram_EventScript_NPC3_Text_0:
# 516 "data//maps/MtCeram/scripts.pory"
	.string "Oof! This place is unbearably hot!$"

Cormoria_MtCeram_EventScript_NPC3_Text_1:
# 518 "data//maps/MtCeram/scripts.pory"
	.string "Oof! This place is unbearably hot!\n"
	.string "Don't forget to stay hydrated!$"

Cormoria_MtCeram_EventScript_NPCNurse_Text_0:
# 533 "data//maps/MtCeram/scripts.pory"
	.string "Chansey and I have been hiding from\n"
	.string "these thugs behind this tree...although\l"
	.string "Chansey can't hide herself too well.$"

Cormoria_MtCeram_EventScript_NPCNurse_Text_1:
# 539 "data//maps/MtCeram/scripts.pory"
	.string "Don't worry, Chansey! We're safe here.\p"
	.string "Meanwhile, if you have a Heal Pass,\n"
	.string "I can heal your Pokémon!$"

Cormoria_MtCeram_EventScript_Gabrielle_Text_0:
# 548 "data//maps/MtCeram/scripts.pory"
	.string "Gabrielle$"

Cormoria_MtCeram_EventScript_Gabrielle_Text_1:
# 549 "data//maps/MtCeram/scripts.pory"
	.string "Why do I always have to be punished?\n"
	.string "I'm trying my best! I don't want to\l"
	.string "steal other people's stuff but I'm still\l"
	.string "doing it for the team.\p"
	.string "It's not fair!\p"
	.string "...$"

Cormoria_MtCeram_EventScript_Gabrielle_Text_2:
# 558 "data//maps/MtCeram/scripts.pory"
	.string "Don't look at me like that!\n"
	.string "You don't know what its like...$"
