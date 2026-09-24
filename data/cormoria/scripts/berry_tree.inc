Cormoria_BerryTreeScript::
	special ObjectEventInteractionGetBerryTreeData
	switch VAR_0x8004
	case BERRY_STAGE_SPARKLING, Cormoria_BerryTree_EventScript_Sparkling
	case BERRY_STAGE_NO_BERRY, Cormoria_BerryTree_EventScript_CheckSoil
	case BERRY_STAGE_PLANTED, Cormoria_BerryTree_EventScript_CheckBerryStage1
	case BERRY_STAGE_SPROUTED, Cormoria_BerryTree_EventScript_CheckBerryStage2
	case BERRY_STAGE_TALLER, Cormoria_BerryTree_EventScript_CheckBerryStage3
	case BERRY_STAGE_TRUNK, Cormoria_BerryTree_EventScript_CheckBerryStageTrunk
	case BERRY_STAGE_BUDDING, Cormoria_BerryTree_EventScript_CheckBerryStageBudding
	case BERRY_STAGE_FLOWERING, Cormoria_BerryTree_EventScript_CheckBerryStage4
	case BERRY_STAGE_BERRIES, Cormoria_BerryTree_EventScript_CheckBerryFullyGrown
	end

Cormoria_BerryTree_EventScript_Sparkling::
	lockall
	message Cormoria_BerryTree_Text_ExclamationPoint
	waitmessage
	waitbuttonpress
	releaseall
	end

Cormoria_BerryTree_EventScript_CheckSoil::
	lock
	faceplayer
	specialvar VAR_RESULT, PlayerHasBerries
	goto_if_eq VAR_RESULT, TRUE, Cormoria_BerryTree_EventScript_WantToPlant
	specialvar VAR_RESULT, PlayerHasMulch
	goto_if_eq VAR_RESULT, TRUE, Cormoria_BerryTree_EventScript_WantToMulch
	message Cormoria_BerryTree_Text_ItsSoftLoamySoil
	waitmessage
	waitbuttonpress
	release
	end

Cormoria_BerryTree_EventScript_WantToPlant::
	specialvar VAR_RESULT, PlayerHasMulch
	goto_if_eq VAR_RESULT, TRUE, Cormoria_BerryTree_EventScript_WantToPlantMulch
	msgbox Cormoria_BerryTree_Text_WantToPlant, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_BerryTree_EventScript_ChooseBerryToPlant
	goto_if_eq VAR_RESULT, NO, Cormoria_BerryTree_EventScript_CancelPlanting
	end

Cormoria_BerryTree_EventScript_WantToMulch::
.if OW_BERRY_MULCH_USAGE == TRUE
	msgbox Cormoria_BerryTree_Text_WantToMulch, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_BerryTree_EventScript_ChooseMulchToUse
	goto_if_eq VAR_RESULT, NO, Cormoria_BerryTree_EventScript_CancelPlanting
.endif
	end

Cormoria_BerryTree_EventScript_WantToPlantMulch::
.if OW_BERRY_MULCH_USAGE == TRUE
	message Cormoria_BerryTree_Text_ItsSoftLoamySoil
	waitmessage
	multichoice 0, 0, MULTI_BERRY_PLOT, FALSE
	switch VAR_RESULT
	case 0, Cormoria_BerryTree_EventScript_ChooseMulchToUse
	case 1, Cormoria_BerryTree_EventScript_ChooseBerryToPlant
	case 2, Cormoria_BerryTree_EventScript_CancelPlanting
	case MULTI_B_PRESSED, Cormoria_BerryTree_EventScript_CancelPlanting

Cormoria_BerryTree_EventScript_ChooseMulchToUse::
	fadescreen FADE_TO_BLACK
	closemessage
	special Bag_ChooseMulch
	waitstate
	goto_if_eq VAR_ITEM_ID, 0, Cormoria_BerryTree_EventScript_CancelPlanting
	removeitem VAR_ITEM_ID
	call Cormoria_BerryTree_EventScript_UseMulch
.endif

