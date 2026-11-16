//! `POST /api/v1/auth/ws-ticket {scope, id}` (`docs/API.md` section 2): a ticket to open a document
//! or workspace socket. The caller must be a member of the workspace the socket belongs to; anyone
//! else gets the same 404 as for an id that does not exist.

use axum::Json;
use axum::extract::State;
use mongodb::bson::DateTime;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::AuthUser;
use super::session::now;
use super::ticket::{self, Scope, TICKET_SECONDS};
use crate::app::AppState;
use crate::db::documents;
use crate::error::ApiError;
use crate::extract::ApiJson;
use crate::rbac::{Role, parse_id, require_role};
use crate::workspaces::handlers::iso;

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "WsTicketRequest")]
pub struct TicketInput {
    pub scope: Scope,
    /// The document id for scope `doc`, the workspace id for scope `workspace`.
    pub id: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(rename = "WsTicketResponse")]
pub struct TicketResponse {
    pub ticket: String,
    /// When the ticket stops working (30 seconds from now).
    pub expires_at: String,
}

fn not_found() -> ApiError {
    ApiError::not_found("not found")
}

pub async fn ws_ticket(
    State(state): State<AppState>,
    user: AuthUser,
    ApiJson(input): ApiJson<TicketInput>,
) -> Result<Json<TicketResponse>, ApiError> {
    let id = parse_id(&input.id)?;
    let workspace_id = match input.scope {
        Scope::Doc => {
            documents::find(&state.db, id)
                .await?
                .ok_or_else(not_found)?
                .workspace_id
        }
        Scope::Workspace => id,
    };
    // Any member may read, so any member may listen; what they may do is decided per frame.
    require_role(&state.db, workspace_id, user.id, Role::Viewer)
        .await
        .map_err(|error| match error {
            ApiError::NotFound(_) => not_found(),
            other => other,
        })?;

    let (token, expires_at) = ticket::issue(
        state.config.ticket_secret.expose(),
        user.id,
        input.scope,
        id,
        now(),
        TICKET_SECONDS,
    )
    .map_err(ApiError::internal)?;
    let millis = i64::try_from(expires_at.saturating_mul(1000)).unwrap_or(i64::MAX);
    Ok(Json(TicketResponse {
        ticket: token,
        expires_at: iso(DateTime::from_millis(millis)),
    }))
}
