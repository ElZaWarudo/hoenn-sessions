#include "global.h"
#include "coop/arrival_proof.h"
#include "gba/flash_internal.h"
#include "load_save.h"

enum CoopArrivalProofState
{
    COOP_ARRIVAL_PROOF_IDLE,
    COOP_ARRIVAL_PROOF_WAITING_FOR_CONTINUE,
    COOP_ARRIVAL_PROOF_HASHING,
    COOP_ARRIVAL_PROOF_READY,
    COOP_ARRIVAL_PROOF_DONE,
    COOP_ARRIVAL_PROOF_REJECTED,
};

struct CoopSha256
{
    u32 state[8];
    u32 bit_length_high;
    u32 bit_length_low;
    u8 block[64];
    u8 block_length;
};

struct CoopArrivalProofRuntime
{
    enum CoopArrivalProofState state;
    bool8 continue_selected;
    u8 nonce[COOP_ARRIVAL_CHALLENGE_NONCE_SIZE];
    u8 digest[COOP_ARRIVAL_PROOF_HASH_SIZE];
    u32 world_id;
    u32 save_generation;
    u8 map_group;
    u8 map_num;
    u16 next_sector;
    struct CoopSha256 sha;
};

static EWRAM_DATA u8 sFlashSector[COOP_ARRIVAL_FLASH_SECTOR_SIZE];
static EWRAM_DATA struct CoopArrivalProofRuntime sArrivalProof;

#if TESTING
static CoopArrivalProofTestReadFlashCallback sTestReadFlash;
#endif

static const u32 sSha256RoundConstants[64] =
{
    0x428A2F98, 0x71374491, 0xB5C0FBCF, 0xE9B5DBA5,
    0x3956C25B, 0x59F111F1, 0x923F82A4, 0xAB1C5ED5,
    0xD807AA98, 0x12835B01, 0x243185BE, 0x550C7DC3,
    0x72BE5D74, 0x80DEB1FE, 0x9BDC06A7, 0xC19BF174,
    0xE49B69C1, 0xEFBE4786, 0x0FC19DC6, 0x240CA1CC,
    0x2DE92C6F, 0x4A7484AA, 0x5CB0A9DC, 0x76F988DA,
    0x983E5152, 0xA831C66D, 0xB00327C8, 0xBF597FC7,
    0xC6E00BF3, 0xD5A79147, 0x06CA6351, 0x14292967,
    0x27B70A85, 0x2E1B2138, 0x4D2C6DFC, 0x53380D13,
    0x650A7354, 0x766A0ABB, 0x81C2C92E, 0x92722C85,
    0xA2BFE8A1, 0xA81A664B, 0xC24B8B70, 0xC76C51A3,
    0xD192E819, 0xD6990624, 0xF40E3585, 0x106AA070,
    0x19A4C116, 0x1E376C08, 0x2748774C, 0x34B0BCB5,
    0x391C0CB3, 0x4ED8AA4A, 0x5B9CCA4F, 0x682E6FF3,
    0x748F82EE, 0x78A5636F, 0x84C87814, 0x8CC70208,
    0x90BEFFFA, 0xA4506CEB, 0xBEF9A3F7, 0xC67178F2,
};

static u32 RotateRight(u32 value, u8 bits)
{
    return (value >> bits) | (value << (32 - bits));
}

static u32 ReadBigEndian32(const u8 *bytes)
{
    return ((u32)bytes[0] << 24)
         | ((u32)bytes[1] << 16)
         | ((u32)bytes[2] << 8)
         | bytes[3];
}

static void WriteBigEndian32(u8 *bytes, u32 value)
{
    bytes[0] = value >> 24;
    bytes[1] = value >> 16;
    bytes[2] = value >> 8;
    bytes[3] = value;
}

