# 1 "data//maps/MirrohBaseCamp/scripts.pory"

# 2 "data//maps/MirrohBaseCamp/scripts.pory"
    .set Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_MARTHA, 5
# 3 "data//maps/MirrohBaseCamp/scripts.pory"
    .set Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_RUE, 7

Cormoria_MirrohBaseCamp_MapScripts::
# 7 "data//maps/MirrohBaseCamp/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_MirrohBaseCamp_MapScripts_MAP_SCRIPT_ON_TRANSITION
	.byte 0

Cormoria_MirrohBaseCamp_MapScripts_MAP_SCRIPT_ON_TRANSITION:
# 8 "data//maps/MirrohBaseCamp/scripts.pory"
	setflag Cormoria_FLAG_VISITED_MIRROH_BASE_CAMP
	return


Cormoria_MirrohBaseCamp_EventScript_Vendor1::
# 16 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 17 "data//maps/MirrohBaseCamp/scripts.pory"
	faceplayer
# 19 "data//maps/MirrohBaseCamp/scripts.pory"
	goto_if_set Cormoria_FLAG_SYS_GAME_CLEAR, Cormoria_MirrohBaseCamp_EventScript_Vendor1_1
# 27 "data//maps/MirrohBaseCamp/scripts.pory"
	message Cormoria_MirrohBaseCamp_EventScript_Vendor1_Text_2
# 28 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmessage
# 29 "data//maps/MirrohBaseCamp/scripts.pory"
	pokemart Cormoria_MirrohBaseCamp_Vendor1_Inventory
# 30 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_EventScript_Vendor1_Text_1, MSGBOX_DEFAULT
# 31 "data//maps/MirrohBaseCamp/scripts.pory"
	release
	end

Cormoria_MirrohBaseCamp_EventScript_Vendor1_1:
# 20 "data//maps/MirrohBaseCamp/scripts.pory"
	message Cormoria_MirrohBaseCamp_EventScript_Vendor1_Text_0
# 21 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmessage
# 22 "data//maps/MirrohBaseCamp/scripts.pory"
	pokemart Cormoria_MirrohBaseCamp_Vendor1_Inventory_PostGame
# 23 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_EventScript_Vendor1_Text_1, MSGBOX_DEFAULT
# 24 "data//maps/MirrohBaseCamp/scripts.pory"
	release
	end


	.align 2
# 36 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Vendor1_Inventory:
# 37 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_GREAT_BALL
# 38 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_ICE_HEAL
# 39 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_SUPER_REPEL
# 40 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_ESCAPE_ROPE
	.2byte ITEM_NONE

	.align 2
# 43 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Vendor1_Inventory_PostGame:
# 44 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_MEDICHAMITE
# 45 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_AGGRONITE
# 46 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_ALTARIANITE
# 47 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_LOPUNNITE
# 48 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_BANETTITE
# 49 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_AMPHAROSITE
# 50 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_GYARADOSITE
# 51 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_MAWILITE
# 52 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_GARDEVOIRITE
# 53 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_GALLADITE
# 54 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_HOUNDOOMINITE
# 55 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_METAGROSSITE
# 56 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_SCIZORITE
# 57 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_KANGASKHANITE
# 58 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_PINSIRITE
# 59 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_SHARPEDONITE
# 60 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_MANECTITE
# 61 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_ABSOLITE
# 62 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_SABLENITE
	.2byte ITEM_NONE

Cormoria_MirrohBaseCamp_EventScript_Vendor2::
# 66 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 67 "data//maps/MirrohBaseCamp/scripts.pory"
	faceplayer
# 68 "data//maps/MirrohBaseCamp/scripts.pory"
	message Cormoria_MirrohBaseCamp_EventScript_Vendor2_Text_0
# 69 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmessage
# 70 "data//maps/MirrohBaseCamp/scripts.pory"
	pokemart Cormoria_MirrohBaseCamp_Vendor2_Inventory
# 71 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_EventScript_Vendor2_Text_1, MSGBOX_DEFAULT
# 72 "data//maps/MirrohBaseCamp/scripts.pory"
	release
	end


	.align 2
# 76 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Vendor2_Inventory:
# 77 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_NEVER_MELT_ICE
# 78 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_ICE_GEM
# 79 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_COVERT_CLOAK
# 80 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_ENERGY_ROOT
# 81 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_COIN_CASE
	.2byte ITEM_NONE

