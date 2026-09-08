#ifndef GUARD_CONSTANTS_JOHTO_EVENTS_H
#define GUARD_CONSTANTS_JOHTO_EVENTS_H

/* Johto event identifiers occupy a disjoint part of the script identifier
 * space.  Keep the complete window reserved so invalid identifiers cannot be
 * interpreted as an index into one of the legacy arrays. */
#define JOHTO_FLAG_START       0x6000
#define JOHTO_FLAG_END         0x62FF
#define JOHTO_VAR_START        0x7000
#define JOHTO_VAR_END          0x705F
#define JOHTO_EVENT_RESERVED_START 0x6000
#define JOHTO_EVENT_RESERVED_END   0x7FFF

#define JOHTO_FLAG_COUNT       (JOHTO_FLAG_END - JOHTO_FLAG_START + 1)
#define JOHTO_VAR_COUNT        (JOHTO_VAR_END - JOHTO_VAR_START + 1)

#endif /* GUARD_CONSTANTS_JOHTO_EVENTS_H */
