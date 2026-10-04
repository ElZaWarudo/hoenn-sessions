.set Cormoria_scripts_day_care_LOCALID_DAYCARE_LADY, 1

Cormoria_Route117_EventScript_DaycareMan::
	lock
	faceplayer
	special GetDaycareMonNicknames
	specialvar VAR_RESULT, GetDaycareState
	goto_if_eq VAR_RESULT, DAYCARE_EGG_WAITING, Cormoria_Route117_EventScript_DaycareEggWaiting
	goto_if_eq VAR_RESULT, DAYCARE_ONE_MON, Cormoria_Route117_EventScript_CheckOnOneMon
	goto_if_eq VAR_RESULT, DAYCARE_TWO_MONS, Cormoria_Route117_EventScript_CheckOnTwoMons
	msgbox Cormoria_Route117_Text_SeeWifeIfYoudLikeMeToRaiseMon, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_EventScript_DaycareEggWaiting::
	msgbox Cormoria_Route117_Text_DoYouWantEgg, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_EventScript_DaycareAcceptEgg
	msgbox Cormoria_Route117_Text_IWillKeepDoYouWantIt, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_EventScript_DaycareAcceptEgg
	msgbox Cormoria_Route117_Text_IllKeepIt, MSGBOX_DEFAULT
	clearflag Cormoria_FLAG_PENDING_DAYCARE_EGG
	special RejectEggFromDayCare
	release
	end

Cormoria_Route117_EventScript_DaycareAcceptEgg::
	specialvar VAR_RESULT, CalculatePlayerPartyCount
	goto_if_ne VAR_RESULT, PARTY_SIZE, Cormoria_Route117_EventScript_DaycareReceiveEgg
	msgbox Cormoria_Route117_Text_YouHaveNoRoomForIt, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_EventScript_DaycareReceiveEgg::
	message Cormoria_Route117_Text_ReceivedEgg
	playfanfare MUS_LEVEL_UP
	waitfanfare
	waitbuttonpress
	msgbox Cormoria_Route117_Text_TakeGoodCareOfIt, MSGBOX_DEFAULT
	special GiveEggFromDaycare
	clearflag Cormoria_FLAG_PENDING_DAYCARE_EGG
	release
	end

Cormoria_Route117_EventScript_CheckMonReceivedMail::
	specialvar VAR_RESULT, CheckDaycareMonReceivedMail
	call_if_eq VAR_RESULT, 1, Cormoria_Route117_EventScript_MonReceivedMail
	return

Cormoria_Route117_EventScript_MonReceivedMail::
	msgbox Cormoria_Route117_Text_FriendlyWithOtherTrainersMon, MSGBOX_DEFAULT
	return

Cormoria_Route117_EventScript_CheckOnOneMon::
	special GetDaycareMonNicknames
	msgbox Cormoria_Route117_Text_YourMonIsDoingFine, MSGBOX_DEFAULT
	setvar VAR_0x8004, 0
	call Cormoria_Route117_EventScript_CheckMonReceivedMail
	release
	end

