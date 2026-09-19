#!/usr/bin/env python3
"""Compile the selected Johto map scripts into a namespaced event unit.

The importer is intentionally fail closed.  It translates donor syntax only at
the event-script boundary and records unresolved runtime work instead of
emitting a linkable looking, partial campaign.
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Iterable, Mapping


CONTRACT_HASH = "sha256:c129907c33f21f94a1dc6f2ef6e2ff8e64253a364565626a862aaa635138dca8"
DONOR_REVISION = "751823abaf677020bcd72c45fe3e7cb2b8a576e4"
MANIFEST_REL = Path("data/johto/region_manifest.json")
SYMBOLS_REL = Path("data/johto/content_symbols.json")
DEPENDENCIES_REL = Path("data/johto/script_dependencies.json")
OUTPUT_REL = Path("data/johto/campaign_scripts.inc")


class ScriptError(ValueError):
    """A source-preserving compiler or preflight failure."""


@dataclass(frozen=True)
class Diagnostic:
    kind: str
    source: str
    line: int
    reason: str

    def render(self) -> str:
        return f"{self.source}:{self.line}: {self.kind}: {self.reason}"


@dataclass(frozen=True)
class MacroContract:
    """The positional ABI parsed from one host assembler macro."""

    required: int
    maximum: int | None
    parameters: tuple[str, ...]
    required_parameters: tuple[str, ...] = ()


@dataclass(frozen=True)
class _ExpressionToken:
    kind: str
    value: str


COMMAND_TRANSLATIONS = {
    "applymovement2": "johto_applymovement2",
    "givenamedmon": "johto_givenamedmon",
    "giveoddegg": "johto_giveoddegg",
    "removenamedmon": "johto_removenamedmon",
    "removegenericmon": "johto_removegenericmon",
    "baobacheckmon": "johto_baobacheckmon",
    "setwildbattleshiny": "johto_setwildbattleshiny",
    "buffermoncategory": "johto_buffermoncategory",
}

SPECIAL_TRANSLATIONS = {
    "CheckHooh": "Johto_CheckHooh",
    "CheckAerodactyl": "Johto_CheckAerodactyl",
    "CheckKabuto": "Johto_CheckKabuto",
    "CheckOmanyte": "Johto_CheckOmanyte",
    "CheckTogepi": "Johto_CheckTogepi",
    "CheckCelebi": "Johto_CheckCelebi",
    "NameRival": "Johto_NameRival",
    "EnterBugContestMode": "Johto_BeginBugContestAdmission",
    "ShowBugContestChosenMon": "Johto_ShowBugContestChosenMon",
}

POLICY_NATIVE_CALLS = {
    "SetTimeBasedEncounters": "direct JohtoWild_CurrentTime consumes encounter time",
    "DisableStaticRandomizer": "static randomizer is outside the approved campaign policy",
    "EnableStaticRandomizer": "static randomizer is outside the approved campaign policy",
}

# These donor shared movement blocks are mechanically equivalent to the
# existing host blocks.  Keep the alias list explicit: unrelated donor labels
# remain unresolved instead of being accepted merely because they look alike.
HOST_SYMBOL_ALIASES = {
    "Common_Movement_WalkUp1": "Common_Movement_WalkUp",
    "Common_Movement_WalkDown1": "Common_Movement_WalkDown",
    "Common_Movement_WalkLeft1": "Common_Movement_WalkLeft",
    "Common_Movement_WalkRight1": "Common_Movement_WalkRight",
    "Common_Text_ReceivedMon": "gText_PlayerObtainedTheMon",
    "Common_Text_PartyIsFull": "gText_NoMoreRoomForPokemon",
    "MoveTutor_EventScript_CanOnlyBeLearnedOnceFinite": "MoveTutor_EventScript_CanOnlyBeLearnedOnce",
    "EventScript_RockSmashHeartscale": "Johto_EventScript_RockSmashHeartscale",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_WouldYouLikeToPlay": "Route121_SafariZoneEntrance_Text_WouldYouLikeToPlay",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_PlayAnotherTime": "Route121_SafariZoneEntrance_Text_PlayAnotherTime",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_NotEnoughMoney": "Route121_SafariZoneEntrance_Text_NotEnoughMoney",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_ThatWillBe500Please": "Route121_SafariZoneEntrance_Text_ThatWillBe500Please",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_HereAreYourSafariBalls": "Route121_SafariZoneEntrance_Text_HereAreYourSafariBalls",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_Received30SafariBalls": "Route121_SafariZoneEntrance_Text_Received30SafariBalls",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_PleaseEnjoyYourself": "Route121_SafariZoneEntrance_Text_PleaseEnjoyYourself",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_PCIsFull": "Route121_SafariZoneEntrance_Text_PCIsFull",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_YouNeedPokeblockCase": "Route121_SafariZoneEntrance_Text_YouNeedPokeblockCase",
}

# Constants supplied by the other authenticated Johto/Kanto world units.  The
# list is intentionally finite and role-specific; an arbitrary unknown upper-
# case token is still rejected.
CAMPAIGN_ABI_CONSTANTS = {
    "HEAL_LOCATION_AZALEA_TOWN", "HEAL_LOCATION_BLACKTHORN_CITY",
    "HEAL_LOCATION_CELADON_CITY", "HEAL_LOCATION_CERULEAN_CITY",
    "HEAL_LOCATION_CHERRYGROVE_CITY", "HEAL_LOCATION_CIANWOOD_CITY",
    "HEAL_LOCATION_CINNABAR_ISLAND", "HEAL_LOCATION_ECRUTEAK_CITY",
    "HEAL_LOCATION_FUCHSIA_CITY", "HEAL_LOCATION_GOLDENROD_CITY",
    "HEAL_LOCATION_INDIGO_PLATEAU", "HEAL_LOCATION_LAVENDER_TOWN",
    "HEAL_LOCATION_MAHOGANYTOWN", "HEAL_LOCATION_MT_SILVER",
    "HEAL_LOCATION_NEW_BARK_TOWN", "HEAL_LOCATION_OLIVINE_CITY",
    "HEAL_LOCATION_PEWTER_CITY", "HEAL_LOCATION_ROUTE_32",
    "HEAL_LOCATION_SAFARI_ZONE_GATE", "HEAL_LOCATION_SAFFRON_CITY",
    "HEAL_LOCATION_VERMILION_CITY", "HEAL_LOCATION_VIOLET_CITY",
    "HEAL_LOCATION_VIRIDIAN_CITY",
    "INGAME_TRADE_MACHOP", "INGAME_TRADE_ONIX", "INGAME_TRADE_VOLTORB",
    "ITEM_LOST_ITEM", "ITEM_MACHINE_PART", "ITEM_RADIO", "ITEM_SQUIRT_BOTTLE",
    "MON_SATISFACTORY", "MON_UNSATISFACTORY", "MOVEMENT_TYPE_TOWER_BEAM",
    "METATILE_R26_21_Broken_Window",
    "MAP_BATTLE_FRONTIER_OUTSIDE_WEST", "MAP_BIRTH_ISLAND_EXTERIOR",
    "MAP_FARAWAY_ISLAND_ENTRANCE", "MAP_SOUTHERN_ISLAND_EXTERIOR",
    "MULTI_5FLOORS", "MULTI_7FLOORS", "MULTI_DAYS_OF_WEEK",
    "MULTI_ELDERQUIIZ1", "MULTI_ELDERQUIIZ2", "MULTI_ELDERQUIIZ3",
    "MULTI_ELDERQUIIZ4", "MULTI_ELDERQUIIZ5", "MULTI_GOLDSILVER",
    "MULTI_HOENN_STARTERS", "MULTI_KURT_BALLS", "MULTI_OLIVINE_HARBOR",
    "MULTI_PRIZE_MONS", "MULTI_VERMILION_HARBOR",
    "MUS_HG_ENCOUNTER_RIVAL", "MUS_HG_EUSINE", "MUS_HG_FOLLOW_ME_1",
    "MUS_HG_GOLDENROD", "MUS_HG_KIMONO_GIRL", "MUS_HG_KIMONO_GIRL_DANCE",
    "MUS_HG_NEW_BARK", "MUS_HG_OAK", "MUS_HG_POKEGEAR_REGISTERED",
    "MUS_HG_RADIO_POKE_FLUTE", "MUS_HG_RADIO_ROCKET", "MUS_HG_RIVAL_EXIT",
    "MUS_HG_ROCKET_TAKEOVER", "MUS_HG_TEAM_ROCKET_HQ", "MUS_HG_VS_HO_OH",
    "MUS_HG_VS_LUGIA", "SPECIAL_BATTLE_LANCE", "TUTOR_MOVE_HEADBUTT",
    "SCROLL_MULTI_BF_MOVE_TUTOR_3", "FLAG_MET_FRONTIER_ELEMENTAL_MOVE_TUTOR",
    "STR_VAR_2",
}

STATIC_CAMPAIGN_CLOSURE = """Johto_EventScript_RockSmashHeartscale::
\tgiveitem ITEM_HEART_SCALE
\treturn

BugContest_EventScript_TimesUp::
\tlockall
\tspecial Johto_RequestBugContestTimeout
\tplayse SE_DING_DONG
\tmsgbox Johto_BugContest_Text_TimesUp, MSGBOX_DEFAULT
\tclosemessage
\twaitse
\tgetpartysize
\tgoto_if_eq VAR_RESULT, 1, Johto_BugContest_EventScript_NoCatch
\tsetvar JOHTO_VAR_BUG_CONTEST_STATE, 3
\tmsgbox Johto_BugContest_Text_ChooseMon, MSGBOX_DEFAULT
Johto_BugContest_EventScript_ChooseMon::
\tspecial ChoosePartyMon
\twaitstate
\tgoto_if_eq VAR_0x8004, 0xFF, Johto_BugContest_EventScript_WrongMon
\tgoto_if_eq VAR_0x8004, 0, Johto_BugContest_EventScript_WrongMon
\tspecial Johto_JudgeBugContestSelectedMon
\tgoto_if_eq VAR_RESULT, 0, Johto_BugContest_EventScript_WrongMon
\tspecial Johto_PrepareBugContestSettlement
\tgoto_if_eq VAR_RESULT, FALSE, Johto_BugContest_EventScript_Abort
\tfadescreen FADE_TO_BLACK
\twarp MAP_NATIONAL_PARK_BUG_CONTEST, 5
\tdelay 90
\tend

Johto_BugContest_EventScript_WrongMon::
\tmsgbox Johto_BugContest_Text_WrongMon, MSGBOX_DEFAULT
\tgoto Johto_BugContest_EventScript_ChooseMon

Johto_BugContest_EventScript_NoCatch::
\tmsgbox Johto_BugContest_Text_NoCatch, MSGBOX_DEFAULT
\tgoto Johto_BugContest_EventScript_Abort

Johto_BugContest_EventScript_Abort::
\tspecial Johto_AbortBugContestAdmission
\tsetvar JOHTO_VAR_BUG_CONTEST_STATE, 0
\tgoto Johto_BugContest_EventScript_ReturnToGate

BugContestEventScript_Judging::
\tlockall
\tspecial Johto_ShowBugContestResult
\tbuffernumberstring STR_VAR_2, VAR_RESULT
\tmsgbox Johto_BugContest_Text_Result, MSGBOX_DEFAULT
Johto_BugContest_EventScript_RetryTransfer::
\tspecial Johto_TransferBugContestSelectedMon
\tgoto_if_eq VAR_RESULT, TRUE, Johto_BugContest_EventScript_RetryReward
\tmsgbox Johto_BugContest_Text_TransferFailed, MSGBOX_YESNO
\tgoto_if_eq VAR_RESULT, TRUE, Johto_BugContest_EventScript_RetryTransfer
\tgoto Johto_BugContest_EventScript_Abort

Johto_BugContest_EventScript_RetryReward::
\tspecial Johto_ClaimBugContestReward
\tgoto_if_eq VAR_RESULT, TRUE, Johto_BugContest_EventScript_Complete
\tmsgbox Johto_BugContest_Text_RewardFailed, MSGBOX_YESNO
\tgoto_if_eq VAR_RESULT, TRUE, Johto_BugContest_EventScript_RetryReward
\tspecial Johto_ForfeitBugContestReward
\tgoto_if_eq VAR_RESULT, FALSE, Johto_BugContest_EventScript_RetryReward

Johto_BugContest_EventScript_Complete::
\tspecial Johto_ExitBugContest
\tgoto_if_eq VAR_RESULT, FALSE, Johto_BugContest_EventScript_RetryReward
\tsetvar JOHTO_VAR_BUG_CONTEST_STATE, 0
\tmsgbox Johto_BugContest_Text_Complete, MSGBOX_DEFAULT
Johto_BugContest_EventScript_ReturnToGate::
\tfadescreen FADE_TO_BLACK
\tgoto_if_eq VAR_TEMP_1, 1, Johto_BugContest_EventScript_ReturnSide
\twarp MAP_GATE_NATIONAL_PARK, 7, 2
\treleaseall
\tend

Johto_BugContest_EventScript_ReturnSide::
\twarp MAP_GATE_NATIONAL_PARK, 35, 7
\treleaseall
\tend