Cormoria_MirrohBaseCamp_EventScript_Vendor3::
# 85 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 86 "data//maps/MirrohBaseCamp/scripts.pory"
	faceplayer
# 87 "data//maps/MirrohBaseCamp/scripts.pory"
	message Cormoria_MirrohBaseCamp_EventScript_Vendor3_Text_0
# 88 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmessage
# 89 "data//maps/MirrohBaseCamp/scripts.pory"
	pokemart Cormoria_MirrohBaseCamp_Vendor3_Inventory
# 90 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_EventScript_Vendor3_Text_1, MSGBOX_DEFAULT
# 91 "data//maps/MirrohBaseCamp/scripts.pory"
	release
	end


	.align 2
# 95 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Vendor3_Inventory:
# 96 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_CASTELIACONE
# 97 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_SODA_POP
# 98 "data//maps/MirrohBaseCamp/scripts.pory"
	.2byte ITEM_LEMONADE
	.2byte ITEM_NONE

Cormoria_MirrohBaseCamp_EventScript_NPC1::
# 105 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_EventScript_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_MirrohBaseCamp_EventScript_NPC2::
# 109 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 110 "data//maps/MirrohBaseCamp/scripts.pory"
	faceplayer
# 111 "data//maps/MirrohBaseCamp/scripts.pory"
	speakername Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_0
# 112 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_1
# 113 "data//maps/MirrohBaseCamp/scripts.pory"
	speakername Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_2
# 114 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_3
# 115 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 116 "data//maps/MirrohBaseCamp/scripts.pory"
	hidefollower
# 117 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohBaseCamp_EventScript_NPC2_Movement_0
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_MARTHA, Cormoria_MirrohBaseCamp_EventScript_NPC2_Movement_1
# 119 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 120 "data//maps/MirrohBaseCamp/scripts.pory"
	removeobject Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_MARTHA
# 121 "data//maps/MirrohBaseCamp/scripts.pory"
	speakername Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_2
# 122 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_4
# 123 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 124 "data//maps/MirrohBaseCamp/scripts.pory"
	setflag Cormoria_FLAG_HIDE_GRAVEYARD_HISTORIAN
# 125 "data//maps/MirrohBaseCamp/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_MIRROHBASECAMP_HISTORIAN
# 126 "data//maps/MirrohBaseCamp/scripts.pory"
	releaseall
	end


Cormoria_MirrohBaseCamp_Greavard_Outside::
# 130 "data//maps/MirrohBaseCamp/scripts.pory"
	playmoncry SPECIES_GREAVARD, CRY_MODE_NORMAL
# 132 "data//maps/MirrohBaseCamp/scripts.pory"
	checkitem ITEM_RARE_BONE
# 134 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_MirrohBaseCamp_Greavard_Outside_1
# 181 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Greavard_Outside_Text_7, MSGBOX_NPC
	end

Cormoria_MirrohBaseCamp_Greavard_Outside_1:
# 135 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 136 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Greavard_Outside_Text_0, MSGBOX_YESNO
# 137 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_MirrohBaseCamp_Greavard_Outside_5
# 178 "data//maps/MirrohBaseCamp/scripts.pory"
	releaseall
	end

Cormoria_MirrohBaseCamp_Greavard_Outside_5:
# 138 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Greavard_Outside_Text_1
# 139 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 140 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement 11, Cormoria_MirrohBaseCamp_Greavard_Outside_Movement_0
# 141 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 142 "data//maps/MirrohBaseCamp/scripts.pory"
	removeobject 11
# 143 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Greavard_Outside_Text_2
# 144 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement 12, Cormoria_MirrohBaseCamp_Greavard_Outside_Movement_1
# 146 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 147 "data//maps/MirrohBaseCamp/scripts.pory"
	removeobject 12
# 148 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Greavard_Outside_Text_3
# 149 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 151 "data//maps/MirrohBaseCamp/scripts.pory"
	setflag Cormoria_FLAG_MIRROH_GREAVARD_BLOCKER
# 153 "data//maps/MirrohBaseCamp/scripts.pory"
	addobject Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_MARTHA
# 154 "data//maps/MirrohBaseCamp/scripts.pory"
	setobjectxy Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_MARTHA, 5, 9
# 156 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohBaseCamp_Greavard_Outside_Movement_2
# 157 "data//maps/MirrohBaseCamp/scripts.pory"
	speakername Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_2
