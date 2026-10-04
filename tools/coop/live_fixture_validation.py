"""Read-only fixture admission shared by lineage seeding and live preflight."""
from live_fixture_custody import check_custody
from live_harness_oracles import FlashSave, OracleFailure, check_shared_witnesses


def validate_player_fixture(save: FlashSave, descriptor: bytes, player: dict) -> list[dict]:
    witnesses = player.get("shared_witnesses", [])
    # Preserve every existing format, ownership, duplicate and population check.
    checked = check_shared_witnesses(save, descriptor, witnesses)
    if "custody_recipe" not in player:
        return checked
    custody = check_custody(save, descriptor, player["custody_recipe"])
    by_field = {w["field_id"]: w for w in witnesses}
    for required in custody["shared_witnesses"]:
        if by_field.get(required["field_id"]) != required:
            raise OracleFailure(
                f"custody requires exact full-field witness 0x{required['field_id']:04X}")
    return checked
