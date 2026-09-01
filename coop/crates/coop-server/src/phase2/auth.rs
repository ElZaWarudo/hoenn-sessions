//! Invite-gated authentication and opaque token rotation.

use axum::http::HeaderMap;
use coop_cloud::{
    AccessToken, CharacterId, LoginRequest, LoginResponse, LogoutRequest, LogoutResponse,
    RefreshRequest, RefreshResponse, RegisterRequest, RegisterResponse,
};
use coop_cloud::{ClientInstanceId, LeaseFence, Revision, SessionEpoch, SessionId};
use zeroize::Zeroizing;

use super::storage::{
    ACCESS_TTL_MS, AccessRecord, FamilyRecord, MAX_ACCESS_RECORDS_GLOBAL,
    MAX_ACCESS_RECORDS_PER_CHARACTER, MAX_FAMILY_RECORDS_GLOBAL, MAX_FAMILY_RECORDS_PER_CHARACTER,
    MAX_REFRESH_RECORDS_GLOBAL, MAX_REFRESH_RECORDS_PER_CHARACTER, REFRESH_TTL_MS, RefreshRecord,
    Store, UserRecord,
};
use super::{AuthenticatedActor, Phase2Error};

/// Inserts a single-use invitation fingerprint.  The code itself is never
/// retained. This is intentionally an explicit bootstrap operation.
pub(crate) fn add_invitation(store: &Store, code: &str) -> Result<(), Phase2Error> {
    let fingerprint = store.invitation_fingerprint(code);
    store.write_transaction(|state| {
        if state.invitations.contains_key(&fingerprint) {
            return Err(Phase2Error::Conflict);
        }
        state.invitations.insert(fingerprint, false);
        Ok(())
    })
}

/// Performs the cheap, non-secret admission checks before any password work.
/// The registration transition repeats these checks while consuming the
/// invitation, so this preflight is only an abuse-control optimization.
pub(crate) fn registration_admissible(
    store: &Store,
    request: &RegisterRequest,
) -> Result<bool, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::Authentication)?;
    let username = request.username.as_str();
    let invitation = store.invitation_fingerprint(request.invitation_code.expose_secret());
    store.read_transaction(|state| {
        Ok::<bool, Phase2Error>(
            state
                .invitations
                .get(&invitation)
                .is_some_and(|consumed| !*consumed)
                && !state.users_by_name.contains_key(username),
        )
    })
}

pub(crate) fn register(
    store: &Store,
    request: &RegisterRequest,
) -> Result<RegisterResponse, Phase2Error> {
    request
        .validate()
        .map_err(|_| Phase2Error::Authentication)?;
    let username = request.username.as_str().to_owned();
    if !registration_admissible(store, request)? {
        return Err(Phase2Error::Authentication);
    }
    let password = Zeroizing::new(request.password.expose_secret().to_owned());
    let invitation = store.invitation_fingerprint(request.invitation_code.expose_secret());
    let password_phc = store.config.password_engine.hash(&password)?;
    let user_id = store.user_id()?;
    let character_id = store.character_id()?;
    let character_state = Store::initial_state(character_id)?;
    let user = UserRecord {
        user_id,
        password_phc,
        character_id,
        disabled: false,
    };
    store.write_transaction(|state| {
        let already_consumed = *state
            .invitations
            .get(&invitation)
            .ok_or(Phase2Error::Authentication)?;
        if already_consumed || state.users_by_name.contains_key(&username) {
            return Err(Phase2Error::Authentication);
        }
        if state.users_by_id.contains_key(&user_id) || state.characters.contains_key(&character_id)
        {
            return Err(Phase2Error::Conflict);
        }
        if let Some(consumed) = state.invitations.get_mut(&invitation) {
            *consumed = true;
        }
        state.characters.insert(
            character_id,
            super::storage::CharacterRecord {
                owner: user_id,
                state: character_state,
                revision: Revision::initial(),
                active_snapshot: None,
                last_session_epoch: 0,
            },
        );
        state.users_by_name.insert(username, user.clone());
        state.users_by_id.insert(user_id, user);
        Ok(RegisterResponse::new(user_id, character_id))
    })
}

