Cormoria_GastreeGym_MapScripts::
	.byte 0


Cormoria_GastreeGym_TrainerBattle_Skip::
# 6 "data//maps/GastreeGym/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_GASTREEGYM_SKIP, Cormoria_GastreeGym_TrainerBattle_Skip_Text_0, Cormoria_GastreeGym_TrainerBattle_Skip_Text_1
# 7 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_TrainerBattle_Skip_Text_2
# 8 "data//maps/GastreeGym/scripts.pory"
	release
	end


Cormoria_GastreeGym_TrainerBattle_Jaco::
# 15 "data//maps/GastreeGym/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_GASTREEGYM_JACO, Cormoria_GastreeGym_TrainerBattle_Jaco_Text_0, Cormoria_GastreeGym_TrainerBattle_Jaco_Text_1
# 16 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_TrainerBattle_Jaco_Text_2
# 17 "data//maps/GastreeGym/scripts.pory"
	release
	end


Cormoria_GastreeGym_LeaderBattle::
# 23 "data//maps/GastreeGym/scripts.pory"
	goto_if_set Cormoria_FLAG_BADGE01_GET, Cormoria_GastreeGym_LeaderBattle_2
# 30 "data//maps/GastreeGym/scripts.pory"
	call Cormoria_Common_EventScript_SaveGameBeforeBattle
# 32 "data//maps/GastreeGym/scripts.pory"
	fadenewbgm MUS_RG_GYM
# 34 "data//maps/GastreeGym/scripts.pory"
	lockall
# 35 "data//maps/GastreeGym/scripts.pory"
	speakername Cormoria_GastreeGym_LeaderBattle_Text_1
# 36 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_LeaderBattle_Text_2
# 37 "data//maps/GastreeGym/scripts.pory"
	release
# 38 "data//maps/GastreeGym/scripts.pory"
	applymovement 9, Cormoria_GastreeGym_LeaderBattle_Movement_0
# 39 "data//maps/GastreeGym/scripts.pory"
	waitmovement 0
# 40 "data//maps/GastreeGym/scripts.pory"
	delay 24
# 42 "data//maps/GastreeGym/scripts.pory"
	speakername Cormoria_GastreeGym_LeaderBattle_Text_1
# 43 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_LeaderBattle_Text_3, MSGBOX_NPC
# 44 "data//maps/GastreeGym/scripts.pory"
	release
# 45 "data//maps/GastreeGym/scripts.pory"
	applymovement 9, Cormoria_GastreeGym_LeaderBattle_Movement_1
# 46 "data//maps/GastreeGym/scripts.pory"
	waitmovement 0
# 47 "data//maps/GastreeGym/scripts.pory"
	delay 24
# 48 "data//maps/GastreeGym/scripts.pory"
	speakername Cormoria_GastreeGym_LeaderBattle_Text_1
# 49 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_Text_Intro
# 50 "data//maps/GastreeGym/scripts.pory"
	trainerbattle_no_intro Cormoria_TRAINER_GASTREEGYM_LEADER, Cormoria_GastreeGym_Text_Defeated
# 51 "data//maps/GastreeGym/scripts.pory"
	goto Cormoria_GastreeGym_EventScript_Victory
# 52 "data//maps/GastreeGym/scripts.pory"
	release
Cormoria_GastreeGym_LeaderBattle_1:
	end

Cormoria_GastreeGym_LeaderBattle_2:
# 24 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_LeaderBattle_Text_0, MSGBOX_NPC
	goto Cormoria_GastreeGym_LeaderBattle_1


Cormoria_GastreeGym_EventScript_Victory::
# 60 "data//maps/GastreeGym/scripts.pory"
	setflag Cormoria_FLAG_BADGE01_GET
# 62 "data//maps/GastreeGym/scripts.pory"
	message Cormoria_GastreeGym_EventScript_Victory_Text_0
# 64 "data//maps/GastreeGym/scripts.pory"
	call Cormoria_Common_EventScript_PlayGymBadgeFanfare
# 65 "data//maps/GastreeGym/scripts.pory"
	release
# 66 "data//maps/GastreeGym/scripts.pory"
	playbgm MUS_RG_GYM, FALSE
# 69 "data//maps/GastreeGym/scripts.pory"
	speakername Cormoria_GastreeGym_LeaderBattle_Text_1
# 70 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_EventScript_Victory_Text_1
# 71 "data//maps/GastreeGym/scripts.pory"
	release
# 72 "data//maps/GastreeGym/scripts.pory"
	delay 16
# 74 "data//maps/GastreeGym/scripts.pory"
	speakername Cormoria_GastreeGym_LeaderBattle_Text_1
# 75 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_EventScript_Victory_Text_2
# 76 "data//maps/GastreeGym/scripts.pory"
	speakername NULL