Johto_BugContest_Text_TimesUp::
\t.string "Ding-dong! Time's up!$"
Johto_BugContest_Text_ChooseMon::
\t.string "The contest has ended.\\nChoose the caught POKéMON you want\\lto keep and have judged.$"
Johto_BugContest_Text_WrongMon::
\t.string "Choose one of the BUG POKéMON you\\ncaught during the contest.$"
Johto_BugContest_Text_NoCatch::
\t.string "You did not catch a POKéMON.\\nYour original party will be returned.$"
Johto_BugContest_Text_Result::
\t.string "Your {STR_VAR_1} placed number\\n{STR_VAR_2}! It will be sent to your PC.$"
Johto_BugContest_Text_TransferFailed::
\t.string "The PC is full, so the caught POKéMON\\ncould not be transferred. Retry?\\pChoosing NO ends the contest without\\lkeeping it.$"
Johto_BugContest_Text_RewardFailed::
\t.string "There is no room for the prize. Retry?\\pChoosing NO explicitly forfeits the\\lprize.$"
Johto_BugContest_Text_Complete::
\t.string "Thank you for participating!$"
"""

REVIEWED_EXTERNAL_WARPS = {
    ("OlivineCity_PortInside", 154): "MAP_SOUTHERN_ISLAND_EXTERIOR, 13, 22",
    ("OlivineCity_PortInside", 165): "MAP_BIRTH_ISLAND_EXTERIOR, 13, 23",
    ("OlivineCity_PortInside", 176): "MAP_FARAWAY_ISLAND_ENTRANCE, 13, 38",
    ("OlivineCity_PortInside", 184): "MAP_BATTLE_FRONTIER_OUTSIDE_WEST, 20, 67",
    ("VermilionCity_PortInside", 74): "MAP_SOUTHERN_ISLAND_EXTERIOR, 13, 22",
    ("VermilionCity_PortInside", 85): "MAP_BIRTH_ISLAND_EXTERIOR, 13, 23",
    ("VermilionCity_PortInside", 96): "MAP_FARAWAY_ISLAND_ENTRANCE, 13, 38",
    ("VermilionCity_PortInside", 104): "MAP_BATTLE_FRONTIER_OUTSIDE_WEST, 20, 67",
}

REVIEWED_NULL_POKEMARTS = {
    ("CherrygroveCity_Mart", 39): "Cherrygrove_Pokemart_Pokemart",
    ("VioletCity_Mart", 9): "Pokemart_DefaultItemList",
}

REQUIRED_CONTEST_CLOSURE_LABELS = {
    "BugContestEventScript_Judging",
    "BugContest_EventScript_TimesUp",
}

# Reviewed host labels used by the imported gift closure.  The source files
# are part of the host script ABI review; a spelling match in an arbitrary
# host file is not enough to make a donor reference valid.
REVIEWED_HOST_LABELS = {
    "Common_EventScript_GetGiftMonPartySlot": "Common_EventScript_GetGiftMonPartySlot",
    "Common_EventScript_NameReceivedBoxMon": "Common_EventScript_NameReceivedBoxMon",
    "Common_EventScript_NameReceivedPartyMon": "Common_EventScript_NameReceivedPartyMon",
    "Common_EventScript_TransferredToPC": "Common_EventScript_TransferredToPC",
    "gText_NicknameThisPokemon": "gText_NicknameThisPokemon",
}

# Commands that carry symbolic operands have an explicit role contract.  A
# command omitted from this table is treated as having no symbolic operands;
# adding a command without describing its roles therefore fails closed via
# its macro ABI check rather than accepting arbitrary identifiers.
OPERAND_ROLES = {
    "goto": ("label",),
    "call": ("label",),
    "switch": ("value",),
    "case": ("value", "label"),
    "map_script": ("value", "label"),
    "map_script_2": ("value", "value", "label"),
    "msgbox": ("text", "constant"),
    "message": ("text",),
    "applymovement": ("value", "movement", "constant"),
    "johto_applymovement2": ("value", "movement"),
    "setvar": ("variable", "value", "constant"),
    "addvar": ("variable", "value"),
    "subvar": ("variable", "value"),
    "additem": ("item", "value"),
    "finditem": ("item",),
    "checkitem": ("item", "value"),
    "setflag": ("flag",),
    "clearflag": ("flag",),
    "settrainerflag": ("trainer",),
    "cleartrainerflag": ("trainer",),
    "giveitem": ("item", "value"),
    "removeitem": ("item", "value"),
    "checkitemspace": ("item", "value"),
    "checkitemtype": ("item", "value"),
    "bufferitemname": ("variable", "item"),
    "bufferspeciesname": ("variable", "species"),
    "waitmovement": ("value",),
    "checkflag": ("flag",),
    "copyvar": ("variable", "variable", "constant"),
    "compare": ("value", "value"),
    "bufferpartymonnick": ("variable", "value"),
    "bufferstring": ("variable", "text"),
    "call_if_defeated": ("trainer", "label"),
    "checkplayergender": (),
    "dofieldeffect": ("constant",),
    "jump_up": (),
    "waitfieldeffect": ("constant",),
    "warpteleport": ("constant", "value", "value", "value"),
    "setmetatile": ("value", "value", "constant", "constant"),
    "setweather": ("constant",),
    "setmaplayoutindex": ("constant",),
    "setrespawn": ("constant",),
    "warp": ("constant", "value", "value"),
    "warpsilent": ("constant", "value", "value"),
    "warpdoor": ("constant", "value", "value"),
    "warphole": ("constant",),
    "setdynamicwarp": ("constant", "value", "value"),
    "setescapewarp": ("constant", "value", "value"),
    "multichoice": ("value", "value", "constant", "value"),
    "random": ("value",),
    "pokenavcall": ("text",),
    "pokemart": ("label",),
    "showmonpic": ("species", "value", "value"),
    "setobjectsubpriority": ("value", "constant", "value"),
    "givemon": ("species", "value", "item", "constant", "constant", "constant",
                 "constant", "constant", "constant", "constant", "constant", "constant",
                 "constant", "constant", "constant", "constant", "constant", "constant",
                 "constant", "constant", "constant", "constant", "constant", "constant",
                 "constant", "constant", "constant"),
    "seteventmon": ("species", "value", "item"),
    "setwildbattle": ("species", "value", "item", "species", "value", "item"),
    "setwildbattleshiny": ("species", "value", "item", "species", "value", "item"),
    "johto_setwildbattleshiny": ("species", "value", "item", "species", "value", "item"),
    "johto_removegenericmon": ("species",),
    "johto_buffermoncategory": ("variable", "species"),
    "playmoncry": ("species", "constant"),
    "trainerbattle_no_intro": ("trainer", "text"),
    "trainerbattle_single": ("trainer", "text", "text", "label"),
    "trainerbattle_double": ("trainer", "text", "text", "text", "label"),
    "trainerbattle_two_trainers": ("trainer", "trainer", "text", "text", "text", "label"),
    "trainerbattle_rematch": ("trainer", "text", "text"),
    "trainerbattle_rematch_double": ("trainer", "text", "text", "text"),
    "delay": ("value",),
}

# Condition-specific script spellings are generated from goto_if/call_if.
# Their concrete contracts are added by _macro_contracts below.
CONDITIONAL_PREFIXES = ("goto_if_", "call_if_")

# External labels are intentionally an allowlist of reviewed closures.  The
# importer must never infer an ABI from a spelling match elsewhere in the
# host tree.  Blocks are copied only from the three donor files reviewed by
# the campaign contract, and only when they are reachable from these seeds.
REVIEWED_DONOR_BLOCKS = {
    "data/scripts/movement.inc": {
        "Common_Movement_JumpUp1",
        "Common_Movement_JumpDown1",
    },
    "data/event_scripts.s": {
        "Common_EventScript_GiftMon",
        "Common_EventScript_GiftMonNamed",
    },
    "data/scripts/set_gym_trainers.inc": {
        "Common_EventScript_SetGymTrainers",
        "AzaleaTown_Gym_SetGymTrainers",
    },
    "data/maps/Route110_TrickHouseEntrance/scripts.inc": {
        "Route110_TrickHouseEntrance_EventScript_DoHidingSpotSparkle",
    },
    "data/maps/Route121_SafariZoneGate_SafariZoneEntrance/scripts.inc": {
        "Route121_SafariZoneGate_SafariZoneEntrance_EventScript_WelcomeAttendant",
        "Route121_SafariZoneGate_SafariZoneEntrance_EventScript_InfoAttendant",
    },
    "data/scripts/safari_zone.inc": {
        "Route121_SafariZoneGate_SafariZoneEntrance_Text_WelcomeToSafariZone",
        "Route121_SafariZoneGate_SafariZoneEntrance_Text_WelcomeFirstTime",
        "Route121_SafariZoneGate_SafariZoneEntrance_Text_ComeInAndEnjoy",
        "Route121_SafariZoneGate_SafariZoneEntrance_Text_FirstTimeInfo",
    },
    "data/maps/BattleFrontier_Lounge9/scripts.inc": {
        "BattleFrontier_Lounge9_Heal",
    },
    "data/maps/SSTidalRooms/scripts.inc": {
        "SSTidalRooms_EventScript_Bed_PokecenterChallenge",
    },
    "data/maps/TrainerHill_Courtyard/scripts.inc": {
        "TrainerHill_Courtyard_EventScript_LeftMoveTutor",
        "TrainerHill_Courtyard_EventScript_RightMoveTutor",
        "TrainerHill_Courtyard_EventScript_ElementalMoveTutor",
        "TrainerHill_Courtyard_ExchangeServiceCorner_EventScript_VitaminClerk",
        "TrainerHill_Courtyard_ExchangeServiceCorner_EventScript_HoldItemClerk",
    },
}

# The reviewed closure is part of the generated unit's provenance.  Keep a
# byte-level pin for each donor source as well as the repository revision so a
# dirty checkout cannot silently change the emitted closure.
REVIEWED_DONOR_SOURCE_HASHES = {
    "data/scripts/movement.inc": "d4b9a5ab513cdece5dd1d189957598cbb36ff14c4b38b36d096d3ff7699dc1bd",
    "data/event_scripts.s": "bdbab93db4933e90c0d4fde68b2e575a5bef60aec3663703e7f890378ce7d504",
    "data/scripts/set_gym_trainers.inc": "70b903633e969664163691f7003dea154bcb1d36a04c40e1d37964b81a8146f4",
    "data/maps/Route110_TrickHouseEntrance/scripts.inc": "1507e49dfba74ff0e2a6822b19fa919003e31bde21d8b88f24e106a52a5bf844",
    "data/maps/Route121_SafariZoneGate_SafariZoneEntrance/scripts.inc": "12a3c9f273e3533535b6ac670bf962125453b0148993e139dabdcf3fb48e1471",
    "data/scripts/safari_zone.inc": "29a6075a24117c767480cb026b5774d46b8fdf2e366f51fbd85457b4c7ab4ca8",
    "data/maps/BattleFrontier_Lounge9/scripts.inc": "8a682abbdea8ddabdcabc6dfb6df046a25a7db2c4fdc7d7e1b594dcbca1109bb",
    "data/maps/SSTidalRooms/scripts.inc": "2805231afacd6bd915c40facf75ba77df5bad11c8cd3915ca73bae7b2596f640",
    "data/maps/TrainerHill_Courtyard/scripts.inc": "2b445b02d832a2a1768d95dc71d86d546f53b494dfd5772dbf38c752403e37b1",
}

REVIEWED_DONOR_BLOCK_HASHES = {
    "Route110_TrickHouseEntrance_EventScript_DoHidingSpotSparkle":
        "29c486b152245d3981a112cd6c36092dbb7c82a9317ff316ee283708b2b6b3d3",
    "Route121_SafariZoneGate_SafariZoneEntrance_EventScript_WelcomeAttendant":
        "4fcbcf2740eb0b69229e497dcf0343eedd2281560a837a33bd0e948e3550be1a",
    "Route121_SafariZoneGate_SafariZoneEntrance_EventScript_InfoAttendant":
        "52d8e1cd2561c9159be120e8f2e523c16ccdb4ebe8db223159057a558cb520d7",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_WelcomeToSafariZone":
        "929c04dcfaaa83f1fdb27e9aa6cb00acfcbefec89cee7a9d6795659e4b052a17",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_WelcomeFirstTime":
        "f4e168dd7005e037127e2b8863af78da81c09883631978101b0f3e83e43d18a4",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_ComeInAndEnjoy":
        "6a822859854f5e18a7bf570dc4a20fcfe0566c62e17679c608eee9d31129986b",
    "Route121_SafariZoneGate_SafariZoneEntrance_Text_FirstTimeInfo":
        "c335ba3e7a29531daed409c4a6c232acbf75463ebfe0cb9530eeb40217403527",
    "BattleFrontier_Lounge9_Heal":
        "8d878122f416b04e022a26d40daa29409bf2ddbe9061fa76fb5181e6414b06ac",
    "SSTidalRooms_EventScript_Bed_PokecenterChallenge":
        "f750aa523229812c8e33a25b603a5e95a1242341b0930370def02f05f461ba51",
    "TrainerHill_Courtyard_EventScript_LeftMoveTutor":
        "a3708b9ee8574fb40a45e706e2c4822437c2551cea0796c65b139a0bdcf20b97",
    "TrainerHill_Courtyard_EventScript_RightMoveTutor":
        "65c6c94ba214bf0356076b7a2348cf7130c75f7c757089088ee16054a8ce0633",
    "TrainerHill_Courtyard_EventScript_ElementalMoveTutor":
        "ca8484f125c75dbb1f615678e55143d3ecd41b9c23fcb5ccc777f220b312e27b",
    "TrainerHill_Courtyard_ExchangeServiceCorner_EventScript_VitaminClerk":
        "fef994ed47a5b35f6605b077d64601716db20bac94936d9f6cbd572618f4384b",
    "TrainerHill_Courtyard_ExchangeServiceCorner_EventScript_HoldItemClerk":
        "f56ee6dc89499043b32c4cf4285b74d4d172ffaa814719e3224d92478fd87729",
}

# Selected maps can intentionally reference a script owned by a donor map that
# is not itself selected.  Pin every approved consumer as well as the closure
# source so a similarly named donor label cannot be chosen by suffix alone.
REVIEWED_EXTERNAL_OBJECT_SCRIPT_SITES = {
    "Route121_SafariZoneGate_SafariZoneEntrance_EventScript_WelcomeAttendant": (
        "SafariZoneGate_SafariZoneEntrance",
    ),
    "Route121_SafariZoneGate_SafariZoneEntrance_EventScript_InfoAttendant": (
        "FuchsiaCity_SafariZoneEntrance",
        "SafariZoneGate_SafariZoneEntrance",
    ),
    "BattleFrontier_Lounge9_Heal": (
        "SaffronCity_FightingDojo",
        "SaffronCity_FightingDojoVIP",
    ),
    "TrainerHill_Courtyard_EventScript_LeftMoveTutor": ("SaffronCity_FightingDojoVIP",),
    "TrainerHill_Courtyard_EventScript_RightMoveTutor": ("SaffronCity_FightingDojoVIP",),
    "TrainerHill_Courtyard_EventScript_ElementalMoveTutor": ("SaffronCity_FightingDojoVIP",),
    "TrainerHill_Courtyard_ExchangeServiceCorner_EventScript_VitaminClerk": (
        "SaffronCity_FightingDojoVIP",
    ),
    "TrainerHill_Courtyard_ExchangeServiceCorner_EventScript_HoldItemClerk": (
        "SaffronCity_FightingDojoVIP",
    ),
}

# These are the only native callable bindings accepted by this compiler.  A
# function declaration found in arbitrary C source is not evidence of the
# script ABI; the effects bit is part of the reviewed binding.
VERIFIED_NATIVE_ABI = {
    "UpdateFollowingPokemon": {"requests_effects": False},
}

# The source special registry is parsed from data/specials.inc.  Johto
# translations are registered in that file by the corresponding runtime
# unit, so the special translation below still fails closed on an old base.

PENDING_DEPENDENCIES = {
    "contest": {
        "required_symbol": "BeginBugContestAdmission/EndBugContestTransaction",
        "adapter": "j3-contest-script-adaptation",
        "locations": ["Gate_NationalPark", "NationalPark_BugContest", "data/scripts/bug_contest.inc"],
    },
    "berry": {
        "required_symbol": "Johto_BerryTreeScript",
        "adapter": "j3-berry-harvest",
        "locations": ["AzaleaTown", "Route29"],
    },
    "whirlpool": {
        "required_symbol": "EventScript_Whirlpool",
        "adapter": "j3-whirlpool-machine",
        "locations": ["DragonsDen_Cavern"],
    },
    "azalea-gym-trainers": {
        "required_symbol": "AzaleaTown_Gym_SetGymTrainers",
        "adapter": "j3-azalea-gym-trainerflags",
        "locations": ["AzaleaTown_Gym"],
    },
    "trainer-hill-elemental-tutor": {
        "required_symbol": "SCROLL_MULTI_BF_MOVE_TUTOR_3/GetBattleFrontierTutorMoveIndex",
        "adapter": "j8-trainer-hill-elemental-tutor-runtime",
        "locations": ["SaffronCity_FightingDojoVIP"],
    },
}


def _read_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ScriptError(f"missing tracked input: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ScriptError(f"invalid JSON {path}: {exc}") from exc


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _campaign_script_body(text: str, label: str) -> tuple[int, int, str]:
    marker = f"{label}::\n"
    start = text.find(marker)
    if start < 0:
        raise ScriptError(f"missing campaign transport label: {label}")
    if text.find(marker, start + len(marker)) >= 0:
        raise ScriptError(f"duplicate campaign transport label: {label}")
    body_start = start + len(marker)
    next_label = re.search(r"(?m)^[A-Za-z_][A-Za-z0-9_]*::\n", text[body_start:])
    body_end = body_start + (next_label.start() if next_label else len(text) - body_start)
    return body_start, body_end, text[body_start:body_end]


def _replace_required_once(text: str, old: str, new: str, label: str) -> str:
    if text.count(old) != 1:
        raise ScriptError(f"campaign transport source drift in {label}: expected one reviewed sequence")
    return text.replace(old, new, 1)


def _apply_transport_overrides(text: str) -> str:
    """Install the reviewed cross-region transport state machine.

    The donor scripts remain the source for their long animations and for the
    Later-Kanto initialization payload.  These label-scoped overrides fail
    closed if a reviewed body drifts, so regeneration cannot silently restore
    the old fixed-Later destinations or repeat the initialization on every
    voyage.
    """
    goldenrod = "Johto_GoldenrodCity_TrainStation_GoldenrodCity_TrainStation_EventScript_BoardTrain"
    goldenrod_map = "Johto_GoldenrodCity_TrainStation_GoldenrodCity_TrainStation_MapScripts"
    olivine = "Johto_OlivineCity_PortInside_OlivinePort_EventScript_ChoseVermilion"
    olivine_map = "Johto_OlivineCity_PortInside_OlivineCity_PortInside_MapScripts"
    maiden = "Johto_OlivineCity_PortInside_OlivinePort_EventScript_Sailor_MaidenVoyage"
    leave_boat = "Johto_SSAqua_1F_SSAqua_1F_EventScript_LeaveBoat"
    vermilion_return = "KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_ChoseOlivine"
    vermilion_maiden = "KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_Sailor_MaidenVoyage"
    vermilion_map = "KantoLater_VermilionCity_PortInside_VermilionCity_PortInside_MapScripts"
    saffron_return = "KantoLater_SaffronCity_TrainStation_SaffronStation_EventScript_BoardTrain"
    saffron_map = "KantoLater_SaffronCity_TrainStation_SaffronCity_TrainStation_MapScripts"
    labels = (
        goldenrod_map,
        goldenrod,
        olivine_map,
        olivine,
        maiden,
        leave_boat,
        vermilion_map,
        vermilion_return,
        vermilion_maiden,
        saffron_map,
        saffron_return,
    )
    spans = {label: _campaign_script_body(text, label) for label in labels}
    bodies = {label: span[2] for label, span in spans.items()}

    train_body = (
        "\tcall EventScript_ChooseKantoEra\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, 0, {goldenrod}_Cancel\n"
        "\tspecial Johto_RecordCurrentHeal\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {goldenrod}_TravelFailed\n"
        "\tspecial Johto_PrepareKantoTravel\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {goldenrod}_TravelFailed\n"
        f"\tgoto {goldenrod}_BeginTravel\n\n"
        f"{goldenrod}_BeginTravel::\n"
        + bodies[goldenrod]
    )
    train_body = _replace_required_once(
        train_body,
        "\twarp MAP_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION, 1\n\tdelay 60\n\tend\n",
        f"\tswitch JOHTO_VAR_PENDING_KANTO_DESTINATION\n"
        f"\tcase 2, {goldenrod}_WarpOriginal\n"
        f"\tcase 3, {goldenrod}_WarpLater\n"
        f"\tgoto {goldenrod}_TravelFailed\n\n"
        f"{goldenrod}_WarpOriginal::\n"
        "\twarp MAP_KANTO_ORIGINAL_SAFFRON_CITY_TRAIN_STATION, 140, 16\n"
        "\tdelay 60\n\tend\n\n"
        f"{goldenrod}_WarpLater::\n"
        "\twarp MAP_KANTO_LATER_SAFFRON_CITY_TRAIN_STATION, 1\n"
        "\tdelay 60\n\tend\n\n"
        f"{goldenrod}_Cancel::\n\trelease\n\tend\n\n"
        f"{goldenrod}_TravelFailed::\n"
        "\tspecial Johto_CancelKantoTravel\n\trelease\n\tend\n",
        goldenrod,
    )

    olivine_body = (
        "\tclosemessage\n\tdelay 20\n\tcheckitem ITEM_SS_TICKET\n"
        "\tbufferitemname STR_VAR_1, ITEM_SS_TICKET\n"
        "\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, Johto_OlivineCity_PortInside_OlivinePort_EventScript_Sailor_NoCredentials\n"
        "\tcall EventScript_ChooseKantoEra\n"
        "\tgoto_if_eq JOHTO_VAR_RESULT, 0, Johto_OlivineCity_PortInside_OlivinePort_EventScript_Sailor_Refused\n"
        "\tspecial Johto_RecordCurrentHeal\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {olivine}_TravelFailed\n"
        "\tspecial Johto_PrepareKantoTravel\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {olivine}_TravelFailed\n"
        f"\tgoto {olivine}_BeginTravel\n\n"
        f"{olivine}_BeginTravel::\n"
        "\tcall Johto_OlivineCity_PortInside_OlivinePort_EventScript_EnterShip\n"
        "\tswitch JOHTO_VAR_PENDING_KANTO_DESTINATION\n"
        f"\tcase 2, {olivine}_WarpOriginal\n"
        f"\tcase 3, {olivine}_WarpLater\n"
        f"\tgoto {olivine}_TravelFailed\n\n"
        f"{olivine}_WarpOriginal::\n"
        "\twarpsilent MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE, 8, 9\n"
        "\trelease\n\tend\n\n"
        f"{olivine}_WarpLater::\n"
        "\twarpsilent MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE, 8, 9\n"
        "\trelease\n\tend\n\n"
        f"{olivine}_TravelFailed::\n"
        "\tspecial Johto_CancelKantoTravel\n\trelease\n\tend\n"
    )

    maiden_body = (
        "\tmsgbox Johto_OlivineCity_PortInside_OlivinePort_Text_FlashTicket, MSGBOX_DEFAULT\n"
        "\tcall EventScript_ChooseKantoEra\n"
        "\tgoto_if_eq JOHTO_VAR_RESULT, 0, Johto_OlivineCity_PortInside_OlivinePort_EventScript_Sailor_Refused\n"
        "\tspecial Johto_RecordCurrentHeal\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {maiden}_TravelFailed\n"
        "\tspecial Johto_PrepareKantoTravel\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {maiden}_TravelFailed\n"
        f"\tgoto {maiden}_BeginTravel\n\n"
        f"{maiden}_BeginTravel::\n"
        "\tcall Johto_OlivineCity_PortInside_OlivinePort_EventScript_EnterShip\n"
        "\tsetvar JOHTO_VAR_SSAQUA_STATE, 1\n"
        "\tclearflag JOHTO_FLAG_HIDE_SSAQUA_1F_GRANDPA\n"
        "\tsetflag JOHTO_FLAG_HIDE_SSAQUA_ROOM_SSE_GRANDDAUGHTER\n\n"
        "\tclearflag JOHTO_FLAG_HIDE_SSAQUA_SAILOR\n"
        "\tclearflag JOHTO_FLAG_HIDE_SSAQUA_CAPTAINS_ROOM_GRANDDAUGHTER\n\n"
        "\twarpsilent MAP_SSAQUA_1F, 29, 3\n\trelease\n\tend\n\n"
        f"{maiden}_TravelFailed::\n"
        "\tspecial Johto_CancelKantoTravel\n\trelease\n\tend\n"
    )

    flagheap_match = re.search(r"(?ms)^\t@Kanto FLAGHEAP\n.*?^\t@end flagheap\n", bodies[leave_boat])
    if flagheap_match is None:
        raise ScriptError(f"campaign transport source drift in {leave_boat}: missing Later-Kanto initialization")
    flagheap = flagheap_match.group(0)
    leave_body = (
        "\tfadescreenswapbuffers FADE_TO_BLACK\n"
        "\tswitch JOHTO_VAR_PENDING_KANTO_DESTINATION\n"
        f"\tcase 2, {leave_boat}_ArriveOriginal\n"
        f"\tcase 3, {leave_boat}_ArriveLater\n"
        f"\tgoto {leave_boat}_TravelFailed\n\n"
        f"{leave_boat}_ArriveLater::\n"
        "\tsetvar JOHTO_VAR_SSAQUA_STATE, 7\n"
        "\twarp MAP_KANTO_LATER_VERMILION_CITY_PORT_INSIDE, 8, 9\n"
        "\trelease\n\tend\n\n"
        f"{leave_boat}_ArriveOriginal::\n"
        "\tsetvar JOHTO_VAR_SSAQUA_STATE, 7\n"
        "\twarp MAP_KANTO_ORIGINAL_VERMILION_CITY_PORT_INSIDE, 8, 9\n"
        "\trelease\n\tend\n\n"
        f"{leave_boat}_TravelFailed::\n"
        "\tspecial Johto_CancelKantoTravel\n"
        "\tfadescreenswapbuffers FADE_FROM_BLACK\n\trelease\n\tend\n\n"
        "Johto_EventScript_InitializeLaterKantoOnce::\n"
        "\tspecial Johto_NeedsLaterKantoInitialization\n"
        "\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, Johto_EventScript_InitializeLaterKantoOnce_AlreadyInitialized\n"
        + flagheap
        + "\tspecial Johto_MarkLaterKantoInitialized\n"
        "\treturn\n\n"
        "Johto_EventScript_InitializeLaterKantoOnce_AlreadyInitialized::\n"
        "\tsetvar JOHTO_VAR_RESULT, TRUE\n\treturn\n"
    )

    vermilion_body = _replace_required_once(
        bodies[vermilion_return],
        "\tcall KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_EnterShip\n",
        "\tspecial Johto_ChooseJohto\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {vermilion_return}_TravelFailed\n"
        "\tspecial Johto_RecordCurrentHeal\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {vermilion_return}_TravelFailed\n"
        "\tspecial Johto_PrepareKantoTravel\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {vermilion_return}_TravelFailed\n"
        "\tcall KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_EnterShip\n",
        vermilion_return,
    )

    vermilion_maiden_body = (
        "\tmsgbox KantoLater_VermilionCity_PortInside_VermilionPort_Text_FlashTicket, MSGBOX_DEFAULT\n"
        "\tspecial Johto_ChooseJohto\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {vermilion_maiden}_TravelFailed\n"
        "\tspecial Johto_RecordCurrentHeal\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {vermilion_maiden}_TravelFailed\n"
        "\tspecial Johto_PrepareKantoTravel\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {vermilion_maiden}_TravelFailed\n"
        "\tcall KantoLater_VermilionCity_PortInside_VermilionPort_EventScript_EnterShip\n"
        "\tsetvar JOHTO_VAR_SSAQUA_STATE, 1\n"
        "\twarpsilent MAP_SSAQUA_1F, 29, 3\n"
        "\trelease\n"
        "\tend\n\n"
        f"{vermilion_maiden}_TravelFailed::\n"
        "\tspecial Johto_CancelKantoTravel\n"
        "\trelease\n"
        "\tend\n"
    )
    vermilion_body = _replace_required_once(
        vermilion_body,
        "\twarpsilent MAP_OLIVINE_CITY_PORT_INSIDE, 8, 16\n\trelease\n\tend\n",
        "\twarpsilent MAP_OLIVINE_CITY_PORT_INSIDE, 8, 16\n"
        "\trelease\n\tend\n\n"
        f"{vermilion_return}_TravelFailed::\n"
        "\tspecial Johto_CancelKantoTravel\n\trelease\n\tend\n",
        vermilion_return,
    )

    saffron_body = (
        "\tspecial Johto_ChooseJohto\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {saffron_return}_TravelFailed\n"
        "\tspecial Johto_RecordCurrentHeal\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {saffron_return}_TravelFailed\n"
        "\tspecial Johto_PrepareKantoTravel\n"
        f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {saffron_return}_TravelFailed\n"
        + bodies[saffron_return]
    )
    saffron_body = _replace_required_once(
        saffron_body,
        "\twarp MAP_GOLDENROD_CITY_TRAIN_STATION, 19, 16\n\tdelay 60\n\tend\n",
        "\twarp MAP_GOLDENROD_CITY_TRAIN_STATION, 19, 16\n"
        "\tdelay 60\n\tend\n\n"
        f"{saffron_return}_TravelFailed::\n"
        "\tspecial Johto_CancelKantoTravel\n\trelease\n\tend\n",
        saffron_return,
    )

    goldenrod_map_body = (
        f"\tmap_script MAP_SCRIPT_ON_TRANSITION, {goldenrod_map}_OnTransition\n"
        + bodies[goldenrod_map]
        + f"\n{goldenrod_map}_OnTransition::\n"
        "\tspecial Johto_CommitKantoTravel\n\tend\n"
    )
    olivine_map_body = (
        f"\tmap_script MAP_SCRIPT_ON_TRANSITION, {olivine_map}_OnTransition\n"
        "\t.byte 0\n\n"
        f"{olivine_map}_OnTransition::\n"
        "\tspecial Johto_CommitKantoTravel\n\tend\n"
    )

    def later_arrival_map_body(map_label: str, original_body: str) -> str:
        if "\t.byte 0\n" not in original_body:
            raise ScriptError(f"campaign transport source drift in {map_label}: missing map-script terminator")
        scripts = original_body.replace(
            "\t.byte 0\n",
            f"\tmap_script MAP_SCRIPT_ON_TRANSITION, {map_label}_OnTransition\n\t.byte 0\n",
            1,
        )
        return (
            scripts
            + f"\n{map_label}_OnTransition::\n"
            "\tcall Johto_EventScript_InitializeLaterKantoOnce\n"
            f"\tgoto_if_eq JOHTO_VAR_RESULT, FALSE, {map_label}_ArrivalFailed\n"
            "\tspecial Johto_CommitKantoTravel\n"
            "\tend\n\n"
            f"{map_label}_ArrivalFailed::\n"
            "\tspecial Johto_CancelKantoTravel\n"
            "\tend\n"
        )

    replacements = {
        goldenrod_map: goldenrod_map_body,
        goldenrod: train_body,
        olivine_map: olivine_map_body,
        olivine: olivine_body,
        maiden: maiden_body,
        leave_boat: leave_body,
        vermilion_map: later_arrival_map_body(vermilion_map, bodies[vermilion_map]),
        vermilion_return: vermilion_body,
        vermilion_maiden: vermilion_maiden_body,
        saffron_map: later_arrival_map_body(saffron_map, bodies[saffron_map]),
        saffron_return: saffron_body,
    }
    for label, (start, end, _) in sorted(spans.items(), key=lambda item: item[1][0], reverse=True):
        text = text[:start] + replacements[label] + text[end:]
    return text


TRANSPORT_SOURCE_MAPS = {
    "GoldenrodCity_TrainStation",
    "OlivineCity_PortInside",
    "SSAqua_1F",
    "VermilionCity_PortInside",
    "SaffronCity_TrainStation",
}


def _identifiers(text: str) -> set[str]:
    return set(re.findall(r"\b[A-Za-z_][A-Za-z0-9_]*\b", text))


def _replace_tokens(text: str, replacements: Mapping[str, str]) -> str:
    """Replace identifiers while leaving quoted strings and comments bytewise."""
    if not replacements:
        return text
    out: list[str] = []
    i = 0
    quote: str | None = None
    block_comment = False
    n = len(text)
    while i < n:
        if block_comment:
            if text.startswith("*/", i):
                out.append("*/")
                i += 2
                block_comment = False
            else:
                out.append(text[i])
                i += 1
            continue
        if quote is not None:
            ch = text[i]
            out.append(ch)
            i += 1
            if ch == "\\" and i < n:
                out.append(text[i])
                i += 1
            elif ch == quote:
                quote = None
            continue
        if text.startswith("/*", i):
            out.append("/*")
            i += 2
            block_comment = True
            continue
        if text[i] in ('"', "'"):
            quote = text[i]
            out.append(text[i])
            i += 1
            continue
        if text[i] in ("@",) or text.startswith("//", i):
            j = text.find("\n", i)
            if j < 0:
                out.append(text[i:])
                break
            out.append(text[i:j])
            i = j
            continue
        match = re.match(r"[A-Za-z_][A-Za-z0-9_]*", text[i:])
        if match:
            token = match.group(0)
            out.append(replacements.get(token, token))
            i += len(token)
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def _strip_comment(line: str) -> str:
    result: list[str] = []
    quote: str | None = None
    i = 0
    while i < len(line):
        ch = line[i]
        if quote:
            result.append(ch)
            if ch == "\\" and i + 1 < len(line):
                result.append(line[i + 1])
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if ch in ('"', "'"):
            quote = ch
            result.append(ch)
            i += 1
            continue
        if ch == "@" or line.startswith("//", i) or line.startswith("/*", i):
            break
        result.append(ch)
        i += 1
    return "".join(result)


def _label_definition(line: str) -> tuple[str, str] | None:
    match = re.match(r"^\s*([A-Za-z_][A-Za-z0-9_]*|[0-9]+)(::|:)\s*(?:@.*)?$", line)
    if match:
        return match.group(1), match.group(2)
    return None


def _macro_names(root: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for path in (root / "asm/macros").rglob("*.inc"):
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for name in re.findall(r"^\s*\.macro\s+([A-Za-z_][A-Za-z0-9_]*)", text, re.M):
            result.setdefault(name.lower(), name)
        # Movement actions are generated from the public create_movement_action
        # declaration rather than .macro definitions.  They are still part of
        # the assembler command ABI and must not be treated as donor-only
        # commands during a campaign preflight.
        for name in re.findall(r"^\s*create_movement_action\s+([A-Za-z_][A-Za-z0-9_]*)\s*,", text, re.M):
            result.setdefault(name.lower(), name)
    for name, target in COMMAND_TRANSLATIONS.items():
        result.setdefault(name.lower(), target)
    for name in ("johto_applymovement2", "johto_givenamedmon", "johto_giveoddegg",
                 "johto_removenamedmon", "johto_removegenericmon", "johto_baobacheckmon",
                 "johto_setwildbattleshiny", "johto_buffermoncategory"):
        result.setdefault(name.lower(), name)
    return result


def _macro_contracts(root: Path) -> dict[str, MacroContract]:
    """Parse the host macro declarations into required/optional arities.

    This intentionally reads declarations rather than using the semantic role
    table as an ABI signature.  The role table answers *what* an operand means;
    this catalog answers *how many* operands the assembler macro accepts.
    """
    result: dict[str, MacroContract] = {}
    if not (root / "asm/macros").exists():
        return result
    parameter_re = re.compile(
        r"([A-Za-z_][A-Za-z0-9_]*)(?::(req|vararg))?(?:=([^,\s]+))?"
    )
    paths = sorted((root / "asm/macros").rglob("*.inc"),
                   key=lambda item: (0 if item.name.lower() == "event.inc" else 1, str(item)))
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for match in re.finditer(
            r"^\s*\.macro\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s+(.*?))?\s*$", text, re.M
        ):
            name, declaration = match.groups()
            declaration = _strip_comment(declaration or "").strip()
            params: list[str] = []
            required = 0
            required_parameters: list[str] = []
            variadic = False
            for segment in _split_operands(declaration or ""):
                for param in parameter_re.finditer(segment):
                    param_name = param.group(1)
                    marker = param.group(2)
                    default = param.group(3)
                    # A declaration segment can contain the two space-separated
                    # trainerbattle parameters before its first comma.  The
                    # regex above deliberately captures both.
                    params.append(param_name)
                    if marker == "req":
                        required += 1
                        required_parameters.append(param_name)
                    if marker == "vararg":
                        variadic = True
                    elif default is not None:
                        pass
            if not params:
                contract = MacroContract(0, 0, (), ())
            else:
                contract = MacroContract(required, None if variadic else len(params), tuple(params), tuple(required_parameters))
            result.setdefault(name.lower(), contract)
        for action in re.findall(r"^\s*create_movement_action\s+([A-Za-z_][A-Za-z0-9_]*)\s*,", text, re.M):
            result.setdefault(action.lower(), MacroContract(0, 0, ()))
    # The event assembler generates condition-specific spellings from the
    # two-operand goto_if/call_if macros.  Their script syntax includes the
    # condition as an explicit first operand.
    for prefix in CONDITIONAL_PREFIXES:
        for suffix in ("eq", "ne", "lt", "le", "gt", "ge"):
            result.setdefault(prefix + suffix, MacroContract(3, 3, ("value", "value", "destination"), ("value", "value", "destination")))
        for suffix in ("set", "unset"):
            result.setdefault(prefix + suffix, MacroContract(2, 2, ("flag", "destination"), ("flag", "destination")))
    # Adapter macros are emitted by the Johto extension include.  In the
    # detached fixer base those declarations are intentionally not present;
    # retain their reviewed concrete source ABI so the validator still checks
    # every supplied operand instead of falling back to supplied-count.
    adapter_abis = {
        "johto_applymovement2": (2, 2),
        "johto_givenamedmon": (1, 1),
        "johto_giveoddegg": (1, 1),
        "johto_removenamedmon": (1, 1),
        "johto_removegenericmon": (1, 1),
        "johto_baobacheckmon": (1, 1),
        "johto_setwildbattleshiny": (2, 6),
        "johto_buffermoncategory": (2, 2),
    }
    for name, (required, maximum) in adapter_abis.items():
        parameters = tuple(f"arg{i}" for i in range(maximum))
        result.setdefault(name, MacroContract(required, maximum, parameters, parameters[:required]))
    return result


def _tokenize_expression(text: str) -> tuple[list[_ExpressionToken], str | None]:
    """Tokenize the supported assembler expression subset."""
    stripped = text.strip()
    if not stripped:
        return [], "empty expression"
    tokens: list[_ExpressionToken] = []
    i = 0
    while i < len(stripped):
        ch = stripped[i]
        if ch.isspace():
            i += 1
            continue
        if ch in ('"', "'"):
            quote = ch
            j = i + 1
            escaped = False
            while j < len(stripped):
                if escaped:
                    escaped = False
                elif stripped[j] == "\\":
                    escaped = True
                elif stripped[j] == quote:
                    break
                j += 1
            if j >= len(stripped) or stripped[j + 1:].strip():
                return [], "quoted text must be the complete operand"
            tokens.append(_ExpressionToken("quoted", stripped[i:j + 1]))
            return tokens, None
        if stripped.startswith(("0x", "0X"), i):
            match = re.match(r"0[xX][0-9A-Fa-f]+", stripped[i:])
            if match is None:
                return [], "malformed hexadecimal literal"
            end = i + len(match.group(0))
            if end < len(stripped) and (stripped[end].isalnum() or stripped[end] == "_"):
                return [], "adjacent tokens after hexadecimal literal"
            tokens.append(_ExpressionToken("number", match.group(0)))
            i = end
            continue
        if stripped.startswith(("0b", "0B"), i):
            match = re.match(r"0[bB][01]+", stripped[i:])
            if match is None:
                return [], "malformed binary literal"
            end = i + len(match.group(0))
            if end < len(stripped) and (stripped[end].isalnum() or stripped[end] == "_"):
                return [], "adjacent tokens after binary literal"
            tokens.append(_ExpressionToken("number", match.group(0)))
            i = end
            continue
        if ch.isdigit():
            match = re.match(r"[0-9]+", stripped[i:])
            assert match is not None
            end = i + len(match.group(0))
            tokens.append(_ExpressionToken("number", match.group(0)))
            i = end
            continue
        if ch.isalpha() or ch == "_":
            match = re.match(r"[A-Za-z_][A-Za-z0-9_]*", stripped[i:])
            assert match is not None
            tokens.append(_ExpressionToken("identifier", match.group(0)))
            i += len(match.group(0))
            continue
        if stripped.startswith(("<<", ">>"), i):
            tokens.append(_ExpressionToken("operator", stripped[i:i + 2]))
            i += 2
            continue
        if ch in "+-*/%&|^~()":
            kind = "paren" if ch in "()" else "operator"
            tokens.append(_ExpressionToken(kind, ch))
            i += 1
            continue
        return [], f"unsupported expression character {ch!r}"
    return tokens, None


def _parse_expression(tokens: list[_ExpressionToken]) -> tuple[list[str], str | None]:
    """Parse expression tokens and return symbolic identifiers in order."""
    if not tokens:
        return [], "empty expression"
    if len(tokens) == 1 and tokens[0].kind == "quoted":
        return [], None
    position = 0
    identifiers: list[str] = []
    precedence = {"|": 1, "^": 2, "&": 3, "<<": 4, ">>": 4,
                  "+": 5, "-": 5, "*": 6, "/": 6, "%": 6}

    def primary() -> str | None:
        nonlocal position
        if position >= len(tokens):
            return "expected expression"
        token = tokens[position]
        if token.kind == "operator" and token.value in {"+", "-", "~"}:
            position += 1
            return primary()
        if token.kind in {"number", "identifier"}:
            if token.kind == "identifier":
                identifiers.append(token.value)
            position += 1
            return None
        if token.kind == "paren" and token.value == "(":
            position += 1
            error = expression(1)
            if error:
                return error
            if position >= len(tokens) or tokens[position].value != ")":
                return "unclosed parenthesized expression"
            position += 1
            return None
        return f"unexpected token {token.value!r}"

    def expression(min_precedence: int) -> str | None:
        nonlocal position
        error = primary()
        if error:
            return error
        while position < len(tokens):
            token = tokens[position]
            if token.kind == "paren" and token.value == ")":
                break
            if token.kind != "operator" or token.value not in precedence:
                return f"unexpected token {token.value!r}"
            level = precedence[token.value]
            if level < min_precedence:
                break
            position += 1
            error = expression(level + 1)
            if error:
                return error
        return None

    error = expression(1)
    if error:
        return identifiers, error
    if position != len(tokens):
        return identifiers, f"unexpected token {tokens[position].value!r}"
    return identifiers, None


def _source_labels(text: str) -> set[str]:
    labels = set()
    for line in text.splitlines():
        definition = _label_definition(line)
        if definition:
            labels.add(definition[0])
    return labels


def _declared_constants(root: Path) -> set[str]:
    """Return constants from the host constant headers only.

    Keeping this catalog separate from labels, specials, and C functions is
    the important part: an event operand cannot become valid merely because
    an unrelated file happened to contain the same identifier.
    """
    result: set[str] = set()
    base = root / "include" / "constants"
    if not base.exists():
        return result
    for path in base.rglob("*.h"):
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        result.update(re.findall(r"^\s*#define\s+([A-Za-z_][A-Za-z0-9_]*)", text, re.M))
        # Most constants are enum members.  Restrict this pattern to names
        # that are visibly constants and avoid collecting struct fields.
        result.update(re.findall(r"^\s*([A-Z][A-Z0-9_]+)\s*(?:=|,)", text, re.M))
    # Event macro files contain assembler-time enum/default assignments such
    # as MSGBOX_DEFAULT.  They are constants, while the macro names remain a
    # separate command-role catalog.
    macro_base = root / "asm" / "macros"
    if macro_base.exists():
        for path in macro_base.rglob("*.inc"):
            try:
                text = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            result.update(re.findall(r"^\s*([A-Z][A-Z0-9_]+)\s*=", text, re.M))
            for default in re.findall(r"\b[A-Za-z_][A-Za-z0-9_]*\s*=\s*([A-Z][A-Z0-9_]*)", text):
                result.add(default)
    result.update(_generated_tm_hm_constants(root))
    return result


def _generated_tm_hm_constants(root: Path) -> set[str]:
    """Expand the authoritative FOREACH_TM/FOREACH_HM item aliases.

    ``items.h`` creates ITEM_TM_<name> and ITEM_HM_<name> through a
    preprocessor zip, so those aliases do not occur as literal declarations.
    Read the source list in tms_hms.h instead of accepting arbitrary prefixes.
    """
    path = root / "include" / "constants" / "tms_hms.h"
    if not path.is_file():
        return set()
    text = path.read_text(encoding="utf-8", errors="ignore")
    result: set[str] = set()
    active_prefix: str | None = None
    for line in text.splitlines():
        define = re.match(r"\s*#define\s+(FOREACH_TM|FOREACH_HM)\(F\)", line)
        if define:
            active_prefix = "ITEM_TM_" if define.group(1) == "FOREACH_TM" else "ITEM_HM_"
            continue
        if active_prefix and re.match(r"\s*#define\s+", line):
            active_prefix = None
        if active_prefix:
            item = re.search(r"\bF\(([A-Z][A-Z0-9_]*)\)", line)
            if item:
                result.add(active_prefix + item.group(1))
    return result


def _registered_specials(root: Path) -> set[str]:
    path = root / "data" / "specials.inc"
    if not path.is_file():
        return set()
    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return set()
    return set(re.findall(r"^\s*def_special\s+([A-Za-z_][A-Za-z0-9_]*)", text, re.M))


def _approved_host_labels(root: Path) -> set[str]:
    """Return labels from the host's linked event-script include graph."""
    result: set[str] = set(HOST_SYMBOL_ALIASES.values())
    pending = [root / "data/event_scripts.s"]
    visited: set[Path] = set()
    root_resolved = root.resolve()
    while pending:
        path = pending.pop()
        try:
            resolved = path.resolve()
            resolved.relative_to(root_resolved)
        except (OSError, ValueError):
            continue
        if resolved in visited or not resolved.is_file():
            continue
        visited.add(resolved)
        try:
            text = resolved.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        result.update(_source_labels(text))
        for include in re.findall(r'(?m)^\s*\.include\s+["<]([^">]+)[">]', text):
            pending.append(root / Path(include))
    return result