Cormoria_BerryTree_EventScript_ChooseBerryToPlant::
	fadescreen FADE_TO_BLACK
	closemessage
	special Bag_ChooseBerry
	waitstate
	goto_if_eq VAR_ITEM_ID, 0, Cormoria_BerryTree_EventScript_CancelPlanting
	removeitem VAR_ITEM_ID
	call Cormoria_BerryTree_EventScript_PlantBerry

Cormoria_BerryTree_EventScript_CancelPlanting::
	release
	end

Cormoria_BerryTree_EventScript_CheckBerryStage1::
	lockall
	special ObjectEventInteractionGetBerryCountString
	message Cormoria_BerryTree_Text_BerryGrowthStage1
	waitmessage
	waitbuttonpress
	goto Cormoria_BerryTree_EventScript_WantToWater

Cormoria_BerryTree_EventScript_CheckBerryStage2::
	lockall
	special ObjectEventInteractionGetBerryName
	message Cormoria_BerryTree_Text_BerryGrowthStage2
	waitmessage
	waitbuttonpress
	goto Cormoria_BerryTree_EventScript_WantToWater

Cormoria_BerryTree_EventScript_CheckBerryStage3::
	lockall
	special ObjectEventInteractionGetBerryName
	message Cormoria_BerryTree_Text_BerryGrowthStage3
	waitmessage
	waitbuttonpress
	goto Cormoria_BerryTree_EventScript_WantToWater

Cormoria_BerryTree_EventScript_CheckBerryStageTrunk::
	lockall
	special ObjectEventInteractionGetBerryName
	message Cormoria_BerryTree_Text_BerryGrowthStageTrunk
	waitmessage
	waitbuttonpress
	goto Cormoria_BerryTree_EventScript_WantToWater

Cormoria_BerryTree_EventScript_CheckBerryStageBudding::
	lockall
	special ObjectEventInteractionGetBerryName
	message Cormoria_BerryTree_Text_BerryGrowthStageBudding
	waitmessage
	waitbuttonpress
	goto Cormoria_BerryTree_EventScript_WantToWater

Cormoria_BerryTree_EventScript_CheckBerryStage4::
	call Cormoria_BerryTree_EventScript_GetCareAdverb
	lockall
	special ObjectEventInteractionGetBerryName
	message Cormoria_BerryTree_Text_BerryGrowthStage4
	waitmessage
	waitbuttonpress
	goto Cormoria_BerryTree_EventScript_WantToWater

@ VAR_0x8005 here is the number of times watered
@ Buffered by ObjectEventInteractionGetBerryTreeData
Cormoria_BerryTree_EventScript_GetCareAdverb::
	goto_if_eq VAR_0x8005, 0, Cormoria_BerryTree_EventScript_SetAdverbPoor
	goto_if_eq VAR_0x8005, 4, Cormoria_BerryTree_EventScript_SetAdverbGreat
	bufferstring STR_VAR_2, Cormoria_BerryTree_Text_CareAdverbGood
	return

Cormoria_BerryTree_EventScript_SetAdverbGreat::
	bufferstring STR_VAR_2, Cormoria_BerryTree_Text_CareAdverbGreat
	return

Cormoria_BerryTree_EventScript_SetAdverbPoor::
	bufferstring STR_VAR_2, Cormoria_BerryTree_Text_CareAdverbPoor
	return

@ VAR_0x8006 here is the number of berries
Cormoria_BerryTree_EventScript_CheckBerryFullyGrown::
	addvar VAR_0x8006, 2
	buffernumberstring STR_VAR_2, VAR_0x8006
	lock
	faceplayer
	special ObjectEventInteractionGetBerryCountString
.if OW_BERRY_MUTATIONS == TRUE
	goto_if_eq VAR_RESULT, TRUE, Cormoria_BerryTree_EventScript_CheckBerryFullyGrown_Mutation
.endif
	msgbox Cormoria_BerryTree_Text_WantToPick
	goto Cormoria_BerryTree_EventScript_PickBerry

.set BERRY_NORMAL_BAG_FULL, 0
.set BERRY_NORMAL_SPACE_IN_BAG, 1
.set BERRY_MUTATION_BAG_FULL, 2
.set BERRY_MUTATION_SPACE_IN_BAG, 3

