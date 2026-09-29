//! Which cooperative trainer battles a member may request, and whether each
//! member joins as a participant or a helper (A7 rematches, A8 Hoenn gyms,
//! A9 Hoenn story battles).
//!
//! The tables mirror the ROM: `gRematchTable` in `src/battle_setup.c`,
//! `sHoennGyms`, `sCoopStoryBattles` and `sCoopStoryTrainers` in
//! `src/coop/trainer_rewards.c`. The tests below parse those sources (and the
//! flag and var headers), so an edit on either side fails here until the
//! other follows.
//! The ROM stays authoritative for the rewards themselves (option A): these
//! roles come from the last finalized cloud save and can lag the live game.

use coop_protocol::TrainerInstanceId;

/// Hoenn gym leaders' first battles, in badge order: the leader at index
/// `n` gives the badge whose flag is `FLAG_BADGE01_GET + n`.
pub(super) const HOENN_GYM_LEADERS: [&str; 8] = [
    "HOENN:TRAINER_ROXANNE_1",
    "HOENN:TRAINER_BRAWLY_1",
    "HOENN:TRAINER_WATTSON_1",
    "HOENN:TRAINER_FLANNERY_1",
    "HOENN:TRAINER_NORMAN_1",
    "HOENN:TRAINER_WINONA_1",
    "HOENN:TRAINER_TATE_AND_LIZA_1",
    "HOENN:TRAINER_JUAN_1",
];

