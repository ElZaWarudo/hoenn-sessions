#ifndef GUARD_CONSTANTS_JOHTO_TRAINERS_H
#define GUARD_CONSTANTS_JOHTO_TRAINERS_H

/* Johto trainer IDs are runtime transport values, not indexes into gTrainers.
 * Keep the range disjoint from the legacy, partner, and special IDs. */
#define JOHTO_TRAINER_ID_MIN       0x4000
#define JOHTO_TRAINER_ID_MAX       0x41FF
#define JOHTO_TRAINER_NAMESPACE_SIZE (JOHTO_TRAINER_ID_MAX - JOHTO_TRAINER_ID_MIN + 1)

/* Only imported records are addressable by the trainer adapter.  The namespace
 * remains larger so later imports can append without renumbering Joey. */
#define JOHTO_TRAINER_ORDINAL_JOEY 0
#define JOHTO_TRAINER_JOEY         (JOHTO_TRAINER_ID_MIN + JOHTO_TRAINER_ORDINAL_JOEY)
#define JOHTO_TRAINER_RECORD_COUNT 1

#endif /* GUARD_CONSTANTS_JOHTO_TRAINERS_H */
