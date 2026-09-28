NavelRock_Harbor_MapScripts::
	.byte 0

NavelRock_Harbor_EventScript_Sailor::
	lock
	faceplayer
	msgbox NavelRock_Harbor_Text_SailorReturn, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, NO, NavelRock_Harbor_EventScript_AsYouLike
	setvar VAR_0x8004, 91
	setvar VAR_0x8005, 2
	special Special_CoopGroupTravelBegin
	goto_if_eq VAR_RESULT, 1, NavelRock_Harbor_EventScript_GroupTravelWaiting
	goto_if_eq VAR_RESULT, 2, NavelRock_Harbor_EventScript_GroupTravelFailed
	msgbox EventTicket_Text_SailHome, MSGBOX_DEFAULT
	closemessage
	applymovement VAR_LAST_TALKED, Common_Movement_WalkInPlaceFasterDown
	waitmovement 0
	delay 30
	hideobjectat LOCALID_NAVEL_ROCK_SAILOR, MAP_NAVEL_ROCK_HARBOR
	setvar VAR_0x8004, LOCALID_NAVEL_ROCK_SS_TIDAL
	call Common_EventScript_FerryDepartIsland
	warp MAP_LILYCOVE_CITY_HARBOR, 8, 11
	waitstate
	release
	end

NavelRock_Harbor_EventScript_GroupTravelWaiting::
	end

NavelRock_Harbor_EventScript_GroupTravelFailed::
	release
	end

NavelRock_Harbor_EventScript_AsYouLike::
	msgbox EventTicket_Text_AsYouLike, MSGBOX_DEFAULT
	release
	end

