use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PROXY_URL: &str = "http://127.0.0.1:7828";
pub const HTTPS_PROXY_ENV: &str = "HTTPS_PROXY";
pub const HTTP_PROXY_ENV: &str = "HTTP_PROXY";

include!(concat!(env!("OUT_DIR"), "/bundled_profiles.rs"));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationProfile {
    pub id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    pub name: String,
    pub summary: String,
    pub provider: String,
    #[serde(default)]
    pub traffic_app_ids: Vec<String>,
    pub connect_args: Vec<String>,
    pub settings: Vec<IntegrationSetting>,
    pub commands: Vec<IntegrationCommand>,
    pub notes: Vec<String>,
    pub automation: AutomationLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveProfileState {
    pub profile_id: String,
    pub selected_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnabledIntegrationState {
    pub profile_id: String,
    pub enabled_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnabledIntegrationsState {
    #[serde(default)]
    pub profiles: Vec<EnabledIntegrationState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationSetting {
    pub key: String,
    pub value: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationCommand {
    pub label: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationLevel {
    Manual,
    ConnectPreset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IntegrationApplyPlan {
    profile_id: String,
    profile_name: String,
    dry_run: bool,
    proxy_url: String,
    changes: Vec<IntegrationFileChange>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationFileChange {
    pub path: PathBuf,
    pub action: FileAction,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Create,
    Update,
    Unchanged,
    Delete,
    Restore,
}

impl FileAction {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Unchanged => "unchanged",
            Self::Delete => "delete",
            Self::Restore => "restore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedIntegrationApply {
    pub profile_id: String,
    pub profile_name: String,
    pub proxy_url: String,
    pub target_path: PathBuf,
    desired_content: String,
    existed: bool,
    current_content: Option<String>,
    pub action: FileAction,
    pub description: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationApplyResult {
    pub profile_id: String,
    pub dry_run: bool,
    pub proxy_url: String,
    pub changes: Vec<IntegrationFileChange>,
    pub record_path: Option<PathBuf>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationRollbackResult {
    pub profile_id: String,
    pub changes: Vec<IntegrationFileChange>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationApplyInspection {
    pub profile_id: String,
    pub proxy_url: String,
    pub target_path: PathBuf,
    pub rollback_record_path: PathBuf,
    pub status: IntegrationApplyStatus,
    pub planned_action: FileAction,
    pub rollback_available: bool,
    pub record_error: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationApplyStatus {
    Applied,
    NeedsApply,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IntegrationApplyRecord {
    profile_id: String,
    applied_at_unix: u64,
    files: Vec<IntegrationBackupFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct IntegrationBackupFile {
    path: PathBuf,
    existed: bool,
    backup_path: Option<PathBuf>,
}

pub fn profiles(proxy_url: &str) -> Vec<IntegrationProfile> {
    bundled_profiles(proxy_url).expect("bundled DAM integration profile JSON must be valid")
}

pub fn profiles_from_state(
    proxy_url: &str,
    integration_state_dir: &Path,
) -> Result<Vec<IntegrationProfile>, String> {
    let bundled = bundled_profiles(proxy_url)?;
    let mut profiles = bundled.clone();
    for integration in read_stored_profile_files(integration_state_dir, proxy_url, &bundled)? {
        upsert_profile(&mut profiles, integration);
    }
    Ok(profiles)
}

pub fn profile(id: &str, proxy_url: &str) -> Option<IntegrationProfile> {
    let profiles = profiles(proxy_url);
    let id = canonical_profile_id_from_catalog(id, &profiles)?;
    profiles.into_iter().find(|profile| profile.id == id)
}

pub fn profile_from_state(
    id: &str,
    proxy_url: &str,
    integration_state_dir: &Path,
) -> Result<Option<IntegrationProfile>, String> {
    let profiles = profiles_from_state(proxy_url, integration_state_dir)?;
    let Some(id) = canonical_profile_id_from_catalog(id, &profiles) else {
        return Ok(None);
    };
    Ok(profiles.into_iter().find(|profile| profile.id == id))
}

pub fn profile_ids() -> Vec<String> {
    profiles(DEFAULT_PROXY_URL)
        .into_iter()
        .map(|profile| profile.id)
        .collect()
}

pub fn default_enabled_profile_ids() -> Vec<String> {
    profile_ids()
}

fn upsert_profile(profiles: &mut Vec<IntegrationProfile>, profile: IntegrationProfile) {
    if let Some(existing) = profiles
        .iter_mut()
        .find(|existing| existing.id == profile.id)
    {
        *existing = profile;
    } else {
        profiles.push(profile);
    }
}

type ProfileCatalogApplyContent = (String, String, String, Vec<String>);

const PROFILE_DEFINITIONS_DIR: &str = "profiles";
const APPLY_RECORDS_DIR: &str = "apply-records";

pub fn profile_definitions_dir(integration_state_dir: &Path) -> PathBuf {
    integration_state_dir.join(PROFILE_DEFINITIONS_DIR)
}

pub fn profile_definition_path(integration_state_dir: &Path, id: &str) -> PathBuf {
    profile_definitions_dir(integration_state_dir).join(format!("{id}.json"))
}

pub fn ensure_bundled_profile_files(integration_state_dir: &Path) -> Result<Vec<PathBuf>, String> {
    migrate_profile_state(integration_state_dir)?;
    let dir = profile_definitions_dir(integration_state_dir);
    fs::create_dir_all(&dir).map_err(|error| {
        format!(
            "failed to create profile directory {}: {error}",
            dir.display()
        )
    })?;
    let mut written = Vec::new();
    for raw in BUNDLED_PROFILE_JSONS {
        let profile = parse_profile_json(raw, DEFAULT_PROXY_URL)?;
        let path = dir.join(format!("{}.json", profile.id));
        if path.exists() {
            let desired = profile_file_content(raw);
            let current = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read profile {}: {error}", path.display()))?;
            if current != desired {
                atomic_write(&path, desired.as_bytes())?;
                written.push(path);
            }
            continue;
        }
        atomic_write(&path, format!("{}\n", raw.trim_end()).as_bytes())?;
        written.push(path);
    }
    prune_retired_profile_files(&dir, &profiles(DEFAULT_PROXY_URL))?;
    Ok(written)
}

fn read_stored_profile_files(
    integration_state_dir: &Path,
    proxy_url: &str,
    bundled: &[IntegrationProfile],
) -> Result<Vec<IntegrationProfile>, String> {
    let mut files = Vec::new();
    let mut ids = BTreeSet::new();
    let dir = profile_definitions_dir(integration_state_dir);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => {
            return Err(format!(
                "failed to read profile directory {}: {error}",
                dir.display()
            ));
        }
    };
    let mut paths = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("failed to read profile entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    for path in paths {
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read profile {}: {error}", path.display()))?;
        let stored_profile = serde_json::from_str::<IntegrationProfile>(&raw)
            .map_err(|error| format!("failed to parse profile {}: {error}", path.display()))?;
        validate_integration_profile(&stored_profile)?;
        if canonical_profile_id_from_catalog(&stored_profile.id, bundled).is_some() {
            continue;
        }
        let profile = render_profile_templates(stored_profile, proxy_url);
        if !ids.insert(profile.id.clone()) {
            return Err(format!(
                "duplicate integration profile id {} in {}",
                profile.id,
                integration_state_dir.display()
            ));
        }
        files.push(profile);
    }
    Ok(files)
}

fn bundled_profiles(proxy_url: &str) -> Result<Vec<IntegrationProfile>, String> {
    let mut profiles = BUNDLED_PROFILE_JSONS
        .iter()
        .map(|raw| parse_profile_json(raw, proxy_url))
        .collect::<Result<Vec<_>, _>>()?;
    profiles.sort_by(|left, right| {
        left.sort_order
            .unwrap_or(i32::MAX)
            .cmp(&right.sort_order.unwrap_or(i32::MAX))
            .then(left.name.cmp(&right.name))
            .then(left.id.cmp(&right.id))
    });
    Ok(profiles)
}

pub fn traffic_app_ids_for_profile_ids_from_state(
    profile_ids: &[String],
    integration_state_dir: &Path,
) -> Result<Vec<String>, String> {
    let mut app_ids = Vec::new();
    for profile_id in profile_ids {
        let profile = profile_from_state(profile_id, DEFAULT_PROXY_URL, integration_state_dir)?
            .ok_or_else(|| unknown_profile_error_with_state(profile_id, integration_state_dir))?;
        for app_id in profile.traffic_app_ids {
            if !app_ids.contains(&app_id) {
                app_ids.push(app_id);
            }
        }
    }
    Ok(app_ids)
}

fn validate_integration_profile(profile: &IntegrationProfile) -> Result<(), String> {
    if profile.id.trim().is_empty() {
        return Err("integration profile id is required".to_string());
    }
    if profile.aliases.iter().any(|alias| alias.trim().is_empty()) {
        return Err(format!(
            "integration profile {} aliases must not be empty",
            profile.id
        ));
    }
    if profile.aliases.iter().any(|alias| alias == &profile.id) {
        return Err(format!(
            "integration profile {} aliases must not repeat its id",
            profile.id
        ));
    }
    if profile.name.trim().is_empty() {
        return Err(format!(
            "integration profile {} name is required",
            profile.id
        ));
    }
    if profile.provider.trim().is_empty() {
        return Err(format!(
            "integration profile {} provider is required",
            profile.id
        ));
    }
    Ok(())
}

fn default_enabled_integrations(_integration_state_dir: &Path) -> Vec<EnabledIntegrationState> {
    profile_ids()
        .into_iter()
        .map(|profile_id| EnabledIntegrationState {
            profile_id,
            enabled_at_unix: 0,
        })
        .collect()
}

fn canonical_profile_id_from_catalog(
    profile_id: &str,
    profiles: &[IntegrationProfile],
) -> Option<String> {
    if let Some(profile) = profiles.iter().find(|profile| profile.id == profile_id) {
        return Some(profile.id.clone());
    }
    profiles
        .iter()
        .find(|profile| profile.aliases.iter().any(|alias| alias == profile_id))
        .map(|profile| profile.id.clone())
}

fn canonical_runtime_profile_id(profile_id: &str) -> Result<String, String> {
    let profiles = profiles(DEFAULT_PROXY_URL);
    canonical_profile_id_from_catalog(profile_id, &profiles)
        .ok_or_else(|| unknown_profile_error(profile_id))
}

fn canonical_runtime_profile_id_from_state(
    profile_id: &str,
    integration_state_dir: &Path,
) -> Result<String, String> {
    let profiles = profiles_from_state(DEFAULT_PROXY_URL, integration_state_dir)?;
    canonical_profile_id_from_catalog(profile_id, &profiles)
        .ok_or_else(|| unknown_profile_error_with_state(profile_id, integration_state_dir))
}

fn alias_pairs_from_catalog(profiles: &[IntegrationProfile]) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for profile in profiles {
        for alias in &profile.aliases {
            pairs.push((alias.clone(), profile.id.clone()));
        }
    }
    pairs
}

fn push_dedup_enabled(
    profiles: &mut Vec<EnabledIntegrationState>,
    profile_id: String,
    enabled_at_unix: u64,
) {
    if !profiles
        .iter()
        .any(|profile| profile.profile_id == profile_id)
    {
        profiles.push(EnabledIntegrationState {
            profile_id,
            enabled_at_unix,
        });
    }
}

fn parse_profile_json(raw: &str, proxy_url: &str) -> Result<IntegrationProfile, String> {
    let profile = serde_json::from_str::<IntegrationProfile>(raw)
        .map_err(|error| format!("failed to parse integration profile JSON: {error}"))?;
    Ok(render_profile_templates(profile, proxy_url))
}

fn bundled_profile_raw(profile_id: &str) -> Result<Option<&'static str>, String> {
    let mut raw_profiles = Vec::new();
    for raw in BUNDLED_PROFILE_JSONS {
        let profile = serde_json::from_str::<IntegrationProfile>(raw).map_err(|error| {
            format!("failed to parse bundled integration profile JSON: {error}")
        })?;
        raw_profiles.push((*raw, profile));
    }
    for (raw, profile) in &raw_profiles {
        if profile.id == profile_id {
            return Ok(Some(*raw));
        }
    }
    for (raw, profile) in &raw_profiles {
        if profile.aliases.iter().any(|alias| alias == profile_id) {
            return Ok(Some(*raw));
        }
    }
    Ok(None)
}

fn render_profile_templates(
    mut profile: IntegrationProfile,
    proxy_url: &str,
) -> IntegrationProfile {
    for setting in &mut profile.settings {
        setting.value = render_template(&setting.value, proxy_url);
    }
    for command in &mut profile.commands {
        for arg in &mut command.command {
            *arg = render_template(arg, proxy_url);
        }
    }
    for note in &mut profile.notes {
        *note = render_template(note, proxy_url);
    }
    profile
}

fn render_template(value: &str, proxy_url: &str) -> String {
    value
        .replace("{{proxy_url}}", proxy_url.trim_end_matches('/'))
        .replace("{{https_proxy_env}}", HTTPS_PROXY_ENV)
        .replace("{{http_proxy_env}}", HTTP_PROXY_ENV)
}

pub fn active_profile_path(integration_state_dir: &Path) -> PathBuf {
    integration_state_dir.join("active-profile.json")
}

pub fn enabled_integrations_path(integration_state_dir: &Path) -> PathBuf {
    integration_state_dir.join("enabled-integrations.json")
}

pub fn read_active_profile(
    integration_state_dir: &Path,
) -> Result<Option<ActiveProfileState>, String> {
    let path = active_profile_path(integration_state_dir);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to read active profile {}: {error}",
                path.display()
            ));
        }
    };
    let state = serde_json::from_str::<ActiveProfileState>(&raw)
        .map_err(|error| format!("failed to parse active profile {}: {error}", path.display()))?;
    let profiles = profiles_from_state(DEFAULT_PROXY_URL, integration_state_dir)?;
    let Some(profile_id) = canonical_profile_id_from_catalog(&state.profile_id, &profiles) else {
        return Ok(None);
    };
    Ok(Some(ActiveProfileState {
        profile_id,
        selected_at_unix: state.selected_at_unix,
    }))
}

pub fn read_enabled_integrations(
    integration_state_dir: &Path,
) -> Result<Vec<EnabledIntegrationState>, String> {
    read_enabled_integrations_file(integration_state_dir)
        .map(|profiles| profiles.unwrap_or_default())
}

pub fn read_effective_enabled_integrations(
    integration_state_dir: &Path,
) -> Result<Vec<EnabledIntegrationState>, String> {
    read_runtime_enabled_integrations(integration_state_dir).map(|profiles| {
        profiles.unwrap_or_else(|| default_enabled_integrations(integration_state_dir))
    })
}

pub fn read_runtime_enabled_integrations(
    integration_state_dir: &Path,
) -> Result<Option<Vec<EnabledIntegrationState>>, String> {
    if let Some(profiles) = read_enabled_integrations_file(integration_state_dir)? {
        return Ok(Some(profiles));
    }

    if let Some(active) = read_active_profile(integration_state_dir)? {
        return Ok(Some(vec![EnabledIntegrationState {
            profile_id: active.profile_id,
            enabled_at_unix: active.selected_at_unix,
        }]));
    }

    Ok(Some(default_enabled_integrations(integration_state_dir)))
}

pub fn enabled_profile_ids(integration_state_dir: &Path) -> Result<Vec<String>, String> {
    read_effective_enabled_integrations(integration_state_dir).map(|profiles| {
        profiles
            .into_iter()
            .map(|profile| profile.profile_id)
            .collect()
    })
}

pub fn runtime_enabled_profile_ids(
    integration_state_dir: &Path,
) -> Result<Option<Vec<String>>, String> {
    read_runtime_enabled_integrations(integration_state_dir).map(|profiles| {
        profiles.map(|profiles| {
            profiles
                .into_iter()
                .map(|profile| profile.profile_id)
                .collect()
        })
    })
}

pub fn traffic_app_ids_for_profile_ids(profile_ids: &[String]) -> Result<Vec<String>, String> {
    let mut app_ids = Vec::new();
    for profile_id in profile_ids {
        let profile = profile(profile_id, DEFAULT_PROXY_URL)
            .ok_or_else(|| unknown_profile_error(profile_id))?;
        for app_id in profile.traffic_app_ids {
            if !app_ids.contains(&app_id) {
                app_ids.push(app_id);
            }
        }
    }
    Ok(app_ids)
}

pub fn set_integration_enabled(
    profile_id: &str,
    enabled: bool,
    integration_state_dir: &Path,
) -> Result<Vec<EnabledIntegrationState>, String> {
    let profile_id = canonical_runtime_profile_id_from_state(profile_id, integration_state_dir)?;
    if profile_from_state(&profile_id, DEFAULT_PROXY_URL, integration_state_dir)?.is_none() {
        return Err(unknown_profile_error_with_state(
            &profile_id,
            integration_state_dir,
        ));
    }
    ensure_bundled_profile_files(integration_state_dir)?;

    fs::create_dir_all(integration_state_dir).map_err(|error| {
        format!(
            "failed to create integration state directory {}: {error}",
            integration_state_dir.display()
        )
    })?;

    let mut profiles = read_effective_enabled_integrations(integration_state_dir)?;
    profiles.retain(|profile| profile.profile_id != profile_id);
    if enabled {
        profiles.push(EnabledIntegrationState {
            profile_id,
            enabled_at_unix: unix_timestamp()?,
        });
    }
    write_enabled_integrations(integration_state_dir, profiles)
}

pub fn clear_enabled_integrations(integration_state_dir: &Path) -> Result<bool, String> {
    let path = enabled_integrations_path(integration_state_dir);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "failed to remove enabled integrations {}: {error}",
            path.display()
        )),
    }
}

pub fn set_active_profile(
    profile_id: &str,
    integration_state_dir: &Path,
) -> Result<ActiveProfileState, String> {
    let profile_id = canonical_runtime_profile_id_from_state(profile_id, integration_state_dir)?;
    if profile_from_state(&profile_id, DEFAULT_PROXY_URL, integration_state_dir)?.is_none() {
        return Err(unknown_profile_error_with_state(
            &profile_id,
            integration_state_dir,
        ));
    }
    ensure_bundled_profile_files(integration_state_dir)?;
    fs::create_dir_all(integration_state_dir).map_err(|error| {
        format!(
            "failed to create integration state directory {}: {error}",
            integration_state_dir.display()
        )
    })?;
    let state = ActiveProfileState {
        profile_id,
        selected_at_unix: unix_timestamp()?,
    };
    write_json_file(&active_profile_path(integration_state_dir), &state)?;
    Ok(state)
}

pub fn clear_active_profile(integration_state_dir: &Path) -> Result<bool, String> {
    let path = active_profile_path(integration_state_dir);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "failed to remove active profile {}: {error}",
            path.display()
        )),
    }
}

fn read_enabled_integrations_file(
    integration_state_dir: &Path,
) -> Result<Option<Vec<EnabledIntegrationState>>, String> {
    let path = enabled_integrations_path(integration_state_dir);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to read enabled integrations {}: {error}",
                path.display()
            ));
        }
    };
    let state = serde_json::from_str::<EnabledIntegrationsState>(&raw).map_err(|error| {
        format!(
            "failed to parse enabled integrations {}: {error}",
            path.display()
        )
    })?;
    let mut enabled_profiles = Vec::new();
    let catalog = profiles_from_state(DEFAULT_PROXY_URL, integration_state_dir)
        .unwrap_or_else(|_| profiles(DEFAULT_PROXY_URL));
    for enabled in state.profiles {
        if let Some(profile_id) = canonical_profile_id_from_catalog(&enabled.profile_id, &catalog) {
            push_dedup_enabled(&mut enabled_profiles, profile_id, enabled.enabled_at_unix);
        }
    }
    Ok(Some(enabled_profiles))
}

