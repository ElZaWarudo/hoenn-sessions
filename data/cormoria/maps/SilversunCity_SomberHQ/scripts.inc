# 1 "data//maps/SilversunCity_SomberHQ/scripts.pory"

# 2 "data//maps/SilversunCity_SomberHQ/scripts.pory"
    .set Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_PROF, 1
# 3 "data//maps/SilversunCity_SomberHQ/scripts.pory"
    .set Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, 2
# 4 "data//maps/SilversunCity_SomberHQ/scripts.pory"
    .set Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB, 3
# 5 "data//maps/SilversunCity_SomberHQ/scripts.pory"
    .set Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GRUNT, 4

Cormoria_SilversunCity_SomberHQ_MapScripts::
	.byte 0


Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger0::
# 18 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto_if_set Cormoria_FLAG_SILVERSUN_SOMBERHQ_MAIN, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger0_2
# 19 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 20 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger0_Movement_0
# 21 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 22 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	call Cormoria_SilversunCity_SomberHQ_EventScript_Main
	return

Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger0_2:
	end


Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger1::
# 25 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto_if_set Cormoria_FLAG_SILVERSUN_SOMBERHQ_MAIN, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger1_2
# 26 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 27 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger1_Movement_0
# 28 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 29 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	call Cormoria_SilversunCity_SomberHQ_EventScript_Main
	return

Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger1_2:
	end


Cormoria_SilversunCity_SomberHQ_EventScript_Main::
# 36 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto_if_set Cormoria_FLAG_SKIP_SILVERSUN_CUTSCENE, Cormoria_SilversunCity_SomberHQ_EventScript_Main_1
# 54 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene
	return

Cormoria_SilversunCity_SomberHQ_EventScript_Main_1:
# 37 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Main_Text_0, MSGBOX_YESNO
# 38 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 39 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_SilversunCity_SomberHQ_EventScript_Main_4
# 43 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	fadescreen FADE_TO_BLACK
# 44 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setobjectxy Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB, 0, 0
# 45 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger0_Movement_0
# 46 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Movement_0
# 47 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_PROF, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Movement_1
# 48 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 49 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	fadescreen FADE_FROM_BLACK
# 51 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto Cormoria_SilversunCity_SomberHQ_EventScript_Skip
	return

Cormoria_SilversunCity_SomberHQ_EventScript_Main_4:
# 40 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene
	return


Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene::
# 61 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setflag Cormoria_FLAG_SKIP_SILVERSUN_CUTSCENE
# 65 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_0
# 66 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 67 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 24
# 68 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_0
# 69 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_1
# 71 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_2
# 72 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_3
# 74 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_0
# 75 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_4
# 76 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 78 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 81 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_PROF, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_1
# 82 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_1
# 83 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GRUNT, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_1
# 84 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 85 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 30
# 89 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_0
# 90 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 91 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 24
# 92 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_0
# 93 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_5
# 94 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 95 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 30
# 97 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setvar VAR_0x8004, 3
# 98 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setvar VAR_0x8005, 3
# 99 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setvar VAR_0x8006, 8
# 100 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setvar VAR_0x8007, 5
# 101 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	special ShakeCamera
# 102 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_0
# 103 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_6
# 104 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 107 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger0_Movement_0
# 108 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_PROF, Cormoria_SilversunCity_SomberHQ_EventScript_Main_Movement_1
# 109 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 110 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_2
# 111 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GRUNT, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_3
# 112 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 113 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 24
# 116 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_0
# 117 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_7
# 118 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 119 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_PROF, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_0
# 120 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 121 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 122 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_2
# 123 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_8
# 124 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 125 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 36
# 128 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_3
# 129 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 130 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 36
# 131 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_2
# 132 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 133 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 36
# 134 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_4
# 135 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 136 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 36
# 137 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_5
# 138 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 139 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 36
# 140 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_0
# 141 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_9
# 142 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 143 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GRUNT, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_6
# 144 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 145 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 146 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_10
# 147 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_11
# 148 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 149 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 151 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_7
# 152 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 153 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 155 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_0
# 156 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_12
# 157 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 158 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 160 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_8
# 161 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 162 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 163 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_9
# 164 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 165 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 166 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_13
# 167 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_14
# 168 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 169 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 170 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_10
# 171 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 172 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 173 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15
# 174 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_16
# 175 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_13
# 176 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_17
# 177 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 178 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 8
# 179 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_11
# 180 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 181 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 182 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15
# 183 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_18
# 184 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 185 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 186 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_7
# 187 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 188 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 189 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_13
# 190 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_19
# 191 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 192 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 194 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_0
# 195 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 196 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 197 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15
# 198 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_20
# 200 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 201 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_13
# 202 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_21
# 203 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 204 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 8
# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_12
# 206 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 207 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setobjectxy Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB, 0, 0
# 208 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	playse SE_WARP_OUT
# 209 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitse
# 213 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15
# 214 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_22
# 215 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 216 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_13
# 217 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 218 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15
# 219 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_23
# 221 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto Cormoria_SilversunCity_SomberHQ_EventScript_Skip
	return


