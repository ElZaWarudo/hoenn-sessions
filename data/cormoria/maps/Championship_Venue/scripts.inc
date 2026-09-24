# 1 "data//maps/Championship_Venue/scripts.pory"

# 2 "data//maps/Championship_Venue/scripts.pory"
    .set Cormoria_maps_Championship_Venue_scripts_LOCALID_TEALA, 1

Cormoria_Championship_Venue_MapScripts::
	.byte 0


Cormoria_ChampionshipVenue_SignUp::
# 13 "data//maps/Championship_Venue/scripts.pory"
	goto_if_set Cormoria_FLAG_SYS_GAME_CLEAR, Cormoria_ChampionshipVenue_SignUp_2
# 19 "data//maps/Championship_Venue/scripts.pory"
	goto_if_unset Cormoria_FLAG_CHAMPIONSHIP_KOHLA_ROOM, Cormoria_ChampionshipVenue_SignUp_5
# 24 "data//maps/Championship_Venue/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMP_SIGNUP_DONE, Cormoria_ChampionshipVenue_SignUp_7
# 28 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_3, MSGBOX_YESNO
# 30 "data//maps/Championship_Venue/scripts.pory"
	compare VAR_RESULT, 0
	goto_if_eq Cormoria_ChampionshipVenue_SignUp_10
# 34 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_5
# 35 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_6, MSGBOX_SIGN
# 36 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_7
# 37 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_9
# 38 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_10
# 39 "data//maps/Championship_Venue/scripts.pory"
	playse SE_NOTE_C
# 40 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_11
# 41 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_12
# 42 "data//maps/Championship_Venue/scripts.pory"
	playse SE_NOTE_D
# 43 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_11
# 44 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_13
# 45 "data//maps/Championship_Venue/scripts.pory"
	playse SE_NOTE_E
# 46 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_11
# 47 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_14
# 48 "data//maps/Championship_Venue/scripts.pory"
	playse SE_NOTE_F
# 49 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_11
# 50 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_15
# 51 "data//maps/Championship_Venue/scripts.pory"
	playse SE_NOTE_G
# 52 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_11
# 53 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_16
# 54 "data//maps/Championship_Venue/scripts.pory"
	playse SE_NOTE_A
# 55 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_11
# 56 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_17
# 57 "data//maps/Championship_Venue/scripts.pory"
	playse SE_NOTE_B
# 58 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_11
# 59 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_18
# 60 "data//maps/Championship_Venue/scripts.pory"
	playse SE_NOTE_C_HIGH
# 61 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_11
# 62 "data//maps/Championship_Venue/scripts.pory"
	delay 16
# 63 "data//maps/Championship_Venue/scripts.pory"
	playse SE_DING_DONG
# 64 "data//maps/Championship_Venue/scripts.pory"
	msgbox_delay Cormoria_ChampionshipVenue_SignUp_Text_8, Cormoria_ChampionshipVenue_SignUp_Text_19
# 66 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_20
# 67 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_21, MSGBOX_YESNO
# 69 "data//maps/Championship_Venue/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_ChampionshipVenue_SignUp_14
Cormoria_ChampionshipVenue_SignUp_13:
# 73 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_23
# 75 "data//maps/Championship_Venue/scripts.pory"
	setflag Cormoria_FLAG_CHAMP_SIGNUP_DONE
# 76 "data//maps/Championship_Venue/scripts.pory"
	setvar Cormoria_VAR_CHAMP_STAGE, 1
# 78 "data//maps/Championship_Venue/scripts.pory"
	releaseall
	end

Cormoria_ChampionshipVenue_SignUp_2:
# 14 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_0, MSGBOX_NPC
# 15 "data//maps/Championship_Venue/scripts.pory"
	releaseall
	end

Cormoria_ChampionshipVenue_SignUp_5:
# 20 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_1, MSGBOX_NPC
	end

