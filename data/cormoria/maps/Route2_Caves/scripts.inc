# 1 "data//maps/Route2_Caves/scripts.pory"

# 2 "data//maps/Route2_Caves/scripts.pory"
    .set Cormoria_maps_Route2_Caves_scripts_LOCALID_CLEFAIRY, 3

Cormoria_Route2_Caves_MapScripts::
	.byte 0


Cormoria_Route2_Caves_EventScript_ClefairyTrigger::
# 8 "data//maps/Route2_Caves/scripts.pory"
	goto_if_set Cormoria_FLAG_FENNILAHL_HIKER_GEAR_GET, Cormoria_Route2_Caves_EventScript_ClefairyTrigger_2
# 11 "data//maps/Route2_Caves/scripts.pory"
	lockall
# 12 "data//maps/Route2_Caves/scripts.pory"
	playmoncry SPECIES_CLEFAIRY, CRY_MODE_NORMAL
# 13 "data//maps/Route2_Caves/scripts.pory"
	applymovement Cormoria_maps_Route2_Caves_scripts_LOCALID_CLEFAIRY, Cormoria_Route2_Caves_EventScript_ClefairyTrigger_Movement_0
# 14 "data//maps/Route2_Caves/scripts.pory"
	waitmovement Cormoria_maps_Route2_Caves_scripts_LOCALID_CLEFAIRY
# 15 "data//maps/Route2_Caves/scripts.pory"
	releaseall
	return

Cormoria_Route2_Caves_EventScript_ClefairyTrigger_2:
	end


Cormoria_Route2_Caves_EventScript_ClefairyBattle::
# 19 "data//maps/Route2_Caves/scripts.pory"
	lockall
# 20 "data//maps/Route2_Caves/scripts.pory"
	setflag Cormoria_FLAG_BATTLE_NO_RUNNING_CATCHING
# 21 "data//maps/Route2_Caves/scripts.pory"
	playmoncry SPECIES_CLEFAIRY, CRY_MODE_NORMAL
# 22 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_Caves_EventScript_ClefairyBattle_Text_0
# 23 "data//maps/Route2_Caves/scripts.pory"
	release
# 25 "data//maps/Route2_Caves/scripts.pory"
	setwildbattle SPECIES_CLEFAIRY, 7
# 26 "data//maps/Route2_Caves/scripts.pory"
	dowildbattle
# 27 "data//maps/Route2_Caves/scripts.pory"
	release
# 29 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_Caves_EventScript_ClefairyBattle_Text_1
# 30 "data//maps/Route2_Caves/scripts.pory"
	release
# 31 "data//maps/Route2_Caves/scripts.pory"
	applymovement Cormoria_maps_Route2_Caves_scripts_LOCALID_CLEFAIRY, Cormoria_Route2_Caves_EventScript_ClefairyBattle_Movement_0
# 32 "data//maps/Route2_Caves/scripts.pory"
	waitmovement 0
# 33 "data//maps/Route2_Caves/scripts.pory"
	removeobject Cormoria_maps_Route2_Caves_scripts_LOCALID_CLEFAIRY
# 34 "data//maps/Route2_Caves/scripts.pory"
	giveitem ITEM_STRANGE_ROCK
# 35 "data//maps/Route2_Caves/scripts.pory"
	release
# 37 "data//maps/Route2_Caves/scripts.pory"
	setflag Cormoria_FLAG_FENNILAHL_HIKER_GEAR_GET
# 38 "data//maps/Route2_Caves/scripts.pory"
	setvar Cormoria_VAR_FENNILAHL_CLEFAIRY_QUEST, 3
# 39 "data//maps/Route2_Caves/scripts.pory"
	clearflag Cormoria_FLAG_BATTLE_NO_RUNNING_CATCHING
# 41 "data//maps/Route2_Caves/scripts.pory"
	releaseall
	return


Cormoria_Route2_AncientCarving::
# 52 "data//maps/Route2_Caves/scripts.pory"
	lockall
# 55 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_AncientCarving_Text_0
# 56 "data//maps/Route2_Caves/scripts.pory"
	release
# 57 "data//maps/Route2_Caves/scripts.pory"
	delay 8
# 59 "data//maps/Route2_Caves/scripts.pory"
	checkitem ITEM_ARCHAEOLENS_2
# 60 "data//maps/Route2_Caves/scripts.pory"
	compare VAR_RESULT, TRUE
	goto_if_eq Cormoria_Route2_AncientCarving_2
# 93 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_AncientCarving_Text_5
# 94 "data//maps/Route2_Caves/scripts.pory"
	playse SE_CLICK
# 95 "data//maps/Route2_Caves/scripts.pory"
	delay 8
# 96 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_AncientCarving_Text_2
# 97 "data//maps/Route2_Caves/scripts.pory"
	release
# 98 "data//maps/Route2_Caves/scripts.pory"
	delay 8
# 99 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_AncientCarving_Text_3
# 100 "data//maps/Route2_Caves/scripts.pory"
	releaseall
Cormoria_Route2_AncientCarving_1:
# 104 "data//maps/Route2_Caves/scripts.pory"
	releaseall
	end

Cormoria_Route2_AncientCarving_2:
# 61 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_AncientCarving_Text_1
# 62 "data//maps/Route2_Caves/scripts.pory"
	playse SE_CLICK