fn prune_token_records(state: &mut super::storage::State, now: u64) {
    state
        .families
        .retain(|_, family| family.expires_at > now && !family.revoked);
    state.access.retain(|_, access| {
        access.expires_at > now && !access.revoked && state.families.contains_key(&access.family_id)
    });
    state.refresh.retain(|_, refresh| {
        refresh.expires_at > now && state.families.contains_key(&refresh.family_id)
    });
}

fn character_token_counts(
    state: &super::storage::State,
    character_id: coop_cloud::CharacterId,
) -> (usize, usize, usize) {
    (
        state
            .access
            .values()
            .filter(|record| record.character_id == character_id)
            .count(),
        state
            .refresh
            .values()
            .filter(|record| record.character_id == character_id)
            .count(),
        state
            .families
            .values()
            .filter(|record| record.character_id == character_id)
            .count(),
    )
}

fn issue_tokens(store: &Store, user: &UserRecord, now: u64) -> Result<LoginResponse, Phase2Error> {
    let mut access_plain = Zeroizing::new(store.random_token()?);
    let mut refresh_plain = Zeroizing::new(store.random_token()?);
    let access_fingerprint = Store::token_fingerprint(access_plain.as_str());
    let refresh_fingerprint = Store::token_fingerprint(refresh_plain.as_str());
    let access =
        AccessToken::new(std::mem::take(&mut *access_plain)).map_err(|_| Phase2Error::Internal)?;
    let refresh = coop_cloud::RefreshToken::new(std::mem::take(&mut *refresh_plain))
        .map_err(|_| Phase2Error::Internal)?;
    let family_id = store.family_id()?;
    let access_expiry = now
        .checked_add(ACCESS_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    let refresh_expiry = now
        .checked_add(REFRESH_TTL_MS)
        .ok_or(Phase2Error::Internal)?;
    let access_expires_at = super::storage::Store::unix_timestamp(access_expiry)?;
    let refresh_expires_at = super::storage::Store::unix_timestamp(refresh_expiry)?;
    let response = LoginResponse::new(
        user.user_id,
        user.character_id,
        access,
        refresh,
        family_id,
        access_expires_at,
        refresh_expires_at,
    )
    .map_err(|_| Phase2Error::Internal)?;
    store.write_transaction(|state| {
        prune_token_records(state, now);
        let (access_count, refresh_count, family_count) =
            character_token_counts(state, user.character_id);
        if access_count >= MAX_ACCESS_RECORDS_PER_CHARACTER
            || refresh_count >= MAX_REFRESH_RECORDS_PER_CHARACTER
            || family_count >= MAX_FAMILY_RECORDS_PER_CHARACTER
            || state.access.len() >= MAX_ACCESS_RECORDS_GLOBAL
            || state.refresh.len() >= MAX_REFRESH_RECORDS_GLOBAL
            || state.families.len() >= MAX_FAMILY_RECORDS_GLOBAL
        {
            return Err(Phase2Error::Busy);
        }
        if state.families.contains_key(&family_id)
            || state.access.contains_key(&access_fingerprint)
            || state.refresh.contains_key(&refresh_fingerprint)
        {
            return Err(Phase2Error::Conflict);
        }
        state.families.insert(
            family_id,
            FamilyRecord {
                user_id: user.user_id,
                character_id: user.character_id,
                expires_at: refresh_expiry,
                revoked: false,
            },
        );
        state.access.insert(
            access_fingerprint,
            AccessRecord {
                user_id: user.user_id,
                character_id: user.character_id,
                family_id,
                expires_at: access_expiry,
                revoked: false,
            },
        );
        state.refresh.insert(
            refresh_fingerprint,
            RefreshRecord {
                user_id: user.user_id,
                character_id: user.character_id,
                family_id,
                generation: 0,
                expires_at: refresh_expiry,
                consumed: false,
            },
        );
        Ok(response)
    })
}

pub(crate) fn login(store: &Store, request: &LoginRequest) -> Result<LoginResponse, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    let username = request.username.as_str().to_owned();
    let password = Zeroizing::new(request.password.expose_secret().to_owned());
    let user = store.read_transaction(|state| {
        Ok::<Option<UserRecord>, Phase2Error>(state.users_by_name.get(&username).cloned())
    })?;
    let (phc, disabled) = user.as_ref().map_or_else(
        || (store.config.password_engine.dummy_phc(), true),
        |u| (u.password_phc.as_str(), u.disabled),
    );
    let valid = store.config.password_engine.verify(&password, phc);
    if !valid || disabled {
        return Err(Phase2Error::Authentication);
    }
    issue_tokens(
        store,
        user.as_ref().ok_or(Phase2Error::Authentication)?,
        store.now(),
    )
}

