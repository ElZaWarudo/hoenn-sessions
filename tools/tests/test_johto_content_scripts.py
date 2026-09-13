"""Focused source and translator tests for the Johto campaign importer."""

from __future__ import annotations

import json
import hashlib
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.johto.content_scripts import (
    COMMAND_TRANSLATIONS,
    STATIC_CAMPAIGN_CLOSURE,
    ScriptCompiler,
    ScriptError,
    _approved_host_labels,
    _label_definition,
    _normalize_sha256,
    _normalized_donor_operands,
    _replace_tokens,
    _generated_tm_hm_constants,
    _macro_contracts,
    REVIEWED_DONOR_BLOCKS,
    REVIEWED_DONOR_BLOCK_HASHES,
    REVIEWED_DONOR_SOURCE_HASHES,
    REVIEWED_EXTERNAL_OBJECT_SCRIPT_SITES,
    MacroContract,
    sha256,
)


ROOT = Path(__file__).parents[2]
DONOR = Path("C:/Users/Mayor/Documents/Caribbean/johto-hns")


def fixture_compiler(name: str, source: str, *, symbols: dict[str, str] | None = None,
                     host: set[str] | None = None, macros: dict[str, str] | None = None) -> ScriptCompiler:
    compiler = object.__new__(ScriptCompiler)
    compiler.root = ROOT
    compiler.donor_root = DONOR
    path = Path("fixture") / f"{name}.inc"
    compiler._map_path = {name: path}
    compiler._map_text = {name: source}
    compiler.map_labels = {name: {item[0] for item in (_label_definition(line) for line in source.splitlines()) if item}}
    compiler.label_owner = {label: name for label in compiler.map_labels[name]}
    compiler.symbol_map = symbols or {}
    compiler.host_symbols = host or {"TRUE", "FALSE", "VAR_RESULT", "PARTY_SIZE", "SPECIES_EEVEE",
                                     "MSGBOX_DEFAULT", "Common_Movement_WalkUp1", "UpdateFollowingPokemon",
                                     "Johto_CheckCelebi", "Johto_NameRival", "Johto_CheckHooh"}
    compiler.constants = {item for item in compiler.host_symbols if item.isupper() or item.startswith(("VAR_", "FLAG_", "ITEM_", "SPECIES_", "TRAINER_", "MAP_", "MOVE_", "MSGBOX_"))}
    compiler.specials = {"Johto_CheckCelebi", "Johto_NameRival", "Johto_CheckHooh"}
    compiler.natives = {"UpdateFollowingPokemon"}
    compiler.labels = {item for item in compiler.host_symbols if item not in compiler.constants and item not in compiler.specials and item not in compiler.natives}
    compiler.imported_symbols = set()
    compiler.imported_symbol_map = {}
    compiler.host_closure_labels = set()
    compiler.reference_symbols = compiler.constants | compiler.labels | set(compiler.symbol_map.values())
    compiler.label_symbols = compiler.labels | set(compiler.symbol_map.values())
    compiler.runtime_symbols = {}
    compiler.debug_exclusions = set()
    compiler._runtime_cache = {}
    compiler.macros = macros or {"lock": "lock", "end": "end", "return": "return", "goto": "goto",
                                 "call": "call", "special": "special", "specialvar": "specialvar",
                                 "callnative": "callnative", "delay": "delay", "givemon": "givemon",
                                 "setvar": "setvar", "setflag": "setflag", "msgbox": "msgbox",
                                 "goto_if_eq": "goto_if_eq", "call_if_eq": "call_if_eq",
                                 "applymovement": "applymovement", "waitstate": "waitstate",
                                 "switch": "switch", "case": "case", "step_end": "step_end",
                                 "waitmovement": "waitmovement", "multi_2_vs_2": "multi_2_vs_2",
                                 **{key: value for key, value in COMMAND_TRANSLATIONS.items()}}
    compiler.macro_contracts = _macro_contracts(ROOT)
    if not compiler.macro_contracts:
        # The detached fixer worktree may omit the host asm checkout.  Keep
        # fixture coverage focused on the compiler semantics with the small
        # ABI subset used below; a populated checkout still parses its real
        # declarations above.
        compiler.macro_contracts = {
            "applymovement": MacroContract(2, 3, ("localId", "movement", "arg2"), ("localId", "movement")),
            "bufferspeciesname": MacroContract(2, 2, ("var", "species"), ("var", "species")),
            "bufferitemname": MacroContract(2, 2, ("var", "item"), ("var", "item")),
            "call": MacroContract(1, 1, ("destination",), ("destination",)),
            "call_if_eq": MacroContract(1, 3, ("a", "b", "c"), ("a",)),
            "case": MacroContract(2, 2, ("value", "destination"), ("value", "destination")),
            "delay": MacroContract(1, 1, ("frames",), ("frames",)),
            "end": MacroContract(0, 0, (), ()),
            "getpartysize": MacroContract(0, 0, (), ()),
            "givemon": MacroContract(
                2, 27,
                ("species", "level", "item", "ball", "nature", "abilityNum", "gender",
                 "hpEv", "atkEv", "defEv", "speedEv", "spAtkEv", "spDefEv", "hpIv",
                 "atkIv", "defIv", "speedIv", "spAtkIv", "spDefIv", "move1", "move2",
                 "move3", "move4", "shinyMode", "gmaxFactor", "teraType", "dmaxLevel"),
                ("species", "level"),
            ),
            "goto_if_eq": MacroContract(1, 3, ("a", "b", "c"), ("a",)),
            "johto_applymovement2": MacroContract(2, 2, ("localId", "movement"), ("localId", "movement")),
            "msgbox": MacroContract(2, 2, ("text", "style"), ("text", "style")),
            "removeitem": MacroContract(2, 2, ("item", "amount"), ("item", "amount")),
            "setflag": MacroContract(1, 1, ("flag",), ("flag",)),
            "setvar": MacroContract(2, 3, ("var", "value", "constant"), ("var", "value")),
            "step_end": MacroContract(0, 0, (), ()),
            "switch": MacroContract(1, 1, ("value",), ("value",)),
            "waitmovement": MacroContract(1, 1, ("value",), ("value",)),
            "waitstate": MacroContract(0, 0, (), ()),
        }
    return compiler