Cormoria_SilversunCity_SomberHQ_EventScript_Skip::
# 226 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15
# 227 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_0
# 230 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_SOMBERHQ_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_1
# 233 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setflag Cormoria_FLAG_SILVERSUN_SOMBERHQ_MAIN
# 238 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15
# 239 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_2
# 240 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 241 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GRUNT, Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_0
# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_1
# 243 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 244 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_4
# 245 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 246 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 12
# 247 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15
# 248 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_3
# 249 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 250 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_2
# 251 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GRUNT, Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_3
# 252 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 253 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	removeobject Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GRUNT
# 254 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	playse SE_WARP_OUT
# 255 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA, Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_4
# 256 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 257 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	removeobject Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_MELEA
# 258 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	playse SE_WARP_OUT
# 259 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitse
# 261 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 262 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_2
# 263 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_PROF, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_8
# 264 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 265 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	speakername Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_2
# 266 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_4
# 267 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 269 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_SILVERSUN_PROF
# 270 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	removeobject Cormoria_maps_SilversunCity_SomberHQ_scripts_LOCALID_GAB
# 273 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	warp MAP_CORMORIA_SILVERSUN_CITY, 23, 11
# 274 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	return


Cormoria_SomberHQ_EventScript_NPCNurse::
# 309 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 310 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_EventScript_NPCNurse_Text_0
# 311 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 312 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto Cormoria_Common_EventScript_NurseJoyOnRoute
# 313 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_NPC1::
# 320 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 321 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	faceplayer
# 322 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC1_Text_0
# 323 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_NPC2::
# 327 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 328 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	faceplayer
# 329 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC2_Text_0
# 330 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_NPC3::
# 334 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 335 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	faceplayer
# 336 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC3_Text_0
# 337 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_NPC4::
# 341 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 342 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	faceplayer
# 343 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC4_Text_0
# 344 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_NPCGrunt::
# 348 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 349 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPCGrunt_Text_0
# 350 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement 12, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_7
# 351 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 352 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	faceplayer
# 353 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPCGrunt_Text_1
# 354 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 355 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement 12, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_2
# 356 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 357 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_NPC_Counterfeiter::
# 363 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 364 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC_Counterfeiter_Text_0
# 365 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 366 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	applymovement 23, Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_9
# 367 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	waitmovement 0
# 368 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 369 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC_Counterfeiter_Text_1
# 370 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_NPC_CounterfeitMachine::
# 377 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	lockall
# 379 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_0, MSGBOX_SIGN
# 380 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 381 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	delay 16
# 382 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	checkitem ITEM_DETECTIVE_STUDENT_ID
# 384 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_SomberHQ_NPC_CounterfeitMachine_2
# 400 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_4, MSGBOX_SIGN
Cormoria_SomberHQ_NPC_CounterfeitMachine_1:
# 403 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end

