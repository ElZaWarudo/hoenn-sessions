#ifndef GUARD_COOP_ARRIVAL_PROOF_H
#define GUARD_COOP_ARRIVAL_PROOF_H

#include "gba/defines.h"
#include "gba/types.h"

#define COOP_ARRIVAL_CHALLENGE_NONCE_SIZE 16
#define COOP_ARRIVAL_PROOF_HASH_SIZE 32
#define COOP_ARRIVAL_PROOF_PAYLOAD_SIZE 64
#define COOP_ARRIVAL_FLASH_SECTOR_SIZE 0x1000
#define COOP_ARRIVAL_FLASH_SECTOR_COUNT 32

/* The challenge is a verifier-only epoch-0 message. It never starts a
 * cloud session or changes the normal save/online state machine. */
bool8 CoopArrivalProof_BeginChallenge(const u8 *nonce, u16 length);
bool8 CoopArrivalProof_IsVerifierMode(void);
bool8 CoopArrivalProof_IsAwaitingContinue(void);
bool8 CoopArrivalProof_IsProofReady(void);
void CoopArrivalProof_OnContinueSelected(void);
void CoopArrivalProof_OnFieldEntered(bool8 save_status_ok,
                                     bool8 save_v2_valid,
                                     u32 world_id,
                                     u32 save_generation,
                                     u8 map_group,
                                     u8 map_num);
void CoopArrivalProof_Poll(void);
bool8 CoopArrivalProof_GetPayload(u8 *payload, u16 capacity);
void CoopArrivalProof_MarkEmitted(void);
void CoopArrivalProof_Reset(void);

#if TESTING
typedef void (*CoopArrivalProofTestReadFlashCallback)(u16 sector,
                                                        u32 offset,
                                                        u8 *destination,
                                                        u32 size);

void CoopArrivalProof_TestSetFlashReadCallback(CoopArrivalProofTestReadFlashCallback callback);
void CoopArrivalProof_TestSha256(const u8 *data, u32 length, u8 digest[COOP_ARRIVAL_PROOF_HASH_SIZE]);
#endif

#endif /* GUARD_COOP_ARRIVAL_PROOF_H */
