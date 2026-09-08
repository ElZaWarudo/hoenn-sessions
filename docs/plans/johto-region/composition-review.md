# Johto packet composition review — 2026-09-08

Reviewer: root, documentary review only. This is not an independent code
review or an implementation approval.

Checked the initiative, roadmap, tracking policy and startup against local
host/donor source and the user's requested scope. Findings incorporated:

1. A donor-wide copy would collide with 518 current map names. Require a
   selected-map manifest and dependency closure rather than importing all 954.
2. Donor gym scripts mutate generic badges/variables. Require separate Johto
   progress and tests that Hoenn/Kanto badges and caps stay unchanged.
3. Route 27, Indigo, S.S. Aqua and the train cross existing Kanto. Require
   explicit geographic mapping, distinct League progression and return tests.
4. Existing saves and limited ROM/trainer/header capacity are foundation
   requirements. Do not postpone them until the final build or accept unchecked
   array growth.
5. Prior preview tests did not cover real player travel. Require recorded
   emulator story, save, blackout, transport and two-player acceptance.
6. Existing cloud-coop state is unrelated authority. Place this packet in a
   separate namespace and leave all original state and uncommitted work intact.

The user accepted the existing-Kanto proposal in the autonomous goal mandate.
Entry/origin UX must be grounded in current engine code in J1;
no worker may invent a destructive party/reset flow.

Result: coherent proposal for user review, not execution-ready. J1 must resolve
the selected content inventory and capacity/save design before mutable waves.
No claim of independent validation, completed implementation or release readiness.
