#ifndef GUARD_JOHTO_RIVAL_H
#define GUARD_JOHTO_RIVAL_H

#include "gba/types.h"

extern const u8 gJohtoRivalNameMarker[];

const u8 *JohtoRival_GetName(void);
bool8 JohtoRival_SetName(const u8 *name);
const u8 *JohtoRival_ResolveTrainerName(const u8 *name);
void Johto_NameRival(void);

#endif /* GUARD_JOHTO_RIVAL_H */
