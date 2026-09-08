@ Johto event-script extensions.

@ Applies movement data without hiding the active Pokémon follower when the
@ object is on the current map.  Map-qualified movement keeps the host
@ applymovementat path, which already preserves follower visibility.
.macro johto_applymovement2 localId:req, movements:req, map
    .ifb \map
        callnative Script_JohtoApplyMovement, requests_effects=1
        .2byte \localId
        .4byte \movements
    .else
        applymovement \localId, \movements, \map
    .endif
.endm