# 63 "data//maps/Route2_Caves/scripts.pory"
	delay 8
# 64 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_AncientCarving_Text_2
# 65 "data//maps/Route2_Caves/scripts.pory"
	release
# 66 "data//maps/Route2_Caves/scripts.pory"
	delay 8
# 67 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_AncientCarving_Text_3
# 68 "data//maps/Route2_Caves/scripts.pory"
	release
# 69 "data//maps/Route2_Caves/scripts.pory"
	delay 8
# 71 "data//maps/Route2_Caves/scripts.pory"
	goto_if_unset Cormoria_FLAG_FLUTTER_MANE, Cormoria_Route2_AncientCarving_5
	goto Cormoria_Route2_AncientCarving_1

Cormoria_Route2_AncientCarving_5:
# 72 "data//maps/Route2_Caves/scripts.pory"
	setvar VAR_0x8004, 2
# 73 "data//maps/Route2_Caves/scripts.pory"
	setvar VAR_0x8005, 2
# 74 "data//maps/Route2_Caves/scripts.pory"
	setvar VAR_0x8006, 30
# 75 "data//maps/Route2_Caves/scripts.pory"
	setvar VAR_0x8007, 5
# 76 "data//maps/Route2_Caves/scripts.pory"
	special ShakeCamera
# 77 "data//maps/Route2_Caves/scripts.pory"
	waitstate
# 78 "data//maps/Route2_Caves/scripts.pory"
	release
# 80 "data//maps/Route2_Caves/scripts.pory"
	msgbox Cormoria_Route2_AncientCarving_Text_4
# 81 "data//maps/Route2_Caves/scripts.pory"
	delay 8
# 83 "data//maps/Route2_Caves/scripts.pory"
	setflag Cormoria_FLAG_FLUTTER_MANE
# 85 "data//maps/Route2_Caves/scripts.pory"
	setwildbattle SPECIES_FLUTTER_MANE, 50
# 86 "data//maps/Route2_Caves/scripts.pory"
	dowildbattle
# 88 "data//maps/Route2_Caves/scripts.pory"
	releaseall
	end


# 13 "data//maps/Route2_Caves/scripts.pory"
Cormoria_Route2_Caves_EventScript_ClefairyTrigger_Movement_0:
# 13 "data//maps/Route2_Caves/scripts.pory"
	emote_exclamation_mark
# 13 "data//maps/Route2_Caves/scripts.pory"
	jump_in_place_down
# 13 "data//maps/Route2_Caves/scripts.pory"
	walk_up
# 13 "data//maps/Route2_Caves/scripts.pory"
	walk_up
# 13 "data//maps/Route2_Caves/scripts.pory"
	face_down
	step_end

# 31 "data//maps/Route2_Caves/scripts.pory"
Cormoria_Route2_Caves_EventScript_ClefairyBattle_Movement_0:
# 31 "data//maps/Route2_Caves/scripts.pory"
	walk_fast_up
# 31 "data//maps/Route2_Caves/scripts.pory"
	walk_fast_up
# 31 "data//maps/Route2_Caves/scripts.pory"
	walk_fast_up
# 31 "data//maps/Route2_Caves/scripts.pory"
	walk_fast_up
# 31 "data//maps/Route2_Caves/scripts.pory"
	walk_fast_up
# 31 "data//maps/Route2_Caves/scripts.pory"
	walk_fast_up
	step_end

Cormoria_Route2_Caves_EventScript_ClefairyBattle_Text_0:
# 22 "data//maps/Route2_Caves/scripts.pory"
	.string "A wild Pokémon attacks you!\n"
	.string "It's holding the Strange Rock!$"

Cormoria_Route2_Caves_EventScript_ClefairyBattle_Text_1:
# 29 "data//maps/Route2_Caves/scripts.pory"
	.string "The Clefairy dropped the Strange Rock.\n"
	.string "Stealing is bad, Clefairy!\p"
	.string "...\p"
	.string "It looks dismayed and runs away...$"

Cormoria_Route2_AncientCarving_Text_0:
# 55 "data//maps/Route2_Caves/scripts.pory"
	.string "It's an ancient cave painting...$"

Cormoria_Route2_AncientCarving_Text_1:
# 61 "data//maps/Route2_Caves/scripts.pory"
	.string "Scanning the ancient painting\n"
	.string "with the Archaeolens 2.0...$"

Cormoria_Route2_AncientCarving_Text_2:
# 64 "data//maps/Route2_Caves/scripts.pory"
	.string "Done!$"

Cormoria_Route2_AncientCarving_Text_3:
# 67 "data//maps/Route2_Caves/scripts.pory"
	.string "It's a never before seen Pokémon!\n"
	.string "It looks pale and ghostly, fluttering\n"
	.string "in the breeze.\p"
	.string "Terrifying!$"

Cormoria_Route2_AncientCarving_Text_4:
# 80 "data//maps/Route2_Caves/scripts.pory"
	.string "What was that?\p"
	.string "...\p"
	.string "The Archaeolens 2.0 is overheating!\n"
	.string "What's happening?$"

Cormoria_Route2_AncientCarving_Text_5:
# 93 "data//maps/Route2_Caves/scripts.pory"
	.string "Scanning the ancient painting\n"
	.string "with the Archaeolens...$"
