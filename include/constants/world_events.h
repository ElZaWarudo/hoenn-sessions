#ifndef GUARD_CONSTANTS_WORLD_EVENTS_H
#define GUARD_CONSTANTS_WORLD_EVENTS_H

/* Reused by each added ROM's own regional save image. Coordinate events
 * interpret 0x8000-0x8fff as flags, including the 0x8000-0x8017 IDs that
 * overlap special vars; those vars remain available to typed script commands. */
#define WORLD_EVENT_FLAG_START 0x8000
#define WORLD_EVENT_FLAG_END   0x8FFF
#define WORLD_EVENT_VAR_START  0x9100
#define WORLD_EVENT_VAR_END    0x91FF
#define WORLD_EVENT_TRAINER_START 0x5000
#define WORLD_EVENT_TRAINER_END   0x5FFF

/* BgEvent.hiddenItemId has 13 bits. Existing low offsets remain legacy;
 * Johto uses the next 2,048 values and an added ROM uses the high 4,096.
 * No regional flag is shifted directly into adjacent packed item fields. */
#define WORLD_EVENT_HIDDEN_ITEM_JOHTO_MARKER 0x0800
#define WORLD_EVENT_HIDDEN_ITEM_MARKER 0x1000
#define WORLD_EVENT_HIDDEN_ITEM_ORDINAL_MASK 0x0FFF
#define WORLD_EVENT_HIDDEN_ITEM_JOHTO_MASK 0x07FF

#endif // GUARD_CONSTANTS_WORLD_EVENTS_H
