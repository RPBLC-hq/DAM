//! Agent-readable setup plan endpoints.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::{Ok, WebError, WebErrorCode, WebResult};

#[derive(Debug, Clone, Serialize)]
pub struct SetupNextActionView {
    pub state: dam_diagnostics::SetupPlanState,
    pub message: String,
    pub state_dir: std::path::PathBuf,
    pub proxy_url: String,
    pub network_mode: dam_net::CaptureMode,
    pub trust_mode: dam_trust::TrustMode,
    pub next_action: Option<dam_diagnostics::SetupStep>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetupRescueRequest {
    pub apply: Option<bool>,
    pub confirm: Option<String>,
}

pub async fn plan(State(state): State<AppState>) -> WebResult<dam_diagnostics::SetupPlan> {
    setup_plan(&state).map(Ok::new)
}

pub async fn next_action(State(state): State<AppState>) -> WebResult<SetupNextActionView> {
    let plan = setup_plan(&state)?;
    Ok(Ok::new(SetupNextActionView {
        state: plan.state,
        message: plan.message,
        state_dir: plan.state_dir,
        proxy_url: plan.proxy_url,
        network_mode: plan.network_mode,
        trust_mode: plan.trust_mode,
        next_action: plan.next_action,
    }))
}

pub async fn rescue(
    State(_state): State<AppState>,
    Json(body): Json<SetupRescueRequest>,
) -> WebResult<dam_diagnostics::SetupRescue> {
    let apply = body.apply.unwrap_or(false);
    if apply && body.confirm.as_deref() != Some("remove_dam_network_setup") {
        return Err(WebError::new(WebErrorCode::InvalidRequest));
    }
    dam_diagnostics::setup_rescue(&dam_diagnostics::SetupRescueOptions {
        state_dir: None,
        proxy_url: None,
        apply,
    })
    .map(Ok::new)
    .map_err(|_| WebError::new(WebErrorCode::Unknown))
}

pub async fn repair(
    State(state): State<AppState>,
    Json(body): Json<SetupRescueRequest>,
) -> WebResult<dam_diagnostics::SetupRepair> {
    let apply = body.apply.unwrap_or(false);
    if apply && body.confirm.as_deref() != Some("remove_dam_network_setup") {
        return Err(WebError::new(WebErrorCode::InvalidRequest));
    }
    dam_diagnostics::setup_repair(
        &state.config,
        &dam_diagnostics::SetupRepairOptions {
            setup: setup_options(&state),
            apply,
        },
    )
    .map(Ok::new)
    .map_err(|_| WebError::new(WebErrorCode::Unknown))
}

pub async fn diagnostics(
    State(state): State<AppState>,
) -> WebResult<dam_diagnostics::SetupDiagnosticsExport> {
    dam_diagnostics::setup_diagnostics_export(
        &state.config,
        &dam_diagnostics::DoctorOptions {
            proxy_url: None,
            state_dir: None,
            config_path: state.config_path.clone(),
        },
        &setup_options(&state),
    )
    .await
    .map(Ok::new)
    .map_err(|_| WebError::new(WebErrorCode::Unknown))
}

fn setup_plan(state: &AppState) -> Result<dam_diagnostics::SetupPlan, WebError> {
    dam_diagnostics::setup_plan(&state.config, &setup_options(state))
        .map_err(|_| WebError::new(WebErrorCode::Unknown))
}

fn setup_options(state: &AppState) -> dam_diagnostics::SetupPlanOptions {
    dam_diagnostics::SetupPlanOptions {
        state_dir: None,
        config_path: state.config_path.clone(),
        proxy_url: None,
        network_mode: dam_net::CaptureMode::ExplicitProxy,
        trust_mode: dam_trust::TrustMode::Disabled,
    }
}