# 158 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Greavard_Outside_Text_4
# 159 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 160 "data//maps/MirrohBaseCamp/scripts.pory"
	hidefollower
# 162 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_MARTHA, Cormoria_MirrohBaseCamp_Greavard_Outside_Movement_3
# 163 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 166 "data//maps/MirrohBaseCamp/scripts.pory"
	speakername Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_2
# 167 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Greavard_Outside_Text_5
# 168 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 169 "data//maps/MirrohBaseCamp/scripts.pory"
	delay 8
# 170 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Greavard_Outside_Text_6, MSGBOX_POKENAV
# 171 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 172 "data//maps/MirrohBaseCamp/scripts.pory"
	delay 8
# 173 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone
# 175 "data//maps/MirrohBaseCamp/scripts.pory"
	releaseall
	end


Cormoria_MirrohBaseCamp_Martha_OfferMegaStone::
# 187 "data//maps/MirrohBaseCamp/scripts.pory"
	message Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_0
# 188 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmessage
# 189 "data//maps/MirrohBaseCamp/scripts.pory"
	dynmultichoice 0, 0, TRUE, 5, 0, DYN_MULTICHOICE_CB_NONE, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_1, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_2, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_3, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_4, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_5, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_6, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_7, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_8, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_9, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_10, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_11, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_12, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_13, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_14, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_15, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_16, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_17, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_18, Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_19
# 196 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_2
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_1:
# 197 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_5
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_4:
# 198 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 2
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_8
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_7:
# 199 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 3
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_11
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_10:
# 200 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 4
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_14
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_13:
# 201 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 5
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_17
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_16:
# 202 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 6
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_20
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_19:
# 203 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 7
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_23
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_22:
# 204 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 8
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_26
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_25:
# 205 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 9
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_29
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_28:
# 206 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 10
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_32
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_31:
# 207 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 11
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_35
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_34:
# 208 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 12
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_38
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_37:
# 209 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 13
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_41
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_40:
# 210 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 14
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_44
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_43:
# 211 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 15
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_47
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_46:
# 212 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 16
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_50
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_49:
# 213 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 17
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_53
Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_52:
# 214 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 18
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_55
	return

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_2:
# 196 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_MEDICHAMITE
# 196 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_1

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_5:
# 197 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_AGGRONITE
# 197 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_4

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_8:
# 198 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_ALTARIANITE
# 198 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_7

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_11:
# 199 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_LOPUNNITE
# 199 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_10

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_14:
# 200 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_BANETTITE
# 200 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_13

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_17:
# 201 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_AMPHAROSITE
# 201 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_16

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_20:
# 202 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_GYARADOSITE
# 202 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_19

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_23:
# 203 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_MAWILITE
# 203 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_22

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_26:
# 204 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_GARDEVOIRITE
# 204 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_25

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_29:
# 205 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_GALLADITE
# 205 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_28

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_32:
# 206 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_HOUNDOOMINITE
# 206 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_31

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_35:
# 207 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_METAGROSSITE
# 207 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_34

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_38:
# 208 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_SCIZORITE
# 208 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_37

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_41:
# 209 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_KANGASKHANITE
# 209 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_40

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_44:
# 210 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_PINSIRITE
# 210 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_43

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_47:
# 211 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_SHARPEDONITE
# 211 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_46

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_50:
# 212 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_MANECTITE
# 212 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_49

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_53:
# 213 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_ABSOLITE
# 213 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_52

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_55:
# 214 "data//maps/MirrohBaseCamp/scripts.pory"
	setvar VAR_0x8006, ITEM_SABLENITE
# 214 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_GiveMegaStone
	return


Cormoria_MirrohBaseCamp_Martha_GiveMegaStone::
# 218 "data//maps/MirrohBaseCamp/scripts.pory"
	showitemdescription
# 219 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Martha_GiveMegaStone_Text_0, MSGBOX_YESNO
# 220 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_MirrohBaseCamp_Martha_GiveMegaStone_1
# 225 "data//maps/MirrohBaseCamp/scripts.pory"
	hideitemdescription
# 226 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_OfferMegaStone
	return

Cormoria_MirrohBaseCamp_Martha_GiveMegaStone_1:
# 221 "data//maps/MirrohBaseCamp/scripts.pory"
	hideitemdescription