Cormoria_SomberHQ_NPC_CounterfeitMachine_2:
# 385 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_1, MSGBOX_YESNO
# 387 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_SomberHQ_NPC_CounterfeitMachine_5
	goto Cormoria_SomberHQ_NPC_CounterfeitMachine_1

Cormoria_SomberHQ_NPC_CounterfeitMachine_5:
# 388 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	removeitem ITEM_DETECTIVE_STUDENT_ID
# 389 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_2
# 390 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 391 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	giveitem ITEM_FAKE_STUDENT_ID
# 392 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_3
# 393 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	release
# 394 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setflag Cormoria_FLAG_GALECREST_FAKEID
	goto Cormoria_SomberHQ_NPC_CounterfeitMachine_1


Cormoria_SomberHQ_Grunt_A::
# 410 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_SOMBERHQ_A, Cormoria_SomberHQ_Grunt_A_Text_0, Cormoria_SomberHQ_Grunt_A_Text_1
# 411 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_Grunt_A_Text_2
# 412 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_Grunt_B::
# 417 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_SOMBERHQ_B, Cormoria_SomberHQ_Grunt_B_Text_0, Cormoria_SomberHQ_Grunt_B_Text_1
# 418 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_Grunt_B_Text_2
# 419 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_Grunt_C::
# 423 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_SOMBERHQ_C, Cormoria_SomberHQ_Grunt_C_Text_0, Cormoria_SomberHQ_Grunt_C_Text_1
# 424 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_Grunt_C_Text_2
# 425 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_Grunt_D::
# 429 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_SOMBERHQ_D, Cormoria_SomberHQ_Grunt_D_Text_0, Cormoria_SomberHQ_Grunt_D_Text_1
# 430 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_Grunt_D_Text_2
# 431 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_Grunt_E::
# 435 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_SOMBERHQ_E, Cormoria_SomberHQ_Grunt_E_Text_0, Cormoria_SomberHQ_Grunt_E_Text_1
# 436 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_Grunt_E_Text_2
# 437 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_Grunt_F::
# 441 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_SOMBERHQ_F, Cormoria_SomberHQ_Grunt_F_Text_0, Cormoria_SomberHQ_Grunt_F_Text_1
# 442 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_Grunt_F_Text_2
# 443 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_Grunt_G::
# 447 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_SOMBERHQ_G, Cormoria_SomberHQ_Grunt_G_Text_0, Cormoria_SomberHQ_Grunt_G_Text_0
# 448 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_Grunt_G_Text_0
# 449 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_Grunt_H::
# 453 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_SOMBERHQ_H, Cormoria_SomberHQ_Grunt_H_Text_0, Cormoria_SomberHQ_Grunt_H_Text_1
# 454 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	msgbox Cormoria_SomberHQ_Grunt_H_Text_2
# 455 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end


Cormoria_SomberHQ_SaveGameTrigger::
# 474 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	goto_if_set Cormoria_FLAG_SILVERSUN_SOMBERHQ_MAIN, Cormoria_SomberHQ_SaveGameTrigger_2
# 476 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	setvar VAR_TEMP_E, 1
# 477 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	call Cormoria_Common_EventScript_SaveGameBeforeSomber
# 478 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	releaseall
	end

Cormoria_SomberHQ_SaveGameTrigger_2:
	end


# 20 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger0_Movement_0:
# 20 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_right
# 20 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_right
	step_end

# 27 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Main_Trigger1_Movement_0:
# 27 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_up
# 27 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_right
# 27 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_right
	step_end

# 46 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Main_Movement_0:
# 46 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
	step_end

# 47 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Main_Movement_1:
# 47 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_right
# 47 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_down
# 47 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_right
# 47 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_left
	step_end

# 65 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_0:
# 65 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	jump_in_place_left
	step_end

# 81 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_1:
# 81 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_left
# 81 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	emote_exclamation_mark
	step_end

# 110 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_2:
# 110 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_down
	step_end

# 111 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_3:
# 111 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_up
	step_end

