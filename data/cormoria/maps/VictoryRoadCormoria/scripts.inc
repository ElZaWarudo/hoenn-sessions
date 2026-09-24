Cormoria_VictoryRoadCormoria_MapScripts::
# 2 "data//maps/VictoryRoadCormoria/scripts.pory"
	map_script MAP_SCRIPT_ON_RESUME, Cormoria_VictoryRoadCormoria_MapScripts_MAP_SCRIPT_ON_RESUME
	.byte 0

Cormoria_VictoryRoadCormoria_MapScripts_MAP_SCRIPT_ON_RESUME:
# 2 "data//maps/VictoryRoadCormoria/scripts.pory"
	setstepcallback STEP_CB_ASH
	return


Cormoria_VR_SetWeather_Normal::
# 7 "data//maps/VictoryRoadCormoria/scripts.pory"
	setweather WEATHER_NONE
# 8 "data//maps/VictoryRoadCormoria/scripts.pory"
	doweather
# 9 "data//maps/VictoryRoadCormoria/scripts.pory"
	releaseall
	end


Cormoria_VR_SetWeather_Shade::
# 13 "data//maps/VictoryRoadCormoria/scripts.pory"
	setweather WEATHER_SHADE
# 14 "data//maps/VictoryRoadCormoria/scripts.pory"
	doweather
# 15 "data//maps/VictoryRoadCormoria/scripts.pory"
	releaseall
	end


Cormoria_VR_SetWeather_Rain::
# 19 "data//maps/VictoryRoadCormoria/scripts.pory"
	setweather WEATHER_RAIN
# 20 "data//maps/VictoryRoadCormoria/scripts.pory"
	doweather
# 21 "data//maps/VictoryRoadCormoria/scripts.pory"
	releaseall
	end


Cormoria_VR_Trainer_A::
# 33 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_A, Cormoria_VR_Trainer_A_Text_0, Cormoria_VR_Trainer_A_Text_1
# 34 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_A_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_B::
# 39 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_B, Cormoria_VR_Trainer_B_Text_0, Cormoria_VR_Trainer_B_Text_1
# 40 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_B_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_C::
# 45 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_C, Cormoria_VR_Trainer_C_Text_0, Cormoria_VR_Trainer_C_Text_1
# 46 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_C_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_D::
# 51 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_D, Cormoria_VR_Trainer_D_Text_0, Cormoria_VR_Trainer_D_Text_1
# 52 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_D_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_E::
# 57 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_E, Cormoria_VR_Trainer_E_Text_0, Cormoria_VR_Trainer_E_Text_1
# 58 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_E_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_F::
# 63 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_F, Cormoria_VR_Trainer_F_Text_0, Cormoria_VR_Trainer_F_Text_1
# 64 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_F_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_G::
# 69 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_G, Cormoria_VR_Trainer_G_Text_0, Cormoria_VR_Trainer_G_Text_1
# 70 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_G_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_H::
# 75 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_H, Cormoria_VR_Trainer_H_Text_0, Cormoria_VR_Trainer_H_Text_1
# 76 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_H_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_I::
# 81 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_I, Cormoria_VR_Trainer_I_Text_0, Cormoria_VR_Trainer_I_Text_1
# 82 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_I_Text_2, MSGBOX_NPC
	end


Cormoria_VR_Trainer_J::
# 88 "data//maps/VictoryRoadCormoria/scripts.pory"
	trainerbattle_single Cormoria_TRAINER_VICTORY_ROAD_J, Cormoria_VR_Trainer_J_Text_0, Cormoria_VR_Trainer_J_Text_0
# 89 "data//maps/VictoryRoadCormoria/scripts.pory"
	msgbox Cormoria_VR_Trainer_J_Text_0, MSGBOX_NPC
	end


Cormoria_VR_Trainer_A_Text_0:
# 33 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Headed for Victory Cape? Too bad - you\n"
	.string "won't be making it further than here!$"

Cormoria_VR_Trainer_A_Text_1:
# 33 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I guess I won't be making it farther\n"
	.string "than here!$"

Cormoria_VR_Trainer_A_Text_2:
# 34 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "The trainers on this route are some of\n"
	.string "the toughest!$"