# 77 "data//maps/GastreeGym/scripts.pory"
	giveitem ITEM_RARE_SHARD
# 78 "data//maps/GastreeGym/scripts.pory"
	speakername Cormoria_GastreeGym_LeaderBattle_Text_1
# 79 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_EventScript_Victory_Text_3
# 80 "data//maps/GastreeGym/scripts.pory"
	release
# 81 "data//maps/GastreeGym/scripts.pory"
	delay 16
# 83 "data//maps/GastreeGym/scripts.pory"
	speakername Cormoria_GastreeGym_LeaderBattle_Text_1
# 84 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_EventScript_Victory_Text_4
# 85 "data//maps/GastreeGym/scripts.pory"
	releaseall
	return


Cormoria_GastreeGym_Red::
# 104 "data//maps/GastreeGym/scripts.pory"
	goto_if_set Cormoria_FLAG_BADGE01_GET, Cormoria_GastreeGym_Red_2
# 116 "data//maps/GastreeGym/scripts.pory"
	goto_if_set Cormoria_FLAG_GASTREEGYM_SPENSER_WATER, Cormoria_GastreeGym_Red_8
# 119 "data//maps/GastreeGym/scripts.pory"
	lockall
# 120 "data//maps/GastreeGym/scripts.pory"
	faceplayer
# 121 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_Red_Text_3
# 122 "data//maps/GastreeGym/scripts.pory"
	giveitem ITEM_FRESH_WATER
# 123 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_Red_Text_4
# 124 "data//maps/GastreeGym/scripts.pory"
	setflag Cormoria_FLAG_GASTREEGYM_SPENSER_WATER
# 125 "data//maps/GastreeGym/scripts.pory"
	clearflag Cormoria_FLAG_HIDE_ROUTE3_UNDERPASS_GYM
# 126 "data//maps/GastreeGym/scripts.pory"
	releaseall
Cormoria_GastreeGym_Red_1:
	end

Cormoria_GastreeGym_Red_2:
# 105 "data//maps/GastreeGym/scripts.pory"
	goto_if_set Cormoria_FLAG_GASTREEGYM_SPENSER_REWARD, Cormoria_GastreeGym_Red_5
# 109 "data//maps/GastreeGym/scripts.pory"
	lockall
# 110 "data//maps/GastreeGym/scripts.pory"
	faceplayer
# 112 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_Red_Text_1
# 113 "data//maps/GastreeGym/scripts.pory"
	goto Cormoria_GastreeGym_Red_Reward
	goto Cormoria_GastreeGym_Red_1

Cormoria_GastreeGym_Red_5:
# 106 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_Red_Text_0, MSGBOX_NPC
	end

Cormoria_GastreeGym_Red_8:
# 117 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_Red_Text_2, MSGBOX_NPC
	goto Cormoria_GastreeGym_Red_1


Cormoria_GastreeGym_Red_Reward::
# 133 "data//maps/GastreeGym/scripts.pory"
	message Cormoria_GastreeGym_Red_Reward_Text_0
# 134 "data//maps/GastreeGym/scripts.pory"
	waitmessage
# 135 "data//maps/GastreeGym/scripts.pory"
	dynmultichoice 0, 0, TRUE, 2, 0, DYN_MULTICHOICE_CB_NONE, Cormoria_GastreeGym_Red_Reward_Text_1, Cormoria_GastreeGym_Red_Reward_Text_2
# 136 "data//maps/GastreeGym/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_GastreeGym_Red_Reward_2
Cormoria_GastreeGym_Red_Reward_1:
# 137 "data//maps/GastreeGym/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_GastreeGym_Red_Reward_5
Cormoria_GastreeGym_Red_Reward_4:
# 138 "data//maps/GastreeGym/scripts.pory"
	goto Cormoria_GastreeGym_Red_Reward_Give
	return

Cormoria_GastreeGym_Red_Reward_2:
# 136 "data//maps/GastreeGym/scripts.pory"
	setvar VAR_0x8006, ITEM_ROCK_GEM
	goto Cormoria_GastreeGym_Red_Reward_1

Cormoria_GastreeGym_Red_Reward_5:
# 137 "data//maps/GastreeGym/scripts.pory"
	setvar VAR_0x8006, ITEM_HARD_STONE
	goto Cormoria_GastreeGym_Red_Reward_4


Cormoria_GastreeGym_Red_Reward_Give::
# 142 "data//maps/GastreeGym/scripts.pory"
	showitemdescription
# 143 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_Red_Reward_Give_Text_0, MSGBOX_YESNO
# 144 "data//maps/GastreeGym/scripts.pory"
	hideitemdescription
# 145 "data//maps/GastreeGym/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_GastreeGym_Red_Reward_Give_1
# 150 "data//maps/GastreeGym/scripts.pory"
	goto Cormoria_GastreeGym_Red_Reward
	return