Cormoria_ChampionshipVenue_SignUp_7:
# 25 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_2, MSGBOX_NPC
	end

Cormoria_ChampionshipVenue_SignUp_10:
# 31 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_4, MSGBOX_NPC
	end

Cormoria_ChampionshipVenue_SignUp_14:
# 70 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_22
	goto Cormoria_ChampionshipVenue_SignUp_13


Cormoria_ChampionshipVenue_Entry::
# 87 "data//maps/Championship_Venue/scripts.pory"
	goto_if_set Cormoria_FLAG_SYS_GAME_CLEAR, Cormoria_ChampionshipVenue_Entry_2
# 93 "data//maps/Championship_Venue/scripts.pory"
	goto_if_set Cormoria_FLAG_CHAMP_SIGNUP_DONE, Cormoria_ChampionshipVenue_Entry_5
# 118 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_Entry_Text_5, MSGBOX_NPC
Cormoria_ChampionshipVenue_Entry_4:
# 121 "data//maps/Championship_Venue/scripts.pory"
	release
	end

Cormoria_ChampionshipVenue_Entry_2:
# 88 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_SignUp_Text_0, MSGBOX_NPC
# 89 "data//maps/Championship_Venue/scripts.pory"
	releaseall
	end

Cormoria_ChampionshipVenue_Entry_5:
# 94 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_Entry_Text_0, MSGBOX_YESNO
# 96 "data//maps/Championship_Venue/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_ChampionshipVenue_Entry_8
	goto Cormoria_ChampionshipVenue_Entry_4

Cormoria_ChampionshipVenue_Entry_8:
# 99 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_Entry_Text_1, MSGBOX_POKENAV
# 100 "data//maps/Championship_Venue/scripts.pory"
	dynmultichoice 10, 8, TRUE, 3, 0, DYN_MULTICHOICE_CB_NONE, Cormoria_ChampionshipVenue_Entry_Text_2, Cormoria_ChampionshipVenue_Entry_Text_3
# 102 "data//maps/Championship_Venue/scripts.pory"
	compare VAR_RESULT, 1
	goto_if_eq Cormoria_ChampionshipVenue_Entry_11
# 104 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_ChampionshipVenue_Entry_Text_4
# 105 "data//maps/Championship_Venue/scripts.pory"
	release
# 107 "data//maps/Championship_Venue/scripts.pory"
	applymovement Cormoria_maps_Championship_Venue_scripts_LOCALID_TEALA, Cormoria_ChampionshipVenue_Entry_Movement_0
# 108 "data//maps/Championship_Venue/scripts.pory"
	applymovement OBJ_EVENT_ID_PLAYER, Cormoria_ChampionshipVenue_Entry_Movement_1
# 109 "data//maps/Championship_Venue/scripts.pory"
	waitmovement 0
# 113 "data//maps/Championship_Venue/scripts.pory"
	warp MAP_CORMORIA_CHAMPIONSHIP_R1, 4, 8
	goto Cormoria_ChampionshipVenue_Entry_4

Cormoria_ChampionshipVenue_Entry_11:
	end


Cormoria_Champ_Venue_NPC1::
# 129 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_NPC2::
# 134 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_NPC2_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_NPC3::
# 139 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_NPC3_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_NPC4::
# 144 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_NPC4_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_NPC5::
# 149 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_NPC5_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_NPC6::
# 154 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_NPC6_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_Inger::
# 163 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_Inger_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_Minior::
# 167 "data//maps/Championship_Venue/scripts.pory"
	playmoncry SPECIES_MINIOR, CRY_MODE_NORMAL
# 168 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_Minior_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_Ariana::
# 172 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_Ariana_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_Tauros::
# 176 "data//maps/Championship_Venue/scripts.pory"
	playmoncry SPECIES_TAUROS, CRY_MODE_NORMAL
# 177 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_Tauros_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Venue_NoticeBoard::
# 185 "data//maps/Championship_Venue/scripts.pory"
	lockall