Cormoria_BerryTree_EventScript_PickBerry::
	special ObjectEventInteractionPickBerryTree
	goto_if_eq VAR_0x8004, BERRY_NORMAL_BAG_FULL, Cormoria_BerryTree_EventScript_BerryPocketFull
.if OW_BERRY_MUTATIONS == TRUE
	goto_if_eq VAR_0x8004, BERRY_MUTATION_BAG_FULL, Cormoria_BerryTree_EventScript_BerryPocketFull_Mutation
	goto_if_eq VAR_0x8004, BERRY_MUTATION_SPACE_IN_BAG, Cormoria_BerryTree_EventScript_PickBerry_Mutation
.endif
	special IncrementDailyPickedBerries
	special ObjectEventInteractionRemoveBerryTree
	message Cormoria_BerryTree_Text_PickedTheBerry
	delay 10
	showberrydescription
	playfanfare MUS_OBTAIN_BERRY
	waitmessage
	waitfanfare
	waitbuttonpress
@	message Cormoria_BerryTree_Text_PutAwayBerry
@	waitmessage
@	waitbuttonpress
	hideitemdescription
	release
	end

Cormoria_BerryTree_EventScript_BerryPocketFull::
	message Cormoria_BerryTree_Text_BerryPocketFull
	waitmessage
	waitbuttonpress
	release
	end

Cormoria_BerryTree_EventScript_CancelPickingBerry::
	message Cormoria_BerryTree_Text_BerryLeftUnpicked
	waitmessage
	waitbuttonpress
	release
	end

Cormoria_BerryTree_EventScript_ItemUsePlantBerry::
	lockall
	special ObjectEventInteractionGetBerryTreeData
	call Cormoria_BerryTree_EventScript_PlantBerry
	releaseall
	end

Cormoria_BerryTree_EventScript_WantToWater::
.if OW_BERRY_PESTS == TRUE
	call Cormoria_BerryTree_EventScript_CheckForPests
.endif
.if OW_BERRY_WEEDS == TRUE
	call Cormoria_BerryTree_EventScript_CheckForWeed
.endif
	checkitem ITEM_WAILMER_PAIL
	goto_if_eq VAR_RESULT, FALSE, Cormoria_BerryTree_EventScript_DontWater
	specialvar VAR_RESULT, CanWaterBerryPlot
	goto_if_eq VAR_RESULT, FALSE, Cormoria_BerryTree_EventScript_DontWater
	special ObjectEventInteractionGetBerryName
	msgbox Cormoria_BerryTree_Text_WantToWater, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_BerryTree_EventScript_WaterBerry
	goto_if_eq VAR_RESULT, NO, Cormoria_BerryTree_EventScript_DontWater

Cormoria_BerryTree_EventScript_DontWater::
	releaseall
	end

Cormoria_BerryTree_EventScript_ItemUseWailmerPail::
	special ObjectEventInteractionGetBerryTreeData
	lockall
Cormoria_BerryTree_EventScript_WaterBerry::
	special ObjectEventInteractionGetBerryName
	message Cormoria_BerryTree_Text_WateredTheBerry
	waitmessage
	special ObjectEventInteractionWaterBerryTree
	special DoWateringBerryTreeAnim
	waitstate
	message Cormoria_BerryTree_Text_PlantIsDelighted
	waitmessage
	waitbuttonpress
	releaseall
	end

Cormoria_BerryTree_EventScript_PlantBerry::
	special ObjectEventInteractionPlantBerryTree
	incrementgamestat GAME_STAT_PLANTED_BERRIES
	special IncrementDailyPlantedBerries
	special ObjectEventInteractionGetBerryCountString
	message Cormoria_BerryTree_Text_PlantedOneBerry
	waitmessage
	waitbuttonpress
	return

Cormoria_BerryTree_Text_ItsSoftLoamySoil:
	.string "It's soft, loamy soil.$"

Cormoria_BerryTree_Text_WantToPlant:
	.string "It's soft, loamy soil.\n"
	.string "Want to plant a BERRY?$"