def _normalize_sha256(path: Path) -> str:
    """Hash source text with repository line endings normalized."""
    text = path.read_text(encoding="utf-8", errors="strict")
    return hashlib.sha256(text.replace("\r\n", "\n").encode("utf-8")).hexdigest()


def _portable_path(path: Path, root: Path) -> str:
    """Render a donor-relative POSIX source path for durable diagnostics."""
    try:
        return str(path.resolve().relative_to(root.resolve())).replace("\\", "/")
    except ValueError:
        return path.name


def _labeled_blocks(text: str) -> dict[str, str]:
    """Split an assembler script into its label-preserving blocks."""
    blocks: dict[str, list[str]] = {}
    current: str | None = None
    for line in text.splitlines(keepends=True):
        definition = _label_definition(line.rstrip("\r\n"))
        if definition:
            current = definition[0]
            blocks.setdefault(current, []).append(line)
        elif current is not None:
            blocks[current].append(line)
    return {key: "".join(value) for key, value in blocks.items()}


def _block_references(text: str) -> set[str]:
    """Extract label operands from a reviewed donor block."""
    result: set[str] = set()
    for line in text.splitlines():
        clean = _strip_comment(line).strip()
        match = re.match(r"(?:goto|call|switch|case|map_script(?:_2)?|call_if(?:_[a-z]+)?|goto_if(?:_[a-z]+)?|msgbox|message|applymovement)\b(.*)$", clean)
        if not match:
            continue
        result.update(_identifiers(match.group(1)))
    return result


