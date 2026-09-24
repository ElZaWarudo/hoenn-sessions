@ VAR_0x8004 here is used by ChangePokemonNickname
Cormoria_Common_EventScript_GetGiftMonPartySlot::
	getpartysize
	subvar VAR_RESULT, 1
	copyvar VAR_0x8004, VAR_RESULT
	return

Cormoria_Common_EventScript_NameReceivedBoxMon::
	fadescreen FADE_TO_BLACK
	special ChangeBoxPokemonNickname
	waitstate
	lock
	faceplayer
	return

Cormoria_Common_EventScript_TransferredToPC::
	bufferboxname STR_VAR_1, Cormoria_VAR_PC_BOX_TO_SEND_MON
	bufferspeciesname STR_VAR_2, VAR_TEMP_TRANSFERRED_SPECIES
	call_if_unset Cormoria_FLAG_SYS_PC_LANETTE, Cormoria_EventScript_TransferredSomeonesPC
	call_if_set Cormoria_FLAG_SYS_PC_LANETTE, Cormoria_EventScript_TransferredLanettesPC
	return

Cormoria_EventScript_TransferredSomeonesPC::
	specialvar VAR_RESULT, ShouldShowBoxWasFullMessage
	goto_if_eq VAR_RESULT, TRUE, Cormoria_EventScript_SomeonesPCBoxFull
	msgbox Cormoria_gText_PkmnTransferredSomeonesPC, MSGBOX_DEFAULT
	return

Cormoria_EventScript_SomeonesPCBoxFull::
	specialvar VAR_RESULT, GetPCBoxToSendMon
	bufferboxname STR_VAR_3, VAR_RESULT
	msgbox Cormoria_gText_PkmnTransferredSomeonesPCBoxFull, MSGBOX_DEFAULT
	return

Cormoria_EventScript_TransferredLanettesPC::
	specialvar VAR_RESULT, ShouldShowBoxWasFullMessage
	goto_if_eq VAR_RESULT, TRUE, Cormoria_EventScript_LanettesPCBoxFull
	msgbox Cormoria_gText_PkmnTransferredLanettesPC, MSGBOX_DEFAULT
	return

Cormoria_EventScript_LanettesPCBoxFull::
	specialvar VAR_RESULT, GetPCBoxToSendMon
	bufferboxname STR_VAR_3, VAR_RESULT
	msgbox Cormoria_gText_PkmnTransferredLanettesPCBoxFull, MSGBOX_DEFAULT
	return

Cormoria_Common_EventScript_NoMoreRoomForPokemon::
	msgbox Cormoria_gText_NoMoreRoomForPokemon, MSGBOX_DEFAULT
	release
	end
