#ifndef GUARD_JOHTO_FIELD_MOVES_H
#define GUARD_JOHTO_FIELD_MOVES_H

struct Pokemon;
struct ScriptContext;

u32 JohtoFieldMoves_GetWhirlpoolUser(struct Pokemon *party, u32 count);
void Script_JohtoCheckWhirlpool(struct ScriptContext *ctx);
u32 JohtoFieldMoves_GetHeadbuttUser(struct Pokemon *party, u32 count);
void Script_JohtoCheckHeadbutt(struct ScriptContext *ctx);
const u8 *JohtoFieldMoves_GetHeadbuttScript(u8 metatileBehavior);

#endif