Cormoria_BerryTree_Text_PlantedOneBerry:
	.string "{PLAYER} planted one {STR_VAR_1} in\n"
	.string "the soft, loamy soil.$"

Cormoria_BerryTree_Text_BerryGrowthStage1:
.if OW_BERRY_SIX_STAGES == TRUE
	.string "This is where you planted the\n{STR_VAR_1}!$"
.else
	.string "One {STR_VAR_1} was planted here.$"
.endif

Cormoria_BerryTree_Text_BerryGrowthStage2:
.if OW_BERRY_SIX_STAGES == TRUE
	.string "The {STR_VAR_1} is sprouting!$"
.else
	.string "{STR_VAR_1} has sprouted.$"
.endif

Cormoria_BerryTree_Text_BerryGrowthStage3:
.if OW_BERRY_SIX_STAGES == TRUE
	.string "The {STR_VAR_1}'s seedling is growing well!$"
.else
	.string "This {STR_VAR_1} plant is growing taller.$"
.endif

Cormoria_BerryTree_Text_BerryGrowthStageTrunk:
	.string "The {STR_VAR_1}'s trunk is getting bigger!$"

Cormoria_BerryTree_Text_BerryGrowthStageBudding:
	.string "The {STR_VAR_1} tree has buds!$"

Cormoria_BerryTree_Text_BerryGrowthStage4:
.if OW_BERRY_SIX_STAGES == TRUE
	.string "The {STR_VAR_1} tree is in bloom!$"
.else
	.string "These {STR_VAR_1} flowers are blooming\n"
	.string "{STR_VAR_2}.$"
.endif

Cormoria_BerryTree_Text_CareAdverbGreat:
	.string "very beautifully$"

Cormoria_BerryTree_Text_CareAdverbPoor:
	.string "cutely$"

Cormoria_BerryTree_Text_CareAdverbGood:
	.string "prettily$"

Cormoria_BerryTree_Text_WantToPick:
	.string "You found {STR_VAR_2} {STR_VAR_1}!$"

Cormoria_BerryTree_Text_PickedTheBerry:
	.string "{PLAYER} picked the {STR_VAR_2} {STR_VAR_1}.$"

Cormoria_BerryTree_Text_PutAwayBerry:
	.string "{PLAYER} put away the {STR_VAR_1}\n"
	.string "in the Bag's Berries Pocket.\p"
	.string "The soil returned to its soft and\n"
	.string "loamy state.$"

Cormoria_BerryTree_Text_BerryPocketFull:
	.string "The Bag's Berries Pocket is full.\p"
	.string "The {STR_VAR_1} couldn't be taken.$"

Cormoria_BerryTree_Text_BerryLeftUnpicked:
	.string "{PLAYER} left the {STR_VAR_1}\n"
	.string "unpicked.$"

Cormoria_BerryTree_Text_WantToWater:
	.string "Want to water the {STR_VAR_1} with the\n"
	.string "WAILMER PAIL?$"

Cormoria_BerryTree_Text_WateredTheBerry:
	.string "{PLAYER} watered the {STR_VAR_1}.$"

Cormoria_BerryTree_Text_PlantIsDelighted:
	.string "The plant seems to be delighted.$"

Cormoria_BerryTree_Text_ExclamationPoint:
	.string "!$"

.if OW_BERRY_MUTATIONS == TRUE
Cormoria_BerryTree_EventScript_CheckBerryFullyGrown_Mutation:
	msgbox Cormoria_BerryTree_Text_WantToPick_Mutation, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_BerryTree_EventScript_PickBerry
	goto_if_eq VAR_RESULT, NO, Cormoria_BerryTree_EventScript_CancelPickingBerry_Mutation

Cormoria_BerryTree_EventScript_CancelPickingBerry_Mutation::
	message Cormoria_BerryTree_Text_BerryLeftUnpicked_Mutation
	waitmessage
	waitbuttonpress
	release
	end

Cormoria_BerryTree_EventScript_BerryPocketFull_Mutation::
	message Cormoria_BerryTree_Text_BerryPocketFull_Mutation
	waitmessage
	waitbuttonpress
	release
	end