fn write_enabled_integrations(
    integration_state_dir: &Path,
    profiles: Vec<EnabledIntegrationState>,
) -> Result<Vec<EnabledIntegrationState>, String> {
    let state = EnabledIntegrationsState { profiles };
    write_json_file(&enabled_integrations_path(integration_state_dir), &state)?;
    Ok(state.profiles)
}

pub fn default_apply_path(
    profile_id: &str,
    integration_state_dir: &Path,
) -> Result<PathBuf, String> {
    let profile_id = canonical_runtime_profile_id_from_state(profile_id, integration_state_dir)?;
    if profile_from_state(&profile_id, DEFAULT_PROXY_URL, integration_state_dir)?.is_some() {
        return Ok(profile_definition_path(integration_state_dir, &profile_id));
    }
    Err(unknown_profile_error_with_state(
        &profile_id,
        integration_state_dir,
    ))
}

pub fn prepare_apply(
    profile_id: &str,
    proxy_url: &str,
    target_path: PathBuf,
) -> Result<PreparedIntegrationApply, String> {
    let profile_id = canonical_runtime_profile_id(profile_id)?;
    let profile =
        profile(&profile_id, proxy_url).ok_or_else(|| unknown_profile_error(&profile_id))?;
    prepare_apply_for_profile(&profile_id, profile, proxy_url, target_path)
}