Cormoria_Route117_EventScript_CheckOnTwoMons::
	special GetDaycareMonNicknames
	msgbox Cormoria_Route117_Text_YourMonsAreDoingFine, MSGBOX_DEFAULT
	special SetDaycareCompatibilityString
	special ShowFieldMessageStringVar4
	waitmessage
	waitbuttonpress
	setvar VAR_0x8004, 0
	call Cormoria_Route117_EventScript_CheckMonReceivedMail
	setvar VAR_0x8004, 1
	call Cormoria_Route117_EventScript_CheckMonReceivedMail
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_DaycareWoman::
	lock
	faceplayer
	specialvar VAR_RESULT, GetDaycareState
	goto_if_eq VAR_RESULT, DAYCARE_EGG_WAITING, Cormoria_Route117_PokemonDayCare_EventScript_EggWaiting
	goto_if_eq VAR_RESULT, DAYCARE_ONE_MON, Cormoria_Route117_PokemonDayCare_EventScript_OneMonInDaycare
	goto_if_eq VAR_RESULT, DAYCARE_TWO_MONS, Cormoria_Route117_PokemonDayCare_EventScript_TwoMonsInDaycare
	msgbox Cormoria_Route117_PokemonDayCare_Text_WouldYouLikeUsToRaiseAMon, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_PokemonDayCare_EventScript_GiveMonToRaise
	msgbox Cormoria_Route117_PokemonDayCare_Text_FineThenComeAgain, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_GiveMonToRaise::
	specialvar VAR_RESULT, CountPartyNonEggMons
	goto_if_eq VAR_RESULT, 1, Cormoria_Route117_PokemonDayCare_EventScript_OnlyOneMon
	specialvar VAR_RESULT, CountPartyAliveNonEggMons
	goto_if_eq VAR_RESULT, 2, Cormoria_Route117_PokemonDayCare_EventScript_OnlyTwoAliveMons
	msgbox Cormoria_Route117_PokemonDayCare_Text_WhichMonShouldWeRaise, MSGBOX_DEFAULT
	fadescreen FADE_TO_BLACK
	special ChooseSendDaycareMon
	waitstate
	goto_if_eq VAR_0x8004, PARTY_NOTHING_CHOSEN, Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain
	specialvar VAR_RESULT, CountPartyAliveNonEggMons_IgnoreVar0x8004Slot
	goto_if_eq VAR_RESULT, 0, Cormoria_Route117_PokemonDayCare_EventScript_OnlyOneAliveMon
	specialvar VAR_0x8005, GetSelectedMonNicknameAndSpecies
	waitse
	playmoncry VAR_0x8005, CRY_MODE_NORMAL
	msgbox Cormoria_Route117_PokemonDayCare_Text_WellRaiseYourMon, MSGBOX_DEFAULT
	waitmoncry
	special StoreSelectedPokemonInDaycare
	callnative UpdateFollowingPokemon
	incrementgamestat GAME_STAT_USED_DAYCARE
	specialvar VAR_RESULT, GetDaycareState
	goto_if_eq VAR_RESULT, DAYCARE_ONE_MON, Cormoria_Route117_PokemonDayCare_EventScript_CanRaiseOneMore
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain::
	msgbox Cormoria_Route117_PokemonDayCare_Text_ComeAgain, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_CanRaiseOneMore::
	msgbox Cormoria_Route117_PokemonDayCare_Text_WeCanRaiseOneMore, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_PokemonDayCare_EventScript_GiveMonToRaise
	goto Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain
	end

Cormoria_Route117_PokemonDayCare_EventScript_OnlyOneMon::
	msgbox Cormoria_Route117_PokemonDayCare_Text_YouHaveJustOneMon, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_OnlyOneAliveMon::
	msgbox Cormoria_Route117_PokemonDayCare_Text_WhatWillYouBattleWith, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_OnlyTwoAliveMons::
	msgbox Cormoria_Route117_PokemonDayCare_Text_YoullBeLeftWithJustOne, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_EggWaiting::
	msgbox Cormoria_Route117_PokemonDayCare_Text_HusbandWasLookingForYou, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_YourMonHasGrownXLevels::
	msgbox Cormoria_Route117_PokemonDayCare_Text_YourMonHasGrownXLevels, MSGBOX_DEFAULT
	return

Cormoria_Route117_PokemonDayCare_EventScript_DisplayLevelsGained::
	specialvar VAR_RESULT, GetNumLevelsGainedFromDaycare
	call_if_ne VAR_RESULT, 0, Cormoria_Route117_PokemonDayCare_EventScript_YourMonHasGrownXLevels
	return