def _split_operands(body: str) -> list[str]:
    """Split a script operand list without splitting quoted strings."""
    operands: list[str] = []
    current: list[str] = []
    quote: str | None = None
    escaped = False
    for char in body:
        if quote is not None:
            current.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in ('"', "'"):
            quote = char
            current.append(char)
        elif char == ",":
            operands.append("".join(current).strip())
            current = []
        else:
            current.append(char)
    if current or body.strip():
        operands.append("".join(current).strip())
    return operands


def _normalized_donor_operands(command: str, body: str, contract: MacroContract) -> list[str]:
    """Parse the donor's few GNU-as whitespace-separated macro arguments.

    Most source calls use commas.  The pinned corpus also contains four
    stable shapes where GNU as accepts whitespace as an argument separator.
    Normalize only those ABI-proven shapes; arbitrary whitespace inside an
    expression remains part of that expression and is still validated.
    """
    operands = _split_operands(body)
    key = command.lower()
    if key == "trainerbattle_single" and len(operands) == 2:
        head = operands[0].split()
        if len(head) == 2:
            operands = [head[0], head[1], operands[1]]
    elif key == "msgbox" and len(operands) == 1:
        parts = operands[0].split()
        if len(parts) == 2 and parts[1].startswith("MSGBOX_"):
            operands = parts
    elif key in {"call_if_eq", "map_script_2"} and len(operands) == 2:
        tail = operands[1].split()
        if len(tail) == 2:
            operands = [operands[0], tail[0], tail[1]]
    elif key == "setvar" and len(operands) == 3 and not operands[2].strip():
        # A trailing comma is accepted by GNU as and binds no optional
        # argument.  Drop it so the host macro receives its declared default.
        operands = operands[:2]
    elif len(operands) == 1 and "," not in body and contract.required > 1:
        whitespace_operands = body.split()
        if len(whitespace_operands) >= contract.required:
            operands = whitespace_operands
    return operands


def _operand_identifier(operand: str) -> str | None:
    match = re.match(r"(?:\s*)([A-Za-z_][A-Za-z0-9_]*|[-+]?\d+)", operand)
    return match.group(1) if match else None