Cormoria_GastreeGym_Red_Reward_Give_1:
# 146 "data//maps/GastreeGym/scripts.pory"
	giveitem VAR_0x8006
# 147 "data//maps/GastreeGym/scripts.pory"
	setflag Cormoria_FLAG_GASTREEGYM_SPENSER_REWARD
# 148 "data//maps/GastreeGym/scripts.pory"
	goto Cormoria_GastreeGym_Red_Reward_End
	return


Cormoria_GastreeGym_Red_Reward_End::
# 155 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_Red_Reward_End_Text_0, MSGBOX_NPC
	end


Cormoria_GastreeGym_WeatheredRock::
# 178 "data//maps/GastreeGym/scripts.pory"
	msgbox Cormoria_GastreeGym_WeatheredRock_Text_0, MSGBOX_SIGN
	end


# 38 "data//maps/GastreeGym/scripts.pory"
Cormoria_GastreeGym_LeaderBattle_Movement_0:
# 38 "data//maps/GastreeGym/scripts.pory"
	jump_in_place_down
# 38 "data//maps/GastreeGym/scripts.pory"
	emote_exclamation_mark
	step_end

# 45 "data//maps/GastreeGym/scripts.pory"
Cormoria_GastreeGym_LeaderBattle_Movement_1:
# 45 "data//maps/GastreeGym/scripts.pory"
	emote_question_mark
	step_end

Cormoria_GastreeGym_TrainerBattle_Skip_Text_0:
# 6 "data//maps/GastreeGym/scripts.pory"
	.string "Hold up - nobody's allowed to disturb\n"
	.string "the Kinger while he's training!\p"
	.string "Who's the Kinger?\p"
	.string "Inger the King, of course!$"

Cormoria_GastreeGym_TrainerBattle_Skip_Text_1:
# 6 "data//maps/GastreeGym/scripts.pory"
	.string "Okay, go disturb the Kinger!$"

Cormoria_GastreeGym_TrainerBattle_Skip_Text_2:
# 7 "data//maps/GastreeGym/scripts.pory"
	.string "The Kinger is what everyone should\n"
	.string "aspire to be like!$"

Cormoria_GastreeGym_TrainerBattle_Jaco_Text_0:
# 15 "data//maps/GastreeGym/scripts.pory"
	.string "Hey, did the old man send you here?\n"
	.string "The King is training - get out!$"

Cormoria_GastreeGym_TrainerBattle_Jaco_Text_1:
# 15 "data//maps/GastreeGym/scripts.pory"
	.string "Fine, don't get out!$"

Cormoria_GastreeGym_TrainerBattle_Jaco_Text_2:
# 16 "data//maps/GastreeGym/scripts.pory"
	.string "Inger the King hates losing and trains\n"
	.string "really hard. That's why I've always got\l"
	.string "his back...well, except for losing to you.$"

Cormoria_GastreeGym_LeaderBattle_Text_0:
# 24 "data//maps/GastreeGym/scripts.pory"
	.string "I hate losing! Argh! But that was a good\n"
	.string "battle, {PLAYER}. Go get Professor\l"
	.string "Tenebris back for us all!$"

Cormoria_GastreeGym_LeaderBattle_Text_1:
# 35 "data//maps/GastreeGym/scripts.pory"
	.string "Inger$"

Cormoria_GastreeGym_LeaderBattle_Text_2:
# 36 "data//maps/GastreeGym/scripts.pory"
	.string "C'mon! C'mon!!\p"
	.string "...\p"
	.string "C'moooooooo--$"

Cormoria_GastreeGym_LeaderBattle_Text_3:
# 43 "data//maps/GastreeGym/scripts.pory"
	.string "Who're you?\p"
	.string "A trainer?\p"
	.string "You're coming to me at the wrong time,\n"
	.string "buddy, because I just lost to some\l"
	.string "child your age and I'm furious!\p"
	.string "...$"

Cormoria_GastreeGym_EventScript_Victory_Text_0:
# 62 "data//maps/GastreeGym/scripts.pory"
	.string "{PLAYER} received the King Badge\n"
	.string "from Inger.$"

Cormoria_GastreeGym_EventScript_Victory_Text_1:
# 70 "data//maps/GastreeGym/scripts.pory"
	.string "The King Badge allows you to use HM\n"
	.string "Splash outside of battle. Hop and skip\l"
	.string "across all the ponds of Cormoria!$"

Cormoria_GastreeGym_EventScript_Victory_Text_2:
# 75 "data//maps/GastreeGym/scripts.pory"
	.string "I also want to give you this.$"