static void Sha256Transform(struct CoopSha256 *sha, const u8 *block)
{
    u32 words[64];
    u32 a, b, c, d, e, f, g, h;
    u32 i;

    for (i = 0; i < 16; i++)
        words[i] = ReadBigEndian32(&block[i * 4]);
    for (i = 16; i < ARRAY_COUNT(words); i++)
    {
        u32 s0 = RotateRight(words[i - 15], 7)
               ^ RotateRight(words[i - 15], 18)
               ^ (words[i - 15] >> 3);
        u32 s1 = RotateRight(words[i - 2], 17)
               ^ RotateRight(words[i - 2], 19)
               ^ (words[i - 2] >> 10);

        words[i] = words[i - 16] + s0 + words[i - 7] + s1;
    }

    a = sha->state[0];
    b = sha->state[1];
    c = sha->state[2];
    d = sha->state[3];
    e = sha->state[4];
    f = sha->state[5];
    g = sha->state[6];
    h = sha->state[7];
    for (i = 0; i < ARRAY_COUNT(words); i++)
    {
        u32 s1 = RotateRight(e, 6) ^ RotateRight(e, 11) ^ RotateRight(e, 25);
        u32 choose = (e & f) ^ (~e & g);
        u32 temporary1 = h + s1 + choose + sSha256RoundConstants[i] + words[i];
        u32 s0 = RotateRight(a, 2) ^ RotateRight(a, 13) ^ RotateRight(a, 22);
        u32 majority = (a & b) ^ (a & c) ^ (b & c);
        u32 temporary2 = s0 + majority;

        h = g;
        g = f;
        f = e;
        e = d + temporary1;
        d = c;
        c = b;
        b = a;
        a = temporary1 + temporary2;
    }
    sha->state[0] += a;
    sha->state[1] += b;
    sha->state[2] += c;
    sha->state[3] += d;
    sha->state[4] += e;
    sha->state[5] += f;
    sha->state[6] += g;
    sha->state[7] += h;
}

static void Sha256Init(struct CoopSha256 *sha)
{
    static const u32 initial_state[8] =
    {
        0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
        0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
    };

    memcpy(sha->state, initial_state, sizeof(initial_state));
    sha->bit_length_high = 0;
    sha->bit_length_low = 0;
    sha->block_length = 0;
}

static void Sha256AddLength(struct CoopSha256 *sha, u32 length)
{
    u32 bits = length << 3;
    u32 previous = sha->bit_length_low;

    sha->bit_length_low += bits;
    sha->bit_length_high += (length >> 29) + (sha->bit_length_low < previous);
}

static void Sha256Update(struct CoopSha256 *sha, const u8 *data, u32 length)
{
    u32 consumed = 0;

    Sha256AddLength(sha, length);
    while (consumed < length)
    {
        u32 available = sizeof(sha->block) - sha->block_length;
        u32 copy_size = min(available, length - consumed);

        memcpy(&sha->block[sha->block_length], &data[consumed], copy_size);
        sha->block_length += copy_size;
        consumed += copy_size;
        if (sha->block_length == sizeof(sha->block))
        {
            Sha256Transform(sha, sha->block);
            sha->block_length = 0;
        }
    }
}

static void Sha256Final(struct CoopSha256 *sha, u8 digest[COOP_ARRIVAL_PROOF_HASH_SIZE])
{
    u8 length[8];
    u8 padding[64] = {0x80};
    u32 i;
    u32 padding_length = sha->block_length < 56 ? 56 - sha->block_length : 120 - sha->block_length;

    WriteBigEndian32(&length[0], sha->bit_length_high);
    WriteBigEndian32(&length[4], sha->bit_length_low);
    Sha256Update(sha, padding, padding_length);
    Sha256Update(sha, length, sizeof(length));
    for (i = 0; i < ARRAY_COUNT(sha->state); i++)
        WriteBigEndian32(&digest[i * 4], sha->state[i]);
}

static bool8 IsZeroBytes(const u8 *bytes, u32 length)
{
    u32 i;

    for (i = 0; i < length; i++)
    {
        if (bytes[i] != 0)
            return FALSE;
    }
    return TRUE;
}

static void ReadArrivalFlashSector(u16 sector)
{
#if TESTING
    if (sTestReadFlash != NULL)
    {
        sTestReadFlash(sector, 0, sFlashSector, sizeof(sFlashSector));
        return;
    }
#endif
    ReadFlash(sector, 0, sFlashSector, sizeof(sFlashSector));
}

bool8 CoopArrivalProof_BeginChallenge(const u8 *nonce, u16 length)
{
    if (sArrivalProof.state != COOP_ARRIVAL_PROOF_IDLE
     || nonce == NULL
     || length != COOP_ARRIVAL_CHALLENGE_NONCE_SIZE
     || IsZeroBytes(nonce, length))
        return FALSE;

    memcpy(sArrivalProof.nonce, nonce, sizeof(sArrivalProof.nonce));
    sArrivalProof.continue_selected = FALSE;
    sArrivalProof.state = COOP_ARRIVAL_PROOF_WAITING_FOR_CONTINUE;
    return TRUE;
}

bool8 CoopArrivalProof_IsVerifierMode(void)
{
    return sArrivalProof.state != COOP_ARRIVAL_PROOF_IDLE;
}

