@ Interact with cuttable tree
Cormoria_EventScript_CutTree::
	lockall
	goto_if_unset Cormoria_FLAG_BADGE01_GET, Cormoria_EventScript_CheckTreeCantCut
	checkpartymove MOVE_CUT
	goto_if_eq VAR_RESULT, PARTY_SIZE, Cormoria_EventScript_CheckTreeCantCut
	setfieldeffectargument 0, VAR_RESULT
	bufferpartymonnick STR_VAR_1, VAR_RESULT
	buffermovename STR_VAR_2, MOVE_CUT
@	msgbox Cormoria_Text_WantToCut, MSGBOX_YESNO
@	goto_if_eq VAR_RESULT, NO, Cormoria_EventScript_CancelCut
@	msgbox Cormoria_Text_MonUsedFieldMove, MSGBOX_DEFAULT
@	closemessage
Cormoria_EventScript_CutTreeCommon:
	isfollowerfieldmoveuser VAR_0x8004
	setfieldeffectargument 3, VAR_0x8004 @ skip pose if so
	dofieldeffect FLDEFF_USE_CUT_ON_TREE
	waitstate
Cormoria_EventScript_CutTreeDown:: @ fallthrough
	setflag Cormoria_FLAG_SAFE_FOLLOWER_MOVEMENT
	call_if_eq VAR_0x8004, TRUE, Cormoria_EventScript_FollowerFieldMove
	applymovement VAR_LAST_TALKED, Cormoria_Movement_CutTreeDown
	waitmovement 0
	removeobject VAR_LAST_TALKED
	releaseall
	end

@ Use cut from party menu
Cormoria_EventScript_UseCut::
	lockall
	goto Cormoria_EventScript_CutTreeCommon

Cormoria_Movement_CutTreeDown:
	cut_tree
	step_end

Cormoria_EventScript_CheckTreeCantCut::
	msgbox Cormoria_Text_CantCut, MSGBOX_DEFAULT
	releaseall
	end

Cormoria_EventScript_CancelCut::
	closemessage
	releaseall
	end

Cormoria_Text_WantToCut:
	.string "This tree looks like it can be\n"
	.string "CUT down!\p"
	.string "Would you like to CUT it?$"

Cormoria_Text_MonUsedFieldMove:
	.string "{STR_VAR_1} used {STR_VAR_2}!$"

Cormoria_Text_CantCut:
	.string "This tree looks like it can be\n"
	.string "CUT down!$"

@ Use rock smash from party menu
Cormoria_EventScript_UseRockSmash::
	lockall
	goto Cormoria_EventScript_RockSmashCommon

@ Interact with smashable rock
Cormoria_EventScript_RockSmash::
	lockall
	goto_if_unset Cormoria_FLAG_BADGE03_GET, Cormoria_EventScript_CantSmashRock
	checkpartymove MOVE_ROCK_SMASH
	goto_if_eq VAR_RESULT, PARTY_SIZE, Cormoria_EventScript_CantSmashRock
	setfieldeffectargument 0, VAR_RESULT
	bufferpartymonnick STR_VAR_1, VAR_RESULT
	buffermovename STR_VAR_2, MOVE_ROCK_SMASH
	msgbox Cormoria_Text_WantToSmash, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, NO, Cormoria_EventScript_CancelSmash
	msgbox Cormoria_Text_MonUsedFieldMove, MSGBOX_DEFAULT
	closemessage
Cormoria_EventScript_RockSmashCommon:
	@ check if follower should use the field move
	isfollowerfieldmoveuser VAR_0x8004
	setfieldeffectargument 3, VAR_0x8004 @ skip pose if so
	dofieldeffect FLDEFF_USE_ROCK_SMASH
	waitstate
