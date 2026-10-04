Cormoria_Route6_ZeraoraCave_MapScripts::
# 2 "data//maps/Route6_ZeraoraCave/scripts.pory"
	map_script MAP_SCRIPT_ON_RESUME, Cormoria_Route6_ZeraoraCave_SetHoleWarp
	.byte 0


Cormoria_Route6_ZeraoraCave_SetHoleWarp::
# 6 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setstepcallback STEP_CB_CRACKED_FLOOR
# 7 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setholewarp MAP_CORMORIA_ROUTE6_ZERAORA_CAVE_B1F
	end


Cormoria_Route6_Zeraora::
# 12 "data//maps/Route6_ZeraoraCave/scripts.pory"
	lockall
# 14 "data//maps/Route6_ZeraoraCave/scripts.pory"
	playmoncry SPECIES_ZERAORA, CRY_MODE_NORMAL
# 15 "data//maps/Route6_ZeraoraCave/scripts.pory"
	applymovement 1, Cormoria_Route6_Zeraora_Movement_0
# 16 "data//maps/Route6_ZeraoraCave/scripts.pory"
	waitmovement 0
# 17 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 16
# 18 "data//maps/Route6_ZeraoraCave/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Route6_Zeraora_Movement_1
# 19 "data//maps/Route6_ZeraoraCave/scripts.pory"
	applymovement 1, Cormoria_Route6_Zeraora_Movement_2
# 20 "data//maps/Route6_ZeraoraCave/scripts.pory"
	waitmovement 0
# 21 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 16
# 23 "data//maps/Route6_ZeraoraCave/scripts.pory"
	playse SE_EXIT
# 24 "data//maps/Route6_ZeraoraCave/scripts.pory"
	removeobject 1
# 25 "data//maps/Route6_ZeraoraCave/scripts.pory"
	waitse
# 27 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_Text_0
# 28 "data//maps/Route6_ZeraoraCave/scripts.pory"
	release
# 31 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setflag Cormoria_FLAG_ZERAORA
# 32 "data//maps/Route6_ZeraoraCave/scripts.pory"
	special InitRoamer
# 33 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setseenmon SPECIES_ZERAORA
# 35 "data//maps/Route6_ZeraoraCave/scripts.pory"
	releaseall
	end


Cormoria_Route6_Zeraora_AncientCarving::
# 64 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_AncientCarving_Text_0
# 65 "data//maps/Route6_ZeraoraCave/scripts.pory"
	release
# 66 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 8
# 68 "data//maps/Route6_ZeraoraCave/scripts.pory"
	checkitem ITEM_ARCHAEOLENS_2
# 69 "data//maps/Route6_ZeraoraCave/scripts.pory"
	compare VAR_RESULT, TRUE
	goto_if_eq Cormoria_Route6_Zeraora_AncientCarving_2
# 102 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_AncientCarving_Text_5
# 103 "data//maps/Route6_ZeraoraCave/scripts.pory"
	playse SE_CLICK
# 104 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 8
# 105 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_AncientCarving_Text_2
# 106 "data//maps/Route6_ZeraoraCave/scripts.pory"
	release
# 107 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 8
# 108 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_AncientCarving_Text_3
# 109 "data//maps/Route6_ZeraoraCave/scripts.pory"
	releaseall
Cormoria_Route6_Zeraora_AncientCarving_1:
# 113 "data//maps/Route6_ZeraoraCave/scripts.pory"
	releaseall
	end

Cormoria_Route6_Zeraora_AncientCarving_2:
# 70 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_AncientCarving_Text_1
# 71 "data//maps/Route6_ZeraoraCave/scripts.pory"
	playse SE_CLICK
# 72 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 8
# 73 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_AncientCarving_Text_2
# 74 "data//maps/Route6_ZeraoraCave/scripts.pory"
	release
# 75 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 8
# 76 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_AncientCarving_Text_3
# 77 "data//maps/Route6_ZeraoraCave/scripts.pory"
	release
# 78 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 8
# 80 "data//maps/Route6_ZeraoraCave/scripts.pory"
	goto_if_unset Cormoria_FLAG_GREAT_TUSK, Cormoria_Route6_Zeraora_AncientCarving_5
	goto Cormoria_Route6_Zeraora_AncientCarving_1

