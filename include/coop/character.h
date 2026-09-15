#ifndef GUARD_COOP_CHARACTER_H
#define GUARD_COOP_CHARACTER_H

#include "global.h"
#include "coop/presence.h"

#define COOP_CHARACTER_COUNT COOP_PRESENCE_AVATAR_MAX

u8 CoopCharacter_GetSelection(void);
bool8 CoopCharacter_SetSelection(u8 selection);
u8 CoopCharacter_GetAvatarId(void);
u16 CoopCharacter_GetGraphicsId(u8 avatarId);
u16 CoopCharacter_OverrideNormalGraphics(u16 original);
void CoopCharacter_Open(void);

#endif