Cormoria_EventScript_SmashRock:: @ fallthrough
	setflag Cormoria_FLAG_SAFE_FOLLOWER_MOVEMENT
	call_if_eq VAR_0x8004, TRUE, Cormoria_EventScript_FollowerFieldMove
	applymovement VAR_LAST_TALKED, Cormoria_Movement_SmashRock
	waitmovement 0
	removeobject VAR_LAST_TALKED
	specialvar VAR_RESULT, TryUpdateRusturfTunnelState
	goto_if_eq VAR_RESULT, TRUE, Cormoria_EventScript_EndSmash
	special RockSmashWildEncounter
	goto_if_eq VAR_RESULT, FALSE, Cormoria_EventScript_EndSmash
	waitstate
	releaseall
	end

Cormoria_EventScript_FollowerFieldMove:
	getdirectiontoface VAR_0x8005, OBJ_EVENT_ID_FOLLOWER, OBJ_EVENT_ID_PLAYER
	specialvar VAR_0x8006, GetPlayerFacingDirection
	goto_if_eq VAR_0x8005, DIR_NONE, Cormoria_EventScript_FollowerFieldMoveEnd
	@ Swap follower and player
	call Cormoria_EventScript_FollowerSwap
	@ Face follower in direction and jump
	switch VAR_0x8006
	case DIR_NORTH, Cormoria_EventScript_FollowerJumpNorth
	case DIR_EAST, Cormoria_EventScript_FollowerJumpEast
	case DIR_SOUTH, Cormoria_EventScript_FollowerJumpSouth
	case DIR_WEST, Cormoria_EventScript_FollowerJumpWest
Cormoria_EventScript_FollowerFieldMoveEnd:
	return

Cormoria_EventScript_FollowerSwap:
	switch VAR_0x8005
	case DIR_NORTH, Cormoria_EventScript_FollowerMoveNorth
	case DIR_EAST, Cormoria_EventScript_FollowerMoveEast
	case DIR_SOUTH, Cormoria_EventScript_FollowerMoveSouth
	case DIR_WEST, Cormoria_EventScript_FollowerMoveWest
	return

Cormoria_EventScript_FollowerMoveNorth:
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Movement_WalkDown
	waitmovement 0
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Common_Movement_FaceUp
	waitmovement 0
	return

Cormoria_EventScript_FollowerMoveEast:
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Movement_WalkLeft
	waitmovement 0
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Common_Movement_FaceRight
	waitmovement 0
	return

Cormoria_EventScript_FollowerMoveSouth:
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Movement_WalkUp
	waitmovement 0
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Common_Movement_FaceDown
	waitmovement 0
	return

Cormoria_EventScript_FollowerMoveWest:
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Movement_WalkRight
	waitmovement 0
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_Common_Movement_FaceLeft
	waitmovement 0
	return

Cormoria_EventScript_FollowerJumpNorth:
	applymovement OBJ_EVENT_ID_FOLLOWER, Cormoria_Movement_JumpUp
	waitmovement 0
	return

Cormoria_EventScript_FollowerJumpEast:
	applymovement OBJ_EVENT_ID_FOLLOWER, Cormoria_Movement_JumpRight
	waitmovement 0
	return

Cormoria_EventScript_FollowerJumpSouth:
	applymovement OBJ_EVENT_ID_FOLLOWER, Cormoria_Movement_JumpDown
	waitmovement 0
	return

Cormoria_EventScript_FollowerJumpWest:
	applymovement OBJ_EVENT_ID_FOLLOWER, Cormoria_Movement_JumpLeft
	waitmovement 0
	return

Cormoria_EventScript_EndSmash::
	releaseall
	end

Cormoria_Movement_WalkUp:
	walk_up
	step_end

Cormoria_Movement_JumpUp:
	jump_in_place_up
	step_end

Cormoria_Movement_WalkRight:
	walk_right
	step_end

Cormoria_Movement_JumpRight:
	jump_in_place_right
	step_end

Cormoria_Movement_WalkDown:
	walk_down
	step_end

Cormoria_Movement_JumpDown:
	jump_in_place_down
	step_end

Cormoria_Movement_WalkLeft:
	walk_left
	step_end

Cormoria_Movement_JumpLeft:
	jump_in_place_left
	step_end

Cormoria_Movement_SmashRock:
	rock_smash_break
	step_end

Cormoria_EventScript_CantSmashRock::
	msgbox Cormoria_Text_CantSmash, MSGBOX_DEFAULT
	releaseall
	end