# 186 "data//maps/Championship_Venue/scripts.pory"
	msgbox Cormoria_Champ_Venue_NoticeBoard_Text_0
# 187 "data//maps/Championship_Venue/scripts.pory"
	releaseall
	end


# 107 "data//maps/Championship_Venue/scripts.pory"
Cormoria_ChampionshipVenue_Entry_Movement_0:
# 107 "data//maps/Championship_Venue/scripts.pory"
	walk_up
# 107 "data//maps/Championship_Venue/scripts.pory"
	walk_up
# 107 "data//maps/Championship_Venue/scripts.pory"
	walk_right
# 107 "data//maps/Championship_Venue/scripts.pory"
	face_left
	step_end

# 108 "data//maps/Championship_Venue/scripts.pory"
Cormoria_ChampionshipVenue_Entry_Movement_1:
# 108 "data//maps/Championship_Venue/scripts.pory"
	walk_up
# 108 "data//maps/Championship_Venue/scripts.pory"
	walk_up
# 108 "data//maps/Championship_Venue/scripts.pory"
	walk_up
# 108 "data//maps/Championship_Venue/scripts.pory"
	walk_up
	step_end

Cormoria_ChampionshipVenue_SignUp_Text_0:
# 14 "data//maps/Championship_Venue/scripts.pory"
	.string "Thank you for participating in this\n"
	.string "year's Championship!$"

Cormoria_ChampionshipVenue_SignUp_Text_1:
# 20 "data//maps/Championship_Venue/scripts.pory"
	.string "Welcome, trainer, to this years\n"
	.string "Cormoria Pokémon Championships!\l"
	.string "Registration will open shortly.\p"
	.string "We're waiting for all the participants\n"
	.string "to arrive at the venue.$"

Cormoria_ChampionshipVenue_SignUp_Text_2:
# 25 "data//maps/Championship_Venue/scripts.pory"
	.string "Please head to your designated battle\n"
	.string "hall through the right.\p"
	.string "Good luck, trainer!$"

Cormoria_ChampionshipVenue_SignUp_Text_3:
# 28 "data//maps/Championship_Venue/scripts.pory"
	.string "Welcome, trainer, to this years\n"
	.string "Cormoria Pokémon Championships!\p"
	.string "Are you here to register your\n"
	.string "participation?$"

Cormoria_ChampionshipVenue_SignUp_Text_4:
# 31 "data//maps/Championship_Venue/scripts.pory"
	.string "This sign-up center is for\n"
	.string "participating trainers only.\p"
	.string "If you're looking for the hotel,\n"
	.string "it's to the left of this building.\p"
	.string "Have a nice day!$"

Cormoria_ChampionshipVenue_SignUp_Text_5:
# 34 "data//maps/Championship_Venue/scripts.pory"
	.string "Very good!\p"
	.string "Firstly, I'll need your trainer card.$"

Cormoria_ChampionshipVenue_SignUp_Text_6:
# 35 "data//maps/Championship_Venue/scripts.pory"
	.string "{PLAYER} handed the attendant the\n"
	.string "Trainer Card.$"

Cormoria_ChampionshipVenue_SignUp_Text_7:
# 36 "data//maps/Championship_Venue/scripts.pory"
	.string "I'll just swipe in in here...$"

Cormoria_ChampionshipVenue_SignUp_Text_8:
# 37 "data//maps/Championship_Venue/scripts.pory"
	.string "PC$"

Cormoria_ChampionshipVenue_SignUp_Text_9:
# 37 "data//maps/Championship_Venue/scripts.pory"
	.string "PROCESSING...\p"
	.string "TRAINER NAME: {PLAYER}$"

Cormoria_ChampionshipVenue_SignUp_Text_10:
# 38 "data//maps/Championship_Venue/scripts.pory"
	.string "KING BADGE...$"

