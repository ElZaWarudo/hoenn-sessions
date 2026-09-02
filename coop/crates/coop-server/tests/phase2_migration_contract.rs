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