# 222 "data//maps/MirrohBaseCamp/scripts.pory"
	giveitem VAR_0x8006
# 223 "data//maps/MirrohBaseCamp/scripts.pory"
	goto Cormoria_MirrohBaseCamp_Martha_End
	return


Cormoria_MirrohBaseCamp_Martha_End::
# 232 "data//maps/MirrohBaseCamp/scripts.pory"
	speakername Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_2
# 233 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Martha_End_Text_0
# 234 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 236 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_MARTHA, Cormoria_MirrohBaseCamp_Martha_End_Movement_0
# 237 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 238 "data//maps/MirrohBaseCamp/scripts.pory"
	removeobject Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_MARTHA
# 240 "data//maps/MirrohBaseCamp/scripts.pory"
	releaseall
	end


Cormoria_MirrohBaseCamp_Rue_Trigger::
# 251 "data//maps/MirrohBaseCamp/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROHBASECAMP_MASTERBALL_RUE, Cormoria_MirrohBaseCamp_Rue_Trigger_2
# 252 "data//maps/MirrohBaseCamp/scripts.pory"
	goto_if_set Cormoria_FLAG_BADGE06_GET, Cormoria_MirrohBaseCamp_Rue_Trigger_5
# 254 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 255 "data//maps/MirrohBaseCamp/scripts.pory"
	opendoor 13, 27
# 256 "data//maps/MirrohBaseCamp/scripts.pory"
	waitdooranim
# 258 "data//maps/MirrohBaseCamp/scripts.pory"
	addobject Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_RUE
# 259 "data//maps/MirrohBaseCamp/scripts.pory"
	setobjectxy Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_RUE, 13, 27
# 261 "data//maps/MirrohBaseCamp/scripts.pory"
	delay 8
# 263 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohBaseCamp_Rue_Trigger_Movement_0
# 264 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 265 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_RUE, Cormoria_MirrohBaseCamp_Rue_Trigger_Movement_1
# 266 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 267 "data//maps/MirrohBaseCamp/scripts.pory"
	closedoor 13, 27
# 268 "data//maps/MirrohBaseCamp/scripts.pory"
	waitdooranim
# 269 "data//maps/MirrohBaseCamp/scripts.pory"
	delay 16
# 270 "data//maps/MirrohBaseCamp/scripts.pory"
	speakername Cormoria_MirrohBaseCamp_Rue_Trigger_Text_0
# 271 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Rue_Trigger_Text_1
# 272 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 273 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_RUE, Cormoria_MirrohBaseCamp_Rue_Trigger_Movement_2
# 274 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 275 "data//maps/MirrohBaseCamp/scripts.pory"
	removeobject Cormoria_maps_MirrohBaseCamp_scripts_LOCALID_RUE
# 276 "data//maps/MirrohBaseCamp/scripts.pory"
	setflag Cormoria_FLAG_MIRROHBASECAMP_MASTERBALL_RUE
# 277 "data//maps/MirrohBaseCamp/scripts.pory"
	releaseall
	end

Cormoria_MirrohBaseCamp_Rue_Trigger_2:
	end

Cormoria_MirrohBaseCamp_Rue_Trigger_5:
	end


Cormoria_MirrohBaseCamp_Trainer_A::
# 304 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 305 "data//maps/MirrohBaseCamp/scripts.pory"
	faceplayer
# 307 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Trainer_A_Text_0, MSGBOX_YESNO
# 308 "data//maps/MirrohBaseCamp/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_MirrohBaseCamp_Trainer_A_2
Cormoria_MirrohBaseCamp_Trainer_A_1:
# 319 "data//maps/MirrohBaseCamp/scripts.pory"
	releaseall
	end

Cormoria_MirrohBaseCamp_Trainer_A_2:
# 309 "data//maps/MirrohBaseCamp/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_MIRROHEXT_D, Cormoria_MirrohBaseCamp_Trainer_A_Text_1
# 310 "data//maps/MirrohBaseCamp/scripts.pory"
	setflag Cormoria_FLAG_MIRROHBASECAMP_TRAINER_A
# 311 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Trainer_A_Text_2
# 312 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 313 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohBaseCamp_Trainer_A_Movement_0
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement 8, Cormoria_MirrohBaseCamp_Trainer_A_Movement_1
# 315 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 316 "data//maps/MirrohBaseCamp/scripts.pory"
	removeobject 8
	goto Cormoria_MirrohBaseCamp_Trainer_A_1