pub fn prepare_apply_in_state(
    profile_id: &str,
    proxy_url: &str,
    target_path: PathBuf,
    integration_state_dir: &Path,
) -> Result<PreparedIntegrationApply, String> {
    let profile_id = canonical_runtime_profile_id_from_state(profile_id, integration_state_dir)?;
    if target_path == profile_definition_path(integration_state_dir, &profile_id)
        && let Some((_stored_profile_id, profile_name, desired_content, notes)) =
            profile_catalog_apply_content(&profile_id, integration_state_dir)?
    {
        return prepare_apply_for_content(
            profile_id.clone(),
            profile_name,
            proxy_url,
            target_path,
            desired_content,
            profile_apply_description(&profile_id),
            notes,
        );
    }
    let profile = profile_from_state(&profile_id, proxy_url, integration_state_dir)?
        .ok_or_else(|| unknown_profile_error_with_state(&profile_id, integration_state_dir))?;
    prepare_apply_for_profile(&profile_id, profile, proxy_url, target_path)
}

fn profile_catalog_apply_content(
    profile_id: &str,
    integration_state_dir: &Path,
) -> Result<Option<ProfileCatalogApplyContent>, String> {
    if let Some(raw) = bundled_profile_raw(profile_id)? {
        let profile = parse_profile_json(raw, DEFAULT_PROXY_URL)?;
        return Ok(Some((
            profile.id,
            profile.name,
            profile_file_content(raw),
            profile.notes,
        )));
    }

    let path = profile_definition_path(integration_state_dir, profile_id);
    match fs::read_to_string(&path) {
        Ok(raw) => {
            let profile = serde_json::from_str::<IntegrationProfile>(&raw)
                .map_err(|error| format!("failed to parse profile {}: {error}", path.display()))?;
            validate_integration_profile(&profile)?;
            return Ok(Some((
                profile.id,
                profile.name,
                profile_file_content(&raw),
                profile.notes,
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    }

    Ok(None)
}

fn prepare_apply_for_profile(
    profile_id: &str,
    profile: IntegrationProfile,
    proxy_url: &str,
    target_path: PathBuf,
) -> Result<PreparedIntegrationApply, String> {
    let desired_content = integration_profile_content(&profile)?;
    prepare_apply_for_content(
        profile_id.to_string(),
        profile.name,
        proxy_url,
        target_path,
        desired_content,
        profile_apply_description(profile_id),
        profile.notes,
    )
}

fn prepare_apply_for_content(
    profile_id: String,
    profile_name: String,
    proxy_url: &str,
    target_path: PathBuf,
    desired_content: String,
    description: String,
    notes: Vec<String>,
) -> Result<PreparedIntegrationApply, String> {
    let (existed, current_content) = read_optional_file(&target_path)?;
    let action = match (
        existed,
        current_content.as_deref() == Some(desired_content.as_str()),
    ) {
        (_, true) => FileAction::Unchanged,
        (true, false) => FileAction::Update,
        (false, false) => FileAction::Create,
    };

    Ok(PreparedIntegrationApply {
        profile_id,
        profile_name,
        proxy_url: proxy_url.to_string(),
        target_path,
        desired_content,
        existed,
        current_content,
        action,
        description,
        notes,
    })
}

fn profile_apply_description(profile_id: &str) -> String {
    format!("write DAM-managed JSON profile {profile_id}")
}

pub fn run_apply(
    prepared: PreparedIntegrationApply,
    dry_run: bool,
    state_dir: &Path,
) -> Result<IntegrationApplyResult, String> {
    let changes = vec![IntegrationFileChange {
        path: prepared.target_path.clone(),
        action: prepared.action,
        description: prepared.description.clone(),
    }];
    if dry_run {
        let plan = IntegrationApplyPlan {
            profile_id: prepared.profile_id.clone(),
            profile_name: prepared.profile_name,
            dry_run,
            proxy_url: prepared.proxy_url.clone(),
            changes: changes.clone(),
            notes: prepared.notes,
        };
        return Ok(IntegrationApplyResult {
            profile_id: prepared.profile_id,
            dry_run,
            proxy_url: prepared.proxy_url,
            changes,
            record_path: None,
            message: render_apply_plan_message(&plan),
        });
    }

    migrate_profile_state(state_dir)?;
    let profile_dir = profile_state_dir(state_dir, &prepared.profile_id);
    let record_path = profile_dir.join("latest.json");
    let (rollback_available, record_error) =
        rollback_record_state(&prepared.profile_id, &record_path);
    if let Some(error) = record_error {
        return Err(format!(
            "refusing to apply {} because its rollback record needs attention: {error}",
            prepared.profile_id
        ));
    }
    if rollback_available {
        if prepared.action == FileAction::Unchanged {
            return Ok(IntegrationApplyResult {
                profile_id: prepared.profile_id,
                dry_run: false,
                proxy_url: prepared.proxy_url,
                changes,
                record_path: Some(record_path),
                message: "integration profile already applied".to_string(),
            });
        }
        return Err(format!(
            "refusing to apply {} because DAM already has a rollback record and the target changed; run `dam integrations rollback {}` before applying again",
            prepared.profile_id, prepared.profile_id
        ));
    }
    if prepared.action == FileAction::Unchanged {
        return Ok(IntegrationApplyResult {
            profile_id: prepared.profile_id,
            dry_run: false,
            proxy_url: prepared.proxy_url,
            changes,
            record_path: None,
            message:
                "integration profile content is already present; no rollback record was written"
                    .to_string(),
        });
    }

    fs::create_dir_all(&profile_dir).map_err(|error| {
        format!(
            "failed to create integration state directory {}: {error}",
            profile_dir.display()
        )
    })?;
    let applied_at_unix = unix_timestamp()?;
    let backup_dir = create_backup_dir(&profile_dir, applied_at_unix)?;

    let backup_path = if prepared.existed {
        let backup_path = backup_dir.join("target.backup");
        atomic_write(
            &backup_path,
            prepared.current_content.unwrap_or_default().as_bytes(),
        )?;
        Some(backup_path)
    } else {
        None
    };

    if let Some(parent) = prepared.target_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create target directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let record = IntegrationApplyRecord {
        profile_id: prepared.profile_id.clone(),
        applied_at_unix,
        files: vec![IntegrationBackupFile {
            path: prepared.target_path.clone(),
            existed: prepared.existed,
            backup_path,
        }],
    };
    write_json_file(&record_path, &record)?;
    atomic_write(&prepared.target_path, prepared.desired_content.as_bytes())?;

    Ok(IntegrationApplyResult {
        profile_id: prepared.profile_id,
        dry_run: false,
        proxy_url: prepared.proxy_url,
        changes,
        record_path: Some(record_path),
        message: "integration profile applied".to_string(),
    })
}

pub fn inspect_apply(
    profile_id: &str,
    proxy_url: &str,
    target_path: PathBuf,
    state_dir: &Path,
) -> Result<IntegrationApplyInspection, String> {
    let prepared = prepare_apply(profile_id, proxy_url, target_path)?;
    inspect_prepared_apply(prepared, profile_id, state_dir)
}

pub fn inspect_apply_in_state(
    profile_id: &str,
    proxy_url: &str,
    target_path: PathBuf,
    state_dir: &Path,
    integration_state_dir: &Path,
) -> Result<IntegrationApplyInspection, String> {
    let prepared =
        prepare_apply_in_state(profile_id, proxy_url, target_path, integration_state_dir)?;
    inspect_prepared_apply(prepared, profile_id, state_dir)
}

fn inspect_prepared_apply(
    prepared: PreparedIntegrationApply,
    profile_id: &str,
    state_dir: &Path,
) -> Result<IntegrationApplyInspection, String> {
    migrate_profile_state(state_dir)?;
    let catalog = profiles(DEFAULT_PROXY_URL);
    let record_profile_id = canonical_profile_id_from_catalog(profile_id, &catalog)
        .unwrap_or_else(|| prepared.profile_id.clone());
    let record_path = profile_state_dir(state_dir, &record_profile_id).join("latest.json");
    let (rollback_available, record_error) =
        rollback_record_state(&record_profile_id, &record_path);
    let status = match (prepared.action, rollback_available, false) {
        (FileAction::Unchanged, _, _) => IntegrationApplyStatus::Applied,
        (_, true, false) => IntegrationApplyStatus::Modified,
        _ => IntegrationApplyStatus::NeedsApply,
    };
    let message = match (status, rollback_available, record_error.as_ref()) {
        (IntegrationApplyStatus::Applied, true, None) => {
            "integration profile is applied; rollback is available"
        }
        (IntegrationApplyStatus::Applied, false, None) => {
            "integration profile content is present; no DAM rollback record is available"
        }
        (IntegrationApplyStatus::Applied, false, Some(_)) => {
            "integration profile content is present; rollback record is unreadable"
        }
        (IntegrationApplyStatus::Applied, true, Some(_)) => {
            "integration profile content is present; rollback record needs attention"
        }
        (IntegrationApplyStatus::Modified, true, None) => {
            "integration profile was applied but target content no longer matches DAM's desired content"
        }
        (IntegrationApplyStatus::Modified, _, Some(_)) => {
            "integration profile target content changed and rollback record is unreadable"
        }
        (IntegrationApplyStatus::Modified, false, None) => {
            "integration profile target content does not match DAM's desired content"
        }
        (IntegrationApplyStatus::NeedsApply, true, None) => {
            "integration profile is not applied but rollback is available"
        }
        (IntegrationApplyStatus::NeedsApply, _, Some(_)) => {
            "integration profile is not applied and rollback record is unreadable"
        }
        (IntegrationApplyStatus::NeedsApply, _, None) => "integration profile is not applied",
    }
    .to_string();

    Ok(IntegrationApplyInspection {
        profile_id: prepared.profile_id,
        proxy_url: prepared.proxy_url,
        target_path: prepared.target_path,
        rollback_record_path: record_path,
        status,
        planned_action: prepared.action,
        rollback_available,
        record_error,
        message,
    })
}

pub fn rollback_profile(
    profile_id: &str,
    state_dir: &Path,
) -> Result<IntegrationRollbackResult, String> {
    migrate_profile_state(state_dir)?;
    let catalog = profiles(DEFAULT_PROXY_URL);
    let profile_id = canonical_profile_id_from_catalog(profile_id, &catalog)
        .unwrap_or_else(|| profile_id.to_string());
    let record_path = profile_state_dir(state_dir, &profile_id).join("latest.json");
    let raw = fs::read_to_string(&record_path).map_err(|error| {
        format!(
            "failed to read rollback record for {profile_id} at {}: {error}",
            record_path.display()
        )
    })?;
    let record = serde_json::from_str::<IntegrationApplyRecord>(&raw).map_err(|error| {
        format!(
            "failed to parse rollback record {}: {error}",
            record_path.display()
        )
    })?;
    let result_profile_id = canonical_profile_id_from_catalog(&record.profile_id, &catalog)
        .unwrap_or_else(|| record.profile_id.clone());
    let mut changes = Vec::new();
    for file in &record.files {
        if file.existed {
            let backup_path = file.backup_path.as_ref().ok_or_else(|| {
                format!(
                    "rollback record for {} is missing backup path",
                    file.path.display()
                )
            })?;
            if let Some(parent) = file.path.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "failed to create restore directory {}: {error}",
                        parent.display()
                    )
                })?;
            }
            atomic_copy(backup_path, &file.path)?;
            changes.push(IntegrationFileChange {
                path: file.path.clone(),
                action: FileAction::Restore,
                description: "restore backup".to_string(),
            });
        } else {
            match fs::remove_file(&file.path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "failed to remove created file {}: {error}",
                        file.path.display()
                    ));
                }
            }
            changes.push(IntegrationFileChange {
                path: file.path.clone(),
                action: FileAction::Delete,
                description: "remove file created by DAM".to_string(),
            });
        }
    }
    fs::remove_file(&record_path).map_err(|error| {
        format!(
            "failed to remove rollback record {}: {error}",
            record_path.display()
        )
    })?;

    Ok(IntegrationRollbackResult {
        profile_id: result_profile_id,
        changes,
        message: "integration profile rolled back".to_string(),
    })
}