pub(crate) fn refresh(
    store: &Store,
    request: &RefreshRequest,
) -> Result<RefreshResponse, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    let fingerprint = Store::token_fingerprint(request.refresh_token.expose_secret());
    let now = store.now();
    store.write_transaction(|state| {
        prune_token_records(state, now);
        let record = state
            .refresh
            .get(&fingerprint)
            .cloned()
            .ok_or(Phase2Error::Authentication)?;
        let family_id = record.family_id;
        let family = state
            .families
            .get(&family_id)
            .cloned()
            .ok_or(Phase2Error::Authentication)?;
        if family.user_id != record.user_id || family.character_id != record.character_id {
            return Err(Phase2Error::Authentication);
        }
        if record.consumed {
            for access in state.access.values_mut() {
                if access.family_id == family_id {
                    access.revoked = true;
                }
            }
            if let Some(family) = state.families.get_mut(&family_id) {
                family.revoked = true;
            }
            return Err(Phase2Error::Authentication);
        }
        if family.revoked || record.expires_at <= now || family.expires_at <= now {
            return Err(Phase2Error::Authentication);
        }
        let (access_count, refresh_count, _) = character_token_counts(state, record.character_id);
        if access_count >= MAX_ACCESS_RECORDS_PER_CHARACTER
            || refresh_count >= MAX_REFRESH_RECORDS_PER_CHARACTER
            || state.access.len() >= MAX_ACCESS_RECORDS_GLOBAL
            || state.refresh.len() >= MAX_REFRESH_RECORDS_GLOBAL
        {
            return Err(Phase2Error::Busy);
        }
        let mut access_plain = Zeroizing::new(store.random_token()?);
        let mut refresh_plain = Zeroizing::new(store.random_token()?);
        let access_fingerprint = Store::token_fingerprint(access_plain.as_str());
        let refresh_fingerprint = Store::token_fingerprint(refresh_plain.as_str());
        let access_expiry = now
            .checked_add(ACCESS_TTL_MS)
            .ok_or(Phase2Error::Internal)?;
        let refresh_expiry = family.expires_at;
        let next_generation = record
            .generation
            .checked_add(1)
            .ok_or(Phase2Error::Internal)?;
        let access_expires_at = super::storage::Store::unix_timestamp(access_expiry)?;
        let refresh_expires_at = super::storage::Store::unix_timestamp(refresh_expiry)?;
        let access = AccessToken::new(std::mem::take(&mut *access_plain))
            .map_err(|_| Phase2Error::Internal)?;
        let refresh = coop_cloud::RefreshToken::new(std::mem::take(&mut *refresh_plain))
            .map_err(|_| Phase2Error::Internal)?;
        let response = RefreshResponse::new(
            access,
            refresh,
            family_id,
            access_expires_at,
            refresh_expires_at,
        )
        .map_err(|_| Phase2Error::Internal)?;
        if state.access.contains_key(&access_fingerprint)
            || state.refresh.contains_key(&refresh_fingerprint)
        {
            return Err(Phase2Error::Conflict);
        }
        if let Some(record) = state.refresh.get_mut(&fingerprint) {
            record.consumed = true;
        }
        state.access.insert(
            access_fingerprint,
            AccessRecord {
                user_id: record.user_id,
                character_id: record.character_id,
                family_id,
                expires_at: access_expiry,
                revoked: false,
            },
        );
        state.refresh.insert(
            refresh_fingerprint,
            RefreshRecord {
                user_id: record.user_id,
                character_id: record.character_id,
                family_id,
                generation: next_generation,
                expires_at: refresh_expiry,
                consumed: false,
            },
        );
        Ok(response)
    })
}