Cormoria_MirrohBaseCamp_Trainer_B::
# 325 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 326 "data//maps/MirrohBaseCamp/scripts.pory"
	faceplayer
# 328 "data//maps/MirrohBaseCamp/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROHBASECAMP_TRAINER_A, Cormoria_MirrohBaseCamp_Trainer_B_2
# 339 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Trainer_B_Text_3, MSGBOX_NPC
Cormoria_MirrohBaseCamp_Trainer_B_1:
# 342 "data//maps/MirrohBaseCamp/scripts.pory"
	releaseall
	end

Cormoria_MirrohBaseCamp_Trainer_B_2:
# 329 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Trainer_B_Text_0
# 330 "data//maps/MirrohBaseCamp/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_MIRROHEXT_E, Cormoria_MirrohBaseCamp_Trainer_B_Text_1
# 331 "data//maps/MirrohBaseCamp/scripts.pory"
	setflag Cormoria_FLAG_MIRROHBASECAMP_TRAINER_B
# 332 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Trainer_B_Text_2
# 333 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 334 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohBaseCamp_Trainer_A_Movement_0
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement 9, Cormoria_MirrohBaseCamp_Trainer_B_Movement_0
# 336 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 337 "data//maps/MirrohBaseCamp/scripts.pory"
	removeobject 9
	goto Cormoria_MirrohBaseCamp_Trainer_B_1


Cormoria_MirrohBaseCamp_Trainer_C::
# 349 "data//maps/MirrohBaseCamp/scripts.pory"
	lockall
# 350 "data//maps/MirrohBaseCamp/scripts.pory"
	faceplayer
# 352 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Trainer_C_Text_0, MSGBOX_NPC
# 354 "data//maps/MirrohBaseCamp/scripts.pory"
	goto_if_set Cormoria_FLAG_MIRROHBASECAMP_TRAINER_B, Cormoria_MirrohBaseCamp_Trainer_C_2
Cormoria_MirrohBaseCamp_Trainer_C_1:
# 366 "data//maps/MirrohBaseCamp/scripts.pory"
	releaseall
	end

Cormoria_MirrohBaseCamp_Trainer_C_2:
# 355 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Trainer_C_Text_1
# 356 "data//maps/MirrohBaseCamp/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_MIRROHEXT_F, Cormoria_MirrohBaseCamp_Trainer_C_Text_2
# 357 "data//maps/MirrohBaseCamp/scripts.pory"
	setflag Cormoria_FLAG_MIRROHBASECAMP_TRAINER_C
# 358 "data//maps/MirrohBaseCamp/scripts.pory"
	msgbox Cormoria_MirrohBaseCamp_Trainer_C_Text_3
# 359 "data//maps/MirrohBaseCamp/scripts.pory"
	release
# 360 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_MirrohBaseCamp_Trainer_A_Movement_0
# 361 "data//maps/MirrohBaseCamp/scripts.pory"
	applymovement 10, Cormoria_MirrohBaseCamp_Trainer_B_Movement_0
# 362 "data//maps/MirrohBaseCamp/scripts.pory"
	waitmovement 0
# 363 "data//maps/MirrohBaseCamp/scripts.pory"
	removeobject 10
	goto Cormoria_MirrohBaseCamp_Trainer_C_1


# 117 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_EventScript_NPC2_Movement_0:
# 117 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
# 117 "data//maps/MirrohBaseCamp/scripts.pory"
	face_down
	step_end

# 118 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_EventScript_NPC2_Movement_1:
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 118 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
	step_end

# 140 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Greavard_Outside_Movement_0:
# 140 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 140 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 140 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 140 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 140 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 140 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 140 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
	step_end

# 145 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Greavard_Outside_Movement_1:
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	emote_exclamation_mark
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
# 145 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_fast_up
	step_end

# 156 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Greavard_Outside_Movement_2:
# 156 "data//maps/MirrohBaseCamp/scripts.pory"
	face_down
	step_end

# 162 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Greavard_Outside_Movement_3:
# 162 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_up
# 162 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_up
# 162 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_up
# 162 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_up
# 162 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_up
	step_end

# 236 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Martha_End_Movement_0:
# 236 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 236 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 236 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 236 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 236 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 236 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 236 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
	step_end