Cormoria_Route117_PokemonDayCare_EventScript_OneMonInDaycare::
	msgbox Cormoria_Route117_PokemonDayCare_Text_GoodToSeeYou, MSGBOX_DEFAULT
	setvar VAR_0x8004, 0
	call Cormoria_Route117_PokemonDayCare_EventScript_DisplayLevelsGained
	msgbox Cormoria_Route117_PokemonDayCare_Text_WeCanRaiseOneMore, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_PokemonDayCare_EventScript_GiveMonToRaise
	msgbox Cormoria_Route117_PokemonDayCare_Text_TakeYourMonBack, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_PokemonDayCare_EventScript_TryRetrieveMon
	goto Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain
	end

Cormoria_Route117_PokemonDayCare_EventScript_TryRetrieveMon::
	specialvar VAR_RESULT, CalculatePlayerPartyCount
	goto_if_eq VAR_RESULT, PARTY_SIZE, Cormoria_Route117_PokemonDayCare_EventScript_NoRoom
	specialvar VAR_RESULT, GetDaycareState
	setvar VAR_0x8004, 0
	goto_if_eq VAR_RESULT, DAYCARE_ONE_MON, Cormoria_Route117_PokemonDayCare_EventScript_CostPrompt
	special ShowDaycareLevelMenu
	waitstate
	copyvar VAR_0x8004, VAR_RESULT
	goto_if_eq VAR_RESULT, DAYCARE_EXITED_LEVEL_MENU, Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain
	goto Cormoria_Route117_PokemonDayCare_EventScript_CostPrompt
	end

Cormoria_Route117_PokemonDayCare_EventScript_CostPrompt::
	special GetDaycareCost
	msgbox Cormoria_Route117_PokemonDayCare_Text_ItWillCostX, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_PokemonDayCare_EventScript_CheckEnoughMoney
	goto Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain
	end

Cormoria_Route117_PokemonDayCare_EventScript_CheckEnoughMoney::
	specialvar VAR_RESULT, IsEnoughForCostInVar0x8005
	goto_if_eq VAR_RESULT, 1, Cormoria_Route117_PokemonDayCare_EventScript_RetrieveMon
	msgbox Cormoria_Route117_PokemonDayCare_Text_NotEnoughMoney, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_PokemonDayCare_EventScript_RetrieveMon::
	applymovement Cormoria_scripts_day_care_LOCALID_DAYCARE_LADY, Cormoria_Route117_PokemonDayCare_Movement_RetrieveDaycareMon
	waitmovement 0
	specialvar VAR_RESULT, TakePokemonFromDaycare
	special SubtractMoneyFromVar0x8005
	playse SE_SHOP
	msgbox Cormoria_Route117_PokemonDayCare_Text_HeresYourMon, MSGBOX_DEFAULT
	waitse
	playmoncry VAR_RESULT, CRY_MODE_NORMAL
	msgbox Cormoria_Route117_PokemonDayCare_Text_TookBackMon, MSGBOX_DEFAULT
	waitmoncry
	specialvar VAR_RESULT, GetDaycareState
	goto_if_eq VAR_RESULT, DAYCARE_ONE_MON, Cormoria_Route117_PokemonDayCare_EventScript_AskRetrieveOtherMon
	goto Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain
	end

Cormoria_Route117_PokemonDayCare_EventScript_AskRetrieveOtherMon::
	msgbox Cormoria_Route117_PokemonDayCare_Text_TakeOtherOneBackToo, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_PokemonDayCare_EventScript_TryRetrieveMon
	goto Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain
	end

Cormoria_Route117_PokemonDayCare_EventScript_NoRoom::
	msgbox Cormoria_Route117_PokemonDayCare_Text_YourTeamIsFull, MSGBOX_DEFAULT
	release
	end

Cormoria_Route117_PokemonDayCare_Movement_RetrieveDaycareMon:
	delay_16
	delay_16
	face_left
	delay_16
	delay_16
	face_right
	delay_16
	delay_16
	face_up
	walk_slow_up
	set_invisible
	delay_16
	delay_16
	delay_16
	delay_16
	delay_16
	face_down
	set_visible
	walk_slow_down
	step_end

@ Unused. Possibly a commented script, or a typo end added to the above Movement script
Cormoria_Route117_PokemonDayCare_EventScript_UnusedEnd::
	end

