"""Prepare an authenticated, isolated Cormoria script include preview.

The preview is deliberately not linked. Donor event_scripts.s supplies include
order only: its command table, shared globals, and Hoenn includes stay in the
host assembly unit. Every selected definition receives a Cormoria namespace so
the donor's copied common and Hoenn-map scripts cannot redefine host labels.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path

from tools.cormoria import berry_plots, import_world

ROOT = Path(__file__).resolve().parents[2]
SCRIPT_COUNTS = {"maps": 168, "scripts": 12, "text": 7}
INCLUDE = re.compile(r'^\s*\.include\s+"([^"]+)"\s*(?:@.*)?$', re.MULTILINE)
LABEL = re.compile(r'^\s*([A-Za-z_][A-Za-z_0-9]*)\s*:{1,2}(?!:)', re.MULTILINE)
LOCAL_SET = re.compile(r'^\s*\.set\s+(LOCALID_[A-Za-z_0-9]+)\s*,', re.MULTILINE)
TOKEN = re.compile(r'\b[A-Za-z_][A-Za-z_0-9]*\b')
QUOTED = re.compile(r'("(?:\\.|[^"\\])*")')
GIVEMON_SHINY = re.compile(r'(?m)(^\s*givemon\b[^\n]*?)\bisShiny\s*=\s*(TRUE|FALSE)\b')
GACHA_TOKEN_SETTLEMENT = "data/maps/GalecrestCity_GameCorner/scripts.inc"
RIVETSHORE_HARBOR = "data/maps/RivetshoreCity_Harbor/scripts.inc"
CHAMPIONSHIP_R5 = "data/maps/Championship_R5/scripts.inc"
PELLUCA_SAFARI = "data/maps/PellucaCity/scripts.inc"
GALECREST_CITY = "data/maps/GalecrestCity/scripts.inc"
WINTERLILY_HOLLOW = "data/maps/WinterlilyHollow/scripts.inc"
SILVERSUN_GYM = "data/maps/SilversunCityGym/scripts.inc"
SILVERSUN_CITY = "data/maps/SilversunCity/scripts.inc"
CARABRUE_FINALE = "data/maps/CarabrueTown_TenebrisLab_Finale/scripts.inc"
GALECREST_ACADEMY = "data/maps/GalecrestCity_DetectiveAcademy/scripts.inc"
GASTREE_GYM = "data/maps/GastreeGym/scripts.inc"
CARABRUE_HOME_2F = "data/maps/CarabrueTown_Home2F/scripts.inc"
CARABRUE_TENEBRIS_LAB = "data/maps/CarabrueTown_TenebrisLab/scripts.inc"
INGAME_TRADE_SCRIPTS = {
    "data/maps/CeramBaseCamp_Main/scripts.inc",
    "data/maps/PellucaCityRestaurant/scripts.inc",
    "data/maps/Rivetshore_RangerInstitute_Interior/scripts.inc",
}


class ScriptRegistrationError(ValueError):
    """The staged scripts cannot safely be registered."""


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _stage_bytes(stage: Path, relative: str, records: dict[str, dict]) -> bytes:
    if relative not in records:
        raise ScriptRegistrationError(f"unlisted staged source: {relative}")
    path = stage / import_world.safe_relative(relative)
    if not path.is_file() or not path.resolve().is_relative_to(stage):
        raise ScriptRegistrationError(f"missing or escaping staged source: {relative}")
    data = path.read_bytes()
    record = records[relative]
    if len(data) != record["bytes"] or _sha(data) != record["sha256"]:
        raise ScriptRegistrationError(f"staged source drift: {relative}")
    return data


def _definitions(text: str, path: str) -> list[str]:
    # Assembly directives and preprocessor line markers cannot define labels.
    labels = LABEL.findall(text)
    if len(labels) != len(set(labels)):
        raise ScriptRegistrationError(f"duplicate label within {path}")
    return labels


def _host_labels(root: Path) -> set[str]:
    host = root / "data/event_scripts.s"
    text = host.read_text(encoding="utf-8-sig")
    labels = set(LABEL.findall(text))
    for relative in INCLUDE.findall(text):
        if not relative.startswith("data/"):
            continue
        path = root / import_world.safe_relative(relative)
        if not path.is_file() or not path.resolve().is_relative_to(root.resolve()):
            raise ScriptRegistrationError(f"missing host include: {relative}")
        labels.update(LABEL.findall(path.read_text(encoding="utf-8-sig")))
    return labels


def _rename(text: str, labels: dict[str, str]) -> str:
    chunks = QUOTED.split(text)
    return "".join(chunk if index % 2 else TOKEN.sub(
        lambda match: labels.get(match.group(), match.group()), chunk)
        for index, chunk in enumerate(chunks))


def _adapt_givemon_shininess(text: str) -> tuple[str, int]:
    """Keep the donor's forced shiny/non-shiny gifts under the host enum."""
    return GIVEMON_SHINY.subn(
        lambda match: match[1] + "shinyMode=" + (
            "SHINY_MODE_ALWAYS" if match[2] == "TRUE" else "SHINY_MODE_NEVER"),
        text,
    )


def _adapt_ingame_trade_vars(text: str, path: str) -> str:
    """Pass the trade ID in 0x8005 and the chosen party slot in 0x8004."""
    trade_as_slot = "\tcopyvar VAR_0x8004, VAR_0x8008"
    slot_as_trade = "\tcopyvar VAR_0x8005, VAR_0x800A"
    if text.count(trade_as_slot) != 2 or text.count(slot_as_trade) != 2:
        raise ScriptRegistrationError(f"donor in-game trade variable flow drifted: {path}")
    text = text.replace(trade_as_slot, "\tcopyvar VAR_0x8005, VAR_0x8008", 1)
    text = text.replace(trade_as_slot, "\tcopyvar VAR_0x8004, VAR_0x800A", 1)
    return text.replace(slot_as_trade, "\tcopyvar VAR_0x8005, VAR_0x8008")


