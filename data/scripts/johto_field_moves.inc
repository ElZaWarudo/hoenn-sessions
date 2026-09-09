@ Donor Whirlpool crossing, with its disabled party requirement restored.
Johto_EventScript_Whirlpool::
    lockall
    goto_if_unset JOHTO_FLAG_BADGE07_GET, Johto_EventScript_CantWhirlpool
    callnative Script_JohtoCheckWhirlpool, requests_effects=1
    goto_if_eq VAR_RESULT, PARTY_SIZE, Johto_EventScript_CantWhirlpool
    playse SE_M_WATERFALL
    goto_if_eq VAR_FACING, DIR_NORTH, Johto_EventScript_WhirlpoolGoNorth
    goto_if_eq VAR_FACING, DIR_SOUTH, Johto_EventScript_WhirlpoolGoSouth
    goto_if_eq VAR_FACING, DIR_EAST, Johto_EventScript_WhirlpoolGoEast
    goto_if_eq VAR_FACING, DIR_WEST, Johto_EventScript_WhirlpoolGoWest
    releaseall
    end

Johto_EventScript_WhirlpoolGoNorth::
    applymovement OBJ_EVENT_ID_PLAYER, Johto_Movement_WhirlpoolNorth
    waitmovement 0
    releaseall
    end

Johto_EventScript_WhirlpoolGoSouth::
    applymovement OBJ_EVENT_ID_PLAYER, Johto_Movement_WhirlpoolSouth
    waitmovement 0
    releaseall
    end

Johto_EventScript_WhirlpoolGoEast::
    applymovement OBJ_EVENT_ID_PLAYER, Johto_Movement_WhirlpoolEast
    waitmovement 0
    releaseall
    end

Johto_EventScript_WhirlpoolGoWest::
    applymovement OBJ_EVENT_ID_PLAYER, Johto_Movement_WhirlpoolWest
    waitmovement 0
    releaseall
    end

Johto_EventScript_CantWhirlpool::
    msgbox Johto_Text_CantWhirlpool, MSGBOX_DEFAULT
    closemessage
    releaseall
    end

Johto_Movement_WhirlpoolNorth:
    slide_up
    slide_up
    slide_up
    step_end

Johto_Movement_WhirlpoolSouth:
    slide_down
    slide_down
    slide_down
    step_end

Johto_Movement_WhirlpoolEast:
    slide_right
    slide_right
    slide_right
    step_end

Johto_Movement_WhirlpoolWest:
    slide_left
    slide_left
    slide_left
    step_end

Johto_Text_CantWhirlpool:
    .string "The water is swirling and spinning\n"
    .string "around with a mighty roar.$"

Johto_EventScript_Headbutt::
    lockall
    goto_if_unset JOHTO_FLAG_GET_HEADBUTT, Johto_EventScript_CantHeadbutt
    callnative Script_JohtoCheckHeadbutt, requests_effects=1
    goto_if_eq VAR_RESULT, PARTY_SIZE, Johto_EventScript_CantHeadbutt
    bufferpartymonnick STR_VAR_1, VAR_RESULT
    buffermovename STR_VAR_2, MOVE_HEADBUTT
    msgbox Johto_Text_MonUsedHeadbutt, MSGBOX_DEFAULT
    closemessage
    setvar VAR_0x8004, 0
    setvar VAR_0x8005, 1
    setvar VAR_0x8006, 4
    setvar VAR_0x8007, 2
    special ShakeCamera
    waitstate
    playse SE_M_HEADBUTT
    special RockSmashWildEncounter
    goto_if_eq VAR_RESULT, FALSE, Johto_EventScript_EndHeadbutt
    @ RockSmashWildEncounter owns the ordinary battle return; its field or
    @ whiteout callback restores state and discards this waiting script.
    waitstate
Johto_EventScript_EndHeadbutt::
    releaseall
    end

Johto_EventScript_CantHeadbutt::
    msgbox Johto_Text_CantHeadbutt, MSGBOX_DEFAULT
    releaseall
    end

Johto_Text_MonUsedHeadbutt:
    .string "{STR_VAR_1} used HEADBUTT!$"

Johto_Text_CantHeadbutt:
    .string "It's a small tree, but a wild\n"
    .string "POKéMON may be living within.$"
