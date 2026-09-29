#ifndef GUARD_COOP_SAVE_LAYOUT_H
#define GUARD_COOP_SAVE_LAYOUT_H

/* Save-block offsets the cloud server reads from uploaded saves (coop-save).
 * src/coop/save.c asserts each one against the real structs, and coop-save's
 * tests parse this header, so the ROM and server cannot drift apart. Do not
 * trust the offset comments in global.h: the regional flag table made the
 * flags array larger than vanilla, which moved everything after it. */
#define COOP_SAVE_LAYOUT_SB1_MONEY 0x490
#define COOP_SAVE_LAYOUT_SB1_FLAGS 0x1270
#define COOP_SAVE_LAYOUT_SB1_FLAG_BYTES 0x18B
#define COOP_SAVE_LAYOUT_SB1_VARS 0x13FC
#define COOP_SAVE_LAYOUT_SB1_VAR_COUNT 0x18C
#define COOP_SAVE_LAYOUT_SB2_ENCRYPTION_KEY 0xB4
#define COOP_SAVE_LAYOUT_TRAINER_FLAGS_START 0x500
#define COOP_SAVE_LAYOUT_TRAINER_FLAGS_END 0xB55
/* The eight Hoenn badges are FLAG_BADGE01_GET .. +7 (co-op gym roles). */
#define COOP_SAVE_LAYOUT_FLAG_BADGE01_GET 0xB5D

#endif // GUARD_COOP_SAVE_LAYOUT_H