def _adapt_gacha_token_settlement(text: str) -> tuple[str, int]:
    """Remove donor script spends; the C minigame owns token settlement.

    ``StartGacha`` removes one token only after the Pokémon reaches party or
    PC. The donor script's post-``waitstate`` removal would charge twice on
    success and charge once on cancellation or failed delivery.
    """
    lines = text.splitlines(keepends=True)
    transformed = 0
    for index, line in enumerate(lines):
        if line.strip() != "removeitem ITEM_GACHA_TOKEN":
            continue

        previous = index - 1
        while previous >= 0 and lines[previous].lstrip().startswith("#"):
            previous -= 1
        if previous < 0 or lines[previous].strip() != "waitstate":
            raise ScriptRegistrationError(
                "Gacha token removal is not immediately after waitstate"
            )

        following = index + 1
        while following < len(lines) and lines[following].lstrip().startswith("#"):
            following += 1
        if following >= len(lines) or not lines[following].strip().startswith("goto "):
            raise ScriptRegistrationError(
                "Gacha token removal does not have its authenticated return label"
            )
        lines[index] = ""
        transformed += 1

    return "".join(lines), transformed


def _adapt_gastree_item_rewards(text: str) -> str:
    """Keep Gastree's one-time gifts available when the Bag is full."""
    leader_failure = "Cormoria_GastreeGym_LeaderBattle_RareShardItemFull"
    leader_retry = "Cormoria_GastreeGym_LeaderBattle_RareShardRetry"
    leader_received = "Cormoria_FLAG_GASTREEGYM_LEADER_RARE_SHARD_RECEIVED"
    leader_failure_text = f"{leader_failure}_Text_0"
    failure = "Cormoria_GastreeGym_Red_ItemFull"
    if any(label in text for label in (
            leader_failure, leader_retry, leader_received, leader_failure_text, failure)):
        raise ScriptRegistrationError("Gastree reward overlay drift")

    leader_gift = (
        '# 77 "data//maps/GastreeGym/scripts.pory"\n'
        "\tgiveitem ITEM_RARE_SHARD\n"
        '# 78 "data//maps/GastreeGym/scripts.pory"\n'
        "\tspeakername Cormoria_GastreeGym_LeaderBattle_Text_1\n"
    )
    if text.count(leader_gift) != 1 or text.count("\tgiveitem ITEM_RARE_SHARD\n") != 1:
        raise ScriptRegistrationError("Gastree leader reward flow drift")
    text = text.replace(
        leader_gift,
        '# 77 "data//maps/GastreeGym/scripts.pory"\n'
        "\tgiveitem ITEM_RARE_SHARD\n"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {leader_failure}\n"
        f"\tsetflag {leader_received}\n"
        '# 78 "data//maps/GastreeGym/scripts.pory"\n'
        "\tspeakername Cormoria_GastreeGym_LeaderBattle_Text_1\n",
        1,
    )

    leader_repeat = (
        "Cormoria_GastreeGym_LeaderBattle_2:\n"
        '# 24 "data//maps/GastreeGym/scripts.pory"\n'
        "\tmsgbox Cormoria_GastreeGym_LeaderBattle_Text_0, MSGBOX_NPC\n"
        "\tgoto Cormoria_GastreeGym_LeaderBattle_1\n"
    )
    if text.count(leader_repeat) != 1:
        raise ScriptRegistrationError("Gastree leader retry gate drift")
    text = text.replace(
        leader_repeat,
        "Cormoria_GastreeGym_LeaderBattle_2:\n"
        '# 24 "data//maps/GastreeGym/scripts.pory"\n'
        "\tmsgbox Cormoria_GastreeGym_LeaderBattle_Text_0, MSGBOX_NPC\n"
        f"\tgoto_if_set {leader_received}, Cormoria_GastreeGym_LeaderBattle_1\n"
        f"\tgoto {leader_retry}\n",
        1,
    )

    for item, suffix in (
        ("ITEM_FRESH_WATER",
         '# 123 "data//maps/GastreeGym/scripts.pory"\n'
         "\tmsgbox Cormoria_GastreeGym_Red_Text_4\n"
         '# 124 "data//maps/GastreeGym/scripts.pory"\n'
         "\tsetflag Cormoria_FLAG_GASTREEGYM_SPENSER_WATER\n"
         '# 125 "data//maps/GastreeGym/scripts.pory"\n'
         "\tclearflag Cormoria_FLAG_HIDE_ROUTE3_UNDERPASS_GYM\n"),
        ("VAR_0x8006",
         '# 147 "data//maps/GastreeGym/scripts.pory"\n'
         "\tsetflag Cormoria_FLAG_GASTREEGYM_SPENSER_REWARD\n"
         '# 148 "data//maps/GastreeGym/scripts.pory"\n'
         "\tgoto Cormoria_GastreeGym_Red_Reward_End\n"),
    ):
        gift = f"\tgiveitem {item}\n"
        flow = gift + suffix
        if text.count(gift) != 1 or text.count(flow) != 1:
            raise ScriptRegistrationError(f"Gastree {item} reward flow drift")
        text = text.replace(flow, gift + f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n" + suffix, 1)

    exit_anchor = (
        "Cormoria_GastreeGym_Red_Reward_End::\n"
        '# 155 "data//maps/GastreeGym/scripts.pory"\n'
        "\tmsgbox Cormoria_GastreeGym_Red_Reward_End_Text_0, MSGBOX_NPC\n"
        "\tend\n"
    )
    if text.count(exit_anchor) != 1:
        raise ScriptRegistrationError("Gastree reward exit drift")
    text = text.replace(exit_anchor, exit_anchor + f"\n{failure}::\n\treleaseall\n\tend\n", 1)
    text += (
        f"\n{leader_retry}::\n"
        "\tcheckitemspace ITEM_RARE_SHARD, 1\n"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {leader_failure}\n"
        "\tgiveitem ITEM_RARE_SHARD\n"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {leader_failure}\n"
        f"\tsetflag {leader_received}\n"
        "\tgoto Cormoria_GastreeGym_LeaderBattle_1\n"
        f"\n{leader_failure}::\n"
        f"\tmsgbox {leader_failure_text}, MSGBOX_DEFAULT\n"
        "\treleaseall\n"
        "\tend\n"
        f"\n{leader_failure_text}:\n"
        '\t.string "The Bag is full. Make room for the\\n"\n'
        '\t.string "Rare Shard, then talk to Inger again.$"\n'
    )
    return text


def _adapt_carabrue_welcome_package(text: str) -> str:
    """Keep the package object and flag available if the Key Items pocket is full."""
    failure = "Cormoria_CarabrueTown_Home2F_EventScript_PickUpBag_ItemFull"
    explanation = f"{failure}_Text_0"
    anchor = (
        "\tgiveitem ITEM_LAB_WELCOMEPACKAGE\n"
        '# 10 "data//maps/CarabrueTown_Home2F/scripts.pory"\n'
        "\tsetflag Cormoria_FLAG_TENEBRIS_POLICE_PRESCENCE\n"
    )
    if (failure in text or text.count(anchor) != 1
            or text.count("\tgiveitem ITEM_LAB_WELCOMEPACKAGE\n") != 1):
        raise ScriptRegistrationError("Carabrue welcome package flow drift")
    text = text.replace(anchor, anchor.replace(
        '# 10', f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n# 10", 1), 1)
    exit_anchor = (
        '# 13 "data//maps/CarabrueTown_Home2F/scripts.pory"\n'
        "\treleaseall\n\treturn\n"
    )
    if text.count(exit_anchor) != 1:
        raise ScriptRegistrationError("Carabrue welcome package exit drift")
    return text.replace(exit_anchor, exit_anchor +
                        f"\n{failure}::\n\tmsgbox {explanation}, MSGBOX_DEFAULT\n"
                        "\treleaseall\n\treturn\n"
                        f"\n{explanation}:\n"
                        '\t.string "The Bag is full. Make room for the\\n"\n'
                        '\t.string "Lab Package, then try again.$"\n', 1)


def _adapt_carabrue_starter_supplies(text: str) -> str:
    """Preflight each distinct pocket before granting any of the three gifts."""
    failure = "Cormoria_CarabrueTown_TenebrisLab_EventScript_Start_ItemFull"
    explanation = f"{failure}_Text_0"
    # Poké Balls, Items and Key Items are distinct pockets in src/data/items.h.
    checks = (
        ("ITEM_POKE_BALL", 5),
        ("ITEM_POTION", 1),
        ("ITEM_TOWN_MAP", 1),
    )
    reward_anchor = (
        '# 50 "data//maps/CarabrueTown_TenebrisLab/scripts.pory"\n'
        "\tsetflag Cormoria_FLAG_REAL_PACKAGE_GET\n"
        '# 51 "data//maps/CarabrueTown_TenebrisLab/scripts.pory"\n'
        "\tgiveitem ITEM_POKE_BALL, 5\n"
        '# 52 "data//maps/CarabrueTown_TenebrisLab/scripts.pory"\n'
        "\tgiveitem ITEM_POTION, 1\n"
        '# 53 "data//maps/CarabrueTown_TenebrisLab/scripts.pory"\n'
        "\tgiveitem ITEM_TOWN_MAP\n"
    )
    gifts = ("\tgiveitem ITEM_POKE_BALL, 5\n", "\tgiveitem ITEM_POTION, 1\n",
             "\tgiveitem ITEM_TOWN_MAP\n")
    entry_anchor = (
        "Cormoria_CarabrueTown_TenebrisLab_EventScript_Start::\n"
        '# 22 "data//maps/CarabrueTown_TenebrisLab/scripts.pory"\n'
        "\tlockall\n"
    )
    if (failure in text or text.count(reward_anchor) != 1
            or text.count(entry_anchor) != 1
            or any(text.count(gift) != 1 for gift in gifts)):
        raise ScriptRegistrationError("Carabrue starter supplies flow drift")
    preflight = "".join(
        f"\tcheckitemspace {item}, {count}\n"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n"
        for item, count in checks
    )
    # The map's on-frame trigger repeats while LAB_STATE is 0. A failed check
    # must leave that state intact and move the player out of the map so the
    # script cannot immediately restart before Bag access is possible.
    text = text.replace(entry_anchor, entry_anchor + preflight, 1)
    exit_anchor = (
        '# 98 "data//maps/CarabrueTown_TenebrisLab/scripts.pory"\n'
        "\treleaseall\n\treturn\n"
    )
    if text.count(exit_anchor) != 1:
        raise ScriptRegistrationError("Carabrue starter supplies exit drift")
    return text.replace(exit_anchor, exit_anchor +
                        f"\n{failure}::\n\tmsgbox {explanation}, MSGBOX_DEFAULT\n"
                        "\treleaseall\n"
                        "\twarp MAP_CORMORIA_CARABRUE_TOWN, 8, 18\n\tend\n"
                        f"\n{explanation}:\n"
                        '\t.string "The Bag is full. Make room for the\\n"\n'
                        '\t.string "supplies, then return to the lab.$"\n', 1)


def _adapt_carabrue_starter_capacity(text: str) -> str:
    """Keep starter choices available when a traveler has no Pokémon space."""
    prefix = "Cormoria_CarabrueTown_TenebrisLab_EventScript"
    check = f"{prefix}_StarterCapacity"
    ready = f"{check}_Ready"
    failure = f"{prefix}_StarterStorageFull"
    if check in text or failure in text:
        raise ScriptRegistrationError("Carabrue starter capacity overlay drift")
    for choice, object_id in (("One", 4), ("Two", 7), ("Three", 6)):
        entry = f"{prefix}_Pokeball_{choice}"
        gate = f"\tgoto_if_set Cormoria_FLAG_UNUSED_0x020, {entry}_2\n"
        success = f"{entry}_8:\n"
        remove = f"\tremoveobject {object_id}\n"
        accepted = f"{entry}_5:\n"
        if any(text.count(anchor) != 1 for anchor in (gate, success, accepted)):
            raise ScriptRegistrationError(f"Carabrue {choice} starter flow drift")
        gift_body = text.split(accepted, 1)[1].split(success, 1)[0]
        if gift_body.count(remove) != 1:
            raise ScriptRegistrationError(f"Carabrue {choice} starter object drift")
        text = text.replace(gate, gate + f"\tcall {check}\n"
                            f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n", 1)
        # Do not consume the selected object until either party or PC delivery
        # succeeds. Both shiny branches converge here, retaining VAR_RESULT.
        text = text.replace(accepted + gift_body + success,
                            accepted + gift_body.replace(remove, "", 1) + success, 1)
        text = text.replace(success, success +
                            f"\tgoto_if_eq VAR_RESULT, MON_CANT_GIVE, {failure}\n" + remove, 1)
    return text + (
        f"\n{check}::\n\tgetpartysize\n"
        f"\tgoto_if_ne VAR_RESULT, PARTY_SIZE, {ready}\n"
        "\tspecialvar VAR_RESULT, ScriptCheckFreePokemonStorageSpace\n\treturn\n"
        f"{ready}::\n\tsetvar VAR_RESULT, TRUE\n\treturn\n"
        f"{failure}::\n\tmsgbox {failure}_Text_0, MSGBOX_DEFAULT\n"
        "\treleaseall\n\tend\n"
        f"{failure}_Text_0:\n"
        '\t.string "Your party and PC are full. Make\\n"\n'
        '\t.string "room for a Pokémon, then try again.$"\n'
    )


def _adapt_galecrest_rock_smash(text: str) -> str:
    """Keep Galecrest's Rock Smash gift retryable when the Bag is full."""
    failure = "Cormoria_GalecrestCity_NPC_5_ItemFull"
    explanation = f"{failure}_Text_0"
    gift = "\tgiveitem ITEM_HM_ROCK_SMASH\n"
    gift_flow = (
        '# 345 "data//maps/GalecrestCity/scripts.pory"\n'
        f"{gift}"
        '# 346 "data//maps/GalecrestCity/scripts.pory"\n'
        "\trelease\n"
        '# 347 "data//maps/GalecrestCity/scripts.pory"\n'
        "\tsetflag Cormoria_FLAG_GALECREST_ROCKSMASH\n"
        "\tgoto Cormoria_GalecrestCity_NPC_5_1\n"
    )
    if (failure in text or explanation in text or text.count(gift_flow) != 1
            or text.count(gift) != 1):
        raise ScriptRegistrationError("Galecrest Rock Smash reward flow drift")
    text = text.replace(
        gift_flow,
        '# 345 "data//maps/GalecrestCity/scripts.pory"\n'
        f"{gift}"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n"
        '# 346 "data//maps/GalecrestCity/scripts.pory"\n'
        "\trelease\n"
        '# 347 "data//maps/GalecrestCity/scripts.pory"\n'
        "\tsetflag Cormoria_FLAG_GALECREST_ROCKSMASH\n"
        "\tgoto Cormoria_GalecrestCity_NPC_5_1\n",
        1,
    )
    return text + (
        f"\n{failure}::\n"
        f"\tmsgbox {explanation}, MSGBOX_DEFAULT\n"
        "\treleaseall\n"
        "\tend\n"
        f"\n{explanation}:\n"
        '\t.string "The Bag is full. Make room for the\\n"\n'
        '\t.string "Rock Smash, then talk to me again.$"\n'
    )


def _adapt_winterlily_surf(text: str) -> str:
    """Make Winterlily's Surf gift reachable and retryable when the Bag is full."""
    failure = "Cormoria_WinterlilyHollow_NPC_SurfMan_ItemFull"
    explanation = f"{failure}_Text_0"
    gift = "\tgiveitem ITEM_HM03\n"
    entry_flow = (
        "Cormoria_WinterlilyHollow_NPC_SurfMan::\n"
        '# 272 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_0, MSGBOX_NPC\n"
        '# 273 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tend\n"
        '# 276 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tlockall\n"
        '# 277 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tfaceplayer\n"
        '# 279 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tgoto_if_set Cormoria_FLAG_WINTERLILY_HOLLOW_SURF, "
        "Cormoria_WinterlilyHollow_NPC_SurfMan_2\n"
    )
    completed_flow = (
        "Cormoria_WinterlilyHollow_NPC_SurfMan_2:\n"
        '# 280 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_1\n"
        "\tgoto Cormoria_WinterlilyHollow_NPC_SurfMan_1\n"
    )
    decline_flow = (
        "Cormoria_WinterlilyHollow_NPC_SurfMan_5:\n"
        '# 286 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_3, MSGBOX_NPC\n"
        "\tend\n"
    )
    no_scale_flow = (
        "Cormoria_WinterlilyHollow_NPC_SurfMan_8:\n"
        '# 292 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_4, MSGBOX_NPC\n"
        "\tend\n"
    )
    remove_scale = "\tremoveitem ITEM_HEART_SCALE\n"
    success_dialogue = (
        '# 296 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_5, MSGBOX_SIGN\n"
        '# 297 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_6\n"
    )
    exchange_flow = (
        success_dialogue
        + '# 298 "data//maps/WinterlilyHollow/scripts.pory"\n'
        f"{gift}"
        '# 299 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_7\n"
        '# 301 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tsetflag Cormoria_FLAG_WINTERLILY_HOLLOW_SURF\n"
        "\treturn\n"
    )
    if (failure in text or explanation in text or text.count(entry_flow) != 1
            or text.count(completed_flow) != 1 or text.count(decline_flow) != 1
            or text.count(no_scale_flow) != 1 or text.count(exchange_flow) != 1
            or text.count(gift) != 1 or text.count(remove_scale) != 0):
        raise ScriptRegistrationError("Winterlily Surf reward flow drift")
    # The map object enters this label directly. The donor's `end` after the
    # introductory line made the authenticated trade block unreachable.
    text = text.replace(
        entry_flow,
        entry_flow.replace(
            '# 273 "data//maps/WinterlilyHollow/scripts.pory"\n'
            "\tend\n",
            "",
            1,
        ),
        1,
    )
    # Once the flag is set, acknowledge the completed exchange and terminate;
    # looping back to the trade prompt would grant Surf repeatedly.
    text = text.replace(
        completed_flow,
        "Cormoria_WinterlilyHollow_NPC_SurfMan_2:\n"
        '# 280 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_1\n"
        "\treleaseall\n"
        "\tend\n",
        1,
    )
    text = text.replace(
        decline_flow,
        "Cormoria_WinterlilyHollow_NPC_SurfMan_5:\n"
        '# 286 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_3, MSGBOX_NPC\n"
        "\treleaseall\n"
        "\tend\n",
        1,
    )
    text = text.replace(
        no_scale_flow,
        "Cormoria_WinterlilyHollow_NPC_SurfMan_8:\n"
        '# 292 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_4, MSGBOX_NPC\n"
        "\treleaseall\n"
        "\tend\n",
        1,
    )
    # Text_5 says the scale was given; keep both exchange messages behind the
    # successful HM grant and scale removal so a full Bag cannot claim success.
    text = text.replace(
        exchange_flow,
        '# 298 "data//maps/WinterlilyHollow/scripts.pory"\n'
        f"{gift}"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n"
        f"{remove_scale}"
        f"{success_dialogue}"
        '# 299 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tmsgbox Cormoria_WinterlilyHollow_NPC_SurfMan_Text_7\n"
        '# 301 "data//maps/WinterlilyHollow/scripts.pory"\n'
        "\tsetflag Cormoria_FLAG_WINTERLILY_HOLLOW_SURF\n"
        "\treleaseall\n"
        "\tend\n",
        1,
    )
    return text + (
        f"\n{failure}::\n"
        f"\tmsgbox {explanation}, MSGBOX_DEFAULT\n"
        "\treleaseall\n"
        "\tend\n"
        f"\n{explanation}:\n"
        '\t.string "The Bag is full. Make room for\\n"\n'
        '\t.string "Surf, then talk to me again.$"\n'
    )


def _adapt_silversun_strength(text: str) -> str:
    """Keep the cutscene complete while making its HM claim retryable on re-entry."""
    prefix = "Cormoria_SilversunCity_OnFrame"
    gift = (
        "\tsetflag Cormoria_FLAG_SYS_GOT_STRENGTH\n"
        '# 85 "data//maps/SilversunCity/scripts.pory"\n'
        "\tgiveitem ITEM_HM04\n"
    )
    next_quest = "\tsetflag Cormoria_FLAG_SILVERSUN_NEXTQUEST\n"
    return_label = f"{prefix}_3:\n\treturn\n"
    if text.count(gift) != 1 or text.count(next_quest) != 1 or text.count(return_label) != 1:
        raise ScriptRegistrationError("Silversun Strength cutscene flow drift")
    text = text.replace(gift, (
        "\tcheckitem ITEM_HM04\n"
        f"\tgoto_if_eq VAR_RESULT, TRUE, {prefix}_StrengthOwned\n"
        "\tgiveitem ITEM_HM04\n"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {prefix}_StrengthSkipped\n"
        f"{prefix}_StrengthOwned::\n"
        "\tsetflag Cormoria_FLAG_SYS_GOT_STRENGTH\n"
        f"{prefix}_StrengthSkipped::\n"
    ), 1)
    text = text.replace(next_quest, next_quest + "\tsetvar VAR_TEMP_0, 1\n", 1)
    text = text.replace(return_label, (
        f"{prefix}_3:\n"
        "\tsetvar VAR_TEMP_0, 1\n"
        f"\tgoto_if_set Cormoria_FLAG_SYS_GOT_STRENGTH, {prefix}_StrengthDone\n"
        "\tcheckitem ITEM_HM04\n"
        f"\tgoto_if_eq VAR_RESULT, TRUE, {prefix}_StrengthReconciled\n"
        "\tlockall\n"
        "\tgiveitem ITEM_HM04\n"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {prefix}_StrengthBagFull\n"
        "\tsetflag Cormoria_FLAG_SYS_GOT_STRENGTH\n"
        "\treleaseall\n\tend\n"
        f"{prefix}_StrengthReconciled::\n"
        "\tsetflag Cormoria_FLAG_SYS_GOT_STRENGTH\n"
        f"{prefix}_StrengthDone::\n\treturn\n"
        f"{prefix}_StrengthBagFull::\n"
        f"\tmsgbox {prefix}_StrengthBagFull_Text_0, MSGBOX_DEFAULT\n"
        "\treleaseall\n\tend\n"
    ), 1)
    return text + (
        f"\n{prefix}_StrengthBagFull_Text_0:\n"
        '\t.string "The Bag is full. Make room for\\n"\n'
        '\t.string "Strength, then return to the city.$"\n'
    )


def _adapt_carabrue_gardevoir_reward(text: str) -> str:
    """Keep the one-time berry available when its pocket rejects delivery."""
    prefix = "Cormoria_CarabrueTown_TenebrisLab_Gardevoir"
    gift = "\tgiveitem ITEM_STARF_BERRY\n"
    failure = prefix + "_ItemFull"
    if text.count(gift) != 1 or failure in text:
        raise ScriptRegistrationError("Carabrue Gardevoir reward flow drift")
    return text.replace(gift, gift + f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n", 1) + (
        f"\n{failure}::\n\treleaseall\n\tend\n"
    )


def _adapt_carabrue_waterfall(text: str) -> str:
    """Do not replay the finale, and retry its HM after a full-Bag failure."""
    prefix = "Cormoria_CarabrueTown_TenebrisLab_PostFinale"
    start = "\tsetvar VAR_TEMP_1, 1\n"
    gift = "\tgiveitem ITEM_HM07\n"
    if text.count(start) != 1 or text.count(gift) != 1:
        raise ScriptRegistrationError("Carabrue Waterfall finale flow drift")
    text = text.replace(start, start + (
        f"\tgoto_if_set Cormoria_FLAG_POST_FINALE_CUTSCENE, {prefix}_WaterfallRetry\n"
    ), 1)
    text = text.replace(gift, (
        "\tcheckitem ITEM_HM07\n"
        f"\tgoto_if_eq VAR_RESULT, TRUE, {prefix}_WaterfallAlreadyOwned\n"
        "\tgiveitem ITEM_HM07\n"
        f"{prefix}_WaterfallAlreadyOwned::\n"
    ), 1)
    return text + (
        f"\n{prefix}_WaterfallRetry::\n"
        "\tcheckitem ITEM_HM07\n"
        f"\tgoto_if_eq VAR_RESULT, TRUE, {prefix}_WaterfallDone\n"
        "\tlockall\n"
        "\tgiveitem ITEM_HM07\n"
        f"\tgoto_if_eq VAR_RESULT, FALSE, {prefix}_WaterfallBagFull\n"
        "\treleaseall\n\tend\n"
        f"{prefix}_WaterfallBagFull::\n"
        f"\tmsgbox {prefix}_WaterfallBagFull_Text_0, MSGBOX_DEFAULT\n"
        "\treleaseall\n\tend\n"
        f"{prefix}_WaterfallDone::\n\treturn\n"
        f"{prefix}_WaterfallBagFull_Text_0:\n"
        '\t.string "The Bag is full. Make room for\\n"\n'
        '\t.string "Waterfall, then return here.$"\n'
    )


def _adapt_silversun_backstage_pass(text: str) -> str:
    """Do not retire the only pass pickup unless the Key Items pocket accepts it."""
    failure = "Cormoria_SilversunCityGym_EventScript_BackstagePass_ItemFull"
    owned = "Cormoria_SilversunCityGym_EventScript_BackstagePass_AlreadyOwned"
    gift = "\tgiveitem ITEM_BACKSTAGE_PASS\n"
    possession_anchor = (
        "\tgoto_if_set Cormoria_FLAG_SILVERSUN_BACKSTAGEPASS_GET, "
        "Cormoria_SilversunCityGym_EventScript_BackstagePass_2\n"
        '# 15 "data//maps/SilversunCityGym/scripts.pory"\n'
    )
    anchor = (
        '# 17 "data//maps/SilversunCityGym/scripts.pory"\n'
        "\tsetflag Cormoria_FLAG_SILVERSUN_BACKSTAGEPASS_GET\n"
        '# 18 "data//maps/SilversunCityGym/scripts.pory"\n'
        + gift
    )
    if (failure in text or owned in text or text.count(anchor) != 1
            or text.count(gift) != 1 or text.count(possession_anchor) != 1):
        raise ScriptRegistrationError("Silversun backstage pass flow drift")
    text = text.replace(possession_anchor, (
        possession_anchor.split('# 15', 1)[0]
        + "\tcheckitem ITEM_BACKSTAGE_PASS\n"
        + f"\tgoto_if_eq VAR_RESULT, TRUE, {owned}\n"
        + '# 15 "data//maps/SilversunCityGym/scripts.pory"\n'
    ), 1)
    text = text.replace(anchor, (
        '# 18 "data//maps/SilversunCityGym/scripts.pory"\n'
        + gift
        + f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n"
        + "\tsetflag Cormoria_FLAG_SILVERSUN_BACKSTAGEPASS_GET\n"
    ), 1)
    return text + (
        f"\n{owned}::\n"
        "\tsetflag Cormoria_FLAG_SILVERSUN_BACKSTAGEPASS_GET\n"
        "\tgoto Cormoria_SilversunCityGym_EventScript_BackstagePass_2\n"
        f"\n{failure}::\n"
        f"\tmsgbox {failure}_Text_0, MSGBOX_DEFAULT\n"
        "\treleaseall\n\treturn\n"
        f"\n{failure}_Text_0:\n"
        '\t.string "The Bag is full. Make room for the\\n"\n'
        '\t.string "Backstage Pass, then ask again.$"\n'
    )


def _adapt_galecrest_student_id(text: str) -> str:
    """Charge the fee and mark the ID obtained only after successful delivery."""
    failure = "Cormoria_GalecrestCity_DetectiveAcademy_Receptionist_StudentID_ItemFull"
    gift = "\tgiveitem ITEM_DETECTIVE_STUDENT_ID\n"
    anchor = (
        '# 158 "data//maps/GalecrestCity_DetectiveAcademy/scripts.pory"\n'
        + gift
        + '# 159 "data//maps/GalecrestCity_DetectiveAcademy/scripts.pory"\n'
        + "\tsetflag Cormoria_FLAG_GALECREST_STUDENTID_GET\n"
        + '# 160 "data//maps/GalecrestCity_DetectiveAcademy/scripts.pory"\n'
        + "\tremovemoney 1000\n"
    )
    owned_anchor = (
        "Cormoria_GalecrestCity_DetectiveAcademy_Receptionist_StudentID_2:\n"
        '# 141 "data//maps/GalecrestCity_DetectiveAcademy/scripts.pory"\n'
    )
    if (failure in text or text.count(anchor) != 1 or text.count(gift) != 1
            or text.count(owned_anchor) != 1):
        raise ScriptRegistrationError("Galecrest student ID flow drift")
    text = text.replace(anchor, anchor.replace(
        '# 159', f"\tgoto_if_eq VAR_RESULT, FALSE, {failure}\n# 159", 1), 1)
    text = text.replace(owned_anchor, (
        owned_anchor + "\tsetflag Cormoria_FLAG_GALECREST_STUDENTID_GET\n"
    ), 1)
    return text + (
        f"\n{failure}::\n"
        f"\tmsgbox {failure}_Text_0, MSGBOX_DEFAULT\n"
        "\treleaseall\n\treturn\n"
        f"\n{failure}_Text_0:\n"
        '\t.string "The Bag is full. Make room for the\\n"\n'
        '\t.string "Student ID, then ask again.$"\n'
    )


def build_preview(stage: Path, root: Path = ROOT) -> dict[str, bytes]:
    stage = stage.resolve(strict=True)
    root = root.resolve(strict=True)
    region, symbols, sources = import_world.load_manifests(root)
    source_index = {row["path"]: row for row in sources["files"]}
    stage_manifest = json.loads((stage / "staging_manifest.json").read_text(encoding="utf-8"))
    if (stage_manifest.get("provenance") != region["provenance"]
            or stage_manifest.get("manifest_sha256") != import_world.PINNED_SHA256
            or stage_manifest.get("world_id") != "cormoria"
            or stage_manifest.get("namespaced_script_source_count") != 188
            or stage_manifest.get("runtime_ready") is not False):
        raise ScriptRegistrationError("stage provenance or status drift")
    rows = stage_manifest["files"]
    records = {row["path"]: row for row in rows}
    if len(records) != len(rows):
        raise ScriptRegistrationError("duplicate staged source record")
    source_paths = {row["definition"].rsplit(":", 1)[0] for row in symbols["labels"]}
    root_script = "data/event_scripts.s"
    if root_script not in source_paths or len(source_paths) != 188:
        raise ScriptRegistrationError("pinned script source set drift")
    scripts = source_paths - {root_script}
    counts = {category: sum(path.startswith(f"data/{category}/") for path in scripts)
              for category in SCRIPT_COUNTS}
    if counts != SCRIPT_COUNTS or sum(counts.values()) != len(scripts):
        raise ScriptRegistrationError(f"script category counts drift: {counts}")
    staged_paths = {path.removeprefix("namespaced_scripts/") for path in records
                    if path.startswith("namespaced_scripts/")}
    if staged_paths != source_paths:
        raise ScriptRegistrationError("staged script source set differs from pinned ledger")
    identities = import_world.identities(region, symbols)
    berry_bindings = berry_plots.bindings(root)
    # The donor lets LOCALID_CLEF leak from another map's .set directive.
    # Resolve the two storage-room uses against that map's authenticated
    # object order instead of depending on global assembler include order.
    storage_map = json.loads(_stage_bytes(
        stage, "source/data/maps/SSElegant_Storage/map.json", records))
    storage_objects = storage_map["object_events"]
    clef_ids = [index + 1 for index, obj in enumerate(storage_objects)
                if obj["graphics_id"] == "OBJ_EVENT_GFX_SPECIES(CLEFABLE)"]
    if len(clef_ids) != 1:
        raise ScriptRegistrationError("S.S. Elegant storage Clefable identity drift")
    texts: dict[str, str] = {}
    for relative in sorted(source_paths):
        source = _stage_bytes(stage, f"source/{relative}", records)
        pin = source_index.get(relative)
        if pin is None or len(source) != pin["bytes"] or _sha(source) != pin["sha256"]:
            raise ScriptRegistrationError(f"source differs from pinned donor: {relative}")
        expected = import_world.rewrite_script(source.decode("utf-8-sig"), identities).encode("utf-8")
        actual = _stage_bytes(stage, f"namespaced_scripts/{relative}", records)
        if actual != expected:
            raise ScriptRegistrationError(f"namespaced source drift: {relative}")
        texts[relative] = actual.decode("utf-8")
    ordered = [path for path in INCLUDE.findall(texts[root_script]) if path in scripts]
    if len(ordered) != len(scripts) or set(ordered) != scripts:
        raise ScriptRegistrationError("donor event_scripts.s omits or repeats a campaign include")
    labels: dict[str, str] = {}
    for relative in ordered:
        if INCLUDE.search(texts[relative]):
            raise ScriptRegistrationError(f"nested include requires review: {relative}")
        for original in _definitions(texts[relative], relative):
            if original in labels:
                raise ScriptRegistrationError(f"duplicate campaign label: {original}")
            labels[original] = original if original.startswith("Cormoria_") else f"Cormoria_{original}"
    if len(set(labels.values())) != len(labels):
        raise ScriptRegistrationError("script namespace collision")
    host_labels = _host_labels(root)
    if host_labels.intersection(labels.values()):
        raise ScriptRegistrationError("campaign script duplicates a host global")
    rendered: dict[str, bytes] = {}
    shiny_gifts = 0
    donor_trade_references = {"INGAME_TRADE_WIMPOD": 0,
                              "INGAME_TRADE_HORSEA": 0,
                              "INGAME_TRADE_MEOWTH": 0}
    donor_partner_references = 0
    donor_number_input_references = 0
    donor_gacha_token_references = 0
    gacha_token_transformations = 0
    for relative in ordered:
        target = f"data/cormoria/{relative.removeprefix('data/')}"
        local_sets = LOCAL_SET.findall(texts[relative])
        if len(local_sets) != len(set(local_sets)):
            raise ScriptRegistrationError(f"duplicate local-ID definition within {relative}")
        prefix = re.sub(r'[^A-Za-z_0-9]', '_', relative.removeprefix('data/').removesuffix('.inc'))
        scoped_ids = {name: f"Cormoria_{prefix}_{name}" for name in local_sets}
        if relative == "data/maps/SSElegant_Storage/scripts.inc":
            if texts[relative].count("LOCALID_CLEF") != 2:
                raise ScriptRegistrationError("S.S. Elegant storage Clefable script drift")
            scoped_ids["LOCALID_CLEF"] = str(clef_ids[0])
        for trade in donor_trade_references:
            donor_trade_references[trade] += texts[relative].count(trade)
        donor_partner_references += texts[relative].count("PARTNER_ROUTE6_GAB")
        donor_number_input_references += texts[relative].count("MULTI_NUMBER_INPUT")
        rewritten = _rename(texts[relative], labels | berry_bindings | scoped_ids
                            | {"INGAME_TRADE_WIMPOD": "INGAME_TRADE_CORMORIA_WIMPOD",
                               "INGAME_TRADE_HORSEA": "INGAME_TRADE_CORMORIA_PINSIR",
                               "INGAME_TRADE_MEOWTH": "INGAME_TRADE_CORMORIA_HOUNDOUR",
                               "PARTNER_ROUTE6_GAB": "PARTNER_CORMORIA_GABRIELLE",
                               "MULTI_NUMBER_INPUT": "MULTI_CORMORIA_NUMBER_INPUT",
                               "FLAG_VISITED_RIVETSHORE_RANGER":
                                   "Cormoria_FLAG_VISITED_RIVETSHORE_RANGER"})
        rewritten, gift_count = _adapt_givemon_shininess(rewritten)
        if relative in INGAME_TRADE_SCRIPTS:
            rewritten = _adapt_ingame_trade_vars(rewritten, relative)
        overlay_labels: set[str] = set()
        if relative == PELLUCA_SAFARI:
            marker = "\tmsgbox Cormoria_PellucaCityFlooded_EventScript_TimesUp_Text_0\n"
            if rewritten.count(marker) != 1:
                raise ScriptRegistrationError("Pelluca rescue timeout script drift")
            cleanup = "Cormoria_PellucaCityFlooded_EventScript_FailCleanup"
            rewritten = rewritten.replace(marker, marker + cleanup + "::\n")
            overlay_labels.add(cleanup)
        if relative == RIVETSHORE_HARBOR:
            old_attendant = (
                "Cormoria_RivetshoreCity_Harbor_Attendant::\n"
                '# 19 "data//maps/RivetshoreCity_Harbor/scripts.pory"\n'
                "\tmsgbox Cormoria_RivetshoreCity_Harbor_Attendant_Text_0, MSGBOX_NPC\n"
                "\tend"
            )
            if rewritten.count(old_attendant) != 1:
                raise ScriptRegistrationError("Rivetshore harbor attendant script drift")
            overlay_path = root / "tools/cormoria/rivetshore_portal_overlay.inc"
            overlay = overlay_path.read_text(encoding="utf-8").rstrip()
            overlay_labels = set(_definitions(overlay, str(overlay_path)))
            rewritten = rewritten.replace(old_attendant, overlay)
        if relative == CHAMPIONSHIP_R5:
            # GameClear owns the first-clear decision and sets this world-local flag.
            # Setting it here would make the first victory look like a repeat.
            premature_clear = (
                '# 95 "data//maps/Championship_R5/scripts.pory"\n'
                '\tsetflag Cormoria_FLAG_SYS_GAME_CLEAR\n'
            )
            if rewritten.count(premature_clear) != 1:
                raise ScriptRegistrationError("Championship game-clear script drift")
            rewritten = rewritten.replace(premature_clear, "")
        if relative == GALECREST_CITY:
            rewritten = _adapt_galecrest_rock_smash(rewritten)
            overlay_labels.add("Cormoria_GalecrestCity_NPC_5_ItemFull")
            overlay_labels.add("Cormoria_GalecrestCity_NPC_5_ItemFull_Text_0")
        if relative == WINTERLILY_HOLLOW:
            rewritten = _adapt_winterlily_surf(rewritten)
            overlay_labels.add("Cormoria_WinterlilyHollow_NPC_SurfMan_ItemFull")
            overlay_labels.add("Cormoria_WinterlilyHollow_NPC_SurfMan_ItemFull_Text_0")
        if relative == SILVERSUN_CITY:
            rewritten = _adapt_silversun_strength(rewritten)
            overlay_labels.update({
                "Cormoria_SilversunCity_OnFrame_StrengthOwned",
                "Cormoria_SilversunCity_OnFrame_StrengthSkipped",
                "Cormoria_SilversunCity_OnFrame_StrengthReconciled",
                "Cormoria_SilversunCity_OnFrame_StrengthDone",
                "Cormoria_SilversunCity_OnFrame_StrengthBagFull",
                "Cormoria_SilversunCity_OnFrame_StrengthBagFull_Text_0",
            })
        if relative == CARABRUE_FINALE:
            rewritten = _adapt_carabrue_waterfall(rewritten)
            overlay_labels.update({
                "Cormoria_CarabrueTown_TenebrisLab_PostFinale_WaterfallAlreadyOwned",
                "Cormoria_CarabrueTown_TenebrisLab_PostFinale_WaterfallRetry",
                "Cormoria_CarabrueTown_TenebrisLab_PostFinale_WaterfallBagFull",
                "Cormoria_CarabrueTown_TenebrisLab_PostFinale_WaterfallDone",
                "Cormoria_CarabrueTown_TenebrisLab_PostFinale_WaterfallBagFull_Text_0",
            })
        if relative == SILVERSUN_GYM:
            rewritten = _adapt_silversun_backstage_pass(rewritten)
            overlay_labels.add("Cormoria_SilversunCityGym_EventScript_BackstagePass_AlreadyOwned")
            overlay_labels.add("Cormoria_SilversunCityGym_EventScript_BackstagePass_ItemFull")
            overlay_labels.add("Cormoria_SilversunCityGym_EventScript_BackstagePass_ItemFull_Text_0")
        if relative == GALECREST_ACADEMY:
            rewritten = _adapt_galecrest_student_id(rewritten)
            overlay_labels.add("Cormoria_GalecrestCity_DetectiveAcademy_Receptionist_StudentID_ItemFull")
            overlay_labels.add("Cormoria_GalecrestCity_DetectiveAcademy_Receptionist_StudentID_ItemFull_Text_0")
        if relative == GASTREE_GYM:
            rewritten = _adapt_gastree_item_rewards(rewritten)
            overlay_labels.add("Cormoria_GastreeGym_Red_ItemFull")
            overlay_labels.add("Cormoria_GastreeGym_LeaderBattle_RareShardRetry")
            overlay_labels.add("Cormoria_GastreeGym_LeaderBattle_RareShardItemFull")
            overlay_labels.add("Cormoria_GastreeGym_LeaderBattle_RareShardItemFull_Text_0")
        if relative == CARABRUE_HOME_2F:
            rewritten = _adapt_carabrue_welcome_package(rewritten)
            overlay_labels.add("Cormoria_CarabrueTown_Home2F_EventScript_PickUpBag_ItemFull")
            overlay_labels.add("Cormoria_CarabrueTown_Home2F_EventScript_PickUpBag_ItemFull_Text_0")
        if relative == CARABRUE_TENEBRIS_LAB:
            rewritten = _adapt_carabrue_starter_supplies(rewritten)
            rewritten = _adapt_carabrue_starter_capacity(rewritten)
            rewritten = _adapt_carabrue_gardevoir_reward(rewritten)
            overlay_labels.add("Cormoria_CarabrueTown_TenebrisLab_Gardevoir_ItemFull")
            overlay_labels.add("Cormoria_CarabrueTown_TenebrisLab_EventScript_Start_ItemFull")
            overlay_labels.add("Cormoria_CarabrueTown_TenebrisLab_EventScript_Start_ItemFull_Text_0")
            for suffix in ("StarterCapacity", "StarterCapacity_Ready", "StarterStorageFull", "StarterStorageFull_Text_0"):
                overlay_labels.add("Cormoria_CarabrueTown_TenebrisLab_EventScript_" + suffix)
        donor_gacha_token_references += texts[relative].count("removeitem ITEM_GACHA_TOKEN")
        if relative == GACHA_TOKEN_SETTLEMENT:
            rewritten, transformed = _adapt_gacha_token_settlement(rewritten)
            gacha_token_transformations += transformed
        shiny_gifts += gift_count
        expected_labels = {labels[name] for name in _definitions(texts[relative], relative)}
        expected_labels.update(overlay_labels)
        if set(_definitions(rewritten, target)) != expected_labels:
            raise ScriptRegistrationError(f"script label rewrite failed: {relative}")
        rendered[target] = rewritten.encode("utf-8")
    if shiny_gifts != 6:
        raise ScriptRegistrationError(f"donor forced-shininess gift count drifted: {shiny_gifts}")
    if any(count != 1 for count in donor_trade_references.values()):
        raise ScriptRegistrationError(f"donor in-game trade reference count drifted: {donor_trade_references}")
    if donor_partner_references != 1:
        raise ScriptRegistrationError(f"donor Route 6 partner reference count drifted: {donor_partner_references}")
    if donor_number_input_references != 1:
        raise ScriptRegistrationError(f"donor number-input menu reference count drifted: {donor_number_input_references}")
    if donor_gacha_token_references != 4:
        raise ScriptRegistrationError(
            f"donor Gacha token settlement site count drifted: {donor_gacha_token_references}"
        )
    if gacha_token_transformations != 4:
        raise ScriptRegistrationError(
            f"Gacha token settlement transformations drifted: {gacha_token_transformations}"
        )
    wrapper = "@ Cormoria content preview; include once from data/event_scripts.s.\n"
    wrapper += "".join(f'\t.include "data/cormoria/{path.removeprefix("data/")}"\n'
                       for path in ordered)
    rendered["data/cormoria/scripts.inc"] = wrapper.encode("utf-8")
    metadata = {"schema_version": 1, "world_id": "cormoria", "runtime_ready": False,
                "source_revision": region["provenance"]["revision"], "counts": counts,
                "include_order": ordered,
                "files": [{"path": path, "bytes": len(data), "sha256": _sha(data)}
                          for path, data in sorted(rendered.items())]}
    rendered["registration.json"] = (json.dumps(metadata, indent=2) + "\n").encode("utf-8")
    return rendered


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        output = args.output.resolve()
        stage = args.stage.resolve(strict=True)
        if (output.exists() or output.is_relative_to(ROOT.resolve())
                or output.is_relative_to(stage) or stage.is_relative_to(output)):
            raise ScriptRegistrationError("output must be fresh and external to stage and host")
        rendered = build_preview(stage)
        output.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="cormoria-script-preview-", dir=output.parent) as temporary:
            directory = Path(temporary)
            for relative, data in rendered.items():
                target = directory / import_world.safe_relative(relative)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            directory.rename(output)
        print(f"Prepared {len(rendered) - 2} Cormoria script includes at {output}")
    except (OSError, UnicodeError, ValueError, KeyError, TypeError) as exc:
        print(f"Cormoria script registration: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