Cormoria_Route117_PokemonDayCare_EventScript_TwoMonsInDaycare::
	msgbox Cormoria_Route117_PokemonDayCare_Text_GoodToSeeYou, MSGBOX_DEFAULT
	setvar VAR_0x8004, 0
	call Cormoria_Route117_PokemonDayCare_EventScript_DisplayLevelsGained
	setvar VAR_0x8004, 1
	call Cormoria_Route117_PokemonDayCare_EventScript_DisplayLevelsGained
	msgbox Cormoria_Route117_PokemonDayCare_Text_TakeYourMonBack, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_Route117_PokemonDayCare_EventScript_TryRetrieveMon
	msgbox Cormoria_Route117_PokemonDayCare_Text_ComeAgain, MSGBOX_DEFAULT
	release
	end

@ Unused
Cormoria_Route117_PokemonDayCare_EventScript_UnusedRetrieveMon::
	special ShowDaycareLevelMenu
	waitstate
	goto_if_eq VAR_RESULT, 2, Cormoria_Route117_PokemonDayCare_EventScript_ComeAgain
	copyvar VAR_0x8004, VAR_RESULT
	specialvar VAR_RESULT, TakePokemonFromDaycare
	msgbox Cormoria_Route117_PokemonDayCare_Text_HeresYourMon, MSGBOX_DEFAULT
	msgbox Cormoria_Route117_PokemonDayCare_Text_ComeAgain, MSGBOX_DEFAULT
	release
	end

Cormoria_EventScript_EggHatch::
	lockall
	msgbox Cormoria_Text_EggHatchHuh, MSGBOX_DEFAULT
	special EggHatch
	waitstate
	releaseall
	end

Cormoria_Route117_Text_SeeWifeIfYoudLikeMeToRaiseMon:
	.string "I'm the Ranger Day-Care Trainee.\p"
	.string "I'm training to become an expert\n"
	.string "in Pokémon growth and development!\p"
	.string "If you'd like me to raise your Pokémon,\n"
	.string "have a word with my boss inside.$"

Cormoria_Route117_Text_DoYouWantEgg:
	.string "Ah, it's you!\p"
	.string "We were raising your Pokémon,\n"
	.string "and my goodness, were we surprised!\p"
	.string "Your Pokémon had an Egg!\p"
	.string "We don't know how it got there,\n"
	.string "but your Pokémon had it.\p"
	.string "You do want it, yes?$"

Cormoria_Route117_Text_YourMonIsDoingFine:
	.string "Ah, it's you! Good to see you.\n"
	.string "Your {STR_VAR_1}'s doing fine.$"

Cormoria_Route117_Text_IllKeepIt:
	.string "Well then, I'll keep it.\n"
	.string "Thanks!$"

Cormoria_Route117_Text_YouHaveNoRoomForIt:
	.string "You have no room for it...\n"
	.string "Come back when you've made room.$"

Cormoria_Route117_Text_ReceivedEgg:
	.string "{PLAYER} received the Egg from\n"
	.string "the Ranger Day-Care Trainee.$"

Cormoria_Route117_Text_TakeGoodCareOfIt:
	.string "Take good care of it!$"

Cormoria_Route117_Text_FriendlyWithOtherTrainersMon:
	.string "By the way, about your {STR_VAR_1},\n"
	.string "it seemed to be friendly with\l"
	.string "{STR_VAR_2}'s {STR_VAR_3}.\p"
	.string "I may even have seen it receiving\n"
	.string "a piece of Mail.$"

@ Unused
Cormoria_Route117_Text_SeeWifeIfYouWantToPickUpMon:
	.string "If you want to pick up your Pokémon,\n"
	.string "have a word with my boss inside.$"

Cormoria_Route117_Text_YourMonsAreDoingFine:
	.string "Ah, it's you! Your {STR_VAR_1} and\n"
	.string "{STR_VAR_2} are doing fine.$"

