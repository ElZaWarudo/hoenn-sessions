NewBarkTown_MapScripts::
	.byte 0

NewBarkTown_EventScript_Guide::
	lock
	faceplayer
	msgbox NewBarkTown_Text_Return, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, NO, NewBarkTown_EventScript_Stay
	warp MAP_OLDALE_TOWN_POKEMON_CENTER_1F, 10, 4
	waitstate
	release
	end

NewBarkTown_EventScript_Stay:
	release
	end

NewBarkTown_EventScript_TownSign::
	msgbox NewBarkTown_Text_TownSign, MSGBOX_SIGN
	end

NewBarkTown_EventScript_Closed::
	msgbox NewBarkTown_Text_Closed, MSGBOX_SIGN
	end

NewBarkTown_Text_Return:
	.string "Welcome to NEW BARK TOWN!\p"
	.string "Would you like to return to\n"
	.string "OLDALE TOWN?$"

NewBarkTown_Text_TownSign:
	.string "NEW BARK TOWN\n"
	.string "The town where winds of a new\l"
	.string "beginning blow.$"

NewBarkTown_Text_Closed:
	.string "Closed for now.\p"
	.string "The rest of JOHTO will open\n"
	.string "in a future update.$"