pub fn profile_state_dir(state_dir: &Path, profile_id: &str) -> PathBuf {
    state_dir.join(APPLY_RECORDS_DIR).join(profile_id)
}

#[cfg(test)]
fn legacy_profile_state_dir(state_dir: &Path, profile_id: &str) -> PathBuf {
    state_dir.join(PROFILE_DEFINITIONS_DIR).join(profile_id)
}

fn migrate_profile_state(state_dir: &Path) -> Result<(), String> {
    migrate_legacy_apply_records(state_dir)?;
    migrate_profile_id_alias_records(state_dir)?;
    migrate_enabled_profile_aliases(state_dir)?;
    migrate_active_profile_alias(state_dir)
}

fn migrate_profile_id_alias_records(state_dir: &Path) -> Result<(), String> {
    for (alias, canonical) in alias_pairs_from_catalog(&profiles(DEFAULT_PROXY_URL)) {
        let old_dir = profile_state_dir(state_dir, &alias);
        if !old_dir.exists() {
            continue;
        }
        let new_dir = profile_state_dir(state_dir, &canonical);
        if new_dir.exists() {
            continue;
        }
        if let Some(parent) = new_dir.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create integration state directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        fs::rename(&old_dir, &new_dir).map_err(|error| {
            format!(
                "failed to migrate profile state {} to {}: {error}",
                old_dir.display(),
                new_dir.display()
            )
        })?;
        rewrite_alias_apply_record(&new_dir, &old_dir, &alias, &canonical)?;
    }
    Ok(())
}