class ContentScriptTests(unittest.TestCase):
    def test_host_label_catalog_follows_only_linked_in_repo_includes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linked = root / "data/scripts/linked.inc"
            nested = root / "data/text/nested.inc"
            unlinked = root / "data/scripts/unlinked.inc"
            linked.parent.mkdir(parents=True)
            nested.parent.mkdir(parents=True)
            (root / "data/event_scripts.s").write_text(
                '.include "data/scripts/linked.inc"\nRootLabel::\n', encoding="utf-8"
            )
            linked.write_text(
                '.include "data/text/nested.inc"\nLinkedLabel::\n', encoding="utf-8"
            )
            nested.write_text("NestedLabel::\n", encoding="utf-8")
            unlinked.write_text("UnlinkedLabel::\n", encoding="utf-8")

            labels = _approved_host_labels(root)

            self.assertTrue({"RootLabel", "LinkedLabel", "NestedLabel"} <= labels)
            self.assertNotIn("UnlinkedLabel", labels)

    def test_alias_role_resolution_does_not_retain_per_map_symbol_catalogs(self):
        compiler = fixture_compiler("AliasCache", ".set TEMP_ALIAS, VAR_RESULT\n")

        roles = compiler._local_alias_roles(
            compiler._map_text["AliasCache"], set(), set(compiler.reference_symbols)
        )

        self.assertEqual(roles["TEMP_ALIAS"], "variable")
        self.assertFalse(hasattr(compiler, "_alias_cache"))

    def test_pinned_gnu_as_whitespace_operand_shapes_are_normalized(self):
        trainer = MacroContract(3, 5, ("trainer", "intro_text", "lose_text", "event_script", "music"))
        msgbox = MacroContract(1, 2, ("text", "type"))
        triple = MacroContract(3, 3, ("a", "b", "c"))
        setvar = MacroContract(2, 3, ("var", "value", "constant"))
        self.assertEqual(
            _normalized_donor_operands(
                "trainerbattle_single",
                "TRAINER_JUSTIN Route32_Text_Seen, Route32_Text_Beaten",
                trainer,
            ),
            ["TRAINER_JUSTIN", "Route32_Text_Seen", "Route32_Text_Beaten"],
        )
        self.assertEqual(
            _normalized_donor_operands("msgbox", "VioletCity_Text_Earl MSGBOX_YESNO", msgbox),
            ["VioletCity_Text_Earl", "MSGBOX_YESNO"],
        )
        self.assertEqual(
            _normalized_donor_operands("call_if_eq", "VAR_RESULT, TRUE Target", triple),
            ["VAR_RESULT", "TRUE", "Target"],
        )
        self.assertEqual(
            _normalized_donor_operands("map_script_2", "VAR_STATE, 1 Target", triple),
            ["VAR_STATE", "1", "Target"],
        )
        self.assertEqual(
            _normalized_donor_operands("setvar", "VAR_RESULT, 1,", setvar),
            ["VAR_RESULT", "1"],
        )

    def test_lexer_preserves_strings_and_comments(self):
        source = 'goto Foo @ Foo\n.string "Foo // do not rewrite"\n/* Foo */ Foo\n'
        self.assertEqual(
            _replace_tokens(source, {"Foo": "Bar"}),
            'goto Bar @ Foo\n.string "Foo // do not rewrite"\n/* Foo */ Bar\n',
        )
        self.assertEqual(_label_definition("1:"), ("1", ":"))

        def render_fixture(newline):
            compiler = object.__new__(ScriptCompiler)
            compiler.imported_blocks = [f"Closure::{newline}\treturn \t{newline}"]
            compiler._map_text = {"Fixture": "authenticated donor source"}
            report = {
                "diagnostic_count": 0,
                "translated_sources": {
                    "Fixture": f"Fixture_Script::{newline}\t.string \"keep inside \" \t{newline}\tend\t{newline}",
                },
            }
            return compiler, report, compiler.render(report)

        lf_compiler, lf_report, lf_rendered = render_fixture("\n")
        _, _, crlf_rendered = render_fixture("\r\n")
        self.assertEqual(lf_rendered, crlf_rendered)
        self.assertNotIn("\r", crlf_rendered)
        self.assertIn('.string "keep inside "\n', crlf_rendered)
        self.assertTrue(all(line == line.rstrip(" \t") for line in crlf_rendered.split("\n")))

        with tempfile.TemporaryDirectory() as directory:
            lf_compiler.root = Path(directory)
            lf_compiler.preflight = mock.Mock(return_value=lf_report)
            lf_compiler._assert_donor_provenance = mock.Mock()
            lf_compiler.write_or_check(True)
            checked = lf_compiler.write_or_check(False)
            self.assertTrue(checked["emitted"])

    def test_namespace_keeps_aliases_local_and_qualifies_references(self):
        source = ".set LOCALID_NPC, 2\nStart::\n\tgoto_if_eq VAR_TEMP_0, 1, Done\n\tapplymovement2 LOCALID_NPC, Move\nDone::\n\tend\nMove:\n\tstep_end\n"
        compiler = fixture_compiler("Route29", source, symbols={"VAR_TEMP_0": "JOHTO_VAR_TEMP_0"},
                                    host={"goto_if_eq", "step_end", "VAR_TEMP_0"})
        rendered, diagnostics, _ = compiler.translate_source("Route29")
        self.assertFalse(diagnostics)
        self.assertIn(".set Johto_Route29_LOCALID_NPC, 2", rendered)
        self.assertIn("goto_if_eq JOHTO_VAR_TEMP_0, 1, Johto_Route29_Done", rendered)
        self.assertIn("johto_applymovement2 Johto_Route29_LOCALID_NPC, Johto_Route29_Move", rendered)
        self.assertNotIn('"Foo"', rendered)

    def test_manifest_namespaces_isolate_same_shaped_labels_and_cross_map_calls(self):
        compiler = fixture_compiler("Original", "Shared::\n\tcall LaterOnly\n\tend\n", host={"call", "end"})
        compiler._map_path["Later"] = Path("fixture/Later.inc")
        compiler._map_text["Later"] = "Shared::\n\tend\nLaterOnly::\n\tend\n"
        compiler.map_labels = {
            "Original": {"Shared"},
            "Later": {"Shared", "LaterOnly"},
        }
        compiler.label_owner = {"Shared": "", "LaterOnly": "Later"}
        compiler.script_namespaces = {
            "Original": "Johto_Original",
            "Later": "KantoLater_Later",
        }

        original = compiler._namespace("Original")
        later = compiler._namespace("Later")
        self.assertEqual(original["Shared"], "Johto_Original_Shared")
        self.assertEqual(later["Shared"], "KantoLater_Later_Shared")
        self.assertEqual(original["LaterOnly"], "KantoLater_Later_LaterOnly")

    def test_later_command_operand_contracts_accept_real_roles(self):
        source = (
            "Start::\n"
            "\tbufferpartymonnick STR_VAR_1, VAR_0x8004\n"
            "\tbufferstring STR_VAR_2, StatText\n"
            "\tcall_if_defeated TRAINER_ALICE, Defeated\n"
            "\tcheckplayergender\n"
            "\tcompare VAR_RESULT, TRUE\n"
            "\tdofieldeffect FLDEFF_SPARKLE\n"
            "\tjump_up\n"
            "\twaitfieldeffect FLDEFF_SPARKLE\n"
            "\twarpteleport MAP_NEW_BARK_TOWN, 20, 12\n"
            "\tend\n"
            "Defeated::\n\treturn\n"
            "StatText::\n\t.string \"HP$\"\n"
        )
        host = {
            "STR_VAR_1", "STR_VAR_2", "VAR_0x8004", "VAR_RESULT", "TRUE",
            "TRAINER_ALICE", "FLDEFF_SPARKLE", "MAP_NEW_BARK_TOWN", "end", "return",
        }
        compiler = fixture_compiler("Later", source, host=host)
        compiler.script_namespaces = {"Later": "KantoLater_Later"}
        compiler.macros.update({name: name for name in (
            "bufferpartymonnick", "bufferstring", "call_if_defeated", "checkplayergender",
            "compare", "dofieldeffect", "jump_up", "waitfieldeffect", "warpteleport",
        )})
        compiler.macro_contracts = _macro_contracts(ROOT)
        rendered, diagnostics, _ = compiler.translate_source("Later")
        self.assertFalse(diagnostics)
        self.assertIn("bufferstring STR_VAR_2, KantoLater_Later_StatText", rendered)
        self.assertIn("call_if_defeated TRAINER_ALICE, KantoLater_Later_Defeated", rendered)

    def test_adapter_abi_translations_are_explicit(self):
        source = "Start::\n\tspecialvar VAR_RESULT, CheckCelebi\n\tspecial NameRival\n\tgivenamedmon 2\n\tgiveoddegg 1\n\tremovenamedmon 2\n\tremovegenericmon SPECIES_EEVEE\n\tbaobacheckmon 1\n\tsetwildbattleshiny SPECIES_EEVEE, 5\n\tbuffermoncategory STR_VAR_1, SPECIES_EEVEE\n\tend\n"
        compiler = fixture_compiler("Route29", source, symbols={"SPECIES_EEVEE": "SPECIES_EEVEE"},
                                    host={"VAR_RESULT", "specialvar", "special", "end", "STR_VAR_1", "SPECIES_EEVEE"})
        rendered, diagnostics, _ = compiler.translate_source("Route29")
        self.assertFalse([item for item in diagnostics if item.kind == "unknown-command"])
        for macro in ("johto_givenamedmon", "johto_giveoddegg", "johto_removenamedmon",
                      "johto_removegenericmon", "johto_baobacheckmon", "johto_setwildbattleshiny",
                      "johto_buffermoncategory"):
            self.assertIn(macro, rendered)
        self.assertIn("Johto_CheckCelebi", rendered)
        self.assertIn("Johto_NameRival", rendered)

    def test_classic_policy_keeps_ordinary_branch(self):
        source = "Start::\n\tgetpartysize\n\tspecialvar VAR_RESULT, IsNuzlockeNicknamingActive\n\tcheckrandomizer\n\tcall_if_eq VAR_RESULT, TRUE, Optional\n\tcall Common_Ordinary\nOptional::\n\tend\n"
        compiler = fixture_compiler("Lab", source, host={"call_if_eq", "call", "end", "TRUE", "VAR_RESULT", "Common_Ordinary"})
        rendered, diagnostics, notes = compiler.translate_source("Lab")
        self.assertFalse(diagnostics)
        self.assertEqual(rendered.count("setvar VAR_RESULT, FALSE"), 2)
        self.assertIn("call Common_Ordinary", rendered)
        self.assertTrue(any("fixed false" in note for note in notes))

    def test_lance_special_sequence_uses_real_multi_abi(self):
        source = "RocketHideout_B2F_EventScript_DoLanceMultiBattle::\n\tspecial ReducePlayerPartyToSelectedMons\n\tfrontier_set FRONTIER_DATA_SELECTED_MON_ORDER\n\tsetvar VAR_0x8004, SPECIAL_BATTLE_LANCE\n\tapplymovement LOCALID_A, Common_Movement_WalkUp1\n\tspecial DoSpecialTrainerBattle\n\twaitstate\n\tfrontier_saveparty\n\tspecial LoadPlayerParty\n\tswitch VAR_RESULT\n\tcase 1, RocketHideout_B2F_EventScript_DefeatedAriana\n\tend\nRocketHideout_B2F_EventScript_ArianaTrainer::\n\tend\n"
        compiler = fixture_compiler("RocketHideout_B2F", source,
                                    symbols={"VAR_RESULT": "JOHTO_VAR_RESULT"},
                                    host={"applymovement", "waitstate", "switch", "case", "end",
                                          "Common_Movement_WalkUp1", "LOCALID_A"})
        rendered, diagnostics, _ = compiler.translate_source("RocketHideout_B2F")
        self.assertNotIn("DoSpecialTrainerBattle", rendered)
        self.assertNotIn("ReducePlayerPartyToSelectedMons", rendered)
        self.assertIn("multi_2_vs_2 JOHTO_TRAINER_ARIANA_1", rendered)
        self.assertIn("PARTNER_LANCE", rendered)
        self.assertTrue(any("Lance special sequence" in note for note in _))
        self.assertFalse([item for item in diagnostics if item.kind == "unknown-command"])

    def test_unresolved_runtime_and_unsupported_abi_fail_closed(self):
        source = "Start::\n\tremove5mons\n\tcallnative MissingNative\n\tspecial MissingSpecial\n\tunknown_donor_command\n\tend\n"
        compiler = fixture_compiler("NationalPark_BugContest", source, host={"end"})
        _, diagnostics, _ = compiler.translate_source("NationalPark_BugContest")
        kinds = {item.kind for item in diagnostics}
        self.assertIn("missing-runtime", kinds)
        self.assertIn("unknown-native", kinds)
        self.assertIn("unknown-special", kinds)
        self.assertIn("unknown-command", kinds)

    def test_role_catalog_rejects_same_name_wrong_role_and_unknown_constant(self):
        source = "Start::\n\tspecial SameName\n\tcallnative SameName\n\tcall MISSING_MAP_LABEL\n\tend\n"
        compiler = fixture_compiler("RoleFixture", source, host={"SameName", "end"})
        compiler.specials = {"RegisteredSpecial"}
        compiler.natives = {"RegisteredNative"}
        compiler.reference_symbols = {"RegisteredLabel"}
        _, diagnostics, _ = compiler.translate_source("RoleFixture")
        by_kind = {(item.kind, item.reason) for item in diagnostics}
        self.assertTrue(any(kind == "unknown-special" and "SameName" in reason for kind, reason in by_kind))
        self.assertTrue(any(kind == "unknown-native" and "SameName" in reason for kind, reason in by_kind))
        self.assertTrue(any(kind == "unknown-reference" and "MISSING_MAP_LABEL" in reason for kind, reason in by_kind))

    def test_operand_roles_reject_invalid_symbol_positions(self):
        source = "Start::\n\tmsgbox VAR_RESULT, MSGBOX_DEFAULT\n\tgoto_if_eq VAR_RESULT, TRUE, VAR_RESULT\n\tgivemon MISSING_SPECIES, 5\n\tend\n"
        compiler = fixture_compiler("RoleFixture", source, host={"VAR_RESULT", "TRUE", "MSGBOX_DEFAULT", "givemon", "end"})
        _, diagnostics, _ = compiler.translate_source("RoleFixture")
        reasons = " ".join(item.reason for item in diagnostics)
        self.assertIn("VAR_RESULT", reasons)
        self.assertIn("MISSING_SPECIES", reasons)

    def test_varget_item_and_species_operands_are_valid(self):
        source = "Start::\n\tremoveitem VAR_ITEM_ID, 1\n\tgivemon VAR_TEMP_1, 15\n\tbufferspeciesname STR_VAR_1, VAR_TEMP_1\n\tend\n"
        compiler = fixture_compiler(
            "VarGetFixture", source,
            host={"VAR_ITEM_ID", "VAR_TEMP_1", "STR_VAR_1", "end", "removeitem", "givemon"},
        )
        _, diagnostics, _ = compiler.translate_source("VarGetFixture")
        self.assertFalse([item for item in diagnostics if item.kind == "unknown-reference"])

    def test_complete_expression_and_arity_validation_is_fail_closed(self):
        source = (
            ".set LOCAL_DEST, VAR_RESULT\n"
            "Start::\n"
            "\tsetvar VAR_RESULT, 1+MISSING_VALUE\n"
            "\tsetvar VAR_RESULT\n"
            "\tgivemon SPECIES_EEVEE, 5, MISSING_ITEM\n"
            "\tbufferitemname STR_VAR_1, MISSING_ITEM\n"
            "\tcall LOCAL_DEST\n"
            "\tend\n"
        )
        compiler = fixture_compiler(
            "ExpressionFixture", source,
            host={"VAR_RESULT", "SPECIES_EEVEE", "STR_VAR_1", "setvar", "givemon", "end"},
        )
        _, diagnostics, _ = compiler.translate_source("ExpressionFixture")
        reasons = " ".join(item.reason for item in diagnostics)
        self.assertIn("MISSING_VALUE", reasons)
        self.assertIn("requires at least 2", reasons)
        self.assertIn("MISSING_ITEM", reasons)
        self.assertIn("LOCAL_DEST", reasons)

    def test_real_literals_expression_grammar_and_host_arity(self):
        source = (
            "Start::\n"
            "\tsetvar VAR_RESULT, 0x8000\n"
            "\tsetvar VAR_RESULT, 0b101, FALSE\n"
            "\tsetvar VAR_RESULT, 3, warn=FALSE\n"
            "\tgivemon SPECIES_EEVEE, 5, ITEM_NONE\n"
            "\tsetvar VAR_RESULT, (1 + )\n"
            "\tgivemon SPECIES_EEVEE\n"
            "\tapplymovement LOCALID_NPC\n"
            "\tdelay\n"
            "\tdelay 1, 2\n"
            "\tend\n"
        )
        compiler = fixture_compiler(
            "AbiFixture", source,
            host={"VAR_RESULT", "ITEM_NONE", "SPECIES_EEVEE", "LOCALID_NPC", "end"},
        )
        _, diagnostics, _ = compiler.translate_source("AbiFixture")
        reasons = " ".join(item.reason for item in diagnostics)
        self.assertFalse([item for item in diagnostics if "0x8000" in item.reason or "0b101" in item.reason])
        self.assertFalse([item for item in diagnostics if item.kind == "unknown-reference" and "ITEM_NONE" in item.reason])
        self.assertIn("unexpected token ')'", reasons)
        self.assertIn("givemon requires at least 2", reasons)
        self.assertIn("applymovement requires at least 2", reasons)
        self.assertIn("delay requires at least 1", reasons)
        self.assertIn("delay accepts at most 1", reasons)

    def test_typed_symbols_require_authoritative_declarations(self):
        source = (
            "Start::\n"
            "\tgivemon SPECIES_DOES_NOT_EXIST, 5, ITEM_DOES_NOT_EXIST\n"
            "\tsetvar VAR_DOES_NOT_EXIST, 1\n"
            "\tsetflag FLAG_DOES_NOT_EXIST\n"
            "\tspecialvar VAR_DOES_NOT_EXIST, Johto_CheckCelebi\n"
            "\tend\n"
        )
        compiler = fixture_compiler(
            "TypedFixture", source,
            host={"VAR_RESULT", "end", "Johto_CheckCelebi"},
        )
        _, diagnostics, _ = compiler.translate_source("TypedFixture")
        reasons = " ".join(item.reason for item in diagnostics)
        for symbol in ("SPECIES_DOES_NOT_EXIST", "ITEM_DOES_NOT_EXIST",
                       "VAR_DOES_NOT_EXIST", "FLAG_DOES_NOT_EXIST"):
            self.assertIn(symbol, reasons)

    def test_local_aliases_preserve_typed_roles_across_chains(self):
        source = (
            ".set LOCAL_SPECIES, SPECIES_EEVEE\n"
            ".equ CHAIN_SPECIES, LOCAL_SPECIES\n"
            ".set LOCAL_ITEM, ITEM_NONE\n"
            ".equ CHAIN_ITEM, LOCAL_ITEM\n"
            ".set LOCAL_VAR, VAR_RESULT\n"
            ".equ CHAIN_VAR, LOCAL_VAR\n"
            ".set LOCAL_NUMERIC, 2\n"
            "Start::\n"
            "\tgivemon CHAIN_SPECIES, LOCAL_NUMERIC, CHAIN_ITEM\n"
            "\tsetvar CHAIN_VAR, LOCAL_NUMERIC\n"
            "\tbufferspeciesname STR_VAR_1, CHAIN_SPECIES\n"
            "\tend\n"
        )
        compiler = fixture_compiler(
            "TypedAliasFixture", source,
            host={"SPECIES_EEVEE", "ITEM_NONE", "VAR_RESULT", "STR_VAR_1", "end"},
        )
        _, diagnostics, _ = compiler.translate_source("TypedAliasFixture")
        self.assertFalse([item for item in diagnostics if item.kind == "unknown-reference"])

    def test_local_aliases_reject_wrong_role_unknown_and_cycles(self):
        source = (
            ".set SPECIES_AS_ITEM, SPECIES_EEVEE\n"
            ".set ITEM_AS_SPECIES, ITEM_NONE\n"
            ".set VAR_AS_FLAG, VAR_RESULT\n"
            ".set UNKNOWN_ALIAS, SPECIES_DOES_NOT_EXIST\n"
            ".set CYCLE_A, CYCLE_B\n"
            ".set CYCLE_B, CYCLE_A\n"
            "Start::\n"
            "\tgivemon SPECIES_EEVEE, 5, SPECIES_AS_ITEM\n"
            "\tgivemon ITEM_AS_SPECIES, 5, ITEM_NONE\n"
            "\tsetflag VAR_AS_FLAG\n"
            "\tgivemon UNKNOWN_ALIAS, 5, ITEM_NONE\n"
            "\tgivemon CYCLE_A, 5, ITEM_NONE\n"
            "\tend\n"
        )
        compiler = fixture_compiler(
            "InvalidTypedAliasFixture", source,
            host={"SPECIES_EEVEE", "ITEM_NONE", "end"},
        )
        _, diagnostics, _ = compiler.translate_source("InvalidTypedAliasFixture")
        reasons = " ".join(item.reason for item in diagnostics)
        for alias in ("SPECIES_AS_ITEM", "ITEM_AS_SPECIES", "VAR_AS_FLAG", "UNKNOWN_ALIAS", "CYCLE_A"):
            self.assertIn(alias, reasons)

    def test_numeric_aliases_match_direct_numeric_variable_rejection(self):
        direct = fixture_compiler(
            "DirectNumericVariableFixture",
            "Start::\n\tsetvar 1, 0\n\tend\n",
            host={"VAR_RESULT", "end"},
        )
        _, direct_diagnostics, _ = direct.translate_source("DirectNumericVariableFixture")
        aliased = fixture_compiler(
            "AliasedNumericVariableFixture",
            ".set LOCAL_NUMERIC, 1\nStart::\n\tsetvar LOCAL_NUMERIC, 0\n\tend\n",
            host={"VAR_RESULT", "end"},
        )
        _, aliased_diagnostics, _ = aliased.translate_source("AliasedNumericVariableFixture")
        self.assertTrue(any(item.kind == "unknown-reference" for item in direct_diagnostics))
        self.assertTrue(any(item.kind == "unknown-reference" for item in aliased_diagnostics))

        valid = fixture_compiler(
            "NumericValueAliasFixture",
            ".set LOCAL_NUMERIC, 1\nStart::\n\tsetvar VAR_RESULT, LOCAL_NUMERIC\n\tend\n",
            host={"VAR_RESULT", "end"},
        )
        _, valid_diagnostics, _ = valid.translate_source("NumericValueAliasFixture")
        self.assertFalse([item for item in valid_diagnostics if item.kind == "unknown-reference"])

    def test_numeric_local_alias_role_precedes_colliding_variable_names(self):
        for alias in ("VAR_RESULT", "STR_VAR_1", "JOHTO_VAR_COLLISION"):
            with self.subTest(alias=alias):
                source = f".set {alias}, 1\nStart::\n\tsetvar {alias}, 0\n\tend\n"
                compiler = fixture_compiler(
                    "NumericAliasCollisionFixture",
                    source,
                    host={alias, "end"},
                )
                _, diagnostics, _ = compiler.translate_source("NumericAliasCollisionFixture")
                self.assertTrue(any(
                    item.kind == "unknown-reference" and alias in item.reason
                    for item in diagnostics
                ))

    def test_provenance_diagnostics_preserve_both_outputs_byte_for_byte(self):
        for kind in (
            "donor-revision",
            "source-hash",
            "donor-source-hash",
            "donor-block-hash",
            "source-inventory",
        ):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                dependencies = root / "data/johto/script_dependencies.json"
                output = root / "data/johto/campaign_scripts.inc"
                dependencies.parent.mkdir(parents=True)
                dependencies.write_bytes(b"dependencies-before\r\n\x00")
                output.write_bytes(b"output-before\r\n\xff")
                compiler = object.__new__(ScriptCompiler)
                compiler.root = root
                compiler.preflight = mock.Mock(return_value={
                    "diagnostics": [{"kind": kind, "source": "donor", "line": 0, "reason": "drift"}],
                    "diagnostic_count": 1,
                    "translated_sources": {},
                })

                with self.assertRaises(ScriptError):
                    compiler.write_or_check(True)

                self.assertEqual(dependencies.read_bytes(), b"dependencies-before\r\n\x00")
                self.assertEqual(output.read_bytes(), b"output-before\r\n\xff")

    def test_transaction_rolls_back_first_output_when_second_replace_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dependencies = root / "data/johto/script_dependencies.json"
            output = root / "data/johto/campaign_scripts.inc"
            dependencies.parent.mkdir(parents=True)
            dependencies.write_bytes(b"dependencies-before\r\n\x00")
            output.write_bytes(b"output-before\r\n\xff")
            compiler = object.__new__(ScriptCompiler)
            compiler.root = root
            complete = {
                "diagnostics": [],
                "diagnostic_count": 0,
                "translated_sources": {},
                "emitted": False,
            }
            compiler.preflight = mock.Mock(side_effect=[dict(complete), dict(complete)])
            compiler.render = mock.Mock(return_value="output-after\n")
            compiler._assert_donor_provenance = mock.Mock()

            real_replace = __import__("os").replace
            installed = 0

            def fail_second_install(source, destination):
                nonlocal installed
                source = Path(source)
                if source.name.endswith(".new"):
                    installed += 1
                    if installed == 2:
                        raise OSError("injected second-output failure")
                return real_replace(source, destination)

            with mock.patch("tools.johto.content_scripts.os.replace", side_effect=fail_second_install):
                with self.assertRaises(OSError):
                    compiler.write_or_check(True)

            self.assertEqual(dependencies.read_bytes(), b"dependencies-before\r\n\x00")
            self.assertEqual(output.read_bytes(), b"output-before\r\n\xff")

    def _assert_transaction_restore_failures(self, failed_names):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dependencies = root / "data/johto/script_dependencies.json"
            output = root / "data/johto/campaign_scripts.inc"
            dependencies.parent.mkdir(parents=True)
            dependencies.write_bytes(b"dependencies-before\r\n\x00")
            output.write_bytes(b"output-before\r\n\xff")
            compiler = object.__new__(ScriptCompiler)
            compiler.root = root
            complete = {
                "diagnostics": [],
                "diagnostic_count": 0,
                "translated_sources": {},
                "emitted": False,
            }
            compiler.preflight = mock.Mock(side_effect=[dict(complete), dict(complete)])
            compiler.render = mock.Mock(return_value="output-after\n")
            compiler._assert_donor_provenance = mock.Mock()

            real_replace = __import__("os").replace
            installed = 0

            def fail_install_then_restore(source, destination):
                nonlocal installed
                source = Path(source)
                destination = Path(destination)
                if source.name.endswith(".new"):
                    installed += 1
                    if installed == 2:
                        raise OSError("injected second-output failure")
                if source.name.endswith(".bak") and destination.name in failed_names:
                    raise OSError(f"injected rollback failure for {destination.name}")
                return real_replace(source, destination)

            with mock.patch(
                "tools.johto.content_scripts.os.replace",
                side_effect=fail_install_then_restore,
            ):
                with self.assertRaisesRegex(ScriptError, "rollback was incomplete") as raised:
                    compiler.write_or_check(True)

            message = str(raised.exception)
            self.assertIn("injected second-output failure", message)
            rollback_details = message.split("rollback was incomplete: ", 1)[1].split(";", 1)[0]
            for target, original in (
                (dependencies, b"dependencies-before\r\n\x00"),
                (output, b"output-before\r\n\xff"),
            ):
                backups = list(target.parent.glob(f".{target.name}.*.bak"))
                if target.name in failed_names:
                    self.assertEqual(len(backups), 1)
                    self.assertEqual(backups[0].read_bytes(), original)
                    self.assertIn(
                        f"restore {backups[0]} -> {target}",
                        message,
                    )
                    self.assertIn(str(target), rollback_details)
                    self.assertIn(f"injected rollback failure for {target.name}", message)
                    self.assertFalse(target.exists())
                else:
                    self.assertFalse(backups)
                    self.assertEqual(target.read_bytes(), original)

    def test_transaction_reports_dependency_restore_failure(self):
        self._assert_transaction_restore_failures({"script_dependencies.json"})

    def test_transaction_reports_output_restore_failure(self):
        self._assert_transaction_restore_failures({"campaign_scripts.inc"})

    def test_transaction_reports_both_restore_failures(self):
        self._assert_transaction_restore_failures({
            "script_dependencies.json",
            "campaign_scripts.inc",
        })

    def test_transaction_reports_cleanup_failure_without_masking_install_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dependencies = root / "data/johto/script_dependencies.json"
            output = root / "data/johto/campaign_scripts.inc"
            dependencies.parent.mkdir(parents=True)
            dependencies.write_bytes(b"dependencies-before")
            output.write_bytes(b"output-before")
            compiler = object.__new__(ScriptCompiler)
            compiler.root = root
            complete = {
                "diagnostics": [],
                "diagnostic_count": 0,
                "translated_sources": {},
                "emitted": False,
            }
            compiler.preflight = mock.Mock(side_effect=[dict(complete), dict(complete)])
            compiler.render = mock.Mock(return_value="output-after\n")
            compiler._assert_donor_provenance = mock.Mock()

            real_replace = __import__("os").replace
            real_unlink = Path.unlink
            cleanup_failures = []

            def fail_first_install(source, destination):
                source = Path(source)
                if source.name.endswith(".new"):
                    raise OSError("primary install failure")
                return real_replace(source, destination)

            def fail_staged_cleanup(path, *args, **kwargs):
                if path.name.endswith(".new"):
                    cleanup_failures.append(path)
                    raise OSError(f"secondary cleanup failure for {path.name}")
                return real_unlink(path, *args, **kwargs)

            with mock.patch(
                "tools.johto.content_scripts.os.replace", side_effect=fail_first_install
            ), mock.patch("pathlib.Path.unlink", autospec=True, side_effect=fail_staged_cleanup):
                with self.assertRaises(ScriptError) as raised:
                    compiler.write_or_check(True)

            message = str(raised.exception)
            self.assertIn("primary install failure", message)
            self.assertIn("secondary cleanup failure(s)", message)
            self.assertEqual(len(cleanup_failures), 2)
            for path in cleanup_failures:
                self.assertIn(str(path), message)
                self.assertIn(f"secondary cleanup failure for {path.name}", message)
            self.assertIsInstance(raised.exception.__cause__, OSError)
            self.assertEqual(str(raised.exception.__cause__), "primary install failure")

    def _assert_committed_install_backup_cleanup_failures(self, failed_names):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dependencies = root / "script_dependencies.json"
            output = root / "campaign_scripts.inc"
            dependencies.write_bytes(b"dependencies-before")
            output.write_bytes(b"output-before")

            real_unlink = Path.unlink
            attempted_backups = []

            def fail_selected_backup_cleanup(path, *args, **kwargs):
                if path.name.endswith(".bak"):
                    attempted_backups.append(path)
                    if any(path.name.startswith(f".{name}.") for name in failed_names):
                        raise OSError(f"injected committed cleanup failure for {path.name}")
                return real_unlink(path, *args, **kwargs)

            with mock.patch(
                "pathlib.Path.unlink", autospec=True, side_effect=fail_selected_backup_cleanup
            ):
                with self.assertRaises(ScriptError) as raised:
                    ScriptCompiler._replace_outputs_transactionally({
                        dependencies: b"dependencies-after",
                        output: b"output-after",
                    })

            message = str(raised.exception)
            self.assertIn("installation committed", message)
            self.assertIn("backup cleanup was incomplete", message)
            self.assertEqual(dependencies.read_bytes(), b"dependencies-after")
            self.assertEqual(output.read_bytes(), b"output-after")
            self.assertEqual(len(attempted_backups), 2)
            for target, original in (
                (dependencies, b"dependencies-before"),
                (output, b"output-before"),
            ):
                backups = list(root.glob(f".{target.name}.*.bak"))
                if target.name in failed_names:
                    self.assertEqual(len(backups), 1)
                    self.assertEqual(backups[0].read_bytes(), original)
                    self.assertIn(str(target), message)
                    self.assertIn(
                        f"restore {backups[0]} -> {target}",
                        message,
                    )
                    self.assertIn(
                        f"injected committed cleanup failure for {backups[0].name}",
                        message,
                    )
                else:
                    self.assertFalse(backups)

    def test_transaction_reports_first_committed_backup_cleanup_failure(self):
        self._assert_committed_install_backup_cleanup_failures({"script_dependencies.json"})

    def test_transaction_reports_multiple_committed_backup_cleanup_failures(self):
        self._assert_committed_install_backup_cleanup_failures({
            "script_dependencies.json",
            "campaign_scripts.inc",
        })

    def test_transaction_reports_backup_move_and_placeholder_cleanup_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dependencies = root / "script_dependencies.json"
            output = root / "campaign_scripts.inc"
            dependencies.write_bytes(b"dependencies-before")
            output.write_bytes(b"output-before")

            real_replace = __import__("os").replace
            real_unlink = Path.unlink
            failed_placeholder = None

            def fail_first_backup_move(source, destination):
                source = Path(source)
                destination = Path(destination)
                if source == dependencies and destination.name.endswith(".bak"):
                    raise OSError("injected target-to-backup move failure")
                return real_replace(source, destination)

            def fail_placeholder_cleanup(path, *args, **kwargs):
                nonlocal failed_placeholder
                if path.name.startswith(f".{dependencies.name}.") and path.name.endswith(".bak"):
                    failed_placeholder = path
                    raise OSError("injected backup placeholder cleanup failure")
                return real_unlink(path, *args, **kwargs)

            with mock.patch(
                "tools.johto.content_scripts.os.replace", side_effect=fail_first_backup_move
            ), mock.patch(
                "pathlib.Path.unlink", autospec=True, side_effect=fail_placeholder_cleanup
            ):
                with self.assertRaises(ScriptError) as raised:
                    ScriptCompiler._replace_outputs_transactionally({
                        dependencies: b"dependencies-after",
                        output: b"output-after",
                    })

            message = str(raised.exception)
            self.assertIn("injected target-to-backup move failure", message)
            self.assertIn("secondary cleanup failure(s)", message)
            self.assertIn(str(dependencies), message)
            self.assertIn("backup placeholder", message)
            self.assertIn("target state: exists", message)
            self.assertIn("injected backup placeholder cleanup failure", message)
            self.assertIsNotNone(failed_placeholder)
            self.assertIn(str(failed_placeholder), message)
            self.assertTrue(failed_placeholder.exists())
            self.assertEqual(failed_placeholder.read_bytes(), b"")
            self.assertEqual(dependencies.read_bytes(), b"dependencies-before")
            self.assertEqual(output.read_bytes(), b"output-before")
            self.assertEqual(str(raised.exception.__cause__), "injected target-to-backup move failure")

    def test_donor_hash_is_reauthenticated_after_render_before_output_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            donor = root / "donor"
            shared = donor / "shared.inc"
            shared.parent.mkdir(parents=True)
            shared.write_bytes(b"authenticated source\n")
            dependencies = root / "data/johto/script_dependencies.json"
            output = root / "data/johto/campaign_scripts.inc"
            dependencies.parent.mkdir(parents=True)
            dependencies.write_bytes(b"dependencies-before")
            output.write_bytes(b"output-before")

            compiler = object.__new__(ScriptCompiler)
            compiler.root = root
            compiler.donor_root = donor
            compiler._expected_source_hashes = {"shared.inc": sha256(shared)}
            compiler._current_donor_revision = mock.Mock(return_value="751823abaf677020bcd72c45fe3e7cb2b8a576e4")
            compiler.preflight = mock.Mock(return_value={
                "diagnostics": [],
                "diagnostic_count": 0,
                "translated_sources": {},
                "emitted": False,
            })

            def render_then_drift(_):
                shared.write_bytes(b"changed after source reads\n")
                return "output-after\n"

            compiler.render = render_then_drift
            with self.assertRaises(ScriptError):
                compiler.write_or_check(True)

            self.assertEqual(dependencies.read_bytes(), b"dependencies-before")
            self.assertEqual(output.read_bytes(), b"output-before")

    def test_donor_revision_is_reauthenticated_after_render_before_output_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dependencies = root / "data/johto/script_dependencies.json"
            output = root / "data/johto/campaign_scripts.inc"
            dependencies.parent.mkdir(parents=True)
            dependencies.write_bytes(b"dependencies-before")
            output.write_bytes(b"output-before")

            compiler = object.__new__(ScriptCompiler)
            compiler.root = root
            compiler.donor_root = root / "donor"
            compiler._expected_source_hashes = {}
            compiler._current_donor_revision = mock.Mock(return_value="changed-after-source-reads")
            compiler.preflight = mock.Mock(return_value={
                "diagnostics": [],
                "diagnostic_count": 0,
                "translated_sources": {},
                "emitted": False,
            })
            compiler.render = mock.Mock(return_value="output-after\n")

            with self.assertRaises(ScriptError):
                compiler.write_or_check(True)

            self.assertEqual(dependencies.read_bytes(), b"dependencies-before")
            self.assertEqual(output.read_bytes(), b"output-before")

    def test_reviewed_closure_aliases_use_the_same_role_checks(self):
        compiler = fixture_compiler("ClosureAliasFixture", "Start::\n\tend\n")
        compiler._translate_reviewed_blocks([
            "@ reviewed donor closure: data/event_scripts.s:BadAliasClosure\n"
            ".set CLOSURE_ITEM, SPECIES_EEVEE\n"
            "BadAliasClosure::\n"
            "\tgivemon SPECIES_EEVEE, 5, CLOSURE_ITEM\n"
        ])
        self.assertTrue(any(
            item.kind == "unknown-reference" and "CLOSURE_ITEM" in item.reason
            for item in compiler.closure_diagnostics
        ))

    def test_reviewed_donor_sources_are_hash_pinned_and_reported(self):
        compiler = object.__new__(ScriptCompiler)
        compiler.donor_root = DONOR
        compiler.source_diagnostics = []
        compiler._load_reviewed_blocks()
        self.assertEqual(compiler.reviewed_donor_source_hashes, REVIEWED_DONOR_SOURCE_HASHES)

    def test_route110_sparkle_closure_is_exact_and_excludes_dispatch_blocks(self):
        compiler = object.__new__(ScriptCompiler)
        compiler.donor_root = DONOR
        compiler.source_diagnostics = []
        blocks, symbols = compiler._load_reviewed_blocks()
        label = "Route110_TrickHouseEntrance_EventScript_DoHidingSpotSparkle"
        selected = next(block for block in blocks if block.startswith(
            "@ reviewed donor closure: data/maps/Route110_TrickHouseEntrance/scripts.inc:" + label
        ))
        donor_block = selected.split("\n", 1)[1].encode("utf-8")
        self.assertEqual(hashlib.sha256(donor_block).hexdigest(), REVIEWED_DONOR_BLOCK_HASHES[label])
        self.assertIn(label, symbols)
        self.assertFalse(any(label + suffix in symbols for suffix in ("1", "2", "3")))

    def test_preflight_rejects_incomplete_authenticated_source_inventory(self):
        compiler = object.__new__(ScriptCompiler)
        compiler.records = []
        compiler.source_diagnostics = []
        compiler.closure_diagnostics = []
        compiler._expected_source_hashes = {f"source/{index}": "0" * 64 for index in range(817)}
        compiler._map_text = {}
        compiler._map_path = {}
        compiler.reviewed_donor_source_hashes = {}
        compiler.imported_symbols = set()
        compiler._dependency_rows = lambda: []
        compiler._reauthenticate_donor = lambda: []
        report = compiler.preflight()
        self.assertTrue(any(
            item["kind"] == "source-inventory" and "got 817" in item["reason"]
            for item in report["diagnostics"]
        ))

    def test_reviewed_donor_source_one_byte_drift_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            donor = Path(directory)
            for relative in REVIEWED_DONOR_BLOCKS:
                destination = donor / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(DONOR / relative, destination)
            drifted = donor / "data/scripts/movement.inc"
            contents = bytearray(drifted.read_bytes())
            contents[0] ^= 1
            drifted.write_bytes(contents)

            compiler = object.__new__(ScriptCompiler)
            compiler.donor_root = donor
            compiler.source_diagnostics = []
            compiler._load_reviewed_blocks()
            self.assertTrue(any(
                item.kind == "donor-source-hash" and item.source == "data/scripts/movement.inc"
                for item in compiler.source_diagnostics
            ))
            self.assertEqual(
                compiler.reviewed_donor_source_hashes["data/scripts/movement.inc"],
                sha256(drifted),
            )

    def test_keyword_bindings_follow_declared_parameters_and_conditionals(self):
        source = (
            "Start::\n"
            "\tgivemon shinyMode=1, level=5, species=SPECIES_EEVEE\n"
            "\tgivemon SPECIES_EEVEE, 5, shinyMode=1\n"
            "\tgoto_if_eq Done\n"
            "\tgoto_if_eq VAR_RESULT, TRUE, Done\n"
            "\tgoto_if_eq VAR_RESULT, TRUE, Done, 9\n"
            "Done::\n\tend\n"
        )
        compiler = fixture_compiler(
            "KeywordFixture", source,
            host={"VAR_RESULT", "SPECIES_EEVEE", "TRUE", "end"},
        )
        _, diagnostics, _ = compiler.translate_source("KeywordFixture")
        reasons = " ".join(item.reason for item in diagnostics)
        self.assertNotIn("shinyMode", reasons)
        self.assertNotIn("requires declared parameter", reasons)
        self.assertIn("accepts at most 3", reasons)

    def test_special_forms_validate_main_and_reviewed_closure(self):
        source = "Start::\n\tspecial\n\tcallnative\n\tspecialvar MISSING_DEST, Johto_CheckCelebi\n\tend\n"
        compiler = fixture_compiler("SpecialFixture", source, host={"end", "Johto_CheckCelebi"})
        _, diagnostics, _ = compiler.translate_source("SpecialFixture")
        reasons = " ".join(item.reason for item in diagnostics)
        self.assertIn("special requires exactly 1", reasons)
        self.assertIn("callnative requires 1 or 2", reasons)
        self.assertIn("MISSING_DEST", reasons)
        compiler._translate_reviewed_blocks([
            "@ reviewed donor closure: data/event_scripts.s:BadClosure\n"
            "BadClosure::\n\t special MissingSpecial\n"
        ])
        self.assertTrue(any(item.kind == "unknown-special" for item in compiler.closure_diagnostics))

    def test_callnative_effects_are_checked(self):
        source = "Start::\n\tcallnative UpdateFollowingPokemon, requests_effects=1\n\tend\n"
        compiler = fixture_compiler("NativeFixture", source, host={"end", "UpdateFollowingPokemon"})
        _, diagnostics, _ = compiler.translate_source("NativeFixture")
        self.assertTrue(any(item.kind == "native-effects" for item in diagnostics))

    def test_generated_tm_hm_aliases_are_authoritative(self):
        aliases = _generated_tm_hm_constants(ROOT)
        self.assertIn("ITEM_TM_BLIZZARD", aliases)
        self.assertIn("ITEM_HM_SURF", aliases)
        self.assertNotIn("ITEM_TM_NOT_A_REAL_MOVE", aliases)

    def test_policy_replacement_qualifies_destination(self):
        source = ".set LOCAL_DEST, VAR_RESULT\nStart::\n\tspecialvar LOCAL_DEST, GetMaxPartySize\n\tspecialvar LOCAL_DEST, IsNuzlockeNicknamingActive\n\tend\n"
        compiler = fixture_compiler("PolicyFixture", source, host={"end", "VAR_RESULT", "PARTY_SIZE"})
        rendered, diagnostics, _ = compiler.translate_source("PolicyFixture")
        self.assertFalse(diagnostics)
        self.assertIn("setvar Johto_PolicyFixture_LOCAL_DEST, PARTY_SIZE", rendered)
        self.assertIn("setvar Johto_PolicyFixture_LOCAL_DEST, FALSE", rendered)

    def test_invalid_reviewed_closure_refuses_emission(self):
        compiler = fixture_compiler("ClosureFixture", "Start::\n\tend\n")
        compiler.imported_symbol_map = {}
        compiler._translate_reviewed_blocks([
            "@ reviewed donor closure: data/event_scripts.s:BadClosure\n"
            "BadClosure::\n\tgoto MissingClosureLabel\n"
        ])
        self.assertTrue(compiler.closure_diagnostics)
        with self.assertRaises(ScriptError):
            compiler.render({"diagnostic_count": len(compiler.closure_diagnostics), "translated_sources": {}})

    def test_reviewed_field_adapters_preserve_cancellation_and_exact_move(self):
        move = fixture_compiler("CianwoodPokecenter", "Start::\n\tcheckpartymove MOVE_SURF\n\tend\n")
        rendered, diagnostics, _ = move.translate_source("CianwoodPokecenter")
        self.assertFalse(diagnostics)
        self.assertIn("checkfieldmove FIELD_MOVE_SURF, FALSE", rendered)
        berry_source = (
            "Start::\n" + ("\n" * 43)
            + "\tchooseitem BERRIES_POCKET\n\tend\n"
            + "IcePath_B3F_EventScript_SwinubBlowPlayer::\n\tend\n"
        )
        berry = fixture_compiler("IcePath_B3F", berry_source)
        rendered, diagnostics, _ = berry.translate_source("IcePath_B3F")
        self.assertFalse(diagnostics)
        self.assertIn("special Bag_ChooseBerry", rendered)
        self.assertIn("goto_if_eq VAR_ITEM_ID, 0, Johto_IcePath_B3F_IcePath_B3F_EventScript_SwinubBlowPlayer", rendered)

    def test_reviewed_null_marts_bind_to_terminated_product_lists(self):
        cherrygrove_source = (
            "Start::\n" + ("\n" * 37) + "\tpokemart 0\n"
            "Cherrygrove_Pokemart_Pokemart::\n"
            "\t.2byte ITEM_POTION\n\t.2byte ITEM_ANTIDOTE\n\t.2byte ITEM_NONE\n"
        )
        cherrygrove = fixture_compiler("CherrygroveCity_Mart", cherrygrove_source)
        rendered, diagnostics, adaptations = cherrygrove.translate_source("CherrygroveCity_Mart")
        self.assertFalse(diagnostics)
        self.assertIn(
            "pokemart Johto_CherrygroveCity_Mart_Cherrygrove_Pokemart_Pokemart",
            rendered,
        )
        self.assertTrue(any("terminated product list" in item for item in adaptations))

        violet_source = "Start::\n" + ("\n" * 7) + "\tpokemart 0\n"
        violet = fixture_compiler(
            "VioletCity_Mart", violet_source,
            host={"Pokemart_DefaultItemList", "TRUE", "FALSE", "VAR_RESULT", "PARTY_SIZE"},
        )
        rendered, diagnostics, _ = violet.translate_source("VioletCity_Mart")
        self.assertFalse(diagnostics)
        self.assertIn("pokemart Pokemart_DefaultItemList", rendered)

        wrong_site = fixture_compiler("VioletCity_Mart", "Start::\n\tpokemart 0\n")
        _, diagnostics, _ = wrong_site.translate_source("VioletCity_Mart")
        self.assertTrue(any(item.kind == "unsupported-adapter" for item in diagnostics))

    def test_runtime_dependency_probe_covers_missing_then_present(self):
        compiler = object.__new__(ScriptCompiler)
        with tempfile.TemporaryDirectory() as directory:
            compiler.root = Path(directory)
            self.assertFalse(compiler._runtime_available("berry"))
            script_path = compiler.root / "data/scripts"
            script_path.mkdir(parents=True)
            (script_path / "johto_berry_tree.inc").write_text("Johto_BerryTreeScript::\n\tend\n", encoding="utf-8")
            (compiler.root / "data/event_scripts.s").write_text('.include "data/scripts/johto_berry_tree.inc"\n', encoding="utf-8")
            implementation = compiler.root / "src/johto"
            implementation.mkdir(parents=True)
            (implementation / "berry_plots.c").write_text(
                "void JohtoBerryPlots_TryHarvest(void) {}\nvoid Script_JohtoHarvestBerryTree(void) {}\n",
                encoding="utf-8",
            )
            (compiler.root / "Makefile").write_text(
                "TOOLCHAIN := tools\n"
                "TOOLCHAIN_EXISTS := $(wildcard $(TOOLCHAIN)/bin)\n"
                "C_SUBDIR := src\n"
                "C_SRCS_IN := $(wildcard $(C_SUBDIR)/*.c $(C_SUBDIR)/*/*.c $(C_SUBDIR)/*/*/*.c)\n",
                encoding="utf-8",
            )
            compiler._runtime_cache.clear()
            self.assertTrue(compiler._runtime_available("berry"))

            nested = implementation / "nested"
            nested.mkdir()
            nested_source = nested / "probe.c"
            nested_source.write_text(
                "void JohtoBerryPlots_TryHarvest(void) {}\nvoid Script_JohtoHarvestBerryTree(void) {}\n",
                encoding="utf-8",
            )
            compiler._runtime_cache.clear()
            self.assertTrue(compiler._linked_native_source(nested_source, ("Script_JohtoHarvestBerryTree",)))

    def test_runtime_linkage_rejects_declaration_or_wrong_include(self):
        compiler = object.__new__(ScriptCompiler)
        with tempfile.TemporaryDirectory() as directory:
            compiler.root = Path(directory)
            script_path = compiler.root / "data/scripts"
            script_path.mkdir(parents=True)
            script = script_path / "johto_berry_tree.inc"
            script.write_text("Johto_BerryTreeScript::\n\tend\n", encoding="utf-8")
            (compiler.root / "data/event_scripts.s").write_text(
                '.include "data/scripts/not_johto_berry_tree.inc"\n', encoding="utf-8"
            )
            implementation = compiler.root / "src/johto"
            implementation.mkdir(parents=True)
            (implementation / "berry_plots.c").write_text(
                "void JohtoBerryPlots_TryHarvest(void);\n"
                "void Script_JohtoHarvestBerryTree(void);\n", encoding="utf-8"
            )
            (compiler.root / "Makefile").write_text(
                "C_SUBDIR := src\nC_SRCS_IN := $(wildcard $(C_SUBDIR)/*/*.c)\n", encoding="utf-8"
            )
            compiler._runtime_cache = {}
            self.assertFalse(compiler._runtime_available("berry"))

    def test_runtime_source_inventory_rejects_depth_exclusion_and_replacement(self):
        compiler = object.__new__(ScriptCompiler)
        with tempfile.TemporaryDirectory() as directory:
            compiler.root = Path(directory)
            top_level = compiler.root / "src"
            top_level.mkdir(parents=True)
            top_source = top_level / "probe.c"
            top_source.write_text("void Probe(void) {}\n", encoding="utf-8")
            implementation = compiler.root / "src/johto"
            implementation.mkdir(parents=True)
            source = implementation / "probe.c"
            source.write_text("void Probe(void) {}\n", encoding="utf-8")
            nested = implementation / "nested"
            nested.mkdir()
            nested_source = nested / "probe.c"
            nested_source.write_text("void Probe(void) {}\n", encoding="utf-8")
            compiler.root.joinpath("Makefile").write_text(
                "C_SUBDIR := src\nC_SRCS_IN := $(wildcard $(C_SUBDIR)/*.c)\n",
                encoding="utf-8",
            )
            self.assertTrue(compiler._linked_native_source(top_source, ("Probe",)))
            self.assertFalse(compiler._linked_native_source(nested_source, ("Probe",)))
            compiler.root.joinpath("Makefile").write_text(
                "C_SUBDIR := src\nC_SRCS_IN := $(filter-out src/johto/probe.c,$(wildcard $(C_SUBDIR)/*/*.c))\n",
                encoding="utf-8",
            )
            self.assertFalse(compiler._linked_native_source(source, ("Probe",)))
            compiler.root.joinpath("Makefile").write_text(
                "C_SUBDIR := src\nC_SRCS_IN := src/johto/probe.c\nC_SRCS_IN := src/other.c\n",
                encoding="utf-8",
            )
            self.assertFalse(compiler._linked_native_source(source, ("Probe",)))

    def test_provenance_hash_changes_when_source_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "script.inc"
            path.write_text("Start::\n\tend\n", encoding="utf-8", newline="\n")
            original = _normalize_sha256(path)
            path.write_text("Start::\n\treturn\n", encoding="utf-8", newline="\n")
            self.assertNotEqual(original, _normalize_sha256(path))

    def test_actual_corpus_has_complete_recoverable_contest_closure(self):
        if not DONOR.is_dir():
            self.skipTest("pinned donor checkout is unavailable")
        compiler = ScriptCompiler(ROOT, DONOR)
        report = compiler.preflight()
        self.assertEqual(report["selected_map_count"], 407)
        self.assertEqual(report["selected_script_count"], 407)
        self.assertEqual(report["original_selected_map_count"], 239)
        self.assertEqual(report["later_selected_map_count"], 168)
        self.assertEqual(report["authenticated_source_count"], 823)
        self.assertEqual(len(report["source_hashes"]), 416)
        self.assertEqual(len(report["source_hashes_normalized"]), 416)
        self.assertEqual(len(report["reviewed_donor_source_hashes"]), 9)
        for relative in REVIEWED_DONOR_BLOCKS:
            self.assertEqual(report["source_hashes"][relative], sha256(DONOR / relative))
        sparkle = "Route110_TrickHouseEntrance_EventScript_DoHidingSpotSparkle"
        self.assertIn(sparkle, report["reviewed_donor_closure"])
        self.assertNotIn("Route110_TrickHouseEntrance_EventScript_DoHidingSpotSparkle1", report["reviewed_donor_closure"])
        self.assertIn(
            "call Johto_Closure_data_maps_Route110_TrickHouseEntrance_scripts_inc_" + sparkle,
            report["translated_sources"]["CeruleanCity_Gym"],
        )
        self.assertIn("KantoLater_", report["map_script_label_contract"]["pallet_town_preview"])
        self.assertEqual(report["diagnostic_count"], 0)
        pending = {item["id"]: item["status"] for item in report["dependencies"]}
        self.assertEqual(pending["contest"], "resolved")
        # A checked-out runtime adapter is resolved only when its tracked
        # implementation and build inclusion are present.  The isolated
        # fixer base intentionally has neither; canonical may have both.
        self.assertIn(pending["berry"], {"pending", "resolved"})
        self.assertIn(pending["whirlpool"], {"pending", "resolved"})
        self.assertEqual(pending["trainer-hill-elemental-tutor"], "pending")
        self.assertTrue(report["complete"])
        rendered = compiler.render(report)
        self.assertTrue(all(line == line.rstrip(" \t") for line in rendered.split("\n")))
        self.assertEqual(rendered.count("\n@ source map: "), 407)
        for label in REVIEWED_EXTERNAL_OBJECT_SCRIPT_SITES:
            qualified = compiler.imported_symbol_map[label]
            self.assertTrue(qualified.endswith("_" + label))
            self.assertEqual(rendered.count(qualified + "::"), 1)
            self.assertNotIn(label + "1::", rendered)
        self.assertNotIn("Johto_Closure_data_maps_TrainerHill_Courtyard_scripts_inc_TrainerHill_Courtyard_MapScripts", rendered)
        self.assertNotIn("@ source map: TrainerHill_Courtyard\n", rendered)
        self.assertNotIn("ToggleShinyColors", rendered)

    def test_switch_mon_ability_has_real_registered_runtime(self):
        specials = (ROOT / "data/specials.inc").read_text(encoding="utf-8")
        runtime = (ROOT / "src/field_specials.c").read_text(encoding="utf-8")
        self.assertRegex(specials, r"(?m)^\s*def_special SwitchMonAbility$")
        self.assertIn("void SwitchMonAbility(void)", runtime)
        self.assertIn("newAbilityNum = !currentAbilityNum;", runtime)
        self.assertIn("newAbility = GetSpeciesAbility(species, newAbilityNum);", runtime)
        for special in (
            "Johto_BeginBugContestAdmission",
            "Johto_ShowBugContestChosenMon",
            "Johto_AbortBugContestAdmission",
            "Johto_RequestBugContestTimeout",
            "Johto_JudgeBugContestSelectedMon",
            "Johto_PrepareBugContestSettlement",
            "Johto_ShowBugContestResult",
            "Johto_TransferBugContestSelectedMon",
            "Johto_ClaimBugContestReward",
            "Johto_ForfeitBugContestReward",
            "Johto_ExitBugContest",
        ):
            self.assertRegex(specials, rf"(?m)^\s*def_special {special}$")
            self.assertIn(f"void {special}(void)", runtime)
        self.assertIn("JohtoBugContest_Begin(gMain.vblankCounter1)", runtime)

    def test_bug_contest_campaign_closure_uses_atomic_recoverable_settlement(self):
        closure = STATIC_CAMPAIGN_CLOSURE
        self.assertIn("BugContest_EventScript_TimesUp::", closure)
        self.assertIn("BugContestEventScript_Judging::", closure)
        self.assertIn("special Johto_RequestBugContestTimeout", closure)
        self.assertIn("special Johto_JudgeBugContestSelectedMon", closure)
        self.assertLess(
            closure.index("special Johto_PrepareBugContestSettlement"),
            closure.index("special Johto_TransferBugContestSelectedMon"),
        )
        self.assertIn("Johto_BugContest_EventScript_RetryTransfer::", closure)
        self.assertIn("Johto_BugContest_EventScript_RetryReward::", closure)
        self.assertIn("special Johto_ForfeitBugContestReward", closure)
        self.assertLess(
            closure.index("special Johto_ForfeitBugContestReward"),
            closure.index("special Johto_ExitBugContest"),
        )
        self.assertNotIn("special TransferBugContestMon", closure)
        self.assertNotIn("giveitem VAR_0x8005", closure)

    def test_manifest_rejects_wrong_unsafe_and_duplicate_script_namespaces_before_source_reads(self):
        manifest = json.loads((ROOT / "data/johto/region_manifest.json").read_text(encoding="utf-8"))
        symbols = (ROOT / "data/johto/content_symbols.json").read_bytes()
        mutations = (
            lambda data: data["maps"][239]["identity_namespace"].__setitem__("script", "Johto_PalletTown"),
            lambda data: data["maps"][239]["identity_namespace"].__setitem__("script", "KantoLater_PalletTown\nInjected"),
            lambda data: (
                data["maps"][1].__setitem__("source_name", data["maps"][0]["source_name"]),
                data["maps"][1]["identity_namespace"].__setitem__(
                    "script", data["maps"][0]["identity_namespace"]["script"]
                ),
            ),
        )
        for mutate in mutations:
            with self.subTest(mutate=mutate), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                candidate = json.loads(json.dumps(manifest))
                mutate(candidate)
                manifest_path = root / "data/johto/region_manifest.json"
                symbols_path = root / "data/johto/content_symbols.json"
                manifest_path.parent.mkdir(parents=True)
                manifest_path.write_text(json.dumps(candidate), encoding="utf-8")
                symbols_path.write_bytes(symbols)
                with self.assertRaises(ScriptError):
                    ScriptCompiler(root, DONOR)


if __name__ == "__main__":
    unittest.main()