Cormoria_BerryTree_EventScript_PickBerry_Mutation::
	special IncrementDailyPickedBerries
	special ObjectEventInteractionRemoveBerryTree
	message Cormoria_BerryTree_Text_PickedTheBerry_Mutation
	playfanfare MUS_OBTAIN_BERRY
	waitmessage
	waitfanfare
	waitbuttonpress
	message Cormoria_BerryTree_Text_PutAwayBerry_Mutation
	waitmessage
	waitbuttonpress
	release
	end

Cormoria_BerryTree_Text_WantToPick_Mutation:
	.string "You found {STR_VAR_2} {STR_VAR_1}\n"
	.string "and 1 {STR_VAR_3}!\p"
	.string "Do you want to pick them?$"

Cormoria_BerryTree_Text_BerryLeftUnpicked_Mutation:
	.string "{PLAYER} left the {STR_VAR_1}\n"
	.string "and the {STR_VAR_3} unpicked.$"

Cormoria_BerryTree_Text_BerryPocketFull_Mutation:
	.string "The BAG's BERRIES POCKET is full.\p"
	.string "The {STR_VAR_1} and the\n"
	.string "{STR_VAR_3} couldn't be taken.$"

Cormoria_BerryTree_Text_PickedTheBerry_Mutation:
	.string "{PLAYER} picked the {STR_VAR_2} {STR_VAR_1}\n"
	.string "and the {STR_VAR_3}.$"

Cormoria_BerryTree_Text_PutAwayBerry_Mutation:
	.string "{PLAYER} put away the {STR_VAR_1}\n"
	.string "and the {STR_VAR_3} in the BAG's\l"
	.string "BERRIES POCKET.\p"
	.string "The soil returned to its soft and\n"
	.string "loamy state.$"
.endif

.if OW_BERRY_MULCH_USAGE == TRUE
Cormoria_BerryTree_EventScript_UseMulch::
	special ObjectEventInteractionApplyMulch
	message Cormoria_BerryTree_Text_ScatteredMulch
	waitmessage
	waitbuttonpress
	return

Cormoria_BerryTree_Text_WantToMulch:
	.string "It's soft, loamy soil.\n"
	.string "Put down some fertilizer?$"

Cormoria_BerryTree_Text_ScatteredMulch:
	.string "{PLAYER} scattered the {STR_VAR_1}\n"
	.string "on the soft, loamy soil.$"
.endif

.if OW_BERRY_WEEDS == TRUE
Cormoria_BerryTree_EventScript_CheckForWeed::
	specialvar VAR_RESULT, ObjectEventInteractionBerryHasWeed
	call_if_eq VAR_RESULT, TRUE, Cormoria_BerryTree_EventScript_WeedIsGrowing
	return

Cormoria_BerryTree_EventScript_WeedIsGrowing::
	msgbox Cormoria_BerryTree_Text_WeedIsGrowing, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_BerryTree_EventScript_PullOutWeed
	return

Cormoria_BerryTree_EventScript_PullOutWeed::
	special ObjectEventInteractionPullBerryWeed
	message Cormoria_BerryTree_Text_PulledOutTheWeed
	waitmessage
	waitbuttonpress
	return

Cormoria_BerryTree_Text_WeedIsGrowing:
	.string "A weed is growing here.\n"
	.string "Do you want to pull it out?$"

Cormoria_BerryTree_Text_PulledOutTheWeed:
	.string "{PLAYER} pulled out the weed!$"
.endif

.if OW_BERRY_PESTS == TRUE
Cormoria_BerryTree_EventScript_CheckForPests::
	specialvar VAR_RESULT, ObjectEventInteractionBerryHasPests
	call_if_eq VAR_RESULT, TRUE, Cormoria_BerryTree_EventScript_EncounterPests
	return

Cormoria_BerryTree_EventScript_EncounterPests::
	message Cormoria_BerryTree_Text_APokemonAppeared
	waitmessage
	waitbuttonpress
	dowildbattle
	return

Cormoria_BerryTree_Text_APokemonAppeared:
	.string "A Pokémon appeared!$"
.endif
