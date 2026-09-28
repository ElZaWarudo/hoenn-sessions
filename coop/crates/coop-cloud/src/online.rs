//! Bounded authenticated Online menu contracts. Identifiers stay in the host.

use crate::{
    ApiVersion, GroupId, GroupInvitationId, GroupInvitationView, GroupView, IdempotencyKey,
    LeaseFence,
};
use coop_protocol::{CanonicalUsername, PresenceHandle};
use serde::{Deserialize, Deserializer, Serialize};

pub const ONLINE_PAGE_SIZE: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineSnapshotRequest {
    pub api_version: ApiVersion,
    pub fence: LeaseFence,
    pub incoming_after: Option<GroupInvitationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnlinePeer {
    pub handle: PresenceHandle,
    pub generation: std::num::NonZeroU64,
    pub username: CanonicalUsername,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineInvitation {
    pub invitation: GroupInvitationView,
    pub username: CanonicalUsername,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineGroup {
    pub group: GroupView,
    pub username: CanonicalUsername,
}

/// The most recent partner that can be invited back into a group.
///
/// The character identifier is retained for the authenticated host client so
/// it can correlate the action and is never intended for display in the ROM.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineRememberedPartner {
    pub character_id: crate::CharacterId,
    pub username: CanonicalUsername,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineSnapshotResponse {
    pub api_version: ApiVersion,
    #[serde(deserialize_with = "page")]
    pub nearby: Vec<OnlinePeer>,
    #[serde(deserialize_with = "page")]
    pub incoming: Vec<OnlineInvitation>,
    #[serde(default, deserialize_with = "page")]
    pub outgoing: Vec<OnlineInvitation>,
    pub incoming_next: Option<GroupInvitationId>,
    pub group: Option<OnlineGroup>,
    /// A live outgoing invitation or pairing code can let a peer join remotely.
    /// Older V1 servers omit this field; guard travel until a capable server
    /// reports the authoritative state.
    #[serde(default = "remote_join_unknown")]
    pub remote_join_possible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_partner: Option<OnlineRememberedPartner>,
}

fn remote_join_unknown() -> bool {
    true
}

fn page<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Page<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Page<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most four Online entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::with_capacity(ONLINE_PAGE_SIZE);
            while let Some(value) = seq.next_element()? {
                if values.len() == ONLINE_PAGE_SIZE {
                    return Err(serde::de::Error::custom("Online page exceeds four entries"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Page(std::marker::PhantomData))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum OnlineAction {
    Invite {
        handle: PresenceHandle,
        generation: std::num::NonZeroU64,
    },
    Accept {
        invitation_id: GroupInvitationId,
    },
    Decline {
        invitation_id: GroupInvitationId,
    },
    Cancel {
        invitation_id: GroupInvitationId,
    },
    InviteLastPartner,
    Leave {
        group_id: GroupId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnlineActionRequest {
    pub api_version: ApiVersion,
    pub fence: LeaseFence,
    pub idempotency_key: IdempotencyKey,
    pub action: OnlineAction,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum OnlineActionResponse {
    Invited { invitation: GroupInvitationView },
    Accepted { group: GroupView },
    Declined,
    Cancelled,
    Left,
}