# 134 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_4:
# 134 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_left
	step_end

# 137 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_5:
# 137 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	emote_question_mark
	step_end

# 143 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_6:
# 143 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	jump_in_place_up
	step_end

# 151 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_7:
# 151 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	emote_exclamation_mark
	step_end

# 160 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_8:
# 160 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 160 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_up
# 160 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_left
	step_end

# 163 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_9:
# 163 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_right
# 163 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	emote_exclamation_mark
	step_end

# 170 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_10:
# 170 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	jump_in_place_left
# 170 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	emote_exclamation_mark
	step_end

# 179 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_11:
# 179 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	emote_exclamation_mark
# 179 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	jump_in_place_left
# 179 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	jump_in_place_left
	step_end

# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_12:
# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_fast_left
# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_fast_down
# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_fast_left
# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_fast_left
# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_fast_left
# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_fast_left
# 205 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_fast_left
	step_end

# 216 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Movement_13:
# 216 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_down
# 216 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_left
	step_end

# 241 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_0:
# 241 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_left
# 241 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 241 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 241 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 241 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
	step_end

# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_1:
# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_up
# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_down
# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 242 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_right
	step_end

# 250 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_2:
# 250 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	face_right
	step_end

# 251 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_3:
# 251 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 251 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 251 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_up
	step_end

# 255 "data//maps/SilversunCity_SomberHQ/scripts.pory"
Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Movement_4:
# 255 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 255 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
# 255 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	walk_left
	step_end

Cormoria_SilversunCity_SomberHQ_EventScript_Main_Text_0:
# 37 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Skip cutscene?$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_0:
# 68 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "???$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_1:
# 69 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Wonderful. We have much to thank you\n"
	.string "for, Assistant Professor Rue.\p"
	.string "Your Dreamalyser blueprints will be\n"
	.string "invaluable to our efforts to get the\l"
	.string "second dreamstone.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_2:
# 71 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Rue$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_3:
# 72 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "You mean the one on Mt. Mirroh?$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_4:
# 75 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Precisely.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_5:
# 93 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Who's there?\p"
	.string "...\p"
	.string "Come here child. Let me see your face.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_6:
# 103 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "I said COME!!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_7:
# 117 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "So you're the one who defeated Moxie\n"
	.string "at Mt. Ceram. So young...you must've\l"
	.string "gotten really lucky.\p"
	.string "Meet your Assistant Professor Rue. He's\n"
	.string "been of great help...albeit unwillingly.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_8:
# 123 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "{PLAYER}, I'm sorry. Th-they got me.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_9:
# 141 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "How did you get inside our\n"
	.string "headquarters, I wonder...$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_10:
# 146 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Grunt$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_11:
# 147 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Boss! Boss! Gabrielle was on guard duty.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_12:
# 156 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "So it was you, Gabrielle?\p"
	.string "Hmm...what a blunder. Moxie was right.\n"
	.string "You really are useless.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_13:
# 166 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Gabrielle$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_14:
# 167 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Miss Melea, I-I'm sorry.\n"
	.string "I owed {PLAYER} and I didn't-$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_15:
# 173 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Melea$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_16:
# 174 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Y-you OWED {PLAYER}? You mean that\n"
	.string "it was INTENTIONAL!? And don't use my\l"
	.string "name in public, you moron!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_17:
# 176 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Y-yes, I'm so sorry. I just -$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_18:
# 183 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "You just NOTHING! You BETRAYED us!\n"
	.string "Out! OUT! You're no longer a part of\l"
	.string "Team Somber! And I'm going to have a\l"
	.string "squad chase you all over Cormoria!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_19:
# 190 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "No boss, please! I really need this work!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_20:
# 198 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Should've thought of that before\n"
	.string "betraying us then! Now GET OUT!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_21:
# 202 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "...$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_22:
# 214 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "An absolutely pathetic character.\n"
	.string "I would deal with her myself...if not\l"
	.string "for you, {PLAYER}.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Cutscene_Text_23:
# 219 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "So you know our plan now. At Mt. Ceram,\n"
	.string "we didn't have the tools to control the\l"
	.string "dreamstone. But, with the blueprints\l"
	.string "Rue handed us right now, we're going to\l"
	.string "build a Dreamalyser and take it to Mt.\l"
	.string "Mirroh. We'll obtain the dreamstone\l"
	.string "there...and the rest?\p"
	.string "Well you'll never see any of it.$"

Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_0:
# 227 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "I'm going to put an end to you - right\n"
	.string "here, right now!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_1:
# 230 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "You stupid child!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_2:
# 239 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Unbelievable! Just unbelievable!\p"
	.string "Fine. You can have your Assistant\n"
	.string "Professor Rue back, child!\p"
	.string "We've got what we need from him anyway.\n"
	.string "Come on grunt - let's go!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_3:
# 248 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Heed this warning, {PLAYER}. Do NOT\n"
	.string "try to interfere with us at Mt. Mirroh,\l"
	.string "or else we will crush you!$"

Cormoria_SilversunCity_SomberHQ_EventScript_Skip_Text_4:
# 266 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "You saved me. You've really become a\n"
	.string "strong trainer...perhaps I shouldn't\l"
	.string "have worried about you so much.\p"
	.string "...\p"
	.string "There are detectives outside?\n"
	.string "Let's go and meet them.$"

Cormoria_SomberHQ_EventScript_NPCNurse_Text_0:
# 310 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Chansey and I were posted in the\n"
	.string "Silversun Theater but got lost!\p"
	.string "Now we're surrounded by criminals and\n"
	.string "we don't know how to get out.\p"
	.string "So we've been hiding here.$"

Cormoria_SomberHQ_NPC1_Text_0:
# 322 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "What? You thought we all wear suits and\n"
	.string "go on field trips like school children?\l"
	.string "Someone's gotta make the money!\l"
	.string "I'm a financial whiz. I'm the real\l"
	.string "backbone of Team Somber!$"

Cormoria_SomberHQ_NPC2_Text_0:
# 329 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "The other guy thinks he's hot stuff but\n"
	.string "without my tech skills, nothing in this\l"
	.string "office would function! I'm the real\l"
	.string "backbone of Team Somber!$"

Cormoria_SomberHQ_NPC3_Text_0:
# 336 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "These organised criminals act so\n"
	.string "high-and-mighty...they don't\l"
	.string "understand the struggles of us\l"
	.string "small-time smugglers!$"

Cormoria_SomberHQ_NPC4_Text_0:
# 343 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "If you ship contraband through the\n"
	.string "sewers, things get delayed sometimes.$"

Cormoria_SomberHQ_NPCGrunt_Text_0:
# 349 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "You mean the shipment's been delayed\n"
	.string "again? Why, I've got half a mind to -$"

Cormoria_SomberHQ_NPCGrunt_Text_1:
# 353 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Who're you? Did you bring the shipment?\p"
	.string "Wait a minute...you're that trainer who\n"
	.string "beat us at Mt. Ceram! Oh, you've got\l"
	.string "some nerve coming up and talking to me!\p"
	.string "...\p"
	.string "Eh, I'm on shipment duty, not guard\n"
	.string "duty. None of my business.$"

Cormoria_SomberHQ_NPC_Counterfeiter_Text_0:
# 364 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Hehehe...this machine can create\n"
	.string "counterfeits of any badge, card or ID!\l"
	.string "We could make so much money selling\l"
	.string "fake student IDs...$"

Cormoria_SomberHQ_NPC_Counterfeiter_Text_1:
# 369 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Hey, you're not a grunt! Get outta here!\n"
	.string "I'm not going to let you use this!$"

Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_0:
# 379 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "{PLAYER} turned on the Counterfeit\n"
	.string "Machine.$"

Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_1:
# 385 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "YOU HAVE A STUDENT ID. PROCESS\n"
	.string "COUNTERFEIT?$"

Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_2:
# 389 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "{PLAYER} fed the Student ID into the\n"
	.string "open card slot.\p"
	.string "...\p"
	.string "...\p"
	.string "A new card popped out!$"

Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_3:
# 392 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "This one doesn't look like a one-day\n"
	.string "pass. It looks like a real Student ID!$"

