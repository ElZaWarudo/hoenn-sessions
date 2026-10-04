Cormoria_Championship_Museum_MapScripts::
	.byte 0


Cormoria_Champ_Museum_Past_Martha::
# 9 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 10 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_Past_Martha_Text_0
# 11 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


Cormoria_Champ_Museum_Past_Chef::
# 16 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 17 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_Past_Chef_Text_0
# 18 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


Cormoria_Champ_Museum_Past_Ravine::
# 23 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 24 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_Past_Ravine_Text_0
# 25 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


Cormoria_Champ_Museum_Past_Greta::
# 30 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 31 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_Past_Greta_Text_0
# 32 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


Cormoria_Champ_Museum_Past_Empty::
# 37 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 38 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_Past_Empty_Text_0
# 39 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


Cormoria_Champ_Museum_NPC1::
# 50 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_NPC1_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Museum_NPC2::
# 55 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 56 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_NPC2_Text_0
# 57 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


Cormoria_Champ_Museum_NPC3::
# 62 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_NPC3_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Museum_NPC4::
# 68 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_NPC4_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Museum_NPC5::
# 74 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_NPC5_Text_0, MSGBOX_NPC
	end


Cormoria_Champ_Museum_NPC6::
# 80 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 81 "data//maps/Championship_Museum/scripts.pory"
	speakername Cormoria_Champ_Museum_NPC6_Text_0
# 82 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_NPC6_Text_1
# 83 "data//maps/Championship_Museum/scripts.pory"
	speakername NULL
# 84 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


Cormoria_Champ_Museum_NPC7::
# 90 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 91 "data//maps/Championship_Museum/scripts.pory"
	applymovement 5, Cormoria_Champ_Museum_NPC7_Movement_0
# 92 "data//maps/Championship_Museum/scripts.pory"
	waitmovement 0
# 93 "data//maps/Championship_Museum/scripts.pory"
	delay 16
# 94 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_NPC7_Text_0
# 95 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


Cormoria_Champ_Museum_NPC8::
# 100 "data//maps/Championship_Museum/scripts.pory"
	lockall
# 101 "data//maps/Championship_Museum/scripts.pory"
	applymovement 6, Cormoria_Champ_Museum_NPC8_Movement_0
# 102 "data//maps/Championship_Museum/scripts.pory"
	waitmovement 0
# 103 "data//maps/Championship_Museum/scripts.pory"
	delay 16
# 104 "data//maps/Championship_Museum/scripts.pory"
	msgbox Cormoria_Champ_Museum_NPC8_Text_0
# 105 "data//maps/Championship_Museum/scripts.pory"
	releaseall
	end


# 91 "data//maps/Championship_Museum/scripts.pory"
Cormoria_Champ_Museum_NPC7_Movement_0:
# 91 "data//maps/Championship_Museum/scripts.pory"
	jump_in_place_right
	step_end

# 101 "data//maps/Championship_Museum/scripts.pory"
Cormoria_Champ_Museum_NPC8_Movement_0:
# 101 "data//maps/Championship_Museum/scripts.pory"
	jump_in_place_left
	step_end

Cormoria_Champ_Museum_Past_Martha_Text_0:
# 10 "data//maps/Championship_Museum/scripts.pory"
	.string "It's a painting of a trainer battling\n"
	.string "with a Houndstone...\p"
	.string "Martha.\n"
	.string "First Cormoria Pokémon Champion.$"

Cormoria_Champ_Museum_Past_Chef_Text_0:
# 17 "data//maps/Championship_Museum/scripts.pory"
	.string "It's a painting of a trainer battling\n"
	.string "with a Hydrapple...\p"
	.string "Lynch.\n"
	.string "Second Cormoria Pokémon Champion.$"

Cormoria_Champ_Museum_Past_Ravine_Text_0:
# 24 "data//maps/Championship_Museum/scripts.pory"
	.string "It's a painting of a trainer battling\n"
	.string "with a Kangaskhan...\p"
	.string "Ravine.\n"
	.string "Third Cormoria Pokémon Champion.$"

Cormoria_Champ_Museum_Past_Greta_Text_0:
# 31 "data//maps/Championship_Museum/scripts.pory"
	.string "It's a painting of a trainer battling\n"
	.string "with a Banette...\p"
	.string "Greta.\n"
	.string "Fourth Cormoria Pokémon Champion.$"

Cormoria_Champ_Museum_Past_Empty_Text_0:
# 38 "data//maps/Championship_Museum/scripts.pory"
	.string "It's blank.$"

Cormoria_Champ_Museum_NPC1_Text_0:
# 50 "data//maps/Championship_Museum/scripts.pory"
	.string "Welcome to the Championship Museum!\p"
	.string "Please be careful and don't touch any\n"
	.string "of the paintings, okay?$"

Cormoria_Champ_Museum_NPC2_Text_0:
# 56 "data//maps/Championship_Museum/scripts.pory"
	.string "Baby, will you please stop running!\n"
	.string "We've come here to learn about\l"
	.string "Cormoria's history.$"

Cormoria_Champ_Museum_NPC3_Text_0:
# 62 "data//maps/Championship_Museum/scripts.pory"
	.string "The tiles are so smooth!$"

Cormoria_Champ_Museum_NPC4_Text_0:
# 68 "data//maps/Championship_Museum/scripts.pory"
	.string "Haha! I remember watching Martha win.\n"
	.string "She was a formidable trainer!\p"
	.string "Nothing like the crop these days...\n"
	.string "Ace Rookie...pah!$"

Cormoria_Champ_Museum_NPC5_Text_0:
# 74 "data//maps/Championship_Museum/scripts.pory"
	.string "It's so intimidating being here!\n"
	.string "How are we supposed to compete against\l"
	.string "trainers like these?$"

Cormoria_Champ_Museum_NPC6_Text_0:
# 81 "data//maps/Championship_Museum/scripts.pory"
	.string "Greta$"

Cormoria_Champ_Museum_NPC6_Text_1:
# 82 "data//maps/Championship_Museum/scripts.pory"
	.string "They haven't got my face right...$"

Cormoria_Champ_Museum_NPC7_Text_0:
# 94 "data//maps/Championship_Museum/scripts.pory"
	.string "If I win the championship and they put\n"
	.string "my painting up here, will you marry me?$"

Cormoria_Champ_Museum_NPC8_Text_0:
# 104 "data//maps/Championship_Museum/scripts.pory"
	.string "Don't make promises you can't keep!$"