Cormoria_EventScript_CancelSmash::
	closemessage
	releaseall
	end

Cormoria_Text_WantToSmash:
	.string "This rock appears to be breakable.\n"
	.string "Would you like to use Rock Smash?$"

Cormoria_Text_CantSmash:
	.string "It's a rugged rock, but a Pokémon\n"
	.string "may be able to smash it.$"

Cormoria_EventScript_StrengthBoulder::
	lockall
	goto_if_unset Cormoria_FLAG_BADGE04_GET, Cormoria_EventScript_CantStrength
	goto_if_unset Cormoria_FLAG_SYS_GOT_STRENGTH, Cormoria_EventScript_CantStrength
	goto_if_set Cormoria_FLAG_SYS_USE_STRENGTH, Cormoria_EventScript_CheckActivatedBoulder
	checkpartymove MOVE_STRENGTH
@	goto_if_eq VAR_RESULT, PARTY_SIZE, Cormoria_EventScript_CantStrength
	setfieldeffectargument 0, VAR_RESULT
@	msgbox Cormoria_Text_WantToStrength, MSGBOX_YESNO
@	goto_if_eq VAR_RESULT, NO, Cormoria_EventScript_CancelStrength
@	closemessage
	dofieldeffect FLDEFF_USE_STRENGTH
	waitstate
	goto Cormoria_EventScript_ActivateStrength
	end

Cormoria_EventScript_UseStrength::
	lockall
	dofieldeffect FLDEFF_USE_STRENGTH
	waitstate
	goto Cormoria_EventScript_ActivateStrength
	end

Cormoria_EventScript_ActivateStrength::
	setflag Cormoria_FLAG_SYS_USE_STRENGTH
	msgbox Cormoria_Text_MonUsedStrength, MSGBOX_DEFAULT
	releaseall
	end

Cormoria_EventScript_CantStrength::
	msgbox Cormoria_Text_CantStrength, MSGBOX_DEFAULT
	releaseall
	end

Cormoria_EventScript_CheckActivatedBoulder::
	msgbox Cormoria_Text_StrengthActivated, MSGBOX_DEFAULT
	releaseall
	end

Cormoria_EventScript_CancelStrength::
	closemessage
	releaseall
	end

Cormoria_Text_WantToStrength:
	.string "It's a big boulder, but a Pokémon\n"
	.string "may be able to push it aside.\p"
	.string "Would you like to use Strength?$"

Cormoria_Text_MonUsedStrength:
	.string "{STR_VAR_1} used Strength!$"
@	.string "{STR_VAR_1}'s Strength made it\n"
@	.string "possible to move boulders around!$"

Cormoria_Text_CantStrength:
	.string "It's a big boulder, but a Pokémon\n"
	.string "may be able to push it aside.$"

Cormoria_Text_StrengthActivated:
	.string "Strength made it possible to move\n"
	.string "boulders around.$"

Cormoria_EventScript_UseWaterfall::
	lockall
	checkpartymove MOVE_WATERFALL
	goto_if_eq VAR_RESULT, PARTY_SIZE, Cormoria_EventScript_CantWaterfall
	bufferpartymonnick STR_VAR_1, VAR_RESULT
	setfieldeffectargument 0, VAR_RESULT
	msgbox Cormoria_Text_WantToWaterfall, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, NO, Cormoria_EventScript_EndWaterfall
	msgbox Cormoria_Text_MonUsedWaterfall, MSGBOX_DEFAULT
	dofieldeffect FLDEFF_USE_WATERFALL
	goto Cormoria_EventScript_EndWaterfall

Cormoria_EventScript_CannotUseWaterfall::
	lockall

Cormoria_EventScript_CantWaterfall::
	msgbox Cormoria_Text_CantWaterfall, MSGBOX_DEFAULT

Cormoria_EventScript_EndWaterfall::
	releaseall
	end

Cormoria_Text_CantWaterfall:
	.string "A wall of water is crashing down with\n"
	.string "a mighty roar.$"

