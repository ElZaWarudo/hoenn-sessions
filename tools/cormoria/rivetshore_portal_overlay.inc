Cormoria_RivetshoreCity_Harbor_Attendant::
	lock
	faceplayer
	callnative CoopNetBridge_ScriptPortalAvailable
	goto_if_eq VAR_RESULT, FALSE, Cormoria_RivetshoreCity_Harbor_Attendant_Original
	msgbox Cormoria_RivetshoreCity_Harbor_Text_MainVoyage, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, NO, Cormoria_RivetshoreCity_Harbor_Attendant_Original
	callnative CoopNetBridge_ScriptTravelToMain
	goto_if_eq VAR_RESULT, FALSE, Cormoria_RivetshoreCity_Harbor_Attendant_Unavailable
	msgbox Cormoria_RivetshoreCity_Harbor_Text_MainPreparing, MSGBOX_DEFAULT
	special CoopPortalSaveGame
	waitstate
	goto_if_eq VAR_RESULT, FALSE, Cormoria_RivetshoreCity_Harbor_Attendant_Unavailable
	release
	end

Cormoria_RivetshoreCity_Harbor_Attendant_Unavailable::
	msgbox Cormoria_RivetshoreCity_Harbor_Text_MainUnavailable, MSGBOX_DEFAULT
	release
	end

Cormoria_RivetshoreCity_Harbor_Attendant_Original::
	msgbox Cormoria_RivetshoreCity_Harbor_Attendant_Text_0, MSGBOX_DEFAULT
	release
	end

Cormoria_RivetshoreCity_Harbor_Text_MainVoyage:
	.string "Would you like to sail to HOENN?$"

Cormoria_RivetshoreCity_Harbor_Text_MainPreparing:
	.string "We'll prepare your voyage. Please wait\n"
	.string "while your progress is saved.$"

Cormoria_RivetshoreCity_Harbor_Text_MainUnavailable:
	.string "We can't depart just now. Please try\n"
	.string "again once your session is ready.$"