/// Match-call rematch entries by first-battle trainer: every later column
/// of a `gRematchTable` row that names a different trainer. The Elite Four
/// rows repeat their own ID and are not rematches.
pub(super) const HOENN_REMATCHES: &[(&str, &[&str])] = &[
    (
        "HOENN:TRAINER_ROSE_1",
        &[
            "HOENN:TRAINER_ROSE_2",
            "HOENN:TRAINER_ROSE_3",
            "HOENN:TRAINER_ROSE_4",
            "HOENN:TRAINER_ROSE_5",
        ],
    ),
    (
        "HOENN:TRAINER_ANDRES_1",
        &[
            "HOENN:TRAINER_ANDRES_2",
            "HOENN:TRAINER_ANDRES_3",
            "HOENN:TRAINER_ANDRES_4",
            "HOENN:TRAINER_ANDRES_5",
        ],
    ),
    (
        "HOENN:TRAINER_DUSTY_1",
        &[
            "HOENN:TRAINER_DUSTY_2",
            "HOENN:TRAINER_DUSTY_3",
            "HOENN:TRAINER_DUSTY_4",
            "HOENN:TRAINER_DUSTY_5",
        ],
    ),
    (
        "HOENN:TRAINER_LOLA_1",
        &[
            "HOENN:TRAINER_LOLA_2",
            "HOENN:TRAINER_LOLA_3",
            "HOENN:TRAINER_LOLA_4",
            "HOENN:TRAINER_LOLA_5",
        ],
    ),
    (
        "HOENN:TRAINER_RICKY_1",
        &[
            "HOENN:TRAINER_RICKY_2",
            "HOENN:TRAINER_RICKY_3",
            "HOENN:TRAINER_RICKY_4",
            "HOENN:TRAINER_RICKY_5",
        ],
    ),
    (
        "HOENN:TRAINER_LILA_AND_ROY_1",
        &[
            "HOENN:TRAINER_LILA_AND_ROY_2",
            "HOENN:TRAINER_LILA_AND_ROY_3",
            "HOENN:TRAINER_LILA_AND_ROY_4",
            "HOENN:TRAINER_LILA_AND_ROY_5",
        ],
    ),
    (
        "HOENN:TRAINER_CRISTIN_1",
        &[
            "HOENN:TRAINER_CRISTIN_2",
            "HOENN:TRAINER_CRISTIN_3",
            "HOENN:TRAINER_CRISTIN_4",
            "HOENN:TRAINER_CRISTIN_5",
        ],
    ),
    (
        "HOENN:TRAINER_BROOKE_1",
        &[
            "HOENN:TRAINER_BROOKE_2",
            "HOENN:TRAINER_BROOKE_3",
            "HOENN:TRAINER_BROOKE_4",
            "HOENN:TRAINER_BROOKE_5",
        ],
    ),
    (
        "HOENN:TRAINER_WILTON_1",
        &[
            "HOENN:TRAINER_WILTON_2",
            "HOENN:TRAINER_WILTON_3",
            "HOENN:TRAINER_WILTON_4",
            "HOENN:TRAINER_WILTON_5",
        ],
    ),
    (
        "HOENN:TRAINER_VALERIE_1",
        &[
            "HOENN:TRAINER_VALERIE_2",
            "HOENN:TRAINER_VALERIE_3",
            "HOENN:TRAINER_VALERIE_4",
            "HOENN:TRAINER_VALERIE_5",
        ],
    ),
    (
        "HOENN:TRAINER_CINDY_1",
        &[
            "HOENN:TRAINER_CINDY_3",
            "HOENN:TRAINER_CINDY_4",
            "HOENN:TRAINER_CINDY_5",
            "HOENN:TRAINER_CINDY_6",
        ],
    ),
    (
        "HOENN:TRAINER_THALIA_1",
        &[
            "HOENN:TRAINER_THALIA_2",
            "HOENN:TRAINER_THALIA_3",
            "HOENN:TRAINER_THALIA_4",
            "HOENN:TRAINER_THALIA_5",
        ],
    ),
    (
        "HOENN:TRAINER_JESSICA_1",
        &[
            "HOENN:TRAINER_JESSICA_2",
            "HOENN:TRAINER_JESSICA_3",
            "HOENN:TRAINER_JESSICA_4",
            "HOENN:TRAINER_JESSICA_5",
        ],
    ),
    (
        "HOENN:TRAINER_WINSTON_1",
        &[
            "HOENN:TRAINER_WINSTON_2",
            "HOENN:TRAINER_WINSTON_3",
            "HOENN:TRAINER_WINSTON_4",
            "HOENN:TRAINER_WINSTON_5",
        ],
    ),
    (
        "HOENN:TRAINER_STEVE_1",
        &[
            "HOENN:TRAINER_STEVE_2",
            "HOENN:TRAINER_STEVE_3",
            "HOENN:TRAINER_STEVE_4",
            "HOENN:TRAINER_STEVE_5",
        ],
    ),
    (
        "HOENN:TRAINER_TONY_1",
        &[
            "HOENN:TRAINER_TONY_2",
            "HOENN:TRAINER_TONY_3",
            "HOENN:TRAINER_TONY_4",
            "HOENN:TRAINER_TONY_5",
        ],
    ),
    (
        "HOENN:TRAINER_NOB_1",
        &[
            "HOENN:TRAINER_NOB_2",
            "HOENN:TRAINER_NOB_3",
            "HOENN:TRAINER_NOB_4",
            "HOENN:TRAINER_NOB_5",
        ],
    ),
    (
        "HOENN:TRAINER_KOJI_1",
        &[
            "HOENN:TRAINER_KOJI_2",
            "HOENN:TRAINER_KOJI_3",
            "HOENN:TRAINER_KOJI_4",
            "HOENN:TRAINER_KOJI_5",
        ],
    ),
    (
        "HOENN:TRAINER_FERNANDO_1",
        &[
            "HOENN:TRAINER_FERNANDO_2",
            "HOENN:TRAINER_FERNANDO_3",
            "HOENN:TRAINER_FERNANDO_4",
            "HOENN:TRAINER_FERNANDO_5",
        ],
    ),
    (
        "HOENN:TRAINER_DALTON_1",
        &[
            "HOENN:TRAINER_DALTON_2",
            "HOENN:TRAINER_DALTON_3",
            "HOENN:TRAINER_DALTON_4",
            "HOENN:TRAINER_DALTON_5",
        ],
    ),
    (
        "HOENN:TRAINER_BERNIE_1",
        &[
            "HOENN:TRAINER_BERNIE_2",
            "HOENN:TRAINER_BERNIE_3",
            "HOENN:TRAINER_BERNIE_4",
            "HOENN:TRAINER_BERNIE_5",
        ],
    ),
    (
        "HOENN:TRAINER_ETHAN_1",
        &[
            "HOENN:TRAINER_ETHAN_2",
            "HOENN:TRAINER_ETHAN_3",
            "HOENN:TRAINER_ETHAN_4",
            "HOENN:TRAINER_ETHAN_5",
        ],
    ),
    (
        "HOENN:TRAINER_JOHN_AND_JAY_1",
        &[
            "HOENN:TRAINER_JOHN_AND_JAY_2",
            "HOENN:TRAINER_JOHN_AND_JAY_3",
            "HOENN:TRAINER_JOHN_AND_JAY_4",
            "HOENN:TRAINER_JOHN_AND_JAY_5",
        ],
    ),
    (
        "HOENN:TRAINER_JEFFREY_1",
        &[
            "HOENN:TRAINER_JEFFREY_2",
            "HOENN:TRAINER_JEFFREY_3",
            "HOENN:TRAINER_JEFFREY_4",
            "HOENN:TRAINER_JEFFREY_5",
        ],
    ),
    (
        "HOENN:TRAINER_CAMERON_1",
        &[
            "HOENN:TRAINER_CAMERON_2",
            "HOENN:TRAINER_CAMERON_3",
            "HOENN:TRAINER_CAMERON_4",
            "HOENN:TRAINER_CAMERON_5",
        ],
    ),
    (
        "HOENN:TRAINER_JACKI_1",
        &[
            "HOENN:TRAINER_JACKI_2",
            "HOENN:TRAINER_JACKI_3",
            "HOENN:TRAINER_JACKI_4",
            "HOENN:TRAINER_JACKI_5",
        ],
    ),
    (
        "HOENN:TRAINER_WALTER_1",
        &[
            "HOENN:TRAINER_WALTER_2",
            "HOENN:TRAINER_WALTER_3",
            "HOENN:TRAINER_WALTER_4",
            "HOENN:TRAINER_WALTER_5",
        ],
    ),
    (
        "HOENN:TRAINER_KAREN_1",
        &[
            "HOENN:TRAINER_KAREN_2",
            "HOENN:TRAINER_KAREN_3",
            "HOENN:TRAINER_KAREN_4",
            "HOENN:TRAINER_KAREN_5",
        ],
    ),
    (
        "HOENN:TRAINER_JERRY_1",
        &[
            "HOENN:TRAINER_JERRY_2",
            "HOENN:TRAINER_JERRY_3",
            "HOENN:TRAINER_JERRY_4",
            "HOENN:TRAINER_JERRY_5",
        ],
    ),
    (
        "HOENN:TRAINER_ANNA_AND_MEG_1",
        &[
            "HOENN:TRAINER_ANNA_AND_MEG_2",
            "HOENN:TRAINER_ANNA_AND_MEG_3",
            "HOENN:TRAINER_ANNA_AND_MEG_4",
            "HOENN:TRAINER_ANNA_AND_MEG_5",
        ],
    ),
    (
        "HOENN:TRAINER_ISABEL_1",
        &[
            "HOENN:TRAINER_ISABEL_2",
            "HOENN:TRAINER_ISABEL_3",
            "HOENN:TRAINER_ISABEL_4",
            "HOENN:TRAINER_ISABEL_5",
        ],
    ),
    (
        "HOENN:TRAINER_MIGUEL_1",
        &[
            "HOENN:TRAINER_MIGUEL_2",
            "HOENN:TRAINER_MIGUEL_3",
            "HOENN:TRAINER_MIGUEL_4",
            "HOENN:TRAINER_MIGUEL_5",
        ],
    ),
    (
        "HOENN:TRAINER_TIMOTHY_1",
        &[
            "HOENN:TRAINER_TIMOTHY_2",
            "HOENN:TRAINER_TIMOTHY_3",
            "HOENN:TRAINER_TIMOTHY_4",
            "HOENN:TRAINER_TIMOTHY_5",
        ],
    ),
    (
        "HOENN:TRAINER_SHELBY_1",
        &[
            "HOENN:TRAINER_SHELBY_2",
            "HOENN:TRAINER_SHELBY_3",
            "HOENN:TRAINER_SHELBY_4",
            "HOENN:TRAINER_SHELBY_5",
        ],
    ),
    (
        "HOENN:TRAINER_CALVIN_1",
        &[
            "HOENN:TRAINER_CALVIN_2",
            "HOENN:TRAINER_CALVIN_3",
            "HOENN:TRAINER_CALVIN_4",
            "HOENN:TRAINER_CALVIN_5",
        ],
    ),
    (
        "HOENN:TRAINER_ELLIOT_1",
        &[
            "HOENN:TRAINER_ELLIOT_2",
            "HOENN:TRAINER_ELLIOT_3",
            "HOENN:TRAINER_ELLIOT_4",
            "HOENN:TRAINER_ELLIOT_5",
        ],
    ),
    (
        "HOENN:TRAINER_ISAIAH_1",
        &[
            "HOENN:TRAINER_ISAIAH_2",
            "HOENN:TRAINER_ISAIAH_3",
            "HOENN:TRAINER_ISAIAH_4",
            "HOENN:TRAINER_ISAIAH_5",
        ],
    ),
    (
        "HOENN:TRAINER_MARIA_1",
        &[
            "HOENN:TRAINER_MARIA_2",
            "HOENN:TRAINER_MARIA_3",
            "HOENN:TRAINER_MARIA_4",
            "HOENN:TRAINER_MARIA_5",
        ],
    ),
    (
        "HOENN:TRAINER_ABIGAIL_1",
        &[
            "HOENN:TRAINER_ABIGAIL_2",
            "HOENN:TRAINER_ABIGAIL_3",
            "HOENN:TRAINER_ABIGAIL_4",
            "HOENN:TRAINER_ABIGAIL_5",
        ],
    ),
    (
        "HOENN:TRAINER_DYLAN_1",
        &[
            "HOENN:TRAINER_DYLAN_2",
            "HOENN:TRAINER_DYLAN_3",
            "HOENN:TRAINER_DYLAN_4",
            "HOENN:TRAINER_DYLAN_5",
        ],
    ),
    (
        "HOENN:TRAINER_KATELYN_1",
        &[
            "HOENN:TRAINER_KATELYN_2",
            "HOENN:TRAINER_KATELYN_3",
            "HOENN:TRAINER_KATELYN_4",
            "HOENN:TRAINER_KATELYN_5",
        ],
    ),
    (
        "HOENN:TRAINER_BENJAMIN_1",
        &[
            "HOENN:TRAINER_BENJAMIN_2",
            "HOENN:TRAINER_BENJAMIN_3",
            "HOENN:TRAINER_BENJAMIN_4",
            "HOENN:TRAINER_BENJAMIN_5",
        ],
    ),
    (
        "HOENN:TRAINER_PABLO_1",
        &[
            "HOENN:TRAINER_PABLO_2",
            "HOENN:TRAINER_PABLO_3",
            "HOENN:TRAINER_PABLO_4",
            "HOENN:TRAINER_PABLO_5",
        ],
    ),
    (
        "HOENN:TRAINER_NICOLAS_1",
        &[
            "HOENN:TRAINER_NICOLAS_2",
            "HOENN:TRAINER_NICOLAS_3",
            "HOENN:TRAINER_NICOLAS_4",
            "HOENN:TRAINER_NICOLAS_5",
        ],
    ),
    (
        "HOENN:TRAINER_ROBERT_1",
        &[
            "HOENN:TRAINER_ROBERT_2",
            "HOENN:TRAINER_ROBERT_3",
            "HOENN:TRAINER_ROBERT_4",
            "HOENN:TRAINER_ROBERT_5",
        ],
    ),
    (
        "HOENN:TRAINER_LAO_1",
        &[
            "HOENN:TRAINER_LAO_2",
            "HOENN:TRAINER_LAO_3",
            "HOENN:TRAINER_LAO_4",
            "HOENN:TRAINER_LAO_5",
        ],
    ),
    (
        "HOENN:TRAINER_CYNDY_1",
        &[
            "HOENN:TRAINER_CYNDY_2",
            "HOENN:TRAINER_CYNDY_3",
            "HOENN:TRAINER_CYNDY_4",
            "HOENN:TRAINER_CYNDY_5",
        ],
    ),
    (
        "HOENN:TRAINER_MADELINE_1",
        &[
            "HOENN:TRAINER_MADELINE_2",
            "HOENN:TRAINER_MADELINE_3",
            "HOENN:TRAINER_MADELINE_4",
            "HOENN:TRAINER_MADELINE_5",
        ],
    ),
    (
        "HOENN:TRAINER_JENNY_1",
        &[
            "HOENN:TRAINER_JENNY_2",
            "HOENN:TRAINER_JENNY_3",
            "HOENN:TRAINER_JENNY_4",
            "HOENN:TRAINER_JENNY_5",
        ],
    ),
    (
        "HOENN:TRAINER_DIANA_1",
        &[
            "HOENN:TRAINER_DIANA_2",
            "HOENN:TRAINER_DIANA_3",
            "HOENN:TRAINER_DIANA_4",
            "HOENN:TRAINER_DIANA_5",
        ],
    ),
    (
        "HOENN:TRAINER_AMY_AND_LIV_1",
        &[
            "HOENN:TRAINER_AMY_AND_LIV_2",
            "HOENN:TRAINER_AMY_AND_LIV_4",
            "HOENN:TRAINER_AMY_AND_LIV_5",
            "HOENN:TRAINER_AMY_AND_LIV_6",
        ],
    ),
    (
        "HOENN:TRAINER_ERNEST_1",
        &[
            "HOENN:TRAINER_ERNEST_2",
            "HOENN:TRAINER_ERNEST_3",
            "HOENN:TRAINER_ERNEST_4",
            "HOENN:TRAINER_ERNEST_5",
        ],
    ),
    (
        "HOENN:TRAINER_CORY_1",
        &[
            "HOENN:TRAINER_CORY_2",
            "HOENN:TRAINER_CORY_3",
            "HOENN:TRAINER_CORY_4",
            "HOENN:TRAINER_CORY_5",
        ],
    ),
    (
        "HOENN:TRAINER_EDWIN_1",
        &[
            "HOENN:TRAINER_EDWIN_2",
            "HOENN:TRAINER_EDWIN_3",
            "HOENN:TRAINER_EDWIN_4",
            "HOENN:TRAINER_EDWIN_5",
        ],
    ),
    (
        "HOENN:TRAINER_LYDIA_1",
        &[
            "HOENN:TRAINER_LYDIA_2",
            "HOENN:TRAINER_LYDIA_3",
            "HOENN:TRAINER_LYDIA_4",
            "HOENN:TRAINER_LYDIA_5",
        ],
    ),
    (
        "HOENN:TRAINER_ISAAC_1",
        &[
            "HOENN:TRAINER_ISAAC_2",
            "HOENN:TRAINER_ISAAC_3",
            "HOENN:TRAINER_ISAAC_4",
            "HOENN:TRAINER_ISAAC_5",
        ],
    ),
    (
        "HOENN:TRAINER_GABRIELLE_1",
        &[
            "HOENN:TRAINER_GABRIELLE_2",
            "HOENN:TRAINER_GABRIELLE_3",
            "HOENN:TRAINER_GABRIELLE_4",
            "HOENN:TRAINER_GABRIELLE_5",
        ],
    ),
    (
        "HOENN:TRAINER_CATHERINE_1",
        &[
            "HOENN:TRAINER_CATHERINE_2",
            "HOENN:TRAINER_CATHERINE_3",
            "HOENN:TRAINER_CATHERINE_4",
            "HOENN:TRAINER_CATHERINE_5",
        ],
    ),
    (
        "HOENN:TRAINER_JACKSON_1",
        &[
            "HOENN:TRAINER_JACKSON_2",
            "HOENN:TRAINER_JACKSON_3",
            "HOENN:TRAINER_JACKSON_4",
            "HOENN:TRAINER_JACKSON_5",
        ],
    ),
    (
        "HOENN:TRAINER_HALEY_1",
        &[
            "HOENN:TRAINER_HALEY_2",
            "HOENN:TRAINER_HALEY_3",
            "HOENN:TRAINER_HALEY_4",
            "HOENN:TRAINER_HALEY_5",
        ],
    ),
    (
        "HOENN:TRAINER_JAMES_1",
        &[
            "HOENN:TRAINER_JAMES_2",
            "HOENN:TRAINER_JAMES_3",
            "HOENN:TRAINER_JAMES_4",
            "HOENN:TRAINER_JAMES_5",
        ],
    ),
    (
        "HOENN:TRAINER_TRENT_1",
        &[
            "HOENN:TRAINER_TRENT_2",
            "HOENN:TRAINER_TRENT_3",
            "HOENN:TRAINER_TRENT_4",
            "HOENN:TRAINER_TRENT_5",
        ],
    ),
    (
        "HOENN:TRAINER_SAWYER_1",
        &[
            "HOENN:TRAINER_SAWYER_2",
            "HOENN:TRAINER_SAWYER_3",
            "HOENN:TRAINER_SAWYER_4",
            "HOENN:TRAINER_SAWYER_5",
        ],
    ),
    (
        "HOENN:TRAINER_KIRA_AND_DAN_1",
        &[
            "HOENN:TRAINER_KIRA_AND_DAN_2",
            "HOENN:TRAINER_KIRA_AND_DAN_3",
            "HOENN:TRAINER_KIRA_AND_DAN_4",
            "HOENN:TRAINER_KIRA_AND_DAN_5",
        ],
    ),
    (
        "HOENN:TRAINER_WALLY_VR_2",
        &[
            "HOENN:TRAINER_WALLY_VR_3",
            "HOENN:TRAINER_WALLY_VR_4",
            "HOENN:TRAINER_WALLY_VR_5",
        ],
    ),
    (
        "HOENN:TRAINER_ROXANNE_1",
        &[
            "HOENN:TRAINER_ROXANNE_2",
            "HOENN:TRAINER_ROXANNE_3",
            "HOENN:TRAINER_ROXANNE_4",
            "HOENN:TRAINER_ROXANNE_5",
        ],
    ),
    (
        "HOENN:TRAINER_BRAWLY_1",
        &[
            "HOENN:TRAINER_BRAWLY_2",
            "HOENN:TRAINER_BRAWLY_3",
            "HOENN:TRAINER_BRAWLY_4",
            "HOENN:TRAINER_BRAWLY_5",
        ],
    ),
    (
        "HOENN:TRAINER_WATTSON_1",
        &[
            "HOENN:TRAINER_WATTSON_2",
            "HOENN:TRAINER_WATTSON_3",
            "HOENN:TRAINER_WATTSON_4",
            "HOENN:TRAINER_WATTSON_5",
        ],
    ),
    (
        "HOENN:TRAINER_FLANNERY_1",
        &[
            "HOENN:TRAINER_FLANNERY_2",
            "HOENN:TRAINER_FLANNERY_3",
            "HOENN:TRAINER_FLANNERY_4",
            "HOENN:TRAINER_FLANNERY_5",
        ],
    ),
    (
        "HOENN:TRAINER_NORMAN_1",
        &[
            "HOENN:TRAINER_NORMAN_2",
            "HOENN:TRAINER_NORMAN_3",
            "HOENN:TRAINER_NORMAN_4",
            "HOENN:TRAINER_NORMAN_5",
        ],
    ),
    (
        "HOENN:TRAINER_WINONA_1",
        &[
            "HOENN:TRAINER_WINONA_2",
            "HOENN:TRAINER_WINONA_3",
            "HOENN:TRAINER_WINONA_4",
            "HOENN:TRAINER_WINONA_5",
        ],
    ),
    (
        "HOENN:TRAINER_TATE_AND_LIZA_1",
        &[
            "HOENN:TRAINER_TATE_AND_LIZA_2",
            "HOENN:TRAINER_TATE_AND_LIZA_3",
            "HOENN:TRAINER_TATE_AND_LIZA_4",
            "HOENN:TRAINER_TATE_AND_LIZA_5",
        ],
    ),
    (
        "HOENN:TRAINER_JUAN_1",
        &[
            "HOENN:TRAINER_JUAN_2",
            "HOENN:TRAINER_JUAN_3",
            "HOENN:TRAINER_JUAN_4",
            "HOENN:TRAINER_JUAN_5",
        ],
    ),
];