Cormoria_SomberHQ_NPC_CounterfeitMachine_Text_4:
# 400 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "YOU HAVE NO ITEMS TO COUNTERFEIT.$"

Cormoria_SomberHQ_Grunt_A_Text_0:
# 410 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Hey! Who let you in?$"

Cormoria_SomberHQ_Grunt_A_Text_1:
# 410 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "I've gotta let you in too...$"

Cormoria_SomberHQ_Grunt_A_Text_2:
# 411 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "I can't believe you found our secret\n"
	.string "hideout! Who leaked it?$"

Cormoria_SomberHQ_Grunt_B_Text_0:
# 417 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "You stepped into the wrong place buddy.\n"
	.string "Moxie isn't here - it's someone way\l"
	.string "scarier!$"

Cormoria_SomberHQ_Grunt_B_Text_1:
# 417 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "I'm gonna tell the boss!$"

Cormoria_SomberHQ_Grunt_B_Text_2:
# 418 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Moxie is pretty scary and mean, but\n"
	.string "this boss...she's on another level!$"

Cormoria_SomberHQ_Grunt_C_Text_0:
# 423 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "We're going to get the next dreamstone\n"
	.string "with a certain someone's help!$"

Cormoria_SomberHQ_Grunt_C_Text_1:
# 423 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "I'm not telling you who it is!$"

Cormoria_SomberHQ_Grunt_C_Text_2:
# 424 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Okay fine, I'll tell you who's going\n"
	.string "to help us. It's Nurse Joy.\p"
	.string "...\p"
	.string "Hah! Can't believe you fell for that.$"

Cormoria_SomberHQ_Grunt_D_Text_0:
# 429 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Bet you think there's a door to the\n"
	.string "boss's office past me, don't you? Joke's\l"
	.string "on you - there isn't!$"

Cormoria_SomberHQ_Grunt_D_Text_1:
# 429 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Go and see for yourself!$"

Cormoria_SomberHQ_Grunt_D_Text_2:
# 430 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "The only way to get to the office is\n"
	.string "to warp there - but I'll never tell\l"
	.string "you which warp to use!$"

Cormoria_SomberHQ_Grunt_E_Text_0:
# 435 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Thought you could sneak past me?$"

Cormoria_SomberHQ_Grunt_E_Text_1:
# 435 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "I wish you had sneaked past me!$"

Cormoria_SomberHQ_Grunt_E_Text_2:
# 436 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "What's the point of catching people\n"
	.string "sneaking around if I just lose?$"

Cormoria_SomberHQ_Grunt_F_Text_0:
# 441 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "You again? That stupid Gabrielle\n"
	.string "must've let you in - I know it!$"

Cormoria_SomberHQ_Grunt_F_Text_1:
# 441 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Stupid Gabrielle!$"

Cormoria_SomberHQ_Grunt_F_Text_2:
# 442 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Gabrielle's such a klutz - she's\n"
	.string "singlehandedly ruining this great\l"
	.string "organisation's image.$"

Cormoria_SomberHQ_Grunt_G_Text_0:
# 447 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "$"

Cormoria_SomberHQ_Grunt_H_Text_0:
# 453 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Gabrielle and I were next up for admin\n"
	.string "positions...until she goofed at Mt.\l"
	.string "Ceram! Now it's just me.\p"
	.string "Let me show you why!$"

Cormoria_SomberHQ_Grunt_H_Text_1:
# 453 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Guess I'm not admin level yet!$"

Cormoria_SomberHQ_Grunt_H_Text_2:
# 454 "data//maps/SilversunCity_SomberHQ/scripts.pory"
	.string "Fine, go ahead.\n"
	.string "You can't beat the boss anyway!$"
