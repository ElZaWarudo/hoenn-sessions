run_id: johto-region-j1
unit_id: j1
orchestrator: seneschal
mode: artifacts
interaction: brokered
jira_policy: skip
production: unknown
canonical_state: nested planning artifacts written; root independent review pending
phase: closeout
status: needs_review
terminal_ready: true
acceptance_criteria_resolved: false
source_revision: 121ee4c192d476e57ca033a1e420e16c4dcec865
sealed_baseline_tree: 5e4c97f0cc2a129d51ea98cd9099695853ffc261
contract_hash: sha256:f996d3fdd1501e13ed197136228be1eae003f149f122ba6209e8328c96accc61
donor_revision: 751823abaf677020bcd72c45fe3e7cb2b8a576e4
workspace: C:/Users/Mayor/Documents/Caribbean/hoenn-sessions-seneschal/johto-region-20260908/j1-planning
owned_artifacts:
  - docs/plans/johto-region/j1/requirements.md
  - docs/plans/johto-region/j1/plan.md
  - docs/plans/johto-region/j1/work-package.md
  - docs/plans/johto-region/j1/review.md
  - docs/plans/johto-region/j1/inventory.json
  - docs/orchestration/compound-master/johto-region-j1/state.md
observed_revision: 121ee4c192d476e57ca033a1e420e16c4dcec865
changed_files:
  - docs/plans/johto-region/j1/requirements.md
  - docs/plans/johto-region/j1/plan.md
  - docs/plans/johto-region/j1/work-package.md
  - docs/plans/johto-region/j1/review.md
  - docs/plans/johto-region/j1/inventory.json
  - docs/orchestration/compound-master/johto-region-j1/state.md
scope:
  selected_maps: 239
  map_id_strategy: preserve host gMapGroup_Johto index 75 and NewBarkTown index 0; append 238 maps at 1..238
  host_sections: 210
  selected_unique_sections_estimate: 55
  host_unused_sections: 11
  selected_trainer_identities: 284
  host_trainer_records: 1478
  host_trainer_max: 1622
  selected_wild_records: 149
gates:
  requirements: drafted
  inventory: drafted with counts, closure, exclusions, and dependency ledger
  plan: drafted with exact proposed edit paths and next implementation slice
  work_package: drafted as J1-RU1
  review: pending independent root review
  verification: not run; contract verification lists are empty and extra verification is forbidden
decision_requests:
  - id: J1-SECTION-U8
    issue: 210 host section entries plus approximately 55 new Johto semantics exceed the u8 map-header range if appended
    recommended_resolution: audited aliasing, safe recycling, or generated compaction with unchanged host map behavior
    owner: root and implementation reviewer
  - id: J1-TRAINER-CAPACITY
    issue: 284 selected Johto identities exceed the 144 free standard trainer records
    recommended_resolution: qualified regional trainer adapter backed by cooperative bits, or a reviewed SaveBlock1 trainer-flag expansion/migration preserving records 0..1477
    owner: root and implementation reviewer
  - id: J1-REGISTRY-MIGRATION
    issue: current save loader exact-matches registry version/digest and would reject valid old v1 saves after append
    recommended_resolution: accept current v1 metadata, copy old bits/state, zero only appended bits, reseal current metadata and CRC
    owner: root and implementation reviewer
affected_sibling_units:
  - J2 map/layout/tileset import consumes the frozen 239-map and section ledger
  - J3 scripts/events consumes qualified flags, vars, specials, and contamination mapping
  - J4 trainer/battle data consumes 284 identity and party mappings
  - J5 save/registry integration consumes migration and identity ledgers
  - J6 graphics/assets consumes script and tileset closure
  - J7 campaign traversal consumes the connected map/event graph
remaining_actions: []
unowned_failures: []
release_readiness: not_ready; nested artifact proposal requires root independent review and technical decisions
last_required_command: none
exact_next_unit: J1-RU1 ledger and compatibility seams; after root review, freeze the 239-map manifest, prove the u8-safe section ledger, append qualified identities without ordinal reorder, and implement/read-test current-v1 registry migration
resume_invocation: krt-compound-master mode:resume run:johto-region-j1 unit:J1-RU1 package:docs/plans/johto-region/j1/work-package.md jira-policy:skip parallel:false

root_reconciliation:
  original_plan: needs-fix; not execution-ready for runtime changes
  independent_review: /root/johto_plan_feasibility
  review_record: docs/orchestration/runs/johto-region-20260908/j1-plan-review-result.json
  first_bounded_unit: J1-RU1a manifest tooling only
  first_bounded_package: docs/plans/johto-region/j1/manifest-work-package.md
  broker_decisions: docs/plans/johto-region/j1-decisions.md
  runtime_implementation: gated pending ledger and save/trainer design