fn rewrite_alias_apply_record(
    new_dir: &Path,
    old_dir: &Path,
    alias: &str,
    canonical: &str,
) -> Result<(), String> {
    let record_path = new_dir.join("latest.json");
    let raw = match fs::read_to_string(&record_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to read migrated rollback record {}: {error}",
                record_path.display()
            ));
        }
    };
    let mut record = serde_json::from_str::<IntegrationApplyRecord>(&raw).map_err(|error| {
        format!(
            "failed to parse migrated rollback record {}: {error}",
            record_path.display()
        )
    })?;
    if record.profile_id == alias {
        record.profile_id = canonical.to_string();
    }
    for file in &mut record.files {
        if let Some(backup_path) = &file.backup_path
            && let Ok(relative) = backup_path.strip_prefix(old_dir)
        {
            file.backup_path = Some(new_dir.join(relative));
        }
    }
    write_json_file(&record_path, &record)
}

fn prune_retired_profile_files(
    profile_dir: &Path,
    profiles: &[IntegrationProfile],
) -> Result<(), String> {
    for (alias, _canonical) in alias_pairs_from_catalog(profiles) {
        let path = profile_dir.join(format!("{alias}.json"));
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "failed to remove retired profile file {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn migrate_enabled_profile_aliases(state_dir: &Path) -> Result<(), String> {
    let path = enabled_integrations_path(state_dir);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to read enabled integrations {}: {error}",
                path.display()
            ));
        }
    };
    let state = serde_json::from_str::<EnabledIntegrationsState>(&raw).map_err(|error| {
        format!(
            "failed to parse enabled integrations {}: {error}",
            path.display()
        )
    })?;
    let mut changed = false;
    let mut enabled_profiles = Vec::new();
    let catalog = profiles_from_state(DEFAULT_PROXY_URL, state_dir)
        .unwrap_or_else(|_| profiles(DEFAULT_PROXY_URL));
    for enabled in state.profiles {
        let original_profile_id = enabled.profile_id;
        let enabled_at_unix = enabled.enabled_at_unix;
        let Some(profile_id) = canonical_profile_id_from_catalog(&original_profile_id, &catalog)
        else {
            changed = true;
            continue;
        };
        changed |= profile_id != original_profile_id;
        let before_len = enabled_profiles.len();
        push_dedup_enabled(&mut enabled_profiles, profile_id, enabled_at_unix);
        changed |= enabled_profiles.len() == before_len;
    }
    if changed {
        write_enabled_integrations(state_dir, enabled_profiles)?;
    }
    Ok(())
}