Cormoria_VR_Trainer_B_Text_0:
# 39 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "You know why I'm standing here?\p"
	.string "To get rid of some of the competition\n"
	.string "right away!\p"
	.string "I'm the first member of the Crush Crew\n"
	.string "and I'm ending your journey right here!$"

Cormoria_VR_Trainer_B_Text_1:
# 39 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I can't get rid of you!$"

Cormoria_VR_Trainer_B_Text_2:
# 40 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Hmpf. I'll see you at Victory Cape.$"

Cormoria_VR_Trainer_C_Text_0:
# 45 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "My Pokémon and I have trained all over\n"
	.string "Cormoria. You're not beating us!$"

Cormoria_VR_Trainer_C_Text_1:
# 45 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "You beat us!$"

Cormoria_VR_Trainer_C_Text_2:
# 46 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "The talent coming to the Championship\n"
	.string "sure is on another level...$"

Cormoria_VR_Trainer_D_Text_0:
# 51 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Hah! That's your team?\p"
	.string "Stupid kid...I hate kids like you who\n"
	.string "think that type variety matters.\p"
	.string "It's all about raw power! And DRAGON\n"
	.string "type Pokémon are the strongest!$"

Cormoria_VR_Trainer_D_Text_1:
# 51 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "No! My dragons!$"

Cormoria_VR_Trainer_D_Text_2:
# 52 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I need to find more DRAGON types!$"

Cormoria_VR_Trainer_E_Text_0:
# 57 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Think you're making it to the Pokémon\n"
	.string "Championship?\p"
	.string "Think again! I'm the second member of\n"
	.string "the Crush Crew and your journey will end\l"
	.string "with me!$"

Cormoria_VR_Trainer_E_Text_1:
# 57 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Your journey doesn't end with me!$"

Cormoria_VR_Trainer_E_Text_2:
# 58 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "The Crush Crew's leader paid us to\n"
	.string "eliminate the competition for him.$"

Cormoria_VR_Trainer_F_Text_0:
# 63 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "The Championship will know the strength\n"
	.string "of the mountains!$"

Cormoria_VR_Trainer_F_Text_1:
# 63 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Is the strength of the mountains not\n"
	.string "enough?$"

Cormoria_VR_Trainer_F_Text_2:
# 64 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I'm in shock - I thought my mountain\n"
	.string "Pokémon were unbeatable!$"

Cormoria_VR_Trainer_G_Text_0:
# 69 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I'm the strongest Pokémon Breeder in\n"
	.string "all of Cormoria!\p"
	.string "Can you handle the storm?$"

Cormoria_VR_Trainer_G_Text_1:
# 69 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "You can handle the storm!$"

Cormoria_VR_Trainer_G_Text_2:
# 70 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I don't just love my birds like other\n"
	.string "breeders...I train them to be the\l"
	.string "strongest!$"

Cormoria_VR_Trainer_H_Text_0:
# 75 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I thought I could find a shortcut to\n"
	.string "Victory Cape, but I'm lost and just keep\l"
	.string "losing to these tough trainers...oh no,\l"
	.string "don't tell me you're another one!$"

Cormoria_VR_Trainer_H_Text_1:
# 75 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Why did we have to lock eyes?$"

Cormoria_VR_Trainer_H_Text_2:
# 76 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Just let me get to Victory Cape\n"
	.string "already!$"

Cormoria_VR_Trainer_I_Text_0:
# 81 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "Didn't my crew crush you?\p"
	.string "Hmpf. Useless fools.\p"
	.string "I'm going to do everything it takes to\n"
	.string "win the Pokémon Championship, and\l"
	.string "money is no object! Hahaha!$"

Cormoria_VR_Trainer_I_Text_1:
# 81 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I spent so much money on Charizard!$"

Cormoria_VR_Trainer_I_Text_2:
# 82 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "I guess money can't beat skill. I should\n"
	.string "invest in training for the next\l"
	.string "Championship.$"

Cormoria_VR_Trainer_J_Text_0:
# 88 "data//maps/VictoryRoadCormoria/scripts.pory"
	.string "$"
