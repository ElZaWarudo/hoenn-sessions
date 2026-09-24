Cormoria_Roulette_EventScript_Table1::
	checkitem ITEM_COIN_CASE
	goto_if_eq VAR_RESULT, FALSE, Cormoria_MauvilleCity_GameCorner_EventScript_NoCoinCase
	setvar VAR_0x8004, 0
	getpokenewsactive POKENEWS_GAME_CORNER
	goto_if_eq VAR_RESULT, FALSE, Cormoria_Roulette_EventScript_Play
	addvar VAR_0x8004, ROULETTE_SPECIAL_RATE
	goto Cormoria_Roulette_EventScript_Play
	end

Cormoria_Roulette_EventScript_Table2::
	checkitem ITEM_COIN_CASE
	goto_if_eq VAR_RESULT, FALSE, Cormoria_MauvilleCity_GameCorner_EventScript_NoCoinCase
	setvar VAR_0x8004, 1
	getpokenewsactive POKENEWS_GAME_CORNER
	goto_if_eq VAR_RESULT, FALSE, Cormoria_Roulette_EventScript_Play
	addvar VAR_0x8004, ROULETTE_SPECIAL_RATE
	goto Cormoria_Roulette_EventScript_Play
	end

Cormoria_Roulette_EventScript_Play::
	special PlayRoulette
	waitstate
	end

Cormoria_Roulette_Text_PlayMinimumWagerIsX::
	.string "The minimum wager at this table\n"
	.string "is {STR_VAR_1}. Do you want to play?$"

Cormoria_Roulette_Text_NotEnoughCoins::
	.string "You don't have enough COINS.$"

Cormoria_Roulette_Text_SpecialRateTable::
	.string "Special rate table right now!$"

Cormoria_Roulette_Text_ControlsInstruction::
	.string "Place your wager with the + Control\n"
	.string "Pad, then press the A Button.$"

Cormoria_Roulette_Text_ItsAHit::
	.string "It's a hit!$"

Cormoria_Roulette_Text_Jackpot::
	.string "Jackpot!$"

Cormoria_Roulette_Text_NothingDoing::
	.string "Nothing doing!$"

Cormoria_Roulette_Text_YouveWonXCoins::
	.string "You've won {STR_VAR_1} COINS!$"

Cormoria_Roulette_Text_NoCoinsLeft::
	.string "No COINS left…$"

Cormoria_Roulette_Text_KeepPlaying::
	.string "Keep playing?$"

Cormoria_Roulette_Text_BoardWillBeCleared::
	.string "The ROULETTE board will be cleared.$"

@ Unused
Cormoria_Roulette_Text_YouDontHaveACoinCase::
	.string "You don't have a COIN CASE.$"

Cormoria_Roulette_Text_CoinCaseIsFull::
	.string "Your COIN CASE is full!\n"
	.string "Coins can be exchanged for prizes.$"