fn migrate_active_profile_alias(state_dir: &Path) -> Result<(), String> {
    let path = active_profile_path(state_dir);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to read active profile {}: {error}",
                path.display()
            ));
        }
    };
    let state = serde_json::from_str::<ActiveProfileState>(&raw)
        .map_err(|error| format!("failed to parse active profile {}: {error}", path.display()))?;
    let catalog = profiles_from_state(DEFAULT_PROXY_URL, state_dir)
        .unwrap_or_else(|_| profiles(DEFAULT_PROXY_URL));
    let Some(profile_id) = canonical_profile_id_from_catalog(&state.profile_id, &catalog) else {
        fs::remove_file(&path).map_err(|error| {
            format!(
                "failed to remove retired active profile {}: {error}",
                path.display()
            )
        })?;
        return Ok(());
    };
    if profile_id != state.profile_id {
        write_json_file(
            &path,
            &ActiveProfileState {
                profile_id,
                selected_at_unix: state.selected_at_unix,
            },
        )?;
    }
    Ok(())
}

fn migrate_legacy_apply_records(state_dir: &Path) -> Result<(), String> {
    let legacy_root = state_dir.join(PROFILE_DEFINITIONS_DIR);
    let entries = match fs::read_dir(&legacy_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to read legacy profile state directory {}: {error}",
                legacy_root.display()
            ));
        }
    };

    let mut legacy_dirs = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("failed to read legacy profile state entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    legacy_dirs.sort();
    for legacy_dir in legacy_dirs {
        if !legacy_dir.is_dir() || !legacy_dir.join("latest.json").exists() {
            continue;
        }
        let Some(profile_id) = legacy_dir
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let target_dir = profile_state_dir(state_dir, &profile_id);
        if target_dir.join("latest.json").exists() {
            continue;
        }
        if target_dir.exists() {
            return Err(format!(
                "cannot migrate legacy rollback record for {profile_id}: target directory already exists at {}",
                target_dir.display()
            ));
        }
        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create apply records directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let record_path = legacy_dir.join("latest.json");
        let record_raw = fs::read_to_string(&record_path).map_err(|error| {
            format!(
                "failed to read legacy rollback record {}: {error}",
                record_path.display()
            )
        })?;
        let mut record =
            serde_json::from_str::<IntegrationApplyRecord>(&record_raw).map_err(|error| {
                format!(
                    "failed to parse legacy rollback record {}: {error}",
                    record_path.display()
                )
            })?;
        for file in &mut record.files {
            if let Some(backup_path) = &mut file.backup_path
                && backup_path.starts_with(&legacy_dir)
                && let Ok(suffix) = backup_path.strip_prefix(&legacy_dir)
            {
                *backup_path = target_dir.join(suffix);
            }
        }

        fs::rename(&legacy_dir, &target_dir).map_err(|error| {
            format!(
                "failed to migrate legacy rollback record {} to {}: {error}",
                legacy_dir.display(),
                target_dir.display()
            )
        })?;
        write_json_file(&target_dir.join("latest.json"), &record)?;
    }
    Ok(())
}