Cormoria_Text_WantToWaterfall:
	.string "It's a large waterfall.\n"
	.string "Would you like to use Waterfall?$"

Cormoria_Text_MonUsedWaterfall:
	.string "{STR_VAR_1} used Waterfall.$"

Cormoria_EventScript_UseDive::
	lockall
	checkpartymove MOVE_DIVE
	goto_if_eq VAR_RESULT, PARTY_SIZE, Cormoria_EventScript_CantDive
	bufferpartymonnick STR_VAR_1, VAR_RESULT
	setfieldeffectargument 0, VAR_RESULT
	setfieldeffectargument 1, 1
	msgbox Cormoria_Text_WantToDive, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, NO, Cormoria_EventScript_EndDive
	msgbox Cormoria_Text_MonUsedDive, MSGBOX_DEFAULT
	dofieldeffect FLDEFF_USE_DIVE
	goto Cormoria_EventScript_EndDive
	end

Cormoria_EventScript_CantDive::
	msgbox Cormoria_Text_CantDive, MSGBOX_DEFAULT
	releaseall
	end

Cormoria_EventScript_EndDive::
	releaseall
	end

Cormoria_EventScript_UseDiveUnderwater::
	lockall
@	checkpartymove MOVE_DIVE
@	goto_if_eq VAR_RESULT, PARTY_SIZE, Cormoria_EventScript_CantSurface
	bufferpartymonnick STR_VAR_1, VAR_RESULT
	setfieldeffectargument 0, VAR_RESULT
	setfieldeffectargument 1, 1
	msgbox Cormoria_Text_WantToSurface, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, NO, Cormoria_EventScript_EndSurface
@	msgbox Cormoria_Text_MonUsedDive, MSGBOX_DEFAULT
	dofieldeffect FLDEFF_USE_DIVE
	goto Cormoria_EventScript_EndSurface
	end

Cormoria_EventScript_CantSurface::
	lockall
	msgbox Cormoria_Text_CantSurface, MSGBOX_DEFAULT
	goto Cormoria_EventScript_EndSurface
	end

Cormoria_EventScript_EndSurface::
	releaseall
	end

Cormoria_EventScript_DigCommon:
	isfollowerfieldmoveuser VAR_0x8004
	setfieldeffectargument 3, VAR_0x8004 @ skip pose if true
	dofieldeffect FLDEFF_USE_DIG
	waitstate
Cormoria_EventScript_DigSealedChamber:: @ fallthrough
	setflag Cormoria_FLAG_SAFE_FOLLOWER_MOVEMENT
	call_if_eq VAR_0x8004, TRUE, Cormoria_EventScript_FollowerFieldMove
	callnative DoBrailleDigEffect
	releaseall
	end

@ Use Dig from party menu
Cormoria_EventScript_UseDig::
	lockall
	goto Cormoria_EventScript_DigCommon


Cormoria_EventScript_CutGrassCommon:
	isfollowerfieldmoveuser VAR_0x8004
	setfieldeffectargument 3, VAR_0x8004 @ skip pose if true
	dofieldeffect FLDEFF_USE_CUT_ON_GRASS
	waitstate

@ Use Cut grass from party menu
Cormoria_EventScript_UseCutGrass::
	lockall
	goto Cormoria_EventScript_CutGrassCommon

Cormoria_Text_CantDive:
	.string "The sea is deep here. A Pokémon\n"
	.string "may be able to go underwater.$"

Cormoria_Text_WantToDive:
	.string "The sea is deep here.\n"
	.string "Would you like to use Dive?$"

Cormoria_Text_MonUsedDive:
	.string "{STR_VAR_1} used Dive.$"

Cormoria_Text_CantSurface:
	.string "Light is filtering down from above.\n"
	.string "A Pokémon may be able to surface.$"

Cormoria_Text_WantToSurface:
	.string "Light is filtering down from above.\n"
	.string "Would you like to swim up?$"

Cormoria_EventScript_FailSweetScent::
	msgbox Cormoria_Text_FailSweetScent, MSGBOX_SIGN
	end

Cormoria_Text_FailSweetScent:
	.string "Looks like there's nothing here…$"