bool8 CoopArrivalProof_IsAwaitingContinue(void)
{
    return sArrivalProof.state == COOP_ARRIVAL_PROOF_WAITING_FOR_CONTINUE;
}

bool8 CoopArrivalProof_IsProofReady(void)
{
    return sArrivalProof.state == COOP_ARRIVAL_PROOF_READY;
}

void CoopArrivalProof_OnContinueSelected(void)
{
    if (sArrivalProof.state == COOP_ARRIVAL_PROOF_WAITING_FOR_CONTINUE)
        sArrivalProof.continue_selected = TRUE;
}

void CoopArrivalProof_OnFieldEntered(bool8 save_status_ok,
                                     bool8 save_v2_valid,
                                     u32 world_id,
                                     u32 save_generation,
                                     u8 map_group,
                                     u8 map_num)
{
    if (sArrivalProof.state != COOP_ARRIVAL_PROOF_WAITING_FOR_CONTINUE
     || !sArrivalProof.continue_selected)
        return;
    if (!save_status_ok || !save_v2_valid || save_generation == 0
     || gFlashMemoryPresent != TRUE)
    {
        sArrivalProof.state = COOP_ARRIVAL_PROOF_REJECTED;
        return;
    }

    sArrivalProof.world_id = world_id;
    sArrivalProof.save_generation = save_generation;
    sArrivalProof.map_group = map_group;
    sArrivalProof.map_num = map_num;
    sArrivalProof.next_sector = 0;
    Sha256Init(&sArrivalProof.sha);
    sArrivalProof.state = COOP_ARRIVAL_PROOF_HASHING;
}

void CoopArrivalProof_Poll(void)
{
    if (sArrivalProof.state != COOP_ARRIVAL_PROOF_HASHING)
        return;
    if (gFlashMemoryPresent != TRUE)
    {
        sArrivalProof.state = COOP_ARRIVAL_PROOF_REJECTED;
        return;
    }

    ReadArrivalFlashSector(sArrivalProof.next_sector);
    Sha256Update(&sArrivalProof.sha, sFlashSector, sizeof(sFlashSector));
    sArrivalProof.next_sector++;
    if (sArrivalProof.next_sector == COOP_ARRIVAL_FLASH_SECTOR_COUNT)
    {
        Sha256Final(&sArrivalProof.sha, sArrivalProof.digest);
        sArrivalProof.state = COOP_ARRIVAL_PROOF_READY;
    }
}

bool8 CoopArrivalProof_GetPayload(u8 *payload, u16 capacity)
{
    if (sArrivalProof.state != COOP_ARRIVAL_PROOF_READY
     || payload == NULL
     || capacity < COOP_ARRIVAL_PROOF_PAYLOAD_SIZE)
        return FALSE;

    memset(payload, 0, COOP_ARRIVAL_PROOF_PAYLOAD_SIZE);
    memcpy(&payload[0], sArrivalProof.nonce, sizeof(sArrivalProof.nonce));
    memcpy(&payload[16], sArrivalProof.digest, sizeof(sArrivalProof.digest));
    payload[48] = sArrivalProof.world_id;
    payload[49] = sArrivalProof.world_id >> 8;
    payload[50] = sArrivalProof.world_id >> 16;
    payload[51] = sArrivalProof.world_id >> 24;
    payload[52] = sArrivalProof.save_generation;
    payload[53] = sArrivalProof.save_generation >> 8;
    payload[54] = sArrivalProof.save_generation >> 16;
    payload[55] = sArrivalProof.save_generation >> 24;
    payload[56] = sArrivalProof.map_group;
    payload[57] = sArrivalProof.map_num;
    return TRUE;
}

void CoopArrivalProof_MarkEmitted(void)
{
    if (sArrivalProof.state == COOP_ARRIVAL_PROOF_READY)
        sArrivalProof.state = COOP_ARRIVAL_PROOF_DONE;
}

void CoopArrivalProof_Reset(void)
{
    memset(&sArrivalProof, 0, sizeof(sArrivalProof));
    sArrivalProof.state = COOP_ARRIVAL_PROOF_IDLE;
}

#if TESTING
void CoopArrivalProof_TestSetFlashReadCallback(CoopArrivalProofTestReadFlashCallback callback)
{
    sTestReadFlash = callback;
}

void CoopArrivalProof_TestSha256(const u8 *data, u32 length, u8 digest[COOP_ARRIVAL_PROOF_HASH_SIZE])
{
    struct CoopSha256 sha;

    if (data == NULL || digest == NULL)
        return;
    Sha256Init(&sha);
    Sha256Update(&sha, data, length);
    Sha256Final(&sha, digest);
}
#endif