fn integration_profile_content(profile: &IntegrationProfile) -> Result<String, String> {
    serde_json::to_string_pretty(profile)
        .map(|json| format!("{json}\n"))
        .map_err(|error| {
            format!(
                "failed to serialize integration profile {}: {error}",
                profile.id
            )
        })
}

fn profile_file_content(raw: &str) -> String {
    format!("{}\n", raw.trim_end())
}

fn read_optional_file(path: &Path) -> Result<(bool, Option<String>), String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok((true, Some(content))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((false, None)),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize {}: {error}", path.display()))?;
    atomic_write(path, format!("{raw}\n").as_bytes())
}

fn create_backup_dir(profile_dir: &Path, applied_at_unix: u64) -> Result<PathBuf, String> {
    let backups_dir = profile_dir.join("backups");
    fs::create_dir_all(&backups_dir).map_err(|error| {
        format!(
            "failed to create backup directory {}: {error}",
            backups_dir.display()
        )
    })?;
    tempfile::Builder::new()
        .prefix(&format!("{applied_at_unix}-"))
        .tempdir_in(&backups_dir)
        .map(|dir| dir.keep())
        .map_err(|error| {
            format!(
                "failed to create backup directory in {}: {error}",
                backups_dir.display()
            )
        })
}

fn atomic_copy(source: &Path, target: &Path) -> Result<(), String> {
    let content = fs::read(source)
        .map_err(|error| format!("failed to read backup {}: {error}", source.display()))?;
    atomic_write(target, &content)
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let temp_dir = parent.unwrap_or_else(|| Path::new("."));
    if let Some(parent) = parent {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create directory {}: {error}", parent.display()))?;
    }
    let mut temp = tempfile::NamedTempFile::new_in(temp_dir).map_err(|error| {
        format!(
            "failed to create temporary file for {}: {error}",
            path.display()
        )
    })?;
    temp.write_all(content).map_err(|error| {
        format!(
            "failed to write temporary file for {}: {error}",
            path.display()
        )
    })?;
    temp.as_file_mut().sync_all().map_err(|error| {
        format!(
            "failed to sync temporary file for {}: {error}",
            path.display()
        )
    })?;
    temp.persist(path).map(|_| ()).map_err(|error| {
        format!(
            "failed to replace {} atomically: {}",
            path.display(),
            error.error
        )
    })
}