Cormoria_ChampionshipVenue_SignUp_Text_11:
# 40 "data//maps/Championship_Venue/scripts.pory"
	.string "REGISTERED.$"

Cormoria_ChampionshipVenue_SignUp_Text_12:
# 41 "data//maps/Championship_Venue/scripts.pory"
	.string "FLAME BADGE...$"

Cormoria_ChampionshipVenue_SignUp_Text_13:
# 44 "data//maps/Championship_Venue/scripts.pory"
	.string "CLUE BADGE...$"

Cormoria_ChampionshipVenue_SignUp_Text_14:
# 47 "data//maps/Championship_Venue/scripts.pory"
	.string "DRAMA BADGE...$"

Cormoria_ChampionshipVenue_SignUp_Text_15:
# 50 "data//maps/Championship_Venue/scripts.pory"
	.string "IRONFIST BADGE...$"

Cormoria_ChampionshipVenue_SignUp_Text_16:
# 53 "data//maps/Championship_Venue/scripts.pory"
	.string "CHARGE BADGE...$"

Cormoria_ChampionshipVenue_SignUp_Text_17:
# 56 "data//maps/Championship_Venue/scripts.pory"
	.string "WAVE BADGE...$"

Cormoria_ChampionshipVenue_SignUp_Text_18:
# 59 "data//maps/Championship_Venue/scripts.pory"
	.string "RUST BADGE...$"

Cormoria_ChampionshipVenue_SignUp_Text_19:
# 64 "data//maps/Championship_Venue/scripts.pory"
	.string "TRAINER {PLAYER} IS ELIGIBLE FOR\n"
	.string "THE POKEMON CHAMPIONSHIP.$"

Cormoria_ChampionshipVenue_SignUp_Text_20:
# 66 "data//maps/Championship_Venue/scripts.pory"
	.string "And we're all set!\n"
	.string "You came right on time, {PLAYER}.\p"
	.string "We have 32 spots available for trainers\n"
	.string "and you took the last one!$"

Cormoria_ChampionshipVenue_SignUp_Text_21:
# 67 "data//maps/Championship_Venue/scripts.pory"
	.string "Do you want me to explain the\n"
	.string "tournament rules to you?$"

Cormoria_ChampionshipVenue_SignUp_Text_22:
# 70 "data//maps/Championship_Venue/scripts.pory"
	.string "Alright - make sure you listen to the\n"
	.string "rules carefully, okay?\p"
	.string "The Cormoria Pokémon Championship is a\n"
	.string "single-elimination tournament where\l"
	.string "the 32 strongest trainers in Cormoria\l"
	.string "compete for the title of Champion!\p"
	.string "Each round, you'll be paired against a\n"
	.string "random opponent. If you win your\l"
	.string "battle, you advance to the next stage.\l"
	.string "If you lose, you'll be eliminated from\l"
	.string "the tournament!\p"
	.string "There are four stages - the round of\n"
	.string "32, the round of 16, the quarter-finals\l"
	.string "and the semi-finals. Make it through\l"
	.string "and you'll be in the grand finals!$"

Cormoria_ChampionshipVenue_SignUp_Text_23:
# 73 "data//maps/Championship_Venue/scripts.pory"
	.string "You can check your paired opponent on\n"
	.string "the noticeboard to the left.\p"
	.string "When you're ready, please head inside\n"
	.string "through the door to your right.\p"
	.string "Good luck!$"

Cormoria_ChampionshipVenue_Entry_Text_0:
# 94 "data//maps/Championship_Venue/scripts.pory"
	.string "Welcome trainer!\n"
	.string "Are you ready for your match?$"

Cormoria_ChampionshipVenue_Entry_Text_1:
# 99 "data//maps/Championship_Venue/scripts.pory"
	.string "Warning! You are about to enter the\n"
	.string "Pokémon Championship.\p"
	.string "If you lose at any point in the\n"
	.string "tournament, you will be eliminated\l"
	.string "and your game will auto-save.\p"
	.string "Make sure you're ready!$"

