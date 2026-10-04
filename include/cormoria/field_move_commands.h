#ifndef GUARD_CORMORIA_FIELD_MOVE_COMMANDS_H
#define GUARD_CORMORIA_FIELD_MOVE_COMMANDS_H

#include "gba/types.h"

struct ScriptContext;

/* Dreamstone's party-move check also accepts a carried HM. */
bool8 ScrCmd_CormoriaCheckPartyMove(struct ScriptContext *ctx);

#endif /* GUARD_CORMORIA_FIELD_MOVE_COMMANDS_H */