Cormoria_Route117_Text_IWillKeepDoYouWantIt:
	.string "I really will keep it.\n"
	.string "You do want this, yes?$"

Cormoria_Route117_PokemonDayCare_Text_WouldYouLikeUsToRaiseAMon:
	.string "I'm the Ranger Day-Care Expert.\p"
	.string "We can raise Pokémon for you.\p"
	.string "Would you like us to raise one?$"

Cormoria_Route117_PokemonDayCare_Text_WhichMonShouldWeRaise:
	.string "Which Pokémon should we raise for\n"
	.string "you?$"

Cormoria_Route117_PokemonDayCare_Text_WellRaiseYourMon:
	.string "Fine, we'll raise your {STR_VAR_1}\n"
	.string "for a while.\p"
	.string "Come back for it later.$"

Cormoria_Route117_PokemonDayCare_Text_WeCanRaiseOneMore:
	.string "We can raise two of your Pokémon.\n"
	.string "Would you like us to raise one more?$"

Cormoria_Route117_PokemonDayCare_Text_HusbandWasLookingForYou:
	.string "The Ranger Day-Care Trainee was\n"
	.string "looking for you.$"

Cormoria_Route117_PokemonDayCare_Text_FineThenComeAgain:
	.string "Come again!$"

Cormoria_Route117_PokemonDayCare_Text_NotEnoughMoney:
	.string "You don't have enough money...$"

Cormoria_Route117_PokemonDayCare_Text_TakeOtherOneBackToo:
	.string "Will you take back the other one,\n"
	.string "too?$"

Cormoria_Route117_PokemonDayCare_Text_ComeAgain:
	.string "Come again!$"

Cormoria_Route117_PokemonDayCare_Text_GoodToSeeYou:
	.string "Ah, it's you! Good to see you.\n"
	.string "Your Pokémon can only be doing good!$"

Cormoria_Route117_PokemonDayCare_Text_YourMonHasGrownXLevels:
	.string "By level, your {STR_VAR_1} has\n"
	.string "grown by {STR_VAR_2}.$"

Cormoria_Route117_PokemonDayCare_Text_YourTeamIsFull:
	.string "Your Pokémon team is full.\n"
	.string "Make room, then come see me.$"

@ Unused
Cormoria_Route117_PokemonDayCare_Text_TakeBackWhichMon:
	.string "Which Pokémon will you take back?$"

Cormoria_Route117_PokemonDayCare_Text_ItWillCostX:
	.string "If you want your {STR_VAR_1} back,\n"
	.string "it will cost ¥{STR_VAR_2}.$"

Cormoria_Route117_PokemonDayCare_Text_HeresYourMon:
	.string "Perfect!\n"
	.string "Here's your Pokémon.$"

Cormoria_Route117_PokemonDayCare_Text_TookBackMon:
	.string "{PLAYER} took back {STR_VAR_1} from\n"
	.string "the Ranger Day-Care Expert.$"

Cormoria_Route117_PokemonDayCare_Text_YouHaveJustOneMon:
	.string "Oh? But you have just one\n"
	.string "Pokémon.\p"
	.string "Come back another time.$"

Cormoria_Route117_PokemonDayCare_Text_TakeYourMonBack:
	.string "Will you take your Pokémon back?$"

Cormoria_Route117_PokemonDayCare_Text_WhatWillYouBattleWith:
	.string "If you leave me that Pokémon,\n"
	.string "what will you battle with?\p"
	.string "Director Ravine would scold me if\n"
	.string "I left you without enough Pokémon!$"

Cormoria_Text_EggHatchHuh:
	.string "Huh?$"

Cormoria_Route117_PokemonDayCare_Text_YoullBeLeftWithJustOne:
	.string "Huh?\n"
	.string "Now, now.\p"
	.string "If you leave that Pokémon with\n"
	.string "me, you'll be left with just one.\p"
	.string "Director Ravine would scold me if\n"
	.string "I left you without enough Pokémon!$"


