.set Cormoria_maps_PellucaCableCarStation_scripts_LOCALID_ATTENDANT, 1

Cormoria_PellucaCableCarStation_MapScripts::
	map_script MAP_SCRIPT_ON_TRANSITION, Cormoria_PellucaCableCarStation_OnTransition
	map_script MAP_SCRIPT_ON_FRAME_TABLE, Cormoria_PellucaCableCarStation_OnFrame
	.byte 0

Cormoria_PellucaCableCarStation_OnTransition:
	setescapewarp MAP_ROUTE112, 28, 28
	call_if_eq Cormoria_VAR_CABLE_CAR_STATION_STATE, 2, Cormoria_PellucaCableCarStation_EventScript_MoveAttendantAside
	end

Cormoria_PellucaCableCarStation_EventScript_MoveAttendantAside::
	setobjectxyperm Cormoria_maps_PellucaCableCarStation_scripts_LOCALID_ATTENDANT, 7, 4
	setobjectmovementtype Cormoria_maps_PellucaCableCarStation_scripts_LOCALID_ATTENDANT, MOVEMENT_TYPE_FACE_LEFT
	return

Cormoria_PellucaCableCarStation_OnFrame:
	map_script_2 Cormoria_VAR_CABLE_CAR_STATION_STATE, 2, Cormoria_PellucaCableCarStation_EventScript_ExitCableCar
	.2byte 0

Cormoria_PellucaCableCarStation_EventScript_ExitCableCar::
	lockall
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_PellucaCableCarStation_Movement_ExitCableCar
	applymovement Cormoria_maps_PellucaCableCarStation_scripts_LOCALID_ATTENDANT, Cormoria_PellucaCableCarStation_Movement_FollowPlayerOutFromCableCar
	waitmovement 0
	setvar Cormoria_VAR_CABLE_CAR_STATION_STATE, 0
	setobjectxyperm Cormoria_maps_PellucaCableCarStation_scripts_LOCALID_ATTENDANT, 6, 7
	setobjectmovementtype Cormoria_maps_PellucaCableCarStation_scripts_LOCALID_ATTENDANT, MOVEMENT_TYPE_FACE_DOWN
	releaseall
	end

Cormoria_PellucaCableCarStation_EventScript_Attendant::
	lock
	faceplayer
	goto Cormoria_PellucaCableCarStation_EventScript_AttendantAskForCableCar
@	checkitem ITEM_CABLECAR_PASS
@	goto_if_eq VAR_RESULT, TRUE, Cormoria_PellucaCableCarStation_EventScript_AttendantAskForCableCar
@	goto_if_eq VAR_RESULT, FALSE, Cormoria_PellucaCableCarStation_EventScript_AttendantNeedCableCarPass

Cormoria_PellucaCableCarStation_EventScript_AttendantNeedCableCarPass::
	msgbox Cormoria_PellucaCableCarStation_Text_NeedPass
	release
	end

Cormoria_PellucaCableCarStation_EventScript_AttendantAskForCableCar::
	msgbox Cormoria_PellucaCableCarStation_Text_CableCarReadyGetOn, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_PellucaCableCarStation_EventScript_RideCableCar
	goto_if_eq VAR_RESULT, NO, Cormoria_PellucaCableCarStation_EventScript_DeclineRide
	end

Cormoria_PellucaCableCarStation_EventScript_RideCableCar::
	msgbox Cormoria_PellucaCableCarStation_Text_StepThisWay, MSGBOX_DEFAULT
	closemessage
	applymovement Cormoria_maps_PellucaCableCarStation_scripts_LOCALID_ATTENDANT, Cormoria_PellucaCableCarStation_Movement_LeadPlayerToCableCar
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_PellucaCableCarStation_Movement_BoardCableCar
	waitmovement 0
	setvar VAR_0x8004, FALSE  @ Going up
	setvar Cormoria_VAR_CABLE_CAR_STATION_STATE, 1
	incrementgamestat GAME_STAT_RODE_CABLE_CAR
	special CableCarWarp
	special CableCar
	waitstate
	release
	end

Cormoria_PellucaCableCarStation_EventScript_DeclineRide::
	msgbox Cormoria_PellucaCableCarStation_Text_RideAnotherTime, MSGBOX_DEFAULT
	release
	end

Cormoria_PellucaCableCarStation_Movement_LeadPlayerToCableCar:
	walk_up
	walk_up
	walk_right
	walk_in_place_faster_left
	step_end

Cormoria_PellucaCableCarStation_Movement_FollowPlayerOutFromCableCar:
	delay_16
	walk_left
	walk_down
	walk_down
	step_end

Cormoria_PellucaCableCarStation_Movement_BoardCableCar:
	walk_up
	walk_up
	walk_up
	delay_16
	step_end

Cormoria_PellucaCableCarStation_Movement_ExitCableCar:
	walk_down
	walk_down
	walk_down
	delay_16
	step_end

Cormoria_PellucaCableCarStation_Text_CableCarReadyGetOn:
	.string "Hiii - oh, sorry...\n"
	.string "I thought you were someone else.\p"	
	.string "The Cable Car is ready to go up.\n"
	.string "Would you like to ride it?$"

Cormoria_PellucaCableCarStation_Text_StepThisWay:
	.string "Please step this way.\p"
	.string "...\n"
	.string "When will he ask me out?$"

Cormoria_PellucaCableCarStation_Text_RideAnotherTime:
	.string "Please ride with us another time.$"

Cormoria_PellucaCableCarStation_Text_NeedPass:
	.string "Sorry, you need a CABLE CAR PASS.$"
	