class ScriptCompiler:
    def __init__(self, root: str | Path, donor_root: str | Path):
        self.root = Path(root).resolve()
        self.donor_root = Path(donor_root).resolve()
        self.manifest = _read_json(self.root / MANIFEST_REL)
        self.symbols = _read_json(self.root / SYMBOLS_REL)
        if not isinstance(self.manifest, dict) or not isinstance(self.symbols, dict):
            raise ScriptError("manifest and content symbol ledger must be objects")
        selection = self.manifest.get("maps")
        if not isinstance(selection, list) or len(selection) != 407:
            raise ScriptError("region manifest must contain exactly 407 selected maps")
        self.records = selection
        self.record_by_name: dict[str, dict[str, object]] = {}
        self.script_namespaces: dict[str, str] = {}
        self.manifest_symbol_map: dict[str, str] = {}
        seen_namespaces: set[str] = set()
        for ordinal, record in enumerate(selection):
            if not isinstance(record, dict):
                raise ScriptError(f"malformed manifest record at ordinal {ordinal}")
            if type(record.get("ordinal")) is not int or record["ordinal"] != ordinal:
                raise ScriptError(f"manifest ordinal mismatch at index {ordinal}")
            expected_era = "JOHTO" if ordinal < 239 else "KANTO_LATER"
            if (record.get("era"), record.get("campaign"), record.get("world_era")) != (
                expected_era,
                expected_era,
                expected_era,
            ):
                raise ScriptError(f"wrong campaign era at ordinal {ordinal}")
            name = record.get("source_name")
            if not isinstance(name, str) or re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) is None:
                raise ScriptError(f"manifest map has unsafe source_name at ordinal {ordinal}")
            identity = record.get("identity_namespace")
            expected_namespace = ("Johto_" if ordinal < 239 else "KantoLater_") + name
            if not isinstance(identity, dict) or identity.get("script") != expected_namespace:
                raise ScriptError(f"wrong-era script namespace at ordinal {ordinal}")
            namespace = identity["script"]
            if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", namespace) is None:
                raise ScriptError(f"unsafe script namespace at ordinal {ordinal}")
            if namespace in seen_namespaces:
                raise ScriptError(f"duplicate script namespace at ordinal {ordinal}: {namespace}")
            seen_namespaces.add(namespace)
            source_map = record.get("source_map")
            target_map = identity.get("map")
            layout = record.get("layout")
            source_layout = layout.get("symbol") if isinstance(layout, dict) else None
            target_layout = identity.get("layout")
            for source_symbol, target_symbol, role in (
                (source_map, target_map, "map"),
                (source_layout, target_layout, "layout"),
            ):
                if (not isinstance(source_symbol, str)
                        or not isinstance(target_symbol, str)
                        or re.fullmatch(r"[A-Z][A-Z0-9_]*", source_symbol) is None
                        or re.fullmatch(r"[A-Z][A-Z0-9_]*", target_symbol) is None):
                    raise ScriptError(f"manifest has unsafe {role} identity at ordinal {ordinal}")
                previous = self.manifest_symbol_map.get(source_symbol)
                if previous is not None and previous != target_symbol:
                    raise ScriptError(f"manifest maps {source_symbol} to conflicting {role} identities")
                self.manifest_symbol_map[source_symbol] = target_symbol
            self.record_by_name[name] = record
            self.script_namespaces[name] = namespace
        self.source_diagnostics: list[Diagnostic] = []
        self.reviewed_donor_source_hashes: dict[str, str] = {}
        self._expected_source_hashes: dict[str, str] = {}
        try:
            donor_revision = subprocess.run(
                ["git", "-C", str(self.donor_root), "rev-parse", "HEAD"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            donor_revision = ""
        if donor_revision != DONOR_REVISION:
            self.source_diagnostics.append(Diagnostic(
                "donor-revision", str(self.donor_root), 0,
                f"pinned donor revision mismatch (expected {DONOR_REVISION}, actual {donor_revision or 'missing'})",
            ))
        self._map_text: dict[str, str] = {}
        self._map_path: dict[str, Path] = {}
        seen_names: set[str] = set()
        for record in selection:
            name = record.get("source_name")
            if not isinstance(name, str) or not name or Path(name).name != name or "/" in name or "\\" in name:
                raise ScriptError("manifest map has no source_name")
            if name in seen_names:
                raise ScriptError(f"manifest repeats selected source: {name}")
            seen_names.add(name)
            path = self.donor_root / "data/maps" / name / "scripts.inc"
            if not path.is_file():
                raise ScriptError(f"missing selected script source: {path}")
            expected = record.get("source_sha256")
            if not isinstance(expected, dict):
                raise ScriptError(f"manifest source_sha256 missing for {name}")
            expected_script = expected.get("script")
            expected_map = expected.get("map_json")
            map_path = self.donor_root / "data/maps" / name / "map.json"
            relative_script = _portable_path(path, self.donor_root)
            relative_map = _portable_path(map_path, self.donor_root)
            script_bytes = path.read_bytes()
            actual_script = hashlib.sha256(script_bytes).hexdigest()
            if isinstance(expected_script, str):
                self._expected_source_hashes[relative_script] = expected_script
            if isinstance(expected_map, str):
                self._expected_source_hashes[relative_map] = expected_map
            if not isinstance(expected_script, str) or actual_script != expected_script:
                self.source_diagnostics.append(Diagnostic(
                    "source-hash", relative_script, 0,
                    f"script source hash mismatch (manifest {expected_script!r}, actual {actual_script})",
                ))
            actual_map = sha256(map_path) if map_path.is_file() else "missing"
            if not isinstance(expected_map, str) or actual_map != expected_map:
                self.source_diagnostics.append(Diagnostic(
                    "source-hash", relative_map, 0,
                    f"map JSON hash mismatch (manifest {expected_map!r}, actual {actual_map})",
                ))
            self._map_path[name] = path
            self._map_text[name] = script_bytes.decode("utf-8", errors="strict")
        self.map_labels = {name: _source_labels(text) for name, text in self._map_text.items()}
        self.label_owner: dict[str, str] = {}
        for name, labels in self.map_labels.items():
            for label in labels:
                if label in self.label_owner and self.label_owner[label] != name:
                    # An ambiguous external reference is deliberately not guessed.
                    self.label_owner[label] = ""
                else:
                    self.label_owner[label] = name
        self.macros = _macro_names(self.root)
        self.macro_contracts = _macro_contracts(self.root)
        self.symbol_map = self._symbol_map()
        self.constants = _declared_constants(self.root)
        # Map/layout identities come from the authenticated manifest and are
        # declarations in the generated world unit even before that unit is
        # applied to this isolated compiler worktree.
        self.constants.update(self.manifest_symbol_map)
        self.constants.update(self.manifest_symbol_map.values())
        self.constants.update(CAMPAIGN_ABI_CONSTANTS)
        # The ledger is a second authoritative constant source for the
        # generated Johto flag/var/trainer namespaces.  Keep these out of the
        # label role so `call VAR_RESULT` cannot pass closure by spelling.
        identities = self.symbols.get("identities", {})
        if isinstance(identities, dict):
            for values in identities.values():
                if isinstance(values, list):
                    for item in values:
                        if isinstance(item, dict) and isinstance(item.get("symbol"), str):
                            self.constants.add(item["symbol"])
        self.specials = _registered_specials(self.root)
        self.natives = set(VERIFIED_NATIVE_ABI)
        self.labels = _approved_host_labels(self.root)
        self.static_closure_labels = _source_labels(STATIC_CAMPAIGN_CLOSURE)
        self.pending_contest_closure_labels = REQUIRED_CONTEST_CLOSURE_LABELS - (
            self.labels | self.static_closure_labels
        )
        self.imported_blocks, self.imported_symbols = self._load_reviewed_blocks()
        self.runtime_symbols: dict[str, str] = {}
        if self._runtime_available("berry"):
            self.runtime_symbols["BerryTreeScript"] = "Johto_BerryTreeScript"
        if self._runtime_available("whirlpool"):
            self.runtime_symbols["EventScript_Whirlpool"] = "Johto_EventScript_Whirlpool"
        self.debug_exclusions = self._find_unreachable_debug_blocks()
        # Kept as a compatibility view for callers of the original helper;
        # all production checks below use the role-specific sets instead.
        self.host_symbols = (self.constants | self.specials | self.natives | self.labels
                             | self.imported_symbols | set(self.symbol_map)
                             | set(self.symbol_map.values()))
        self.reference_symbols = (self.constants | self.labels | self.static_closure_labels | self.imported_symbols
                                  | REQUIRED_CONTEST_CLOSURE_LABELS
                                  | set(self.symbol_map) | set(self.symbol_map.values()))
        self.label_symbols = self.reference_symbols - self.constants
        self.label_symbols.update(HOST_SYMBOL_ALIASES)
        self.label_symbols.update(REQUIRED_CONTEST_CLOSURE_LABELS)
        self.label_symbols.update(label for label, owner in self.label_owner.items() if owner)
        self.host_closure_labels = self._reviewed_host_labels()
        self.reference_symbols.update(self.host_closure_labels)
        self.label_symbols.update(self.host_closure_labels)
        self.declared_symbols = frozenset(
            self.constants
            | self.reference_symbols
            | set(self.symbol_map)
            | set(self.symbol_map.values())
            | {"TRUE", "FALSE", "PARTY_SIZE", "STR_VAR_1", "STR_VAR_2", "STR_VAR_3"}
        )
        self.imported_blocks = self._translate_reviewed_blocks(self.imported_blocks)

    def _prefix(self, name: str) -> str:
        """Return the manifest-sealed namespace for one selected map."""
        namespaces = getattr(self, "script_namespaces", {})
        namespace = namespaces.get(name)
        if namespace is None:
            # Unit fixtures that do not load a manifest retain the original
            # Johto namespace; production instances always use the ledger.
            namespace = "Johto_" + re.sub(r"[^A-Za-z0-9_]", "_", name)
        return namespace + "_"

    def _find_unreachable_debug_blocks(self) -> set[str]:
        """Exclude only the two verified New Bark debug blocks.

        Their donor source contains legacy test commands, but neither label is
        referenced by a selected map object or by another selected script.
        This is a reachability decision, not a command suppression rule.
        """
        candidates = {
            "NewBarkTown_EventScript_TestMan1",
            "NewBarkTown_EventScript_TestMan2",
        }
        referenced: set[str] = set()
        for label in candidates:
            # A single definition line is the only occurrence permitted for
            # an unreachable block.  Search all selected scripts without
            # repeatedly parsing all 407 selected map JSON files.
            definition_count = 0
            use_count = 0
            for text in self._map_text.values():
                for match in re.finditer(rf"^\s*{re.escape(label)}::?", text, re.M):
                    definition_count += 1
                use_count += len(re.findall(rf"\b{re.escape(label)}\b", text))
            if definition_count != 1 or use_count != 1:
                referenced.add(label)
        map_path = self.donor_root / "data/maps" / "NewBarkTown" / "map.json"
        try:
            data = _read_json(map_path)
        except ScriptError:
            data = None
        if isinstance(data, dict):
            encoded = json.dumps(data, ensure_ascii=False)
            referenced.update(label for label in candidates if re.search(rf"\b{re.escape(label)}\b", encoded))
        return candidates - referenced

    def _load_reviewed_blocks(self) -> tuple[list[str], set[str]]:
        blocks: list[str] = []
        exported: set[str] = set()
        self.imported_symbol_map = {}
        self.reviewed_donor_source_hashes = {}
        expected_source_hashes = getattr(self, "_expected_source_hashes", None)
        if expected_source_hashes is None:
            expected_source_hashes = self._expected_source_hashes = {}
        for rel, seeds in REVIEWED_DONOR_BLOCKS.items():
            path = self.donor_root / Path(rel)
            expected_source_hashes[rel] = REVIEWED_DONOR_SOURCE_HASHES[rel]
            if not path.is_file():
                self.source_diagnostics.append(Diagnostic(
                    "donor-source-hash", rel, 0,
                    f"reviewed donor source is missing (expected {REVIEWED_DONOR_SOURCE_HASHES[rel]})",
                ))
                continue
            source_bytes = path.read_bytes()
            actual_hash = hashlib.sha256(source_bytes).hexdigest()
            self.reviewed_donor_source_hashes[rel] = actual_hash
            expected_hash = REVIEWED_DONOR_SOURCE_HASHES[rel]
            if actual_hash != expected_hash:
                self.source_diagnostics.append(Diagnostic(
                    "donor-source-hash", rel, 0,
                    f"reviewed donor source hash mismatch (expected {expected_hash}, actual {actual_hash})",
                ))
                continue
            text = source_bytes.decode("utf-8", errors="strict")
            all_blocks = _labeled_blocks(text)
            queue = list(seeds)
            selected: set[str] = set()
            while queue:
                label = queue.pop(0)
                if label in selected:
                    continue
                block = all_blocks.get(label)
                if block is None:
                    self.source_diagnostics.append(Diagnostic(
                        "donor-block-hash", rel, 0,
                        f"reviewed donor closure block is missing: {label}",
                    ))
                    continue
                expected_block_hash = REVIEWED_DONOR_BLOCK_HASHES.get(label)
                if expected_block_hash is not None:
                    actual_block_hash = hashlib.sha256(block.encode("utf-8")).hexdigest()
                    if actual_block_hash != expected_block_hash:
                        self.source_diagnostics.append(Diagnostic(
                            "donor-block-hash", rel, 0,
                            f"reviewed donor closure block {label} hash mismatch "
                            f"(expected {expected_block_hash}, actual {actual_block_hash})",
                        ))
                        continue
                selected.add(label)
                for ref in _block_references(block):
                    if ref in all_blocks and ref not in selected:
                        queue.append(ref)
            # Preserve the donor's block order: a closure block may rely on
            # source fallthrough or on stable diagnostic ordering.
            for label in all_blocks:
                if label not in selected:
                    continue
                block = all_blocks[label]
                blocks.append(f"@ reviewed donor closure: {rel}:{label}\n{block}")
                exported.add(label)
                closure_name = "Johto_Closure_" + re.sub(r"[^A-Za-z0-9_]", "_", rel) + "_" + label
                self.imported_symbol_map[label] = closure_name
        return blocks, exported

    def _current_donor_revision(self) -> str:
        try:
            return subprocess.run(
                ["git", "-C", str(self.donor_root), "rev-parse", "HEAD"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            return ""

    def _reauthenticate_donor(self) -> list[Diagnostic]:
        """Recheck pinned donor state after source reads have completed."""
        diagnostics: list[Diagnostic] = []
        revision = self._current_donor_revision()
        if revision != DONOR_REVISION:
            diagnostics.append(Diagnostic(
                "donor-revision", str(self.donor_root), 0,
                f"pinned donor revision mismatch (expected {DONOR_REVISION}, actual {revision or 'missing'})",
            ))
        for relative, expected in getattr(self, "_expected_source_hashes", {}).items():
            path = self.donor_root / Path(relative)
            actual = sha256(path) if path.is_file() else "missing"
            if actual != expected:
                kind = "donor-source-hash" if relative in REVIEWED_DONOR_SOURCE_HASHES else "source-hash"
                diagnostics.append(Diagnostic(
                    kind, relative, 0,
                    f"donor source hash mismatch after read (expected {expected}, actual {actual})",
                ))
        return diagnostics

    def _assert_donor_provenance(self) -> None:
        diagnostics = self._reauthenticate_donor()
        if diagnostics:
            raise ScriptError("donor provenance changed while compiling: " + diagnostics[0].reason)

    def _reviewed_host_labels(self) -> set[str]:
        """Return labels from the explicitly reviewed host closure files."""
        locations = {
            "Common_EventScript_GetGiftMonPartySlot": "data/scripts/pc_transfer.inc",
            "Common_EventScript_NameReceivedBoxMon": "data/scripts/pc_transfer.inc",
            "Common_EventScript_TransferredToPC": "data/scripts/pc_transfer.inc",
            "Common_EventScript_NameReceivedPartyMon": "data/event_scripts.s",
            "gText_NicknameThisPokemon": "data/text/pc_transfer.inc",
        }
        result: set[str] = set()
        for label, rel in locations.items():
            path = self.root / rel
            if path.is_file() and label in _source_labels(path.read_text(encoding="utf-8", errors="ignore")):
                result.add(label)
        return result

    def _symbol_role(self, token: str, local_labels: set[str], known: set[str]) -> str:
        """Return the authoritative role of a declared assembler symbol."""
        if token in local_labels:
            return "label"
        declared = token in known or token in getattr(self, "constants", set())
        if not declared:
            return "invalid"
        if token == "VAR_RESULT" or token.startswith(("VAR_", "JOHTO_VAR_", "STR_VAR_")):
            return "variable"
        if token.startswith(("FLAG_", "JOHTO_FLAG_")):
            return "flag"
        if token.startswith(("TRAINER_", "JOHTO_TRAINER_")):
            return "trainer"
        if token.startswith(("SPECIES_", "JOHTO_SPECIES_")):
            return "species"
        if token.startswith(("ITEM_", "JOHTO_ITEM_")):
            return "item"
        if token in getattr(self, "label_symbols", set()):
            return "label"
        if token in getattr(self, "labels", set()) or token in getattr(self, "host_closure_labels", set()):
            return "label"
        if token in known or token in getattr(self, "constants", set()):
            return "value"
        return "invalid"

    def _local_alias_roles(self, text: str, local_labels: set[str], known: set[str]) -> dict[str, str]:
        """Resolve source aliases once, preserving their underlying roles.

        Alias declarations are deliberately resolved against the current
        translation's source and symbol catalogs.  An invalid expression,
        unknown target, or cycle remains ``invalid`` so every typed operand
        rejects it instead of treating the alias spelling as a declaration.
        """
        declarations: dict[str, str] = {}
        for line in text.splitlines():
            match = re.match(
                r"^\s*\.(?:set|equ)\s+([A-Za-z_][A-Za-z0-9_]*)\s*,\s*(.+)$",
                _strip_comment(line),
            )
            if match:
                declarations[match.group(1)] = match.group(2).strip()

        resolved: dict[str, str] = {}
        visiting: set[str] = set()

        def resolve(alias: str) -> str:
            previous = resolved.get(alias)
            if previous is not None:
                return previous
            if alias in visiting:
                return "invalid"
            target_expression = declarations.get(alias)
            if target_expression is None:
                return self._symbol_role(alias, local_labels, known)
            visiting.add(alias)
            tokens, token_error = _tokenize_expression(target_expression)
            if token_error:
                role = "invalid"
            else:
                identifiers, parse_error = _parse_expression(tokens)
                if parse_error:
                    role = "invalid"
                elif not identifiers:
                    role = "numeric"
                elif len(tokens) == 1 and tokens[0].kind == "identifier":
                    role = resolve(tokens[0].value)
                else:
                    # Compound expressions are numeric expressions.  They
                    # remain valid only when every symbolic term is itself a
                    # declared, non-label value.
                    roles = {resolve(item) for item in identifiers}
                    role = "numeric" if roles and "invalid" not in roles and "label" not in roles else "invalid"
            visiting.discard(alias)
            resolved[alias] = role
            return role

        for alias in declarations:
            resolve(alias)
        return resolved

    def _translate_reviewed_blocks(self, blocks: list[str]) -> list[str]:
        """Namespace and validate each reviewed closure block before emission."""
        translated: list[str] = []
        self.closure_diagnostics: list[Diagnostic] = []
        replacements = dict(self.imported_symbol_map)
        replacements.update(HOST_SYMBOL_ALIASES)
        replacements.update(self.symbol_map)
        replacements.update(getattr(self, "runtime_symbols", {}))
        closure_labels = set(self.imported_symbol_map)
        known = (set(replacements) | set(replacements.values()) | self.constants
                 | self.labels | self.host_closure_labels | closure_labels)
        for block in blocks:
            header, _, source_block = block.partition("\n")
            match = re.match(r"@ reviewed donor closure: (.+):([A-Za-z_][A-Za-z0-9_]*)$", header)
            if not match:
                self.closure_diagnostics.append(Diagnostic(
                    "closure", "reviewed-closure", 0, "malformed reviewed closure record"
                ))
                continue
            rel, label = match.groups()
            source = rel
            alias_roles = self._local_alias_roles(source_block, closure_labels, known)
            translated_block: list[str] = []
            for line_number, raw in enumerate(source_block.splitlines(keepends=True), 1):
                definition = _label_definition(raw.rstrip("\r\n"))
                if definition:
                    translated_block.append(_replace_tokens(raw, replacements))
                    continue
                clean = _strip_comment(raw).strip()
                command_match = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\b(.*)$", clean)
                if not command_match or clean.startswith(".") or clean.startswith("@"):
                    translated_block.append(_replace_tokens(raw, replacements))
                    continue
                command = command_match.group(1)
                body = command_match.group(2).strip()
                command_key = command.lower()
                translated_command = COMMAND_TRANSLATIONS.get(command_key, command)
                if (translated_command.lower() not in self.macros
                        and command_key not in {"goto", "call", "return", "end", "callnative", "special", "specialvar"}
                        and not command_key.startswith(CONDITIONAL_PREFIXES)):
                    self.closure_diagnostics.append(Diagnostic(
                        "unknown-command", source, line_number,
                        f"reviewed closure command {command} is not present in the tracked host macro ABI"
                    ))
                if command_key in {"special", "specialvar", "callnative"}:
                    allowed_targets = {
                        "IsPokecenterChallengeActivated",
                        "GetBattleFrontierTutorMoveIndex",
                    }
                    form_diagnostics, parsed = self._special_form_diagnostics(
                        None, line_number, command_key, body, closure_labels,
                        replacements, known, source_override=source,
                        allowed_targets=allowed_targets,
                        alias_roles=alias_roles,
                    )
                    self.closure_diagnostics.extend(form_diagnostics)
                    if parsed.get("shape_valid") == "1" and parsed.get("target") in SPECIAL_TRANSLATIONS:
                        raw = _replace_tokens(raw, {parsed["target"]: SPECIAL_TRANSLATIONS[parsed["target"]]})
                    if (parsed.get("shape_valid") == "1" and command_key == "specialvar"
                            and parsed.get("target") == "IsPokecenterChallengeActivated"):
                        body = f"{parsed['destination']}, FALSE"
                        command = command_key = translated_command = "setvar"
                        indent = raw[:len(raw) - len(raw.lstrip())]
                        raw = f"{indent}setvar {body}\n"
                if translated_command != command:
                    raw = re.sub(r"^(\s*)" + re.escape(command) + r"\b", r"\1" + translated_command, raw, count=1)
                self.closure_diagnostics.extend(self._reference_diagnostics(
                    None, line_number, translated_command, body, closure_labels,
                    replacements, known, source_override=source, alias_roles=alias_roles
                ))
                translated_block.append(_replace_tokens(raw, replacements))
            translated.append(header + "\n" + "".join(translated_block))
        return translated

    def _runtime_available(self, dependency_id: str) -> bool:
        """Resolve a dependency from tracked script and native linkage.

        A file containing a label is only a declaration.  The script include,
        native implementation, and the host's recursive C source inventory
        must all be present before a dependency is considered linked.
        """
        cache = getattr(self, "_runtime_cache", None)
        if cache is None:
            cache = self._runtime_cache = {}
        if dependency_id in cache:
            return cache[dependency_id]
        available = False
        if dependency_id == "contest":
            wrappers = (
                "Johto_RequestBugContestTimeout",
                "Johto_JudgeBugContestSelectedMon",
                "Johto_PrepareBugContestSettlement",
                "Johto_ShowBugContestResult",
                "Johto_TransferBugContestSelectedMon",
                "Johto_ClaimBugContestReward",
                "Johto_ForfeitBugContestReward",
                "Johto_ExitBugContest",
                "Johto_AbortBugContestAdmission",
            )
            core = (
                "JohtoBugContest_RequestEnd",
                "JohtoBugContest_Judge",
                "JohtoBugContest_PrepareSettlement",
                "JohtoBugContest_TransferSelected",
                "JohtoBugContest_ClaimReward",
                "JohtoBugContest_ForfeitReward",
                "JohtoBugContest_Exit",
                "JohtoBugContest_Abort",
            )
            available = (
                not self.pending_contest_closure_labels
                and set(wrappers) <= self.specials
                and self._linked_native_source(self.root / "src/field_specials.c", wrappers)
                and self._linked_native_source(self.root / "src/johto/bug_contest.c", core)
            )
        elif dependency_id == "berry":
            script = self.root / "data/scripts/johto_berry_tree.inc"
            implementation = self.root / "src/johto/berry_plots.c"
            available = self._linked_script(script, "Johto_BerryTreeScript") and self._linked_native_source(
                implementation, ("Script_JohtoHarvestBerryTree", "JohtoBerryPlots_TryHarvest")
            )
        elif dependency_id == "whirlpool":
            script = self.root / "data/scripts/johto_field_moves.inc"
            implementation = self.root / "src/johto/field_moves.c"
            available = self._linked_script(script, "Johto_EventScript_Whirlpool") and self._linked_native_source(
                implementation, ("Script_JohtoCheckWhirlpool",)
            )
        elif dependency_id == "azalea-gym-trainers":
            available = ("AzaleaTown_Gym_SetGymTrainers" in getattr(self, "imported_symbols", set())
                         or "AzaleaTown_Gym_SetGymTrainers" in getattr(self, "labels", set()))
        cache[dependency_id] = available
        return available

    def _linked_script(self, path: Path, label: str) -> bool:
        """Check that a script file is included by a tracked assembly unit."""
        if not path.is_file():
            return False
        script_text = path.read_text(encoding="utf-8", errors="ignore")
        if not re.search(rf"(?m)^\s*{re.escape(label)}\s*::?\s*(?:@.*)?$", script_text):
            return False
        include_name = path.as_posix().split("/data/", 1)[-1]
        include_text = f'data/{include_name}'
        for candidate in (self.root / "data").rglob("*.s"):
            try:
                text = candidate.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            includes = re.findall(r'(?m)^\s*\.include\s+["<]([^">]+)[">]', text)
            if any(Path(item).as_posix() == include_text for item in includes):
                return True
        return False

    def _linked_native_source(self, path: Path, required: tuple[str, ...]) -> bool:
        if not path.is_file():
            return False
        text = path.read_text(encoding="utf-8", errors="ignore")
        definition = lambda symbol: re.search(
            rf"(?m)^\s*(?:(?:static|inline|extern|const|volatile)\s+)*"
            rf"[A-Za-z_][A-Za-z0-9_ \t\*]*\b{re.escape(symbol)}\s*"
            rf"\([^;{{}}]*\)\s*\{{", text
        )
        if not all(definition(symbol) for symbol in required):
            return False
        makefile = self.root / "Makefile"
        if not makefile.is_file():
            return False
        make_text = makefile.read_text(encoding="utf-8", errors="ignore")
        # Do not accept an arbitrary wildcard (the first one in the real
        # Makefile checks TOOLCHAIN/bin).  Only the C_SRCS_IN assignment is
        # authoritative for compiled C source linkage.
        make_text = re.sub(r"(?m)#.*$", "", make_text)
        relative = str(path.relative_to(self.root)).replace("\\", "/")
        csubdir_match = re.search(r"(?m)^\s*C_SUBDIR\s*[:?+]?=\s*([^\s]+)", make_text)
        csubdir = (csubdir_match.group(1).replace("\\", "/").strip("./")
                   if csubdir_match else "src")
        assignment_lines: list[str] = []
        lines = make_text.splitlines()
        index = 0
        while index < len(lines):
            line = lines[index]
            if not re.match(r"^\s*C_SRCS_IN\s*(?::=|\+=|\?=|=)", line):
                index += 1
                continue
            combined = line
            while combined.rstrip().endswith("\\") and index + 1 < len(lines):
                combined = combined.rstrip()[:-1] + " " + lines[index + 1].lstrip()
                index += 1
            assignment_lines.append(combined)
            index += 1
        if not assignment_lines:
            return False
        # The compiled list is a small supported Make subset.  Evaluate
        # replacement (:=, =) and append (+=) assignments in order; silently
        # unioning every assignment can certify a source that a later replace
        # removed.  Unsupported functions, filters, and substitutions fail
        # closed instead of extracting a positive-looking wildcard.
        inventory: list[str] = []
        assigned = False
        for assignment in assignment_lines:
            operator_match = re.match(r"^\s*C_SRCS_IN\s*(::=|:=|\+=|\?=|=)\s*(.*)$", assignment)
            if operator_match is None:
                return False
            operator, rhs = operator_match.groups()
            if operator == "?=" and assigned:
                continue
            if any(marker in rhs for marker in ("$(filter", "$(foreach", "$(if", "$(patsubst", "$(sort", "$(subst")):
                return False
            terms: list[str] = []
            index = 0
            while index < len(rhs):
                while index < len(rhs) and rhs[index].isspace():
                    index += 1
                if index >= len(rhs):
                    break
                if rhs.startswith("$(wildcard", index):
                    depth = 0
                    close = None
                    for position in range(index, len(rhs)):
                        if rhs[position] == "(":
                            depth += 1
                        elif rhs[position] == ")":
                            depth -= 1
                            if depth == 0:
                                close = position
                                break
                    if close is None:
                        return False
                    wildcard_body = rhs[index + len("$(wildcard"):close]
                    wildcard_terms = wildcard_body.split()
                    if not wildcard_terms:
                        return False
                    terms.extend(wildcard_terms)
                    index = close + 1
                    continue
                token_match = re.match(r"[^\s]+", rhs[index:])
                if token_match is None:
                    return False
                token = token_match.group(0)
                # Literal source paths are supported; arbitrary Make variable
                # expansions are intentionally not guessed.
                if not token.endswith(".c") or "$" in token:
                    return False
                terms.append(token)
                index += len(token)
            if operator in {":=", "::=", "="}:
                inventory = terms
                assigned = True
            elif operator == "+=":
                inventory.extend(terms)
                assigned = True
            elif operator == "?=" and not assigned:
                inventory = terms
                assigned = True
        if not assigned or not inventory:
            return False
        # C_SRCS deliberately drops generated .inc.c translation units after
        # C_SRCS_IN expansion, so they are never link evidence.
        if relative.endswith(".inc.c"):
            return False

        def segment_match(candidate: str, pattern: str) -> bool:
            candidate_parts = candidate.split("/")
            pattern_parts = pattern.split("/")
            if len(candidate_parts) != len(pattern_parts):
                return False
            return all(fnmatch.fnmatchcase(value, mask) for value, mask in zip(candidate_parts, pattern_parts))

        for pattern in inventory:
            expanded = pattern.replace("$(C_SUBDIR)", csubdir).replace("${C_SUBDIR}", csubdir)
            expanded = expanded.replace("\\", "/").lstrip("./")
            if segment_match(relative, expanded):
                return True
        return False

    def _dependency_rows(self) -> list[dict[str, object]]:
        rows: list[dict[str, object]] = []
        for key, value in PENDING_DEPENDENCIES.items():
            row = {"id": key, **value}
            row["status"] = "resolved" if self._runtime_available(key) else "pending"
            rows.append(row)
        return rows

    def _symbol_map(self) -> dict[str, str]:
        result: dict[str, str] = dict(getattr(self, "manifest_symbol_map", {}))
        identities = self.symbols.get("identities", {})
        if isinstance(identities, dict):
            for values in identities.values():
                if isinstance(values, list):
                    for item in values:
                        if isinstance(item, dict) and isinstance(item.get("symbol"), str) and isinstance(item.get("qualified"), str):
                            result[item["symbol"]] = item["qualified"]
        aliases = self.symbols.get("aliases", {})
        if isinstance(aliases, dict):
            for values in aliases.values():
                if isinstance(values, list):
                    for item in values:
                        if isinstance(item, dict) and isinstance(item.get("symbol"), str) and isinstance(item.get("qualified"), str):
                            result[item["symbol"]] = item["qualified"]
        return result

    def _namespace(self, name: str) -> dict[str, str]:
        prefix = self._prefix(name)
        replacements: dict[str, str] = {}
        for label in self.map_labels[name]:
            replacements[label] = prefix + label
        for label, owner in self.label_owner.items():
            if owner and owner != name:
                replacements[label] = self._prefix(owner) + label
        for key, value in self.symbol_map.items():
            replacements[key] = value
        replacements.update(getattr(self, "imported_symbol_map", {}))
        replacements.update(HOST_SYMBOL_ALIASES)
        replacements.update(getattr(self, "runtime_symbols", {}))
        text = self._map_text[name]
        for line in text.splitlines():
            match = re.match(r"^\s*\.(?:set|equ)\s+([A-Za-z_][A-Za-z0-9_]*)\b", _strip_comment(line))
            if match:
                replacements[match.group(1)] = prefix + match.group(1)
        return replacements

    def _command_contract(self, command: str) -> MacroContract | None:
        key = command.lower()
        contracts = getattr(self, "macro_contracts", None)
        if contracts is None:
            contracts = _macro_contracts(self.root)
            self.macro_contracts = contracts
        if key in contracts:
            return contracts[key]
        for source, translated in COMMAND_TRANSLATIONS.items():
            if translated.lower() == key and source.lower() in contracts:
                return contracts[source.lower()]
        return None

    def _special_form_diagnostics(self, name: str | None, line_number: int, command: str,
                                 body: str, local_labels: set[str], replacements: Mapping[str, str],
                                 known: set[str], source_override: str | None = None,
                                 allowed_targets: set[str] | None = None,
                                 alias_roles: Mapping[str, str] | None = None) -> tuple[list[Diagnostic], dict[str, str]]:
        """Validate special/native syntax before applying any policy rewrite."""
        command_key = command.lower()
        source = source_override
        if source is None and name is not None:
            source = _portable_path(self._map_path[name], self.donor_root)
        source = source or "<script>"
        diagnostics: list[Diagnostic] = []
        parsed: dict[str, str] = {"target": "", "destination": "", "shape_valid": "0"}
        operands = _split_operands(body)
        if command_key == "special":
            if len(operands) != 1 or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", operands[0].strip()):
                diagnostics.append(Diagnostic("operand-arity", source, line_number,
                                               f"special requires exactly 1 identifier operand, got {len(operands)}"))
                return diagnostics, parsed
            target = operands[0].strip()
            parsed.update(target=target, shape_valid="1")
            allowed = allowed_targets or set()
            translated = SPECIAL_TRANSLATIONS.get(target, target)
            if target == "ToggleShinyColors":
                diagnostics.append(Diagnostic("excluded-option", source, line_number,
                                               "unique shiny option must be removed with its two approved signs"))
            elif target not in allowed and translated not in getattr(self, "specials", set()):
                diagnostics.append(Diagnostic("unknown-special", source, line_number,
                                               f"special target {target} has no tracked host ABI"))
            return diagnostics, parsed
        if command_key == "specialvar":
            if len(operands) != 2 or any(not item.strip() for item in operands):
                diagnostics.append(Diagnostic("operand-arity", source, line_number,
                                               f"specialvar requires exactly 2 operands, got {len(operands)}"))
                return diagnostics, parsed
            destination, target = (item.strip() for item in operands)
            if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", target):
                diagnostics.append(Diagnostic("unknown-special", source, line_number,
                                               f"specialvar target {target} is not an identifier"))
                return diagnostics, parsed
            parsed.update(target=target, destination=destination, shape_valid="1")
            diagnostics.extend(self._reference_diagnostics(
                name, line_number, "setvar", f"{destination}, 0", local_labels,
                replacements, known, source_override, alias_roles=alias_roles
            ))
            allowed = allowed_targets or set()
            translated = SPECIAL_TRANSLATIONS.get(target, target)
            if target not in allowed and translated not in getattr(self, "specials", set()):
                diagnostics.append(Diagnostic("unknown-special", source, line_number,
                                               f"specialvar target {target} has no tracked host ABI"))
            return diagnostics, parsed
        if command_key == "callnative":
            if not operands or len(operands) > 2:
                diagnostics.append(Diagnostic("operand-arity", source, line_number,
                                               f"callnative requires 1 or 2 operands, got {len(operands)}"))
                return diagnostics, parsed
            native = operands[0].strip()
            if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", native):
                diagnostics.append(Diagnostic("unknown-native", source, line_number,
                                               f"callnative target {native} is not an identifier"))
                return diagnostics, parsed
            parsed.update(target=native, shape_valid="1")
            if len(operands) == 2:
                effect = re.fullmatch(r"requests_effects\s*=\s*([01])", operands[1].strip())
                abi = VERIFIED_NATIVE_ABI.get(native)
                if effect is None or abi is None:
                    diagnostics.append(Diagnostic("native-effects", source, line_number,
                                                   f"callnative {native} has unsupported ABI argument {operands[1]}"))
                elif bool(int(effect.group(1))) != bool(abi["requests_effects"]):
                    diagnostics.append(Diagnostic("native-effects", source, line_number,
                                                   f"callnative {native} requests_effects={effect.group(1)} but reviewed ABI requires {int(bool(abi['requests_effects']))}"))
            if native not in POLICY_NATIVE_CALLS and (native not in getattr(self, "natives", set())
                                                       or native not in VERIFIED_NATIVE_ABI):
                diagnostics.append(Diagnostic("unknown-native", source, line_number,
                                               f"callnative {native} has no tracked host ABI"))
            return diagnostics, parsed
        return diagnostics, parsed

    def _reference_diagnostics(self, name: str | None, line_number: int, command: str, body: str,
                               local_labels: set[str], replacements: Mapping[str, str],
                               known: set[str] | None = None,
                               source_override: str | None = None,
                               alias_roles: Mapping[str, str] | None = None) -> list[Diagnostic]:
        """Validate expression syntax, command ABI arity, and operand roles."""
        command_key = command.lower()
        if command_key in {"special", "specialvar", "callnative"}:
            return []
        contract = self._command_contract(command_key)
        if contract is None:
            operands = _split_operands(body)
            if operands or command_key in getattr(self, "macros", {}):
                source = source_override or ("<script>" if name is None else _portable_path(self._map_path[name], self.donor_root))
                return [Diagnostic("unknown-abi", source, line_number,
                                   f"{command} has no tracked host macro declaration")]
            return []
        operands = _normalized_donor_operands(command_key, body, contract)
        roles = OPERAND_ROLES.get(command_key)
        if command_key.startswith(CONDITIONAL_PREFIXES) and command_key != "call_if_defeated":
            operator = command_key.split("_", 2)[-1]
            if operator in {"set", "unset"}:
                roles = ("flag", "label")
            else:
                roles = ("label",) if len(operands) == 1 else ("value", "value", "label")
        if roles is None:
            roles = tuple("value" for _ in range(contract.maximum or len(operands)))
        maximum = contract.maximum
        diagnostics: list[Diagnostic] = []
        reference_symbols = getattr(self, "reference_symbols", getattr(self, "host_symbols", set()))
        constants = getattr(self, "constants", set())
        implicit_constants = {"TRUE", "FALSE", "PARTY_SIZE"}
        if not constants:
            constants = {item for item in getattr(self, "host_symbols", set()) if item.isupper()}
        label_symbols = getattr(self, "label_symbols", set())
        if not label_symbols:
            label_symbols = reference_symbols - constants
        if known is None:
            known = set(replacements) | set(replacements.values()) | reference_symbols | local_labels
        source = source_override
        if source is None and name is not None:
            source = _portable_path(self._map_path[name], self.donor_root)
        source = source or "<script>"
        # These are assembler string-variable slots rather than C constants,
        # but are fixed host ABI symbols and therefore part of the catalog.
        declared_symbols = getattr(self, "declared_symbols", None)
        if declared_symbols is None:
            declared_symbols = (
                set(constants)
                | set(reference_symbols)
                | set(getattr(self, "symbol_map", {}))
                | set(getattr(self, "symbol_map", {}).values())
                | implicit_constants
                | {"STR_VAR_1", "STR_VAR_2", "STR_VAR_3"}
            )
        if alias_roles is None:
            alias_text = self._map_text.get(name, "") if name is not None else ""
            alias_roles = self._local_alias_roles(alias_text, local_labels, known)
        local_aliases = set(alias_roles)
        if (command_key.startswith(CONDITIONAL_PREFIXES)
                and command_key != "call_if_defeated"
                and command_key.split("_", 2)[-1] not in {"set", "unset"}):
            if len(operands) not in {1, 3}:
                diagnostics.append(Diagnostic("operand-arity", source, line_number,
                                              f"{command} accepts either 1 destination or 3 comparison operands, got {len(operands)}"))
        elif len(operands) < contract.required:
            diagnostics.append(Diagnostic("operand-arity", source, line_number,
                                          f"{command} requires at least {contract.required} operand(s), got {len(operands)}"))
        if maximum is not None and len(operands) > maximum:
            diagnostics.append(Diagnostic("operand-arity", source, line_number,
                                          f"{command} accepts at most {maximum} operand(s), got {len(operands)}"))

        # Bind supplied arguments to declared parameters before applying roles.
        # Named optional arguments can skip earlier defaults and required
        # fields may be supplied in a different order.
        bindings: dict[int, tuple[str, int]] = {}
        parameter_indices = {item.lower(): index for index, item in enumerate(contract.parameters)}
        next_positional = 0
        for supplied_index, operand in enumerate(operands):
            keyword = re.fullmatch(r"([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)", operand)
            if keyword:
                parameter_index = parameter_indices.get(keyword.group(1).lower())
                if parameter_index is None:
                    diagnostics.append(Diagnostic("unknown-reference", source, line_number,
                                                   f"{command} does not accept keyword operand {keyword.group(1)}"))
                    continue
                if parameter_index in bindings:
                    diagnostics.append(Diagnostic("operand-arity", source, line_number,
                                                   f"{command} binds parameter {contract.parameters[parameter_index]} more than once"))
                    continue
                bindings[parameter_index] = (keyword.group(2), supplied_index)
                continue
            while next_positional in bindings:
                next_positional += 1
            if next_positional >= len(contract.parameters):
                if maximum is None:
                    diagnostics.append(Diagnostic("unknown-reference", source, line_number,
                                                   f"{command} has no declared parameter for operand {supplied_index + 1}"))
                continue
            bindings[next_positional] = (operand, supplied_index)
            next_positional += 1
        required_names = contract.required_parameters or contract.parameters[:contract.required]
        for required_name in required_names:
            parameter_index = parameter_indices.get(required_name.lower())
            if parameter_index is not None and parameter_index not in bindings:
                diagnostics.append(Diagnostic("operand-arity", source, line_number,
                                               f"{command} requires declared parameter {required_name}"))

        def identity_kind(token: str) -> str:
            alias_kind = alias_roles.get(token)
            if alias_kind is not None:
                if alias_kind == "invalid":
                    return "unknown"
                if alias_kind == "label":
                    return "label"
                return "value"
            if token in constants or token in implicit_constants:
                return "constant"
            if token in label_symbols or token in local_labels:
                return "label"
            if token in known or token in replacements or token in replacements.values():
                return "value"
            return "unknown"

        def accepts(role: str, token: str) -> bool:
            if role in {"label", "text", "movement"}:
                return identity_kind(token) == "label" and token not in constants
            if role == "variable":
                if token in local_aliases:
                    return alias_roles.get(token) == "variable"
                return token in declared_symbols and (
                    token == "VAR_RESULT" or token.startswith(("VAR_", "JOHTO_VAR_", "STR_VAR_")))
            if role == "flag":
                if token in local_aliases:
                    return alias_roles.get(token) == "flag"
                return token in declared_symbols and token.startswith(("FLAG_", "JOHTO_FLAG_"))
            if role == "trainer":
                if token in local_aliases:
                    return alias_roles.get(token) == "trainer"
                return token in declared_symbols and token.startswith(("TRAINER_", "JOHTO_TRAINER_"))
            if role == "species":
                if token in local_aliases:
                    return alias_roles.get(token) in {"species", "variable"}
                return token in declared_symbols and token.startswith(
                    ("SPECIES_", "JOHTO_SPECIES_", "VAR_", "JOHTO_VAR_", "STR_VAR_"))
            if role == "item":
                if token in local_aliases:
                    return alias_roles.get(token) in {"item", "variable"}
                return token in declared_symbols and token.startswith(
                    ("ITEM_", "JOHTO_ITEM_", "VAR_", "JOHTO_VAR_", "STR_VAR_"))
            if role in {"value", "constant"}:
                return identity_kind(token) in {"constant", "value"} and token not in label_symbols and token not in local_labels
            return False

        for parameter_index, (expression, supplied_index) in bindings.items():
            role = roles[parameter_index] if parameter_index < len(roles) else "value"
            tokens, token_error = _tokenize_expression(expression)
            if token_error:
                diagnostics.append(Diagnostic("expression-syntax", source, line_number,
                                               f"{command} {role} operand {supplied_index + 1}: {token_error}"))
                continue
            if len(tokens) == 1 and tokens[0].kind == "quoted":
                if role != "text":
                    diagnostics.append(Diagnostic("unknown-reference", source, line_number,
                                                   f"{command} {role} operand {supplied_index + 1} cannot be quoted text"))
                continue
            identifiers, parse_error = _parse_expression(tokens)
            if parse_error:
                diagnostics.append(Diagnostic("expression-syntax", source, line_number,
                                               f"{command} {role} operand {supplied_index + 1}: {parse_error}"))
                continue
            if not identifiers and role in {"label", "text", "movement", "species", "item", "flag", "trainer", "variable"}:
                diagnostics.append(Diagnostic("unknown-reference", source, line_number,
                                               f"{command} {role} operand {supplied_index + 1} has no valid symbolic value"))
                continue
            invalid = [token for token in identifiers if not accepts(role, token)]
            if invalid:
                diagnostics.append(Diagnostic("unknown-reference", source, line_number,
                                               f"{command} {role} operand {supplied_index + 1} references unsupported symbol {invalid[0]}"))
        return diagnostics

    def translate_source(self, name: str, text: str | None = None) -> tuple[str, list[Diagnostic], list[str]]:
        if name not in self._map_path:
            raise ScriptError(f"map is not selected: {name}")
        text = self._map_text[name] if text is None else text
        replacements = self._namespace(name)
        prefix = self._prefix(name)
        local_labels = self.map_labels[name]
        diagnostics: list[Diagnostic] = []
        adaptations: list[str] = []
        output: list[str] = []
        source_path = _portable_path(
            self._map_path[name], getattr(self, "donor_root", self._map_path[name].parent)
        )
        known_symbols = (set(replacements) | getattr(self, "reference_symbols", getattr(self, "host_symbols", set()))
                         | set(self.symbol_map.values()) | local_labels)
        alias_roles = self._local_alias_roles(text, local_labels, known_symbols)
        lines = text.splitlines(keepends=True)
        excluded_prefixes = (
            "GoldenrodCity_NameRatersHouse_EventScript_ToggleShinies",
            "GoldenrodCity_NameRatersHouse_Text_AskToggleShinies",
            "GoldenrodCity_NameRatersHouse_Text_ToggleShinies",
        )
        skip_block = False
        skip_lance_blobs = name == "RocketHideout_B2F"
        in_lance = False
        lance_inserted = False
        for line_number, raw in enumerate(lines, 1):
            definition = _label_definition(raw.rstrip("\r\n"))
            current_label = definition[0] if definition else None
            if definition:
                skip_block = (current_label.startswith(excluded_prefixes)
                              or current_label in getattr(self, "debug_exclusions", set()))
                if current_label in getattr(self, "debug_exclusions", set()):
                    adaptations.append(f"{name}:{line_number}: excluded unreachable debug block {current_label}; selected map references verified absent")
                if skip_lance_blobs and current_label in {
                    "RocketHideout_B2F_EventScript_ArianaTrainer",
                    "RocketHideout_B2F_EventScript_GruntTrainer",
                }:
                    skip_block = True
                in_lance = current_label == "RocketHideout_B2F_EventScript_DoLanceMultiBattle"
                lance_inserted = False
            if skip_block:
                continue
            if definition:
                # Label declarations are data, not event commands.  Their
                # references are rewritten by the same lexer below, while
                # numeric local labels remain untouched.
                output.append(_replace_tokens(raw, replacements))
                continue
            clean = _strip_comment(raw).strip()
            command_match = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\b(.*)$", clean)
            command = command_match.group(1) if command_match else ""
            body = command_match.group(2).strip() if command_match else ""
            if not clean or clean.startswith(".") or clean.startswith("@"): # directives/comments
                output.append(_replace_tokens(raw, replacements))
                continue
            command_key = command.lower()
            if command_key == "warpsilent" and any(
                    map_id in body for map_id in (
                        "MAP_SOUTHERN_ISLAND_EXTERIOR", "MAP_BIRTH_ISLAND_EXTERIOR",
                        "MAP_FARAWAY_ISLAND_ENTRANCE", "MAP_BATTLE_FRONTIER_OUTSIDE_WEST",
                    )):
                expected_warp = REVIEWED_EXTERNAL_WARPS.get((name, line_number))
                if expected_warp != body:
                    diagnostics.append(Diagnostic(
                        "unsupported-adapter", source_path, line_number,
                        f"external destination warp differs from reviewed target/coordinates: {body}",
                    ))
            if command_key == "map_script" and re.search(r"\bSetTimeEncounters\b", body):
                adaptations.append(f"{name}:{line_number}: omitted policy map callback SetTimeEncounters; host JohtoWild_CurrentTime consumes encounter time")
                output.append("@ Johto policy adaptation: encounter time is selected by host RTC at battle time.\n")
                continue
            if command_key == "checkpartymove":
                if name == "CianwoodPokecenter" and body.replace(" ", "") == "MOVE_SURF":
                    adaptations.append(f"{name}:{line_number}: checkpartymove MOVE_SURF translated to host checkfieldmove FIELD_MOVE_SURF, FALSE")
                    indent = raw[:len(raw) - len(raw.lstrip())]
                    output.append(f"{indent}checkfieldmove FIELD_MOVE_SURF, FALSE\n")
                else:
                    diagnostics.append(Diagnostic("unsupported-adapter", source_path, line_number,
                                                   f"checkpartymove operand is not the reviewed Cianwood MOVE_SURF case: {body}"))
                continue
            if command_key == "pokemart" and body == "0":
                products = REVIEWED_NULL_POKEMARTS.get((name, line_number))
                if products is None:
                    diagnostics.append(Diagnostic(
                        "unsupported-adapter", source_path, line_number,
                        "null pokemart pointer has no reviewed terminated product list",
                    ))
                else:
                    indent = raw[:len(raw) - len(raw.lstrip())]
                    adaptations.append(
                        f"{name}:{line_number}: null pokemart pointer mapped to terminated product list {products}"
                    )
                    output.append(f"{indent}pokemart {_replace_tokens(products, replacements)}\n")
                continue
            if command_key == "chooseitem":
                berry_cancel_targets = {
                    ("IcePath_B3F", 45): "IcePath_B3F_EventScript_SwinubBlowPlayer",
                    ("IcePath_B3F", 85): "IcePath_B3F_EventScript_DelibirdBlowPlayer",
                    ("Route14", 248): "Route14_EventScript_PidgeottoBlowPlayer",
                }
                cancel_target = berry_cancel_targets.get((name, line_number))
                if cancel_target is not None and body.replace(" ", "") == "BERRIES_POCKET":
                    indent = raw[:len(raw) - len(raw.lstrip())]
                    adaptations.append(f"{name}:{line_number}: chooseitem BERRIES_POCKET translated to host Bag_ChooseBerry flow; cancellation branches to {cancel_target}")
                    output.append(
                        f"{indent}fadescreen FADE_TO_BLACK\n"
                        f"{indent}closemessage\n"
                        f"{indent}special Bag_ChooseBerry\n"
                        f"{indent}waitstate\n"
                        f"{indent}goto_if_eq VAR_ITEM_ID, 0, {replacements[cancel_target]}\n"
                    )
                else:
                    diagnostics.append(Diagnostic("unsupported-adapter", source_path, line_number,
                                                   f"chooseitem operand is outside the reviewed BERRIES_POCKET adapter: {body}"))
                continue
            if command_key == "warpsilent" and any(
                    body.startswith(symbol + ",") for symbol in (
                        "MAP_SOUTHERN_ISLAND_EXTERIOR", "MAP_BIRTH_ISLAND_EXTERIOR",
                        "MAP_FARAWAY_ISLAND_ENTRANCE", "MAP_BATTLE_FRONTIER_OUTSIDE_WEST",
                    )):
                expected_warp = REVIEWED_EXTERNAL_WARPS.get((name, line_number))
                if expected_warp != body:
                    diagnostics.append(Diagnostic(
                        "unsupported-adapter", source_path, line_number,
                        f"external host warp is outside the reviewed target/coordinate contract: {body}",
                    ))
                else:
                    adaptations.append(f"{name}:{line_number}: retained reviewed identical-host external warp {body}")
            if command_key == "copyvar" and re.fullmatch(r"\s*VAR_ICE_STEP_COUNT\s*,\s*1\s*", body):
                adaptations.append(f"{name}:{line_number}: copyvar literal translated to the host setvar ABI")
                output.append(_replace_tokens(raw.replace("copyvar", "setvar", 1), replacements))
                continue
            if command_key in {"special", "specialvar", "callnative"}:
                allowed_targets = set()
                if command_key == "specialvar":
                    allowed_targets.update({"GetMaxPartySize", "IsNuzlockeNicknamingActive",
                                            "IsPokecenterChallengeActivated", "IsRandomMovesActivated"})
                if in_lance and command_key == "special":
                    allowed_targets.update({"ReducePlayerPartyToSelectedMons", "LoadPlayerParty", "DoSpecialTrainerBattle"})
                form_diagnostics, parsed = self._special_form_diagnostics(
                    name, line_number, command_key, body, local_labels,
                    replacements, known_symbols, source_override=source_path,
                    allowed_targets=allowed_targets,
                    alias_roles=alias_roles,
                )
                diagnostics.extend(form_diagnostics)
                target = parsed.get("target", "")
                if name == "Gate_NationalPark" and target == "SavePlayerParty":
                    adaptations.append(f"{name}:{line_number}: contest party snapshot is owned atomically by Johto_BeginBugContestAdmission")
                    continue
                if name == "Gate_NationalPark" and target == "EnterBugContestMode":
                    adaptations.append(f"{name}:{line_number}: contest mode was entered atomically by Johto_BeginBugContestAdmission")
                    continue
                if name == "Gate_NationalPark" and target == "LoadPlayerParty":
                    adaptations.append(f"{name}:{line_number}: contest cancellation restores party, mail, and loaned balls atomically")
                    output.append("\tspecial Johto_AbortBugContestAdmission\n")
                    continue
                if parsed.get("shape_valid") == "1" and command_key == "callnative" and target in POLICY_NATIVE_CALLS:
                    adaptations.append(f"{name}:{line_number}: omitted policy native {target}: {POLICY_NATIVE_CALLS[target]}")
                    output.append("@ Johto policy adaptation: donor option/time callback omitted; host policy applies.\n")
                    continue
                if parsed.get("shape_valid") == "1" and command_key == "specialvar":
                    destination = parsed.get("destination", "")
                    policy_value = None
                    if target == "GetMaxPartySize":
                        policy_value = "PARTY_SIZE"
                        adaptations.append(f"{name}:{line_number}: GetMaxPartySize replaced with PARTY_SIZE policy")
                    elif target in {"IsNuzlockeNicknamingActive", "IsPokecenterChallengeActivated", "IsRandomMovesActivated"}:
                        policy_value = "FALSE"
                        adaptations.append(f"{name}:{line_number}: excluded option predicate {target} fixed false")
                    if policy_value is not None:
                        diagnostics.extend(self._reference_diagnostics(
                            name, line_number, "setvar", f"{destination}, {policy_value}",
                            local_labels, replacements, known_symbols, source_override=source_path,
                            alias_roles=alias_roles,
                        ))
                        if not any(item.kind == "unknown-reference" and item.line == line_number for item in form_diagnostics):
                            output.append(f"\tsetvar {_replace_tokens(destination, replacements)}, {policy_value}\n")
                        else:
                            output.append(_replace_tokens(raw, replacements))
                        continue
                if parsed.get("shape_valid") == "1" and target in SPECIAL_TRANSLATIONS:
                    raw = _replace_tokens(raw, {target: SPECIAL_TRANSLATIONS[target]})
            if command_key == "checkrandomizer":
                adaptations.append(f"{name}:{line_number}: challenge/randomizer predicate fixed false")
                output.append(re.sub(r"^\s*checkrandomizer\b.*", "\tsetvar VAR_RESULT, FALSE\n", raw, flags=re.I))
                continue
            if command_key == "givebp" and body.strip() == "10":
                indent = raw[:len(raw) - len(raw.lstrip())]
                adaptations.append(f"{name}:{line_number}: givebp 10 translated to saturating host GiveFrontierBattlePoints ABI")
                output.append(
                    f"{indent}setvar VAR_0x8004, 10\n"
                    f"{indent}buffernumberstring STR_VAR_1, VAR_0x8004\n"
                    f"{indent}special GiveFrontierBattlePoints\n"
                )
                continue
            external_warp_symbols = {
                "MAP_SOUTHERN_ISLAND_EXTERIOR", "MAP_BIRTH_ISLAND_EXTERIOR",
                "MAP_FARAWAY_ISLAND_ENTRANCE", "MAP_BATTLE_FRONTIER_OUTSIDE_WEST",
            }
            if command_key == "warpsilent" and any(symbol in body for symbol in external_warp_symbols):
                expected_warp = REVIEWED_EXTERNAL_WARPS.get((name, line_number))
                if expected_warp != body:
                    diagnostics.append(Diagnostic(
                        "unsupported-adapter", source_path, line_number,
                        f"unreviewed external warp target/coordinates: {body}",
                    ))
                else:
                    adaptations.append(f"{name}:{line_number}: retained reviewed identical-host external warp {body}")
            if command_key == "remove5mons" and name == "Gate_NationalPark" and not body:
                adaptations.append(f"{name}:{line_number}: party reduction and Safari Ball loan translated to atomic contest admission")
                output.append("\tspecial Johto_BeginBugContestAdmission\n")
                continue
            if (command_key == "giveitem" and name == "Gate_NationalPark"
                    and re.fullmatch(r"ITEM_SAFARI_BALL\s*,\s*30", body)):
                adaptations.append(f"{name}:{line_number}: Safari Ball loan already committed by atomic contest admission")
                continue
            if in_lance and command_key in {"special", "frontier_set"} and any(x in body for x in ("DoSpecialTrainerBattle", "FRONTIER_DATA_SELECTED_MON_ORDER")):
                if "DoSpecialTrainerBattle" in body:
                    output.append("\tmulti_2_vs_2 JOHTO_TRAINER_ARIANA_1, Johto_RocketHideout_B2F_RocketHideout_B2F_Text_ArianaLoss, JOHTO_TRAINER_GRUNT_23, Johto_RocketHideout_B2F_RocketHideout_B2F_Text_GruntLoss, PARTNER_LANCE\n")
                    lance_inserted = True
                    adaptations.append(f"{name}:{line_number}: Lance special sequence converted to host multi_2_vs_2")
                continue
            if in_lance and command_key == "special" and any(x in body for x in ("ReducePlayerPartyToSelectedMons", "LoadPlayerParty")):
                adaptations.append(f"{name}:{line_number}: host multi battle owns party snapshot/restore")
                continue
            if in_lance and command_key in {"frontier_saveparty"}:
                adaptations.append(f"{name}:{line_number}: host multi battle owns party synchronization")
                continue
            translated_command = COMMAND_TRANSLATIONS.get(command_key)
            if command_key == "getpartysize":
                # This is a real host command with its own effects ABI.  The
                # classic policy only rewrites GetMaxPartySize above.
                translated_command = "getpartysize"
            if translated_command is None:
                translated_command = self.macros.get(command_key)
            if translated_command is None:
                diagnostics.append(Diagnostic("unknown-command", source_path, line_number,
                                               f"command {command} is not present in the tracked host macro ABI"))
            elif translated_command != command:
                raw = re.sub(r"^(\s*)" + re.escape(command) + r"\b", r"\1" + translated_command, raw, count=1)
            if command_key in {"remove5mons", "enterbugcontestmode"}:
                diagnostics.append(Diagnostic("missing-runtime", source_path, line_number,
                                               "contest transaction adapter is mandatory and not yet tracked"))
            if command_key == "call" and "BerryTreeScript" in body:
                if self._runtime_available("berry"):
                    raw = _replace_tokens(raw, {"BerryTreeScript": "Johto_BerryTreeScript"})
                else:
                    diagnostics.append(Diagnostic("missing-runtime", source_path, line_number,
                                                   "Johto berry harvest script is mandatory and not yet tracked"))
            if command_key == "call" and "EventScript_Whirlpool" in body:
                if self._runtime_available("whirlpool"):
                    raw = _replace_tokens(raw, {"EventScript_Whirlpool": "Johto_EventScript_Whirlpool"})
                else:
                    diagnostics.append(Diagnostic("missing-runtime", source_path, line_number,
                                                   "Johto_EventScript_Whirlpool badge/field adapter is mandatory and not yet tracked"))
            diagnostics.extend(self._reference_diagnostics(
                name, line_number, translated_command or command_key, body, local_labels,
                replacements, known_symbols, alias_roles=alias_roles,
            ))
            output.append(_replace_tokens(raw, replacements))
        return "".join(output), diagnostics, adaptations

    def preflight(self) -> dict[str, object]:
        diagnostics: list[Diagnostic] = list(self.source_diagnostics)
        diagnostics.extend(getattr(self, "closure_diagnostics", []))
        for label in sorted(getattr(self, "pending_contest_closure_labels", set())):
            diagnostics.append(Diagnostic(
                "missing-runtime", "data/maps/NationalPark_BugContest/scripts.inc", 0,
                f"required recoverable contest closure label is not linked: {label}",
            ))
        authenticated_source_count = len(getattr(self, "_expected_source_hashes", {}))
        if authenticated_source_count != 823:
            diagnostics.append(Diagnostic(
                "source-inventory", MANIFEST_REL.as_posix(), 0,
                f"authenticated source inventory must contain exactly 823 paths, got {authenticated_source_count}",
            ))
        adaptations: list[str] = []
        translated: dict[str, str] = {}
        external_object_script_sites: dict[str, list[str]] = {
            label: [] for label in REVIEWED_EXTERNAL_OBJECT_SCRIPT_SITES
        }
        for name in self._map_text:
            result, found, notes = self.translate_source(name)
            translated[name] = result
            diagnostics.extend(found)
            adaptations.extend(notes)
            map_json = self.donor_root / "data/maps" / name / "map.json"
            try:
                map_data = _read_json(map_json)
            except ScriptError as exc:
                diagnostics.append(Diagnostic("source", _portable_path(map_json, self.donor_root), 0, str(exc)))
                continue
            if isinstance(map_data, dict):
                for obj in map_data.get("object_events", []) or []:
                    if isinstance(obj, dict) and obj.get("script") in external_object_script_sites:
                        external_object_script_sites[obj["script"]].append(name)
                    if (isinstance(obj, dict) and obj.get("script") == "BerryTreeScript"
                            and not self._runtime_available("berry")):
                        diagnostics.append(Diagnostic("missing-runtime", _portable_path(map_json, self.donor_root), 0,
                                                       "Johto_BerryTreeScript is required by a selected object"))
                if name == "DragonsDen_Cavern":
                    for index, obj in enumerate(map_data.get("object_events", []) or []):
                        if (isinstance(obj, dict) and obj.get("script") == "EventScript_Whirlpool"
                                and not self._runtime_available("whirlpool")):
                            diagnostics.append(Diagnostic("missing-runtime", _portable_path(map_json, self.donor_root), index + 1,
                                                           "EventScript_Whirlpool Johto badge/field adapter is required"))
        for label, expected_sites in REVIEWED_EXTERNAL_OBJECT_SCRIPT_SITES.items():
            actual_sites = tuple(sorted(external_object_script_sites[label]))
            if actual_sites != tuple(sorted(expected_sites)):
                diagnostics.append(Diagnostic(
                    "external-object-script", MANIFEST_REL.as_posix(), 0,
                    f"reviewed consumers for {label} changed: expected {tuple(sorted(expected_sites))!r}, "
                    f"got {actual_sites!r}",
                ))
            target = getattr(self, "imported_symbol_map", {}).get(label)
            if label not in getattr(self, "imported_symbols", set()) or not target or not target.endswith("_" + label):
                diagnostics.append(Diagnostic(
                    "external-object-script", MANIFEST_REL.as_posix(), 0,
                    f"reviewed external object script {label} has no unique suffix-resolvable closure",
                ))
            owner = getattr(self, "label_owner", {}).get(label)
            if owner:
                diagnostics.append(Diagnostic(
                    "external-object-script", _portable_path(self._map_path[owner], self.donor_root), 0,
                    f"reviewed external object script {label} is also defined by selected map {owner}",
                ))
        if ("AzaleaTown_Gym" in self._map_text
                and "Common_EventScript_SetGymTrainers" in self._map_text["AzaleaTown_Gym"]
                and not self._runtime_available("azalea-gym-trainers")):
            diagnostics.append(Diagnostic("missing-runtime", _portable_path(self._map_path["AzaleaTown_Gym"], self.donor_root), 0,
                                           "selected Azalea gym trainerflag helper is not yet tracked"))
        diagnostics.extend(self._reauthenticate_donor())
        # Deduplicate exact diagnostics while retaining source order.
        unique: list[Diagnostic] = []
        seen = set()
        for item in diagnostics:
            key = (item.kind, item.source, item.line, item.reason)
            if key not in seen:
                seen.add(key)
                unique.append(item)
        pending = self._dependency_rows()
        source_hashes = {
            str(self._map_path[n].relative_to(self.donor_root)).replace("\\", "/"): sha256(self._map_path[n])
            for n in self._map_text
        }
        source_hashes.update(getattr(self, "reviewed_donor_source_hashes", {}))
        source_hashes_normalized = {
            str(self._map_path[n].relative_to(self.donor_root)).replace("\\", "/"): _normalize_sha256(self._map_path[n])
            for n in self._map_text
        }
        source_hashes_normalized.update({
            rel: _normalize_sha256(self.donor_root / Path(rel))
            for rel in getattr(self, "reviewed_donor_source_hashes", {})
        })
        return {
            "schema": "johto-campaign-script-preflight-v1",
            "contract_hash": CONTRACT_HASH,
            "donor_revision": DONOR_REVISION,
            "selected_map_count": len(self.records),
            "selected_script_count": len(self._map_text),
            "original_selected_map_count": 239,
            "later_selected_map_count": 168,
            "authenticated_source_count": authenticated_source_count,
            "source_hashes": source_hashes,
            "source_hashes_normalized": source_hashes_normalized,
            "reviewed_donor_source_hashes": dict(sorted(getattr(self, "reviewed_donor_source_hashes", {}).items())),
            "map_script_label_contract": {
                "scheme": "<identity_namespace.script>_<source_label>",
                "original_namespace": "Johto_<source_name>",
                "later_namespace": "KantoLater_<source_name>",
                "new_bark_preview": "Johto_NewBarkTown_NewBarkTown_MapScripts",
                "pallet_town_preview": "KantoLater_PalletTown_PalletTown_MapScripts",
                "source_label": "NewBarkTown_MapScripts",
            },
            "reviewed_donor_closure": sorted(self.imported_symbols),
            "diagnostics": [asdict(item) for item in unique],
            "diagnostic_count": len(unique),
            "adaptations": adaptations,
            "dependencies": pending,
            "complete": not unique,
            "emitted": False,
            "translated_sources": translated,
        }

    def render(self, preflight: dict[str, object]) -> str:
        if preflight.get("diagnostic_count"):
            raise ScriptError("semantic closure is incomplete; refusing to emit campaign_scripts.inc")
        chunks = [
            "@ Generated by tools/johto/content_scripts.py; source-preserving Johto campaign unit.\n",
            "@ Macro ABI extensions are intentionally explicit.\n",
            '#include "asm/macros/johto.inc"\n',
            '#include "asm/macros/johto_gifts.inc"\n',
            '#include "asm/macros/johto_quests.inc"\n',
            '#include "asm/macros/johto_text.inc"\n',
        ]
        for block in self.imported_blocks:
            chunks.append(f"\n{block}\n")
        chunks.append(f"\n@ reviewed campaign-owned runtime closure\n{STATIC_CAMPAIGN_CLOSURE}\n")
        for name in self._map_text:
            chunks.append(f"\n@ source map: {name}\n")
            chunks.append(str(preflight["translated_sources"][name]))
        rendered = "".join(chunks).replace("\r\n", "\n").replace("\r", "\n")
        transport_sources = TRANSPORT_SOURCE_MAPS & set(self._map_text)
        if transport_sources and transport_sources != TRANSPORT_SOURCE_MAPS:
            missing = ", ".join(sorted(TRANSPORT_SOURCE_MAPS - transport_sources))
            raise ScriptError(f"incomplete campaign transport source set: {missing}")
        if transport_sources:
            rendered = _apply_transport_overrides(rendered)
        return "\n".join(line.rstrip(" \t") for line in rendered.split("\n"))

    @staticmethod
    def _replace_outputs_transactionally(outputs: Mapping[Path, bytes]) -> None:
        """Install a set of outputs together, restoring exact originals on failure."""
        temporary: dict[Path, Path] = {}
        backups: dict[Path, Path] = {}
        backup_placeholders: dict[Path, Path] = {}
        installed: set[Path] = set()
        retained_backups: set[Path] = set()
        try:
            for target, contents in outputs.items():
                target.parent.mkdir(parents=True, exist_ok=True)
                handle = tempfile.NamedTemporaryFile(
                    mode="wb", dir=target.parent, prefix=f".{target.name}.", suffix=".new", delete=False
                )
                try:
                    handle.write(contents)
                    handle.flush()
                    os.fsync(handle.fileno())
                finally:
                    handle.close()
                temporary[target] = Path(handle.name)
            for target in outputs:
                if target.exists():
                    handle = tempfile.NamedTemporaryFile(
                        mode="wb", dir=target.parent, prefix=f".{target.name}.", suffix=".bak", delete=False
                    )
                    handle.close()
                    backup = Path(handle.name)
                    backup_placeholders[target] = backup
                    os.replace(target, backup)
                    backups[target] = backup
                    del backup_placeholders[target]
            for target, staged in temporary.items():
                os.replace(staged, target)
                installed.add(target)
        except BaseException as transaction_error:
            rollback_errors: list[tuple[Path, OSError]] = []
            for target in reversed(tuple(outputs)):
                backup = backups.get(target)
                try:
                    if target in installed and target.exists():
                        target.unlink()
                    if backup is not None and backup.exists():
                        os.replace(backup, target)
                except OSError as exc:
                    rollback_errors.append((target, exc))
                    if backup is not None and backup.exists():
                        retained_backups.add(backup)
            cleanup_paths = [
                *((target, path, "staged output") for target, path in temporary.items()),
                *((target, path, "backup") for target, path in backups.items() if path not in retained_backups),
                *((target, path, "backup placeholder") for target, path in backup_placeholders.items()),
            ]
            cleanup_errors: list[tuple[Path, Path, str, OSError]] = []
            for target, path, artifact_kind in cleanup_paths:
                try:
                    if path.exists():
                        path.unlink()
                except OSError as exc:
                    cleanup_errors.append((target, path, artifact_kind, exc))
            if rollback_errors or cleanup_errors:
                details = [f"output transaction failed: {transaction_error}"]
                if rollback_errors:
                    errors = ", ".join(
                        f"{target} ({exc})" for target, exc in rollback_errors
                    )
                    details.append(f"rollback was incomplete: {errors}")
                if retained_backups:
                    for target in outputs:
                        backup = backups.get(target)
                        if backup in retained_backups:
                            details.append(f"manual recovery required: restore {backup} -> {target}")
                if cleanup_errors:
                    errors = ", ".join(
                        f"target {target}: remove {artifact_kind} {path} ({exc}); "
                        f"target state: {'exists' if target.exists() else 'missing'}"
                        for target, path, artifact_kind, exc in cleanup_errors
                    )
                    details.append(f"secondary cleanup failure(s): {errors}")
                raise ScriptError("; ".join(details)) from transaction_error
            raise
        else:
            cleanup_errors: list[tuple[Path, Path, OSError]] = []
            for target, backup in backups.items():
                try:
                    if backup.exists():
                        backup.unlink()
                except OSError as exc:
                    cleanup_errors.append((target, backup, exc))
            if cleanup_errors:
                details = ["output transaction installation committed; backup cleanup was incomplete"]
                for target, backup, exc in cleanup_errors:
                    details.append(f"target {target}: backup {backup} ({exc})")
                    details.append(f"manual recovery available: restore {backup} -> {target}")
                raise ScriptError("; ".join(details)) from cleanup_errors[0][2]

    def write_or_check(self, write: bool) -> dict[str, object]:
        full = self.preflight()
        result = dict(full)
        result.pop("translated_sources", None)
        deps_path = self.root / DEPENDENCIES_REL
        rendered_deps = json.dumps(result, indent=2, ensure_ascii=False, sort_keys=False) + "\n"
        output = self.root / OUTPUT_REL
        provenance_kinds = {
            "donor-revision",
            "source-hash",
            "donor-source-hash",
            "donor-block-hash",
            "source-inventory",
        }
        if any(item.get("kind") in provenance_kinds for item in result.get("diagnostics", [])):
            raise ScriptError("donor provenance mismatch; refusing to mutate generated outputs")
        if result["diagnostic_count"]:
            if write:
                deps_path.write_text(rendered_deps, encoding="utf-8", newline="\n")
                if output.exists():
                    output.unlink()
            return result
        rendered = self.render(full)
        self._assert_donor_provenance()
        if write:
            self._replace_outputs_transactionally({
                deps_path: rendered_deps.encode("utf-8"),
                output: rendered.encode("utf-8"),
            })
        else:
            if not deps_path.is_file() or deps_path.read_text(encoding="utf-8") != rendered_deps:
                raise ScriptError(f"stale dependency report: {deps_path}")
            if not output.is_file() or output.read_text(encoding="utf-8") != rendered:
                raise ScriptError(f"stale campaign output: {output}")
        result["emitted"] = True
        return result


def preflight(root: str | Path, donor_root: str | Path) -> dict[str, object]:
    return ScriptCompiler(root, donor_root).preflight()


def main(argv: Iterable[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--donor-root", required=True, type=Path)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--write", action="store_true", help="write a complete report/output; default is check")
    parser.add_argument("--check", action="store_true", help="explicitly request default check mode")
    args = parser.parse_args(argv)
    try:
        compiler = ScriptCompiler(args.root, args.donor_root)
        result = compiler.write_or_check(bool(args.write))
    except (ScriptError, OSError) as exc:
        print(f"content_scripts: {exc}", file=sys.stderr)
        return 2
    print(json.dumps({k: v for k, v in result.items() if k != "translated_sources"}, indent=2, ensure_ascii=False))
    if result.get("diagnostic_count"):
        for item in result["diagnostics"]:
            print(f"{item['source']}:{item['line']}: {item['kind']}: {item['reason']}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