fn rollback_record_state(profile_id: &str, record_path: &Path) -> (bool, Option<String>) {
    match fs::read_to_string(record_path) {
        Ok(raw) => match serde_json::from_str::<IntegrationApplyRecord>(&raw) {
            Ok(record) if record.profile_id == profile_id => (true, None),
            Ok(record) => (
                false,
                Some(format!(
                    "rollback record profile id {} does not match {profile_id}",
                    record.profile_id
                )),
            ),
            Err(error) => (
                false,
                Some(format!(
                    "failed to parse rollback record {}: {error}",
                    record_path.display()
                )),
            ),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (false, None),
        Err(error) => (
            false,
            Some(format!(
                "failed to read rollback record {}: {error}",
                record_path.display()
            )),
        ),
    }
}

fn render_apply_plan_message(plan: &IntegrationApplyPlan) -> String {
    if plan
        .changes
        .iter()
        .all(|change| change.action == FileAction::Unchanged)
    {
        "integration profile already applied".to_string()
    } else if plan.dry_run {
        "dry run complete; no files changed".to_string()
    } else {
        "integration profile prepared".to_string()
    }
}

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "system clock is before unix epoch".to_string())
}

fn unknown_profile_error(profile_id: &str) -> String {
    format!(
        "unknown integration profile: {profile_id}\nknown profiles: {}",
        profile_ids().join(", ")
    )
}

fn unknown_profile_error_with_state(profile_id: &str, integration_state_dir: &Path) -> String {
    let mut ids = profile_ids();
    if let Ok(profiles) = profiles_from_state(DEFAULT_PROXY_URL, integration_state_dir) {
        ids.extend(profiles.into_iter().map(|profile| profile.id));
    }
    ids.sort();
    ids.dedup();
    format!(
        "unknown integration profile: {profile_id}\nknown profiles: {}",
        ids.join(", ")
    )
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
