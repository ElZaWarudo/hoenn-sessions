Cormoria_WinterlilyHollow_PokemonCenter_MapScripts::
# 2 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_WinterlilyHollow_PokemonCenter_OnTransition
	.byte 0


Cormoria_WinterlilyHollow_PokemonCenter_OnTransition::
# 6 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	setrespawn Cormoria_HEAL_LOCATION_WINTERLILY_HOLLOW
	return


Cormoria_WinterlilyHollow_PokemonCenter_EventScript_Nurse::
# 10 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	lockall
# 11 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	setvar VAR_0x800B, 1
# 12 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	call Cormoria_Common_EventScript_PkmnCenterNurse
# 13 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	waitmessage
# 14 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	releaseall
	return


Cormoria_WinterlilyHollow_PokemonCenter_NPC1::
# 19 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	msgbox Cormoria_WinterlilyHollow_PokemonCenter_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_WinterlilyHollow_PokemonCenter_Chansey::
# 23 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	playmoncry SPECIES_CHANSEY, CRY_MODE_NORMAL
# 24 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	msgbox Cormoria_WinterlilyHollow_PokemonCenter_Chansey_Text_0, MSGBOX_NPC
	end


Cormoria_WinterlilyHollow_PokemonCenter_NPC2::
# 28 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	msgbox Cormoria_WinterlilyHollow_PokemonCenter_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_WinterlilyHollow_PokemonCenter_NPC1_Text_0:
# 19 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	.string "{PLAYER} - it's you! Thank you again\n"
	.string "for saving Chansey. We'll stay here a\l"
	.string "while and give the nurse company before\l"
	.string "heading back down.$"

Cormoria_WinterlilyHollow_PokemonCenter_Chansey_Text_0:
# 24 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	.string "Seeyyyy!!$"

Cormoria_WinterlilyHollow_PokemonCenter_NPC2_Text_0:
# 28 "data//maps/WinterlilyHollow_PokemonCenter/scripts.pory"
	.string "This is my first time in Winterlily\n"
	.string "Hollow. I came to visit the Ranger\l"
	.string "Institute further down in Ivy River.$"
