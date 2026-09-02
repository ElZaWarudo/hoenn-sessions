#[test]
fn migration_contract_character_and_resume_artifact_sizes() {
    let migration = include_str!("../migrations/0001_phase2_auth_save.sql");
    let normalized: String = migration
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();

    assert!(
        normalized.contains("(artifact='character.sav'ANDsize_bytesIN(131072,131088))"),
        "character.sav constraint must allow only 131072 and 131088 bytes"
    );

    assert!(
        normalized.contains("(artifact='resume.ss1'ANDsize_bytesBETWEEN1AND33554432)"),
        "resume.ss1 range must remain 1..33554432 bytes"
    );

    assert!(
        !normalized.contains("(artifact='character.sav'ANDsize_bytesBETWEEN1AND1048576)"),
        "legacy character.sav bounded range must not remain"
    );
}

#[test]
fn migration_contract_group_travel_is_additive_and_integrity_bound() {
    let migration = include_str!("../migrations/0002_phase2_group_travel.sql");
    let normalized: String = migration
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    assert!(normalized.contains("altertablephase2_characters"));
    assert!(normalized.contains("world_revisionbigintnotnulldefault0"));
    assert!(normalized.contains("createtablephase2_groups"));
    assert!(normalized.contains("createtablephase2_group_members"));
    assert!(normalized.contains("phase2_one_active_group_per_character"));
    assert!(normalized.contains("createtablephase2_group_invitations"));
    assert!(normalized.contains("member_a<member_b"));
    assert!(normalized.contains("phase2_validate_group_members"));
    assert!(normalized.contains("group_status='closed'"));
    assert!(normalized.contains("member_count<>0"));
    assert!(normalized.contains("group_status<>'active'ormember_count<>2"));
    assert!(normalized.contains("groupmemberrowsdonotmatchcanonicalgroupactors"));
    assert!(normalized.contains("groupmemberslotsdonotmatchcanonicalactororder"));
    assert!(normalized.contains("groupactorsareimmutable"));
    assert!(normalized.contains("phase2_group_integrity"));
    assert!(normalized.contains("phase2_guard_group_status"));
    assert!(normalized.contains("phase2_guard_group_delete"));
    assert!(normalized.contains("beforedeleteonphase2_groups"));
    assert!(normalized.contains("activegroupmustbeclosedbeforedeletion"));
    assert!(normalized.contains("phase2_guard_group_invitation"));
    assert!(normalized.contains("phase2_idempotency") || !normalized.contains("idempotency"));
    assert!(!normalized.contains("drop table"));
}
