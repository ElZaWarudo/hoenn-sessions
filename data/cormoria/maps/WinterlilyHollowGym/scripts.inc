Cormoria_WinterlilyHollowGym_MapScripts::
# 2 "data//maps/WinterlilyHollowGym/scripts.pory"
	map_script MAP_SCRIPT_ON_RESUME, Cormoria_WinterlilyHollowGym_SetHoleWarp
	.byte 0


Cormoria_WinterlilyHollowGym_SetHoleWarp::
# 7 "data//maps/WinterlilyHollowGym/scripts.pory"
	compare Cormoria_VAR_HOLLOW_GYM_LAYOUT, 1
	goto_if_eq Cormoria_WinterlilyHollowGym_SetHoleWarp_2
Cormoria_WinterlilyHollowGym_SetHoleWarp_1:
# 9 "data//maps/WinterlilyHollowGym/scripts.pory"
	setstepcallback STEP_CB_CRACKED_FLOOR
# 10 "data//maps/WinterlilyHollowGym/scripts.pory"
	setholewarp MAP_CORMORIA_WINTERLILY_HOLLOW_GYM_B1F
	end

Cormoria_WinterlilyHollowGym_SetHoleWarp_2:
# 7 "data//maps/WinterlilyHollowGym/scripts.pory"
	setvar Cormoria_VAR_HOLLOW_GYM_LAYOUT, 0
	goto Cormoria_WinterlilyHollowGym_SetHoleWarp_1

