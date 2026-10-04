@ Pelluca's timed rescue uses Safari Mode for its counter and encounter rules.
@ Retiring or running out of balls must finish the rescue in Cormoria.
Cormoria_PellucaSafari_EventScript_RetirePrompt::
	lockall
	msgbox SafariZone_Text_WouldYouLikeToExit, MSGBOX_YESNO
	goto_if_eq VAR_RESULT, YES, Cormoria_PellucaCityFlooded_EventScript_FailCleanup
	releaseall
	end

Cormoria_PellucaSafari_EventScript_OutOfBalls::
	lockall
	msgbox SafariZone_Text_OutOfBalls, MSGBOX_DEFAULT
	goto Cormoria_PellucaCityFlooded_EventScript_FailCleanup