Cormoria_GastreeGym_EventScript_Victory_Text_3:
# 79 "data//maps/GastreeGym/scripts.pory"
	.string "This is an incredibly rare stone that\n"
	.string "reacts in the prescence of an ancient\l"
	.string "statue. Any Pokémon you give it to can\l"
	.string "maximise one of its innate stats!\p"
	.string "Go to an ancient statue to try it.\n"
	.string "There's one in Gastree City!$"

Cormoria_GastreeGym_EventScript_Victory_Text_4:
# 84 "data//maps/GastreeGym/scripts.pory"
	.string "Where are you headed next?\p"
	.string "...\p"
	.string "So you're investigating Professor\n"
	.string "Tenebris' disappearance? Good luck!\l"
	.string "She's the one who taught my Minior\l"
	.string "the move Rollout.\p"
	.string "...\p"
	.string "...but I can't say I've heard anything\n"
	.string "about these dreamstones.\p"
	.string "So next is Mt. Ceram? You'll have to go\n"
	.string "north from here through Route 4.\p"
	.string "If its for the Professor, I would've\n"
	.string "loved to join, but I've got a gym to run.\p"
	.string "Go get 'em, {PLAYER}! Teach Team\n"
	.string "Somber a lesson on the Kinger's behalf!$"

Cormoria_GastreeGym_Red_Text_0:
# 106 "data//maps/GastreeGym/scripts.pory"
	.string "Well done champ-in-the-making! Now get\n"
	.string "on out there and find the next gym -\l"
	.string "I'll see you there!$"

Cormoria_GastreeGym_Red_Text_1:
# 112 "data//maps/GastreeGym/scripts.pory"
	.string "Well done champ-in-the-making! Inger\n"
	.string "told me about your battle.\l"
	.string "In recognition of your skills, I'd like\l"
	.string "to give you something.$"

Cormoria_GastreeGym_Red_Text_2:
# 117 "data//maps/GastreeGym/scripts.pory"
	.string "Welcome to the Gastree gym!\n"
	.string "Unfortunately the leader, Inger, has\l"
	.string "gone off to do some emergency training\l"
	.string "in the underpass to Fennilahl Town.\p"
	.string "The previous trainer beat him and Inger\n"
	.string "just can't stand losing!.$"

Cormoria_GastreeGym_Red_Text_3:
# 121 "data//maps/GastreeGym/scripts.pory"
	.string "Welcome to the Gastree gym!\n"
	.string "Unfortunately the leader, Inger, has\l"
	.string "gone off to do some emergency training\l"
	.string "in the underpass to Fennilahl Town.\p"
	.string "The previous trainer beat him and Inger\n"
	.string "just can't stand losing!\p"
	.string "Before you run off, take this.$"

Cormoria_GastreeGym_Red_Text_4:
# 123 "data//maps/GastreeGym/scripts.pory"
	.string "Inger loves ROCK type Pokémon, so avoid\n"
	.string "using FLYING and BUG type Pokémon.\p"
	.string "Now go get 'em, champ-in-the-making!$"

Cormoria_GastreeGym_Red_Reward_Text_0:
# 133 "data//maps/GastreeGym/scripts.pory"
	.string "Which of these do you fancy?$"

Cormoria_GastreeGym_Red_Reward_Text_1:
# 135 "data//maps/GastreeGym/scripts.pory"
	.string "Rock Gem$"

Cormoria_GastreeGym_Red_Reward_Text_2:
# 135 "data//maps/GastreeGym/scripts.pory"
	.string "Hard Stone$"

Cormoria_GastreeGym_Red_Reward_Give_Text_0:
# 143 "data//maps/GastreeGym/scripts.pory"
	.string "Are you sure?$"

Cormoria_GastreeGym_Red_Reward_End_Text_0:
# 155 "data//maps/GastreeGym/scripts.pory"
	.string "Now get on out there and find the next\n"
	.string "gym - I'll see you there!$"

Cormoria_GastreeGym_WeatheredRock_Text_0:
# 178 "data//maps/GastreeGym/scripts.pory"
	.string "This rock looks like it's been weathered\n"
	.string "down by constant force.$"

Cormoria_GastreeGym_Text_Intro::
# 88 "data//maps/GastreeGym/scripts.pory"
	.string "...you look a little green. It is nerves?\n"
	.string "First gym battle?\p"
	.string "...\p"
	.string "I know that feeling, and any other day,\n"
	.string "I might've gone easy on you...\p"
	.string "...but not today!\p"
	.string "I'm Inger, master of ROCK-types and the\n"
	.string "unshakable strength of the earth.\p"
	.string "My ROCK-type Pokémon have seen\n"
	.string "countless battles, and they never\l"
	.string "crumble under pressure. If you think\l"
	.string "you can break through, show me the\l"
	.string "strength that you bring with you!$"

Cormoria_GastreeGym_Text_Defeated::
# 92 "data//maps/GastreeGym/scripts.pory"
	.string "No way! Two defeats in a day?$"
