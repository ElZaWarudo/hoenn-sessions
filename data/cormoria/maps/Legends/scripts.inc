Cormoria_Legends_MapScripts::
# 2 "data//maps/Legends/scripts.pory"
	map_script MAP_SCRIPT_ON_FRAME_TABLE, Cormoria_Legends_MapScripts_MAP_SCRIPT_ON_FRAME_TABLE
	.byte 0

Cormoria_Legends_MapScripts_MAP_SCRIPT_ON_FRAME_TABLE:
# 3 "data//maps/Legends/scripts.pory"
	map_script_2 VAR_TEMP_0, 0, Cormoria_Legends_SetWarp
	.2byte 0


Cormoria_Legends_SetWarp::
# 8 "data//maps/Legends/scripts.pory"
	setvar VAR_TEMP_0, 1
# 9 "data//maps/Legends/scripts.pory"
	setdynamicwarp MAP_CORMORIA_CARABRUE_TOWN, 13, 9
	return


Cormoria_Legends_Sentry::
# 13 "data//maps/Legends/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_PLAYER_SENTRY, Cormoria_Legends_Sentry_Text_0, Cormoria_Legends_Sentry_Text_1
# 14 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Sentry_Text_1, MSGBOX_NPC
	end


Cormoria_Legends_Sentry_Mon::
# 18 "data//maps/Legends/scripts.pory"
	playmoncry SPECIES_OBSTAGOON, CRY_MODE_NORMAL
# 19 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Sentry_Mon_Text_0, MSGBOX_NPC
	end


Cormoria_Legends_Eden::
# 23 "data//maps/Legends/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_PLAYER_EDEN, Cormoria_Legends_Eden_Text_0, Cormoria_Legends_Eden_Text_1
# 24 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Eden_Text_2, MSGBOX_NPC
	end


Cormoria_Legends_Eden_Mon::
# 28 "data//maps/Legends/scripts.pory"
	playmoncry SPECIES_OBSTAGOON, CRY_MODE_NORMAL
# 29 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Eden_Mon_Text_0, MSGBOX_NPC
	end


Cormoria_Legends_Gozliak::
# 33 "data//maps/Legends/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_PLAYER_GOZLIAK, Cormoria_Legends_Gozliak_Text_0, Cormoria_Legends_Gozliak_Text_1
# 34 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Gozliak_Text_1, MSGBOX_NPC
	end


Cormoria_Legends_Gozliak_Mon::
# 38 "data//maps/Legends/scripts.pory"
	playmoncry SPECIES_HYDRAPPLE, CRY_MODE_NORMAL
# 39 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Gozliak_Mon_Text_0, MSGBOX_NPC
	end


Cormoria_Legends_Sherry::
# 43 "data//maps/Legends/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_PLAYER_SHERRY, Cormoria_Legends_Sherry_Text_0, Cormoria_Legends_Sherry_Text_1
# 44 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Sherry_Text_1, MSGBOX_NPC
	end


Cormoria_Legends_Sherry_Mon::
# 48 "data//maps/Legends/scripts.pory"
	playmoncry SPECIES_GOTHITELLE, CRY_MODE_NORMAL
# 49 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Sherry_Mon_Text_0, MSGBOX_NPC
	end


Cormoria_Legends_Mike::
# 54 "data//maps/Legends/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_PLAYER_MIKE, Cormoria_Legends_Mike_Text_0, Cormoria_Legends_Mike_Text_1
# 55 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Mike_Text_1, MSGBOX_NPC
	end


Cormoria_Legends_Mike_Mon::
# 59 "data//maps/Legends/scripts.pory"
	playmoncry SPECIES_GOTHITELLE, CRY_MODE_NORMAL
# 60 "data//maps/Legends/scripts.pory"
	msgbox Cormoria_Legends_Mike_Mon_Text_0, MSGBOX_NPC
	end


Cormoria_Legends_Sentry_Text_0:
# 13 "data//maps/Legends/scripts.pory"
	.string "I am the first player to complete\n"
	.string "Pokémon Dreamstone Mysteries.\p"
	.string "Come, {PLAYER}.\p"
	.string "Stand before me.\p"
	.string "Duel me for my throne.\p"
	.string "And if you think you deserve it,\n"
	.string "earn the right to my crown!$"

Cormoria_Legends_Sentry_Text_1:
# 13 "data//maps/Legends/scripts.pory"
	.string "My legacy is in good hands.$"

Cormoria_Legends_Sentry_Mon_Text_0:
# 19 "data//maps/Legends/scripts.pory"
	.string "It's Sentry's Obstagoon.$"

Cormoria_Legends_Eden_Text_0:
# 23 "data//maps/Legends/scripts.pory"
	.string "I am the second player to complete\n"
	.string "Pokémon Dreamstone Mysteries.\p"
	.string "After me lies your last challenge.\n"
	.string "But will you ever reach it?$"

Cormoria_Legends_Eden_Text_1:
# 23 "data//maps/Legends/scripts.pory"
	.string "I expected nothing else!$"

Cormoria_Legends_Eden_Text_2:
# 24 "data//maps/Legends/scripts.pory"
	.string "I expected nothing less!$"

Cormoria_Legends_Eden_Mon_Text_0:
# 29 "data//maps/Legends/scripts.pory"
	.string "It's Eden's Obstagoon.$"

Cormoria_Legends_Gozliak_Text_0:
# 33 "data//maps/Legends/scripts.pory"
	.string "I am the third player to complete\n"
	.string "Pokémon Dreamstone Mysteries.\p"
	.string "I have Pokémon that you could only\n"
	.string "dream of catching!$"

Cormoria_Legends_Gozliak_Text_1:
# 33 "data//maps/Legends/scripts.pory"
	.string "You have an impressive team!$"

Cormoria_Legends_Gozliak_Mon_Text_0:
# 39 "data//maps/Legends/scripts.pory"
	.string "It's Gozliak's Hydrapple.$"

Cormoria_Legends_Sherry_Text_0:
# 43 "data//maps/Legends/scripts.pory"
	.string "I am the fourth player to complete\n"
	.string "Pokémon Dreamstone Mysteries.\p"
	.string "Can you match up to my power?$"

Cormoria_Legends_Sherry_Text_1:
# 43 "data//maps/Legends/scripts.pory"
	.string "You're one of us!$"

Cormoria_Legends_Sherry_Mon_Text_0:
# 49 "data//maps/Legends/scripts.pory"
	.string "It's Sherry's Gothitelle.$"

Cormoria_Legends_Mike_Text_0:
# 54 "data//maps/Legends/scripts.pory"
	.string "I am the fifth player to complete\n"
	.string "Pokémon Dreamstone Mysteries.\p"
	.string "You may have won the Championship, but\n"
	.string "you're up against a true Champion now!$"

Cormoria_Legends_Mike_Text_1:
# 54 "data//maps/Legends/scripts.pory"
	.string "Well done, young Champion!$"

Cormoria_Legends_Mike_Mon_Text_0:
# 60 "data//maps/Legends/scripts.pory"
	.string "It's Mike's Gothitelle.$"