# 263 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Rue_Trigger_Movement_0:
# 263 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 263 "data//maps/MirrohBaseCamp/scripts.pory"
	face_right
	step_end

# 265 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Rue_Trigger_Movement_1:
# 265 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 265 "data//maps/MirrohBaseCamp/scripts.pory"
	face_left
# 265 "data//maps/MirrohBaseCamp/scripts.pory"
	emote_exclamation_mark
	step_end

# 273 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Rue_Trigger_Movement_2:
# 273 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
# 273 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
# 273 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
# 273 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
# 273 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
# 273 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
# 273 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_right
	step_end

# 313 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Trainer_A_Movement_0:
# 313 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_down
# 313 "data//maps/MirrohBaseCamp/scripts.pory"
	face_up
	step_end

# 314 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Trainer_A_Movement_1:
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_up
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 314 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
	step_end

# 335 "data//maps/MirrohBaseCamp/scripts.pory"
Cormoria_MirrohBaseCamp_Trainer_B_Movement_0:
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
# 335 "data//maps/MirrohBaseCamp/scripts.pory"
	walk_left
	step_end

Cormoria_MirrohBaseCamp_EventScript_Vendor1_Text_0:
# 20 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "The graveyard manager donated her\n"
	.string "collection of stones to me!\p"
	.string "Have a look!$"

Cormoria_MirrohBaseCamp_EventScript_Vendor1_Text_1:
# 23 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Come again!$"

Cormoria_MirrohBaseCamp_EventScript_Vendor1_Text_2:
# 27 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Our supplies are limited but useful\n"
	.string "for a trek up Mt. Mirroh!$"

Cormoria_MirrohBaseCamp_EventScript_Vendor2_Text_0:
# 68 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "I found all this inside Mt. Mirroh\n"
	.string "just yesterday!$"

Cormoria_MirrohBaseCamp_EventScript_Vendor2_Text_1:
# 71 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "C'mon, have another look!$"

Cormoria_MirrohBaseCamp_EventScript_Vendor3_Text_0:
# 87 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Everything you see is homemade.\n"
	.string "Anything you like?$"

Cormoria_MirrohBaseCamp_EventScript_Vendor3_Text_1:
# 90 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "You didn't like anything?$"

Cormoria_MirrohBaseCamp_EventScript_NPC1_Text_0:
# 105 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "It's so sad and lonely up here...I want\n"
	.string "to grow up fast so I can go see the rest\l"
	.string "of Cormoria!$"

Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_0:
# 111 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "???$"

Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_1:
# 112 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "The Pokémon Graveyard lies ahead.\n"
	.string "Please be respectful.\p"
	.string "...$"

Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_2:
# 113 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Martha$"

Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_3:
# 114 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "My name? It's Martha, dear.\n"
	.string "I manage the graveyard.\p"
	.string "You want to talk to me?\n"
	.string "Something about dreamstones?\p"
	.string "This will be a long conversation, so why\n"
	.string "don't you drop by my house later?$"

Cormoria_MirrohBaseCamp_EventScript_NPC2_Text_4:
# 122 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Come now, boys! Playtime's over!\p"
	.string "...\p"
	.string "Sigh...they just don't listen to me...$"

Cormoria_MirrohBaseCamp_Greavard_Outside_Text_0:
# 136 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Greavard is playing!\n"
	.string "Give it the Old Bone?$"

Cormoria_MirrohBaseCamp_Greavard_Outside_Text_1:
# 138 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Greavard snaps up the Old Bone and\n"
	.string "starts playing with it!$"

Cormoria_MirrohBaseCamp_Greavard_Outside_Text_2:
# 143 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "It ran off with the Old Bone!$"

Cormoria_MirrohBaseCamp_Greavard_Outside_Text_3:
# 148 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "The other one chased after it!\p"
	.string "...\p"
	.string "Will they be okay?$"

Cormoria_MirrohBaseCamp_Greavard_Outside_Text_4:
# 158 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "{PLAYER}! Wait!$"

Cormoria_MirrohBaseCamp_Greavard_Outside_Text_5:
# 167 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Oof...too much running...\p"
	.string "I don't know if it'll help you against\n"
	.string "Team Somber, but I wanted to offer you\l"
	.string "one of these.\p"
	.string "They're extremely rare - prized items in\n"
	.string "my collection!\p"
	.string "But they'll do more good in the world in\n"
	.string "your hands than on my table.$"

