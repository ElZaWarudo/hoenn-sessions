Cormoria_VilethornHouse_MapScripts::
	.byte 0


Cormoria_VilethornHouse_NPC1::
# 4 "data//maps/VilethornHouse/scripts.pory"
	msgbox Cormoria_VilethornHouse_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_VilethornHouse_NPC2::
# 9 "data//maps/VilethornHouse/scripts.pory"
	lockall
# 10 "data//maps/VilethornHouse/scripts.pory"
	faceplayer
# 11 "data//maps/VilethornHouse/scripts.pory"
	msgbox Cormoria_VilethornHouse_NPC2_Text_0, MSGBOX_YESNO
# 13 "data//maps/VilethornHouse/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornHouse_NPC2_2
Cormoria_VilethornHouse_NPC2_1:
# 47 "data//maps/VilethornHouse/scripts.pory"
	msgbox Cormoria_VilethornHouse_NPC2_Text_7, MSGBOX_NPC
# 48 "data//maps/VilethornHouse/scripts.pory"
	releaseall
	end

Cormoria_VilethornHouse_NPC2_2:
# 15 "data//maps/VilethornHouse/scripts.pory"
	checkitem ITEM_RED_SHARD
# 17 "data//maps/VilethornHouse/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornHouse_NPC2_4
# 41 "data//maps/VilethornHouse/scripts.pory"
	msgbox Cormoria_VilethornHouse_NPC2_Text_6, MSGBOX_NPC
# 42 "data//maps/VilethornHouse/scripts.pory"
	releaseall
	end

Cormoria_VilethornHouse_NPC2_4:
# 19 "data//maps/VilethornHouse/scripts.pory"
	message Cormoria_VilethornHouse_NPC2_Text_1
# 20 "data//maps/VilethornHouse/scripts.pory"
	waitmessage
# 21 "data//maps/VilethornHouse/scripts.pory"
	dynmultichoice 0, 0, TRUE, 5, 0, DYN_MULTICHOICE_CB_NONE, Cormoria_VilethornHouse_NPC2_Text_2, Cormoria_VilethornHouse_NPC2_Text_3, Cormoria_VilethornHouse_NPC2_Text_4, Cormoria_VilethornHouse_NPC2_Text_5
# 24 "data//maps/VilethornHouse/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_VilethornHouse_NPC2_8
Cormoria_VilethornHouse_NPC2_7:
# 28 "data//maps/VilethornHouse/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornHouse_NPC2_11
Cormoria_VilethornHouse_NPC2_10:
# 32 "data//maps/VilethornHouse/scripts.pory"
	compare VAR_RESULT, 2
	goto_if_eq Cormoria_VilethornHouse_NPC2_14
Cormoria_VilethornHouse_NPC2_13:
# 36 "data//maps/VilethornHouse/scripts.pory"
	compare VAR_RESULT, 3
	goto_if_eq Cormoria_VilethornHouse_NPC2_16
	goto Cormoria_VilethornHouse_NPC2_1

Cormoria_VilethornHouse_NPC2_8:
# 25 "data//maps/VilethornHouse/scripts.pory"
	setvar VAR_0x8005, MOVE_REFLECT
# 26 "data//maps/VilethornHouse/scripts.pory"
	goto Cormoria_VilethornHouse_Tutor_TeachMove
	goto Cormoria_VilethornHouse_NPC2_7

Cormoria_VilethornHouse_NPC2_11:
# 29 "data//maps/VilethornHouse/scripts.pory"
	setvar VAR_0x8005, MOVE_LIGHT_SCREEN
# 30 "data//maps/VilethornHouse/scripts.pory"
	goto Cormoria_VilethornHouse_Tutor_TeachMove
	goto Cormoria_VilethornHouse_NPC2_10

Cormoria_VilethornHouse_NPC2_14:
# 33 "data//maps/VilethornHouse/scripts.pory"
	setvar VAR_0x8005, MOVE_AURORA_VEIL
# 34 "data//maps/VilethornHouse/scripts.pory"
	goto Cormoria_VilethornHouse_Tutor_TeachMove
	goto Cormoria_VilethornHouse_NPC2_13

Cormoria_VilethornHouse_NPC2_16:
# 37 "data//maps/VilethornHouse/scripts.pory"
	setvar VAR_0x8005, MOVE_PROTECT
# 38 "data//maps/VilethornHouse/scripts.pory"
	goto Cormoria_VilethornHouse_Tutor_TeachMove
	goto Cormoria_VilethornHouse_NPC2_1


Cormoria_VilethornHouse_Tutor_TeachMove::
# 53 "data//maps/VilethornHouse/scripts.pory"
	call Cormoria_MoveTutor_EventScript_OpenPartyMenu
# 55 "data//maps/VilethornHouse/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_VilethornHouse_Tutor_TeachMove_2
Cormoria_VilethornHouse_Tutor_TeachMove_1:
# 59 "data//maps/VilethornHouse/scripts.pory"
	msgbox Cormoria_VilethornHouse_NPC2_Text_7, MSGBOX_NPC
# 60 "data//maps/VilethornHouse/scripts.pory"
	releaseall
	end

Cormoria_VilethornHouse_Tutor_TeachMove_2:
# 56 "data//maps/VilethornHouse/scripts.pory"
	removeitem ITEM_GREEN_SHARD
	goto Cormoria_VilethornHouse_Tutor_TeachMove_1


Cormoria_VilethornHouse_NPC1_Text_0:
# 4 "data//maps/VilethornHouse/scripts.pory"
	.string "We moved out of the city long ago.\n"
	.string "It was too dangerous!$"

Cormoria_VilethornHouse_NPC2_Text_0:
# 11 "data//maps/VilethornHouse/scripts.pory"
	.string "Nobody disturbs us here in the woods.\n"
	.string "The best protection is isolation!\l"
	.string "Your Pokémon also need to learn to\l"
	.string "defend themselves.\p"
	.string "In exchange for one Red Shard, should I\n"
	.string "teach your Pokémon a defensive move?$"

Cormoria_VilethornHouse_NPC2_Text_1:
# 19 "data//maps/VilethornHouse/scripts.pory"
	.string "Which move should I teach?$"

Cormoria_VilethornHouse_NPC2_Text_2:
# 22 "data//maps/VilethornHouse/scripts.pory"
	.string "Reflect$"

Cormoria_VilethornHouse_NPC2_Text_3:
# 22 "data//maps/VilethornHouse/scripts.pory"
	.string "Light Screen$"

Cormoria_VilethornHouse_NPC2_Text_4:
# 22 "data//maps/VilethornHouse/scripts.pory"
	.string "Aurora Veil$"

Cormoria_VilethornHouse_NPC2_Text_5:
# 22 "data//maps/VilethornHouse/scripts.pory"
	.string "Protect$"

Cormoria_VilethornHouse_NPC2_Text_6:
# 41 "data//maps/VilethornHouse/scripts.pory"
	.string "You don't have a Red Shard.\n"
	.string "Come back when you do.$"

Cormoria_VilethornHouse_NPC2_Text_7:
# 47 "data//maps/VilethornHouse/scripts.pory"
	.string "Come again whenever you like!$"
