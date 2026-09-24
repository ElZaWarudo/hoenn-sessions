Cormoria_RivetshoreCity_PokemonCenter_MapScripts::
# 2 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_RivetshoreCity_PokemonCenter_OnTransition
	.byte 0


Cormoria_RivetshoreCity_PokemonCenter_OnTransition::
# 6 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	setrespawn Cormoria_HEAL_LOCATION_RIVETSHORE_CITY
	return


Cormoria_RivetshoreCity_PokemonCenter_EventScript_Nurse::
# 10 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	lockall
# 11 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	setvar VAR_0x800B, 1
# 12 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	call Cormoria_Common_EventScript_PkmnCenterNurse
# 13 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	waitmessage
# 14 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	releaseall
	return


Cormoria_RivetshoreCity_PokemonCenter_NPC1::
# 18 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	msgbox Cormoria_RivetshoreCity_PokemonCenter_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_RivetshoreCity_PokemonCenter_NPC2::
# 24 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	msgbox Cormoria_RivetshoreCity_PokemonCenter_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_RivetshoreCity_PokemonCenter_NPC1_Text_0:
# 18 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	.string "I once met a strange lady here. She\n"
	.string "tried to sell me all these rare items! I\l"
	.string "could never find her again...$"

Cormoria_RivetshoreCity_PokemonCenter_NPC2_Text_0:
# 24 "data//maps/RivetshoreCity_PokemonCenter/scripts.pory"
	.string "We had a heatwave yesterday.\n"
	.string "It's not natural, I tell you!$"