Cormoria_MirrohBaseCamp_Greavard_Outside_Text_6:
# 170 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Choose carefully - you'll only\n"
	.string "get once chance this game!$"

Cormoria_MirrohBaseCamp_Greavard_Outside_Text_7:
# 181 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Greavard is playing!$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_0:
# 187 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Which one would you like?$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_1:
# 190 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Medichamite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_2:
# 190 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Aggronite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_3:
# 190 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Altarianite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_4:
# 190 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Lopunnite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_5:
# 190 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Banettite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_6:
# 191 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Ampharosite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_7:
# 191 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Gyaradosite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_8:
# 191 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Mawilite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_9:
# 191 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Gardevoirite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_10:
# 191 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Galladite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_11:
# 192 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Houndoominite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_12:
# 192 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Metagrossite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_13:
# 192 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Scizorite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_14:
# 192 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Kangaskhanite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_15:
# 192 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Pinsirite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_16:
# 193 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Sharpedonite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_17:
# 193 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Manectricite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_18:
# 193 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Absolite$"

Cormoria_MirrohBaseCamp_Martha_OfferMegaStone_Text_19:
# 193 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Sablenite$"

Cormoria_MirrohBaseCamp_Martha_GiveMegaStone_Text_0:
# 219 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Are you sure?$"

Cormoria_MirrohBaseCamp_Martha_End_Text_0:
# 233 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Well then. That's my job done.\p"
	.string "I wish you all the best, {PLAYER}!$"

Cormoria_MirrohBaseCamp_Rue_Trigger_Text_0:
# 270 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Rue$"

Cormoria_MirrohBaseCamp_Rue_Trigger_Text_1:
# 271 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "{PLAYER}? What are you doing all the\n"
	.string "way over here?\p"
	.string "...\p"
	.string "Ah, yes. The detectives are correct.\n"
	.string "Martha is an expert on Cormorian\l"
	.string "history. She would be helpful.\p"
	.string "I came here to make some social calls.\n"
	.string "Since Mirroh Base Camp is quiet and\l"
	.string "secluded, many academics reside here -\l"
	.string "Martha being one of them.\p"
	.string "Take care, {PLAYER}. Stay safe in Mt.\n"
	.string "Mirroh. Listen to what the detectives\l"
	.string "tell you to do, okay?$"

Cormoria_MirrohBaseCamp_Trainer_A_Text_0:
# 307 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Haven't you heard?\p"
	.string "They're closing the gym here, so\n"
	.string "everyone's queueing up to get a badge.\p"
	.string "Wanna do a warm-up battle?$"

Cormoria_MirrohBaseCamp_Trainer_A_Text_1:
# 309 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "You were warmed up already!$"

Cormoria_MirrohBaseCamp_Trainer_A_Text_2:
# 311 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Oh no! I'll have to heal my Pokémon. Hold\n"
	.string "my place in line - I'll be right back!$"

Cormoria_MirrohBaseCamp_Trainer_B_Text_0:
# 329 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "I saw the battle with the other trainer.\n"
	.string "You're not bad, but you'll have to try\l"
	.string "harder than that to beat me!$"

Cormoria_MirrohBaseCamp_Trainer_B_Text_1:
# 330 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Stop trying so hard!$"

Cormoria_MirrohBaseCamp_Trainer_B_Text_2:
# 332 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "If I can't beat a kid like you, what\n"
	.string "chance do I have against Carona?$"

Cormoria_MirrohBaseCamp_Trainer_B_Text_3:
# 339 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "What a dreadfully sad little town!\p"
	.string "It's a wonder this gym lasted so long.$"

Cormoria_MirrohBaseCamp_Trainer_C_Text_0:
# 352 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Get outta here! Carona has only one\n"
	.string "badge left to give and it's mine!$"

Cormoria_MirrohBaseCamp_Trainer_C_Text_1:
# 355 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "What? If you keep looking at me like\n"
	.string "that, I'll make you pay!$"

Cormoria_MirrohBaseCamp_Trainer_C_Text_2:
# 356 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "You made me pay!$"

Cormoria_MirrohBaseCamp_Trainer_C_Text_3:
# 358 "data//maps/MirrohBaseCamp/scripts.pory"
	.string "Grr - fine! You can go first then.\n"
	.string "I bet Carona wipes the floor with you!$"