pub(crate) fn logout(
    store: &Store,
    request: &LogoutRequest,
) -> Result<LogoutResponse, Phase2Error> {
    if request.api_version.value() != 1 {
        return Err(Phase2Error::InvalidRequest);
    }
    let fingerprint = Store::token_fingerprint(request.refresh_token.expose_secret());
    store.write_transaction(|state| {
        if let Some(record) = state.refresh.get(&fingerprint).cloned() {
            if let Some(family) = state.families.get_mut(&record.family_id) {
                family.revoked = true;
            }
            for access in state.access.values_mut() {
                if access.family_id == record.family_id {
                    access.revoked = true;
                }
            }
            for token in state.refresh.values_mut() {
                if token.family_id == record.family_id {
                    token.consumed = true;
                }
            }
        }
        Ok(LogoutResponse::default())
    })
}

pub(crate) fn actor_from_headers(
    store: &Store,
    headers: &HeaderMap,
) -> Result<AuthenticatedActor, Phase2Error> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)
        .ok_or(Phase2Error::Authentication)?;
    if value.as_bytes().len() > 1024 {
        return Err(Phase2Error::Authentication);
    }
    let value = value.to_str().map_err(|_| Phase2Error::Authentication)?;
    let token = value
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty() && !token.contains(' '))
        .ok_or(Phase2Error::Authentication)?;
    let mut token = Zeroizing::new(token.to_owned());
    let token =
        AccessToken::new(std::mem::take(&mut *token)).map_err(|_| Phase2Error::Authentication)?;
    let fingerprint = Store::token_fingerprint(token.expose_secret());
    let now = store.now();
    store.read_transaction(|state| {
        let access = state
            .access
            .get(&fingerprint)
            .ok_or(Phase2Error::Authentication)?;
        if access.revoked || access.expires_at <= now {
            return Err(Phase2Error::Authentication);
        }
        Ok(AuthenticatedActor {
            user_id: access.user_id,
            character_id: access.character_id,
        })
    })
}

fn header<T: std::str::FromStr>(headers: &HeaderMap, name: &'static str) -> Result<T, Phase2Error> {
    let value = headers.get(name).ok_or(Phase2Error::Authentication)?;
    if value.as_bytes().len() > 128 {
        return Err(Phase2Error::Authentication);
    }
    value
        .to_str()
        .map_err(|_| Phase2Error::Authentication)?
        .parse()
        .map_err(|_| Phase2Error::Authentication)
}

pub(crate) fn fence_from_headers(
    headers: &HeaderMap,
    character_id: CharacterId,
    app: &super::Phase2App,
) -> Result<LeaseFence, Phase2Error> {
    let session_id: SessionId = header(headers, "x-coop-session-id")?;
    let epoch: u32 = header(headers, "x-coop-session-epoch")?;
    let session_epoch = SessionEpoch::new(epoch).map_err(|_| Phase2Error::Authentication)?;
    let client_instance_id: ClientInstanceId = header(headers, "x-coop-client-instance-id")?;
    let revision = app.store.read_transaction(|state| {
        Ok::<Revision, Phase2Error>(
            state
                .leases
                .get(&character_id)
                .map_or(Revision::initial(), |lease| lease.contract.current_revision),
        )
    })?;
    Ok(LeaseFence::new(
        session_id,
        character_id,
        revision,
        session_epoch,
        client_instance_id,
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn dummy_hash_is_valid() {
        let engine = super::super::storage::ArgonPasswordEngine::production();
        let dummy = super::super::storage::PasswordEngine::dummy_phc(&engine).to_owned();
        assert!(super::super::storage::PasswordEngine::verify(
            &engine,
            "dummy password",
            &dummy,
        ));
    }
}