Cormoria_Route6_Zeraora_AncientCarving_5:
# 81 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setvar VAR_0x8004, 2
# 82 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setvar VAR_0x8005, 2
# 83 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setvar VAR_0x8006, 30
# 84 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setvar VAR_0x8007, 5
# 85 "data//maps/Route6_ZeraoraCave/scripts.pory"
	special ShakeCamera
# 86 "data//maps/Route6_ZeraoraCave/scripts.pory"
	waitstate
# 87 "data//maps/Route6_ZeraoraCave/scripts.pory"
	release
# 89 "data//maps/Route6_ZeraoraCave/scripts.pory"
	msgbox Cormoria_Route6_Zeraora_AncientCarving_Text_4
# 90 "data//maps/Route6_ZeraoraCave/scripts.pory"
	delay 8
# 92 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setflag Cormoria_FLAG_GREAT_TUSK
# 94 "data//maps/Route6_ZeraoraCave/scripts.pory"
	setwildbattle SPECIES_GREAT_TUSK, 50
# 95 "data//maps/Route6_ZeraoraCave/scripts.pory"
	dowildbattle
# 97 "data//maps/Route6_ZeraoraCave/scripts.pory"
	releaseall
	end


# 15 "data//maps/Route6_ZeraoraCave/scripts.pory"
Cormoria_Route6_Zeraora_Movement_0:
# 15 "data//maps/Route6_ZeraoraCave/scripts.pory"
	emote_exclamation_mark
# 15 "data//maps/Route6_ZeraoraCave/scripts.pory"
	jump_in_place_down
	step_end

# 18 "data//maps/Route6_ZeraoraCave/scripts.pory"
Cormoria_Route6_Zeraora_Movement_1:
# 18 "data//maps/Route6_ZeraoraCave/scripts.pory"
	walk_faster_right
# 18 "data//maps/Route6_ZeraoraCave/scripts.pory"
	face_down
	step_end

# 19 "data//maps/Route6_ZeraoraCave/scripts.pory"
Cormoria_Route6_Zeraora_Movement_2:
# 19 "data//maps/Route6_ZeraoraCave/scripts.pory"
	jump_2_down
# 19 "data//maps/Route6_ZeraoraCave/scripts.pory"
	jump_2_down
# 19 "data//maps/Route6_ZeraoraCave/scripts.pory"
	jump_2_down
# 19 "data//maps/Route6_ZeraoraCave/scripts.pory"
	jump_2_down
	step_end

Cormoria_Route6_Zeraora_Text_0:
# 27 "data//maps/Route6_ZeraoraCave/scripts.pory"
	.string "The Pokémon escaped! It looked just\n"
	.string "like Director Ravine's description!\l"
	.string "It must be out there somewhere...did\l"
	.string "the Pokédex record it?$"

Cormoria_Route6_Zeraora_AncientCarving_Text_0:
# 64 "data//maps/Route6_ZeraoraCave/scripts.pory"
	.string "It's an ancient cave painting...$"

Cormoria_Route6_Zeraora_AncientCarving_Text_1:
# 70 "data//maps/Route6_ZeraoraCave/scripts.pory"
	.string "Scanning the ancient painting\n"
	.string "with the Archaeolens 2.0...$"

Cormoria_Route6_Zeraora_AncientCarving_Text_2:
# 73 "data//maps/Route6_ZeraoraCave/scripts.pory"
	.string "Done!$"

Cormoria_Route6_Zeraora_AncientCarving_Text_3:
# 76 "data//maps/Route6_ZeraoraCave/scripts.pory"
	.string "It's a never before seen Pokémon!\n"
	.string "It looks fierce, with huge tusks.\p"
	.string "Terrifying!$"

Cormoria_Route6_Zeraora_AncientCarving_Text_4:
# 89 "data//maps/Route6_ZeraoraCave/scripts.pory"
	.string "What was that?\p"
	.string "...\p"
	.string "The Archaeolens 2.0 is overheating!\n"
	.string "What's happening?$"

Cormoria_Route6_Zeraora_AncientCarving_Text_5:
# 102 "data//maps/Route6_ZeraoraCave/scripts.pory"
	.string "Scanning the ancient painting\n"
	.string "with the Archaeolens...$"
