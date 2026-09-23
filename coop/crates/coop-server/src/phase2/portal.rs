//! Same-origin account and download page for the private pilot.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use super::{Phase2App, Phase2Error, auth};

pub(super) async fn page() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        include_str!("portal.html"),
    ).into_response()
}

#[derive(Serialize)]
pub(super) struct InvitationResponse {
    invitation_code: String,
}

pub(super) async fn create_invitation(
    State(app): State<Phase2App>,
    headers: HeaderMap,
) -> Result<
    (
        StatusCode,
        [(axum::http::HeaderName, &'static str); 1],
        Json<InvitationResponse>,
    ),
    Phase2Error,
> {
    let actor = auth::actor_from_headers(&app.store, &headers)?;
    let code = auth::create_invitation(&app.store, actor)?;
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(InvitationResponse {
            invitation_code: code,
        }),
    ))
}
