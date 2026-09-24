Cormoria_AncientMirroh_Tower_3F_MapScripts::
# 2 "data//maps/AncientMirroh_Tower_3F/scripts.pory"
	map_script MAP_SCRIPT_ON_RESUME, Cormoria_AncientMirroh_Tower_3F_SetHoleWarp
	.byte 0


Cormoria_AncientMirroh_Tower_3F_SetHoleWarp::
# 6 "data//maps/AncientMirroh_Tower_3F/scripts.pory"
	setstepcallback STEP_CB_CRACKED_FLOOR
# 7 "data//maps/AncientMirroh_Tower_3F/scripts.pory"
	setholewarp MAP_CORMORIA_ANCIENT_MIRROH_TOWER_2F
	end