/// How a Hoenn story battle treats the partner (`enum CoopStoryKind`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StoryKind {
    /// A partner at the same story point gets the post-battle state.
    Grant,
    /// The post-battle script drives a scene the partner's map cannot
    /// rebuild: the partner only helps.
    Helper,
    /// The script only shows text: the trainer flag decides, as for a route
    /// trainer.
    Ordinary,
}

/// One entry of the ROM's `sCoopStoryBattles`, with its trainers from
/// `sCoopStoryTrainers` (canonical IDs). Flags and vars are SaveBlock1 IDs.
#[derive(Debug)]
pub(super) struct StoryBattle {
    /// The ROM's `COOP_STORY_*` name.
    pub name: &'static str,
    pub kind: StoryKind,
    /// Set by the post-battle script; the partner needs it clear.
    pub story_flag: Option<usize>,
    /// The var and the value the requester's script needed before the
    /// battle; the partner needs the same value.
    pub story_var: Option<(usize, u16)>,
    /// The trainer's object flag; the partner needs it clear (the trainer is
    /// on its map).
    pub presence_flag: Option<usize>,
    pub trainers: &'static [&'static str],
}

impl StoryBattle {
    /// Whether a save is at this battle's story point, as the ROM's
    /// `CoopTrainerRewards_IsStoryPartnerEligible` decides. `None` when a
    /// flag or var is outside the saved arrays.
    pub(super) fn at_story_point(
        &self,
        flag: impl Fn(usize) -> Option<bool>,
        var: impl Fn(usize) -> Option<u16>,
    ) -> Option<bool> {
        if self.kind != StoryKind::Grant {
            return Some(false);
        }
        if let Some(story_flag) = self.story_flag
            && flag(story_flag)?
        {
            return Some(false);
        }
        if let Some((story_var, value)) = self.story_var
            && var(story_var)? != value
        {
            return Some(false);
        }
        if let Some(presence) = self.presence_flag
            && flag(presence)?
        {
            return Some(false);
        }
        Some(true)
    }
}

