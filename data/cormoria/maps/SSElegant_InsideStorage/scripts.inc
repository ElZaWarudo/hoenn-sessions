Cormoria_SSElegant_InsideStorage_MapScripts::
	map_script MAP_SCRIPT_ON_LOAD, Cormoria_InsideStorage_OnLoad
	map_script MAP_SCRIPT_ON_RESUME, Cormoria_InsideStorage_OnResume
	.byte 0

Cormoria_InsideStorage_OnLoad:
	setmetatile 4, 1, METATILE_InsideOfTruck_ExitLight_Top, FALSE
	setmetatile 4, 2, METATILE_InsideOfTruck_ExitLight_Mid, FALSE
	setmetatile 4, 3, METATILE_InsideOfTruck_ExitLight_Bottom, FALSE
	end

Cormoria_InsideStorage_OnResume:
	setstepcallback STEP_CB_TRUCK
	end

Cormoria_InsideStorage_SetFlags::
	lockall
	setdynamicwarp MAP_CORMORIA_SSELEGANT_STORAGE, 2, 2
	releaseall
	end