Cormoria_ChampionshipVenue_Entry_Text_2:
# 100 "data//maps/Championship_Venue/scripts.pory"
	.string "I'm ready!$"

Cormoria_ChampionshipVenue_Entry_Text_3:
# 100 "data//maps/Championship_Venue/scripts.pory"
	.string "Not yet$"

Cormoria_ChampionshipVenue_Entry_Text_4:
# 104 "data//maps/Championship_Venue/scripts.pory"
	.string "Very good. Please follow me.$"

Cormoria_ChampionshipVenue_Entry_Text_5:
# 118 "data//maps/Championship_Venue/scripts.pory"
	.string "Sorry, only participating trainers may\n"
	.string "enter past this point.\p"
	.string "Are you a trainer?\p"
	.string "Please register yourself at the\n"
	.string "front desk to your left.$"

Cormoria_Champ_Venue_NPC1_Text_0:
# 129 "data//maps/Championship_Venue/scripts.pory"
	.string "Move, kid! You're getting in the shot!$"

Cormoria_Champ_Venue_NPC2_Text_0:
# 134 "data//maps/Championship_Venue/scripts.pory"
	.string "We're trying to get an exclusive scoop\n"
	.string "with the Ace Rookie!\p"
	.string "...so get out of the way!$"

Cormoria_Champ_Venue_NPC3_Text_0:
# 139 "data//maps/Championship_Venue/scripts.pory"
	.string "I remember my first Pokémon\n"
	.string "Championship! I was a young lad, around\l"
	.string "your age. I'm old now, but the thrill is\l"
	.string "the same!$"

Cormoria_Champ_Venue_NPC4_Text_0:
# 144 "data//maps/Championship_Venue/scripts.pory"
	.string "I'm not competing - I thought this was\n"
	.string "the hotel.$"

Cormoria_Champ_Venue_NPC5_Text_0:
# 149 "data//maps/Championship_Venue/scripts.pory"
	.string "Will my son be alright? He's a bug maniac\n"
	.string "and I've always supported his dreams.\l"
	.string "But I don't know if he's as strong as\l"
	.string "the other trainers here.$"

Cormoria_Champ_Venue_NPC6_Text_0:
# 154 "data//maps/Championship_Venue/scripts.pory"
	.string "Haaah! Haaaaaaaaaah!\n"
	.string "It's showtime!$"

Cormoria_Champ_Venue_Inger_Text_0:
# 163 "data//maps/Championship_Venue/scripts.pory"
	.string "What's up {PLAYER}? Come to see your\n"
	.string "opponent details?$"

Cormoria_Champ_Venue_Minior_Text_0:
# 168 "data//maps/Championship_Venue/scripts.pory"
	.string "Minior is curling up to Inger!$"

Cormoria_Champ_Venue_Ariana_Text_0:
# 172 "data//maps/Championship_Venue/scripts.pory"
	.string "Gym Leaders aren't allowed to compete,\n"
	.string "since we're part of the system.\l"
	.string "Otherwise Raazi would win every time!$"

Cormoria_Champ_Venue_Tauros_Text_0:
# 177 "data//maps/Championship_Venue/scripts.pory"
	.string "Tauros looks at the noticeboard...\n"
	.string "It huffs, disappointed.$"

Cormoria_Champ_Venue_NoticeBoard_Text_0:
# 186 "data//maps/Championship_Venue/scripts.pory"
	.string "{PLAYER} scanned the noticeboard...\p"
	.string "...\p"
	.string "ROUND 1 PAIRINGS\n"
	.string "Kohla vs. Brent...\l"
	.string "...\l"
	.string "...Gabrielle vs. Anaya\l"
	.string "...\l"
	.string "...\l"
	.string "...Aha!\p"
	.string "{PLAYER} vs. Chara.$"