const VAR_ELITE_4_STATE: usize = 0x409c;

/// The Hoenn story battles (A9), in the ROM's `enum CoopStoryBattle` order.
/// The inventory behind each kind is in
/// docs/plans/2026-09-29-coop-trainers-and-outcome-ledger-plan.md.
pub(super) const HOENN_STORY_BATTLES: &[StoryBattle] = &[
    StoryBattle {
        name: "COOP_STORY_RIVAL_ROUTE103",
        kind: StoryKind::Grant,
        story_flag: Some(0x82), // FLAG_DEFEATED_RIVAL_ROUTE103
        story_var: None,
        presence_flag: Some(0x2d3), // FLAG_HIDE_ROUTE_103_RIVAL
        trainers: &[
            "HOENN:TRAINER_MAY_ROUTE_103_TREECKO",
            "HOENN:TRAINER_MAY_ROUTE_103_TORCHIC",
            "HOENN:TRAINER_MAY_ROUTE_103_MUDKIP",
            "HOENN:TRAINER_BRENDAN_ROUTE_103_TREECKO",
            "HOENN:TRAINER_BRENDAN_ROUTE_103_TORCHIC",
            "HOENN:TRAINER_BRENDAN_ROUTE_103_MUDKIP",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_RIVAL_ROUTE110",
        kind: StoryKind::Grant,
        story_flag: None,
        story_var: Some((0x4069, 0)), // VAR_ROUTE110_STATE
        presence_flag: Some(0x397),   // FLAG_HIDE_ROUTE_110_RIVAL
        trainers: &[
            "HOENN:TRAINER_MAY_ROUTE_110_TREECKO",
            "HOENN:TRAINER_MAY_ROUTE_110_TORCHIC",
            "HOENN:TRAINER_MAY_ROUTE_110_MUDKIP",
            "HOENN:TRAINER_BRENDAN_ROUTE_110_TREECKO",
            "HOENN:TRAINER_BRENDAN_ROUTE_110_TORCHIC",
            "HOENN:TRAINER_BRENDAN_ROUTE_110_MUDKIP",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_RIVAL_ROUTE119",
        kind: StoryKind::Grant,
        story_flag: Some(0x6e),       // FLAG_RECEIVED_HM_FLY
        story_var: Some((0x4072, 0)), // VAR_ROUTE119_STATE
        presence_flag: None,
        trainers: &[
            "HOENN:TRAINER_MAY_ROUTE_119_TREECKO",
            "HOENN:TRAINER_MAY_ROUTE_119_TORCHIC",
            "HOENN:TRAINER_MAY_ROUTE_119_MUDKIP",
            "HOENN:TRAINER_BRENDAN_ROUTE_119_TREECKO",
            "HOENN:TRAINER_BRENDAN_ROUTE_119_TORCHIC",
            "HOENN:TRAINER_BRENDAN_ROUTE_119_MUDKIP",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_RIVAL_LILYCOVE",
        kind: StoryKind::Grant,
        story_flag: Some(0x124), // FLAG_MET_RIVAL_LILYCOVE
        story_var: None,
        presence_flag: Some(0x3cb), // FLAG_HIDE_LILYCOVE_CITY_RIVAL
        trainers: &[
            "HOENN:TRAINER_MAY_LILYCOVE_TREECKO",
            "HOENN:TRAINER_MAY_LILYCOVE_TORCHIC",
            "HOENN:TRAINER_MAY_LILYCOVE_MUDKIP",
            "HOENN:TRAINER_BRENDAN_LILYCOVE_TREECKO",
            "HOENN:TRAINER_BRENDAN_LILYCOVE_TORCHIC",
            "HOENN:TRAINER_BRENDAN_LILYCOVE_MUDKIP",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_WALLY_MAUVILLE",
        kind: StoryKind::Grant,
        story_flag: Some(0xbe), // FLAG_DEFEATED_WALLY_MAUVILLE
        story_var: None,
        presence_flag: Some(0x324), // FLAG_HIDE_MAUVILLE_CITY_WALLY
        trainers: &["HOENN:TRAINER_WALLY_MAUVILLE"],
    },
    StoryBattle {
        name: "COOP_STORY_WALLY_VICTORY_ROAD",
        kind: StoryKind::Grant,
        story_flag: Some(0x7e),       // FLAG_DEFEATED_WALLY_VICTORY_ROAD
        story_var: Some((0x40c3, 0)), // VAR_VICTORY_ROAD_1F_STATE
        presence_flag: None,
        trainers: &["HOENN:TRAINER_WALLY_1"],
    },
    StoryBattle {
        name: "COOP_STORY_GRUNT_JAGGED_PASS",
        kind: StoryKind::Grant,
        story_flag: Some(0x139), // FLAG_BEAT_MAGMA_GRUNT_JAGGED_PASS
        story_var: None,
        presence_flag: Some(0x34f), // FLAG_HIDE_JAGGED_PASS_MAGMA_GUARD
        trainers: &["HOENN:TRAINER_GRUNT_JAGGED_PASS"],
    },
    StoryBattle {
        name: "COOP_STORY_MATT_AQUA_HIDEOUT",
        kind: StoryKind::Grant,
        story_flag: Some(0x70), // FLAG_TEAM_AQUA_ESCAPED_IN_SUBMARINE
        story_var: None,
        presence_flag: Some(0x39c), // FLAG_HIDE_AQUA_HIDEOUT_GRUNTS
        trainers: &["HOENN:TRAINER_MATT"],
    },
    StoryBattle {
        name: "COOP_STORY_MAXIE_MT_CHIMNEY",
        kind: StoryKind::Grant,
        story_flag: Some(0x8b), // FLAG_DEFEATED_EVIL_TEAM_MT_CHIMNEY
        story_var: None,
        presence_flag: Some(0x39f), // FLAG_HIDE_MT_CHIMNEY_TEAM_MAGMA
        trainers: &["HOENN:TRAINER_MAXIE_MT_CHIMNEY"],
    },
    StoryBattle {
        name: "COOP_STORY_SIDNEY",
        kind: StoryKind::Grant,
        story_flag: Some(0x4fb), // FLAG_DEFEATED_ELITE_4_SIDNEY
        story_var: Some((VAR_ELITE_4_STATE, 1)),
        presence_flag: None,
        trainers: &["HOENN:TRAINER_SIDNEY"],
    },
    StoryBattle {
        name: "COOP_STORY_PHOEBE",
        kind: StoryKind::Grant,
        story_flag: Some(0x4fc), // FLAG_DEFEATED_ELITE_4_PHOEBE
        story_var: Some((VAR_ELITE_4_STATE, 2)),
        presence_flag: None,
        trainers: &["HOENN:TRAINER_PHOEBE"],
    },
    StoryBattle {
        name: "COOP_STORY_GLACIA",
        kind: StoryKind::Grant,
        story_flag: Some(0x4fd), // FLAG_DEFEATED_ELITE_4_GLACIA
        story_var: Some((VAR_ELITE_4_STATE, 3)),
        presence_flag: None,
        trainers: &["HOENN:TRAINER_GLACIA"],
    },
    StoryBattle {
        name: "COOP_STORY_DRAKE",
        kind: StoryKind::Grant,
        story_flag: Some(0x4fe), // FLAG_DEFEATED_ELITE_4_DRAKE
        story_var: Some((VAR_ELITE_4_STATE, 4)),
        presence_flag: None,
        trainers: &["HOENN:TRAINER_DRAKE"],
    },
    StoryBattle {
        name: "COOP_STORY_STEVEN_METEOR_FALLS",
        kind: StoryKind::Grant,
        story_flag: Some(0x4f8), // FLAG_DEFEATED_METEOR_FALLS_STEVEN
        story_var: None,
        presence_flag: None,
        trainers: &["HOENN:TRAINER_STEVEN"],
    },
    StoryBattle {
        name: "COOP_STORY_RIVAL_RUSTBORO",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &[
            "HOENN:TRAINER_MAY_RUSTBORO_TREECKO",
            "HOENN:TRAINER_MAY_RUSTBORO_TORCHIC",
            "HOENN:TRAINER_MAY_RUSTBORO_MUDKIP",
            "HOENN:TRAINER_BRENDAN_RUSTBORO_TREECKO",
            "HOENN:TRAINER_BRENDAN_RUSTBORO_TORCHIC",
            "HOENN:TRAINER_BRENDAN_RUSTBORO_MUDKIP",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_GRUNT_PETALBURG_WOODS",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &["HOENN:TRAINER_GRUNT_PETALBURG_WOODS"],
    },
    StoryBattle {
        name: "COOP_STORY_GRUNT_RUSTURF_TUNNEL",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &["HOENN:TRAINER_GRUNT_RUSTURF_TUNNEL"],
    },
    StoryBattle {
        name: "COOP_STORY_GRUNTS_OCEANIC_MUSEUM",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &[
            "HOENN:TRAINER_GRUNT_MUSEUM_1",
            "HOENN:TRAINER_GRUNT_MUSEUM_2",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_SHELLY_WEATHER_INSTITUTE",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &["HOENN:TRAINER_SHELLY_WEATHER_INSTITUTE"],
    },
    StoryBattle {
        name: "COOP_STORY_GRUNTS_SPACE_CENTER",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &[
            "HOENN:TRAINER_GRUNT_SPACE_CENTER_2",
            "HOENN:TRAINER_GRUNT_SPACE_CENTER_5",
            "HOENN:TRAINER_GRUNT_SPACE_CENTER_6",
            "HOENN:TRAINER_GRUNT_SPACE_CENTER_7",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_MAXIE_MAGMA_HIDEOUT",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &["HOENN:TRAINER_MAXIE_MAGMA_HIDEOUT"],
    },
    StoryBattle {
        name: "COOP_STORY_ARCHIE_SEAFLOOR_CAVERN",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &["HOENN:TRAINER_ARCHIE"],
    },
    StoryBattle {
        name: "COOP_STORY_CHAMPION_WALLACE",
        kind: StoryKind::Helper,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &["HOENN:TRAINER_WALLACE"],
    },
    StoryBattle {
        name: "COOP_STORY_AQUA_HIDEOUT_GRUNTS",
        kind: StoryKind::Ordinary,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &[
            "HOENN:TRAINER_GRUNT_AQUA_HIDEOUT_1",
            "HOENN:TRAINER_GRUNT_AQUA_HIDEOUT_2",
            "HOENN:TRAINER_GRUNT_AQUA_HIDEOUT_3",
            "HOENN:TRAINER_GRUNT_AQUA_HIDEOUT_4",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_ADMINS",
        kind: StoryKind::Ordinary,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &[
            "HOENN:TRAINER_SHELLY_SEAFLOOR_CAVERN",
            "HOENN:TRAINER_TABITHA_MT_CHIMNEY",
            "HOENN:TRAINER_TABITHA_MAGMA_HIDEOUT",
        ],
    },
    StoryBattle {
        name: "COOP_STORY_WALLY_VICTORY_ROAD_EXIT",
        kind: StoryKind::Ordinary,
        story_flag: None,
        story_var: None,
        presence_flag: None,
        trainers: &["HOENN:TRAINER_WALLY_VR_2"],
    },
];

/// How a catalogued trainer's battle is authorized and rewarded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum TrainerRule {
    /// A first battle: the trainer flag decides who may still earn it.
    Ordinary,
    /// A match-call rematch: whoever has beaten the first battle earns the
    /// rematch prize, however often the rematch itself was won.
    Rematch { base: TrainerInstanceId },
    /// A Hoenn gym leader's first battle (`badge` is the gym index).
    Gym { badge: u8 },
    /// A Hoenn story battle of kind `Grant` or `Helper` (an index into
    /// `HOENN_STORY_BATTLES`).
    Story { battle: usize },
}

/// What one member's last finalized save says about the requested trainer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MemberStanding {
    /// The requested trainer's own defeat bit.
    pub defeated: bool,
    /// For a rematch, the first-battle trainer's defeat bit.
    pub base_defeated: bool,
    /// Hoenn badge flags, bit 0 for the Stone Badge (read only for gyms).
    pub badges: u8,
    /// For a story battle, whether the save is at its story point
    /// (`StoryBattle::at_story_point`).
    pub story_point: bool,
}

/// The rule for a trainer that already resolved in the identity catalog.
pub(super) fn trainer_rule(trainer: &TrainerInstanceId) -> TrainerRule {
    let id = trainer.as_str();
    if let Some(badge) = HOENN_GYM_LEADERS.iter().position(|leader| *leader == id) {
        return TrainerRule::Gym {
            badge: u8::try_from(badge).expect("eight gyms"),
        };
    }
    if let Some(battle) = HOENN_STORY_BATTLES
        .iter()
        .position(|story| story.trainers.contains(&id))
    {
        if HOENN_STORY_BATTLES[battle].kind != StoryKind::Ordinary {
            return TrainerRule::Story { battle };
        }
        return TrainerRule::Ordinary;
    }
    HOENN_REMATCHES
        .iter()
        .find(|(_, rematches)| rematches.contains(&id))
        .and_then(|(base, _)| TrainerInstanceId::parse(base).ok())
        .map_or(TrainerRule::Ordinary, |base| TrainerRule::Rematch { base })
}

impl TrainerRule {
    /// Whether this member may request the battle.
    ///
    /// A first battle needs an unbeaten trainer; a rematch needs the first
    /// battle beaten (the match call that offers it exists only then); a gym
    /// leader needs the requester not to hold the badge yet.
    /// A story battle is always allowed: the requester's own story script
    /// reached it (the ROM decides), and its flags need not be monotonic
    /// (the Elite Four flags are cleared on every run), so a lagging save
    /// must not refuse it.
    pub(super) fn requester_allowed(&self, standing: MemberStanding) -> bool {
        match self {
            Self::Ordinary => !standing.defeated,
            Self::Rematch { .. } => standing.base_defeated,
            Self::Gym { badge } => standing.badges & (1 << badge) == 0,
            Self::Story { .. } => true,
        }
    }

    /// Whether this member earns the rewards (participant) or only helps.
    ///
    /// For a gym the member must be at the same story point: every earlier
    /// badge and neither this one nor a later one (the badge count equals
    /// the gym's index), as the ROM decides on its side.
    pub(super) fn participates(&self, standing: MemberStanding) -> bool {
        match self {
            Self::Ordinary => !standing.defeated,
            Self::Rematch { .. } => standing.base_defeated,
            Self::Gym { badge } => standing.badges == (1_u8 << badge) - 1,
            // `story_point` is only ever true for a Grant battle.
            Self::Story { .. } => standing.story_point,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coop_protocol::identity_catalog;

    fn rule(id: &str) -> TrainerRule {
        trainer_rule(&TrainerInstanceId::parse(id).unwrap())
    }

    fn standing(defeated: bool, base_defeated: bool, badges: u8) -> MemberStanding {
        MemberStanding {
            defeated,
            base_defeated,
            badges,
            story_point: false,
        }
    }

    /// `(first battle, later entries)` for every `REMATCH(...)` row of the
    /// ROM's `gRematchTable`, with repeated IDs dropped.
    fn rom_rematch_rows() -> Vec<(String, Vec<String>)> {
        let source = include_str!("../../../../../src/battle_setup.c");
        let table = source
            .split("gRematchTable[REMATCH_TABLE_ENTRIES] =")
            .nth(1)
            .expect("gRematchTable")
            .split("};")
            .next()
            .unwrap();
        table
            .lines()
            .filter_map(|line| line.split("= REMATCH(").nth(1))
            .map(|args| {
                let ids: Vec<String> = args
                    .split(',')
                    .take(5)
                    .map(|id| format!("HOENN:{}", id.trim()))
                    .collect();
                let mut later: Vec<String> = Vec::new();
                for id in &ids[1..] {
                    if *id != ids[0] && !later.contains(id) {
                        later.push(id.clone());
                    }
                }
                (ids[0].clone(), later)
            })
            .filter(|(_, later)| !later.is_empty())
            .collect()
    }

    #[test]
    fn rematch_table_matches_the_rom_rematch_table() {
        let rom = rom_rematch_rows();
        assert_eq!(rom.len(), HOENN_REMATCHES.len());
        for ((rom_base, rom_later), (base, later)) in rom.iter().zip(HOENN_REMATCHES) {
            assert_eq!(rom_base, base);
            assert_eq!(
                rom_later,
                &later.iter().map(ToString::to_string).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn gym_leaders_match_the_rom_gym_table_in_badge_order() {
        let source = include_str!("../../../../../src/coop/trainer_rewards.c");
        let table = source
            .split("sHoennGyms[COOP_HOENN_GYM_COUNT] =")
            .nth(1)
            .expect("sHoennGyms")
            .split("};")
            .next()
            .unwrap();
        let rom: Vec<String> = table
            .lines()
            .filter_map(|line| line.trim().strip_prefix("{TRAINER_"))
            .map(|rest| format!("HOENN:TRAINER_{}", rest.split(',').next().unwrap()))
            .collect();
        assert_eq!(rom, HOENN_GYM_LEADERS);
    }

    #[test]
    fn every_table_trainer_is_catalogued() {
        let ids = HOENN_GYM_LEADERS
            .iter()
            .chain(
                HOENN_REMATCHES
                    .iter()
                    .flat_map(|(base, later)| std::iter::once(base).chain(later.iter())),
            )
            .chain(
                HOENN_STORY_BATTLES
                    .iter()
                    .flat_map(|story| story.trainers.iter()),
            );
        for id in ids {
            let trainer = TrainerInstanceId::parse(id).unwrap();
            let entry = identity_catalog::trainer(&trainer).expect(id);
            assert!(entry.ordinal.is_some(), "{id}");
        }
    }

    #[test]
    fn rules_resolve_rematches_gyms_and_first_battles() {
        assert_eq!(rule("HOENN:TRAINER_CALVIN_1"), TrainerRule::Ordinary);
        assert_eq!(
            rule("HOENN:TRAINER_CALVIN_3"),
            TrainerRule::Rematch {
                base: TrainerInstanceId::parse("HOENN:TRAINER_CALVIN_1").unwrap()
            }
        );
        // Cindy's row skips CINDY_2, which is an ordinary trainer.
        assert_eq!(rule("HOENN:TRAINER_CINDY_2"), TrainerRule::Ordinary);
        assert_eq!(
            rule("HOENN:TRAINER_CINDY_6"),
            TrainerRule::Rematch {
                base: TrainerInstanceId::parse("HOENN:TRAINER_CINDY_1").unwrap()
            }
        );
        assert_eq!(
            rule("HOENN:TRAINER_ROXANNE_1"),
            TrainerRule::Gym { badge: 0 }
        );
        assert_eq!(rule("HOENN:TRAINER_JUAN_1"), TrainerRule::Gym { badge: 7 });
        // A leader rematch is a rematch: money, no badge.
        assert_eq!(
            rule("HOENN:TRAINER_JUAN_2"),
            TrainerRule::Rematch {
                base: TrainerInstanceId::parse("HOENN:TRAINER_JUAN_1").unwrap()
            }
        );
        assert_eq!(
            rule("HOENN:TRAINER_SIDNEY"),
            TrainerRule::Story { battle: 9 }
        );
    }

    #[test]
    fn rematch_needs_the_first_battle_and_ignores_the_rematch_flag() {
        let rematch = rule("HOENN:TRAINER_CALVIN_2");
        // Already beaten at this stage: still a rematch to win again.
        assert!(rematch.requester_allowed(standing(true, true, 0)));
        assert!(rematch.requester_allowed(standing(false, true, 0)));
        assert!(!rematch.requester_allowed(standing(false, false, 0)));
        assert!(rematch.participates(standing(true, true, 0)));
        assert!(!rematch.participates(standing(false, false, 0)));
    }

    #[test]
    fn gym_roles_follow_the_badge_order() {
        let norman = rule("HOENN:TRAINER_NORMAN_1");
        assert!(norman.requester_allowed(standing(false, false, 0b0000_1111)));
        assert!(!norman.requester_allowed(standing(false, false, 0b0001_1111)));
        assert!(norman.participates(standing(false, false, 0b0000_1111)));
        // Behind (missing Flannery) or ahead (already has the badge): helper.
        assert!(!norman.participates(standing(false, false, 0b0000_0111)));
        assert!(!norman.participates(standing(false, false, 0b0001_1111)));
        // Out of order: a later badge but not this one.
        assert!(!norman.participates(standing(false, false, 0b0010_1111)));
        let roxanne = rule("HOENN:TRAINER_ROXANNE_1");
        assert!(roxanne.participates(standing(false, false, 0)));
        assert!(!roxanne.participates(standing(false, false, 1)));
        // The trainer bit is not what decides a gym.
        assert!(roxanne.participates(standing(true, false, 0)));
    }

    #[test]
    fn ordinary_trainers_keep_the_defeated_rule() {
        let calvin = rule("HOENN:TRAINER_CALVIN_1");
        assert!(calvin.requester_allowed(standing(false, false, 0)));
        assert!(!calvin.requester_allowed(standing(true, false, 0)));
        assert!(calvin.participates(standing(false, true, 0xff)));
        assert!(!calvin.participates(standing(true, false, 0)));
    }

    /// `#define NAME 0xHEX` lines of a ROM constants header.
    fn hex_defines(source: &str) -> std::collections::HashMap<String, usize> {
        source
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                if words.next()? != "#define" {
                    return None;
                }
                let name = words.next()?;
                let value = words.next()?.strip_prefix("0x")?;
                Some((name.to_owned(), usize::from_str_radix(value, 16).ok()?))
            })
            .collect()
    }

    struct RomStory {
        name: String,
        kind: StoryKind,
        story_flag: Option<usize>,
        story_var: Option<(usize, u16)>,
        presence_flag: Option<usize>,
        trainers: Vec<String>,
    }

    /// `sCoopStoryBattles` and `sCoopStoryTrainers` of the ROM, with flag
    /// and var names resolved from its headers and trainers mapped to their
    /// canonical IDs.
    fn rom_story_battles() -> Vec<RomStory> {
        let source = include_str!("../../../../../src/coop/trainer_rewards.c");
        let mut names = hex_defines(include_str!("../../../../../include/constants/flags.h"));
        names.extend(hex_defines(include_str!(
            "../../../../../include/constants/vars.h"
        )));
        let resolve = |name: &str| -> Option<usize> {
            (name != "0").then(|| *names.get(name).unwrap_or_else(|| panic!("{name}")))
        };
        let battles_table = source
            .split("sCoopStoryBattles[COOP_STORY_COUNT] =")
            .nth(1)
            .expect("sCoopStoryBattles")
            .split("};")
            .next()
            .unwrap();
        let mut battles: Vec<RomStory> = battles_table
            .lines()
            .filter_map(|line| line.trim().strip_prefix('['))
            .map(|line| {
                let (name, rest) = line.split_once(']').unwrap();
                let fields: Vec<&str> = rest
                    .split_once('{')
                    .unwrap()
                    .1
                    .split(',')
                    .map(str::trim)
                    .collect();
                let kind = match fields[0] {
                    "GRANT" => StoryKind::Grant,
                    "HELPER" => StoryKind::Helper,
                    "ORDINARY" => StoryKind::Ordinary,
                    other => panic!("kind {other}"),
                };
                RomStory {
                    name: name.to_owned(),
                    kind,
                    story_flag: resolve(fields[1]),
                    story_var: resolve(fields[2])
                        .map(|var| (var, fields[3].parse().expect("story value"))),
                    presence_flag: resolve(fields[4]),
                    trainers: Vec::new(),
                }
            })
            .collect();
        let trainers_table = source
            .split("sCoopStoryTrainers[] =")
            .nth(1)
            .expect("sCoopStoryTrainers")
            .split("};")
            .next()
            .unwrap();
        for line in trainers_table.lines() {
            let Some(entry) = line.trim().strip_prefix('{') else {
                continue;
            };
            // The table's own opening brace has no entry.
            let Some((legacy, battle)) = entry.split_once(',') else {
                continue;
            };
            let battle = battle.trim().trim_end_matches("},");
            let canonical = identity_catalog::all_identities()
                .iter()
                .find(|identity| {
                    identity.kind == coop_protocol::IdentityKind::Trainer
                        && identity.region == coop_protocol::RegionId::Hoenn
                        && identity.legacy_symbol == Some(legacy.trim())
                })
                .unwrap_or_else(|| panic!("{legacy} is not catalogued"))
                .qualified_id;
            battles
                .iter_mut()
                .find(|story| story.name == battle)
                .unwrap_or_else(|| panic!("{battle}"))
                .trainers
                .push(canonical.to_owned());
        }
        battles
    }

    #[test]
    fn story_table_matches_the_rom_story_table() {
        let rom = rom_story_battles();
        assert_eq!(rom.len(), HOENN_STORY_BATTLES.len());
        for (rom, story) in rom.iter().zip(HOENN_STORY_BATTLES) {
            assert_eq!(rom.name, story.name);
            assert_eq!(rom.kind, story.kind, "{}", story.name);
            assert_eq!(rom.story_flag, story.story_flag, "{}", story.name);
            assert_eq!(rom.story_var, story.story_var, "{}", story.name);
            assert_eq!(rom.presence_flag, story.presence_flag, "{}", story.name);
            assert_eq!(rom.trainers, story.trainers, "{}", story.name);
        }
        // A trainer belongs to one battle, and never to a gym or a rematch.
        let mut seen = std::collections::HashSet::new();
        for id in HOENN_STORY_BATTLES.iter().flat_map(|story| story.trainers) {
            assert!(seen.insert(*id), "{id}");
            assert!(!HOENN_GYM_LEADERS.contains(id), "{id}");
            assert!(
                !HOENN_REMATCHES.iter().any(|(_, later)| later.contains(id)),
                "{id}"
            );
        }
    }

    #[test]
    fn story_rules_resolve_by_kind() {
        let wally = rule("HOENN:TRAINER_WALLY_1");
        assert_eq!(wally, TrainerRule::Story { battle: 5 });
        assert_eq!(HOENN_STORY_BATTLES[5].name, "COOP_STORY_WALLY_VICTORY_ROAD");
        assert!(matches!(
            rule("HOENN:TRAINER_WALLACE"),
            TrainerRule::Story { .. }
        ));
        // Ordinary story entries keep the trainer-flag rule; Wally's exit
        // battle keeps its match-call rematches.
        assert_eq!(
            rule("HOENN:TRAINER_TABITHA_MT_CHIMNEY"),
            TrainerRule::Ordinary
        );
        assert_eq!(rule("HOENN:TRAINER_WALLY_VR_2"), TrainerRule::Ordinary);
        assert_eq!(
            rule("HOENN:TRAINER_WALLY_VR_4"),
            TrainerRule::Rematch {
                base: TrainerInstanceId::parse("HOENN:TRAINER_WALLY_VR_2").unwrap()
            }
        );
    }

    #[test]
    fn story_requester_is_always_allowed_and_partners_need_the_story_point() {
        let sidney = rule("HOENN:TRAINER_SIDNEY");
        let at_point = MemberStanding {
            story_point: true,
            ..MemberStanding::default()
        };
        // A lagging save (flag set from an earlier run) never refuses.
        assert!(sidney.requester_allowed(standing(true, false, 0xff)));
        assert!(sidney.participates(at_point));
        assert!(!sidney.participates(standing(false, false, 0xff)));
    }

    #[test]
    fn story_point_needs_flag_clear_var_value_and_the_trainer_present() {
        let route110 = &HOENN_STORY_BATTLES[1];
        let sidney = &HOENN_STORY_BATTLES[9];
        let champion = HOENN_STORY_BATTLES
            .iter()
            .find(|story| story.name == "COOP_STORY_CHAMPION_WALLACE")
            .unwrap();
        let flags = |set: &'static [usize]| move |flag: usize| Some(set.contains(&flag));
        let var = |value: u16| move |_: usize| Some(value);
        assert_eq!(route110.at_story_point(flags(&[]), var(0)), Some(true));
        assert_eq!(route110.at_story_point(flags(&[]), var(1)), Some(false));
        // The rival is still hidden: an earlier story point.
        assert_eq!(
            route110.at_story_point(flags(&[0x397]), var(0)),
            Some(false)
        );
        assert_eq!(sidney.at_story_point(flags(&[]), var(1)), Some(true));
        assert_eq!(sidney.at_story_point(flags(&[0x4fb]), var(1)), Some(false));
        assert_eq!(sidney.at_story_point(flags(&[]), var(2)), Some(false));
        // A flag the save cannot hold makes the standing unknown.
        assert_eq!(sidney.at_story_point(|_| None, var(1)), None);
        // Helper battles never grant.
        assert_eq!(champion.at_story_point(flags(&[]), var(0)), Some(false));
    }
}
