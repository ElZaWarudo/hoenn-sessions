# 1 "data//maps/Rivetshore_RangerInstitute/scripts.pory"

# 2 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
    .set Cormoria_maps_Rivetshore_RangerInstitute_scripts_LOCALID_DAYCARE_MAN, 2
# 3 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
    .set Cormoria_maps_Rivetshore_RangerInstitute_scripts_LOCALID_DAYCARE_MON_0, 3
# 4 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
    .set Cormoria_maps_Rivetshore_RangerInstitute_scripts_LOCALID_DAYCARE_MON_1, 4

Cormoria_Rivetshore_RangerInstitute_MapScripts::
# 8 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_Rivetshore_RangerInstitute_OnTransition
	.byte 0


Cormoria_Rivetshore_RangerInstitute_OnTransition::
# 13 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	call Cormoria_Rivetshore_RangerInstitute_TryMoveDayCareMan
# 15 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	setflag Cormoria_FLAG_TEMP_1
# 16 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	setflag Cormoria_FLAG_TEMP_2
# 17 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	getdaycaregfx Cormoria_VAR_OBJ_GFX_ID_0, Cormoria_VAR_OBJ_GFX_ID_1, VAR_TEMP_0, VAR_TEMP_1
# 18 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	switch VAR_RESULT
# 20 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	case 2, Cormoria_Rivetshore_RangerInstitute_OnTransition_3
# 21 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	case 1, Cormoria_Rivetshore_RangerInstitute_OnTransition_4
Cormoria_Rivetshore_RangerInstitute_OnTransition_1:
	end

Cormoria_Rivetshore_RangerInstitute_OnTransition_3:
# 20 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	call Cormoria_Route117_EventScript_ShowDaycareMon1
	goto Cormoria_Rivetshore_RangerInstitute_OnTransition_1

Cormoria_Rivetshore_RangerInstitute_OnTransition_4:
# 21 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	call Cormoria_Route117_EventScript_ShowDaycareMon0
	goto Cormoria_Rivetshore_RangerInstitute_OnTransition_1


Cormoria_Rivetshore_RangerInstitute_TryMoveDayCareMan::
# 28 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	goto_if_unset Cormoria_FLAG_PENDING_DAYCARE_EGG, Cormoria_Route117_EventScript_StopMoveDayCareMan
# 29 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	setobjectxyperm Cormoria_maps_Rivetshore_RangerInstitute_scripts_LOCALID_DAYCARE_MAN, 26, 12
	return


Cormoria_Rivetshore_RangerInstitute_DittoMan::
# 35 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	goto_if_set Cormoria_FLAG_RIVETSHORE_RANGER_DITTO, Cormoria_Rivetshore_RangerInstitute_DittoMan_2
# 40 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	msgbox Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_1, MSGBOX_NPC
# 41 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	release
# 42 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	delay 16
# 44 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	checkitem ITEM_RANGER_CREST
# 45 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	compare VAR_RESULT, TRUE
	goto_if_eq Cormoria_Rivetshore_RangerInstitute_DittoMan_4
	return

Cormoria_Rivetshore_RangerInstitute_DittoMan_2:
# 36 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	msgbox Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_0, MSGBOX_NPC
	end

Cormoria_Rivetshore_RangerInstitute_DittoMan_4:
# 46 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	goto_if_unset Cormoria_FLAG_RIVETSHORE_RANGER_DITTO, Cormoria_Rivetshore_RangerInstitute_DittoMan_6
	return

Cormoria_Rivetshore_RangerInstitute_DittoMan_6:
# 47 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	msgbox Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_2
# 49 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	givemon SPECIES_DITTO, 30
# 50 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	setflag Cormoria_FLAG_RIVETSHORE_RANGER_DITTO
# 52 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	showmonpic SPECIES_DITTO, 10, 3
# 53 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	playfanfare MUS_OBTAIN_ITEM
# 54 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	msgbox Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_3
# 55 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	waitfanfare
# 56 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	hidemonpic
# 58 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	msgbox Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_0
# 59 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	releaseall
	end


Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_0:
# 36 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	.string "We can take care of Ditto and another\n"
	.string "Pokémon for you any time.$"

Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_1:
# 40 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	.string "I used to manage this facility before\n"
	.string "Director Ravine promoted me. I like the\l"
	.string "new responsibility but I miss spending\l"
	.string "time with Pokemon!\p"
	.string "Did you know that a Ditto can breed\n"
	.string "with any other Pokémon?\p"
	.string "But since Ditto aren't in Cormoria,\n"
	.string "Director Ravine only lets fully-fledged\l"
	.string "Pokémon Rangers own one.$"

Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_2:
# 47 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	.string "Hey! That's a Ranger Shield!\n"
	.string "Nobody told me you got promoted.\p"
	.string "Congratulations!\p"
	.string "In recognition of your promotion,\n"
	.string "the Ranger Day-Care Facility proudly\l"
	.string "presents you with a Ditto!$"

Cormoria_Rivetshore_RangerInstitute_DittoMan_Text_3:
# 54 "data//maps/Rivetshore_RangerInstitute/scripts.pory"
	.string "The Ranger Day-Care Facility has\n"
	.string "gifted you a Ditto!$"
