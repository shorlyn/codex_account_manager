use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{
    engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD},
    Engine as _,
};
use rand_core::{OsRng, RngCore};
use reqwest::header::{
    HeaderName, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, REFERER, USER_AGENT,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{
    command,
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

#[cfg(target_os = "windows")]
const WINDOWS_CODEX_APP_ID: &str = "OpenAI.Codex_2p2nqsd0c76g0!App";

// ── API response structs ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ApiResponse {
    plan_type: Option<String>,
    rate_limit: Option<RateLimit>,
}

#[derive(Debug, Deserialize)]
struct RateLimit {
    primary_window: Option<Window>,
    secondary_window: Option<Window>,
}

#[derive(Debug, Deserialize)]
struct Window {
    used_percent: Option<i32>,
    limit_window_seconds: Option<i64>,
    reset_after_seconds: Option<i64>,
    reset_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuotaInfo {
    pub plan_type: String,
    pub primary_used_percent: i32,
    pub primary_reset_at: i64,
    pub primary_window_minutes: Option<i64>,
    pub primary_window_present: bool,
    pub secondary_used_percent: i32,
    pub secondary_reset_at: i64,
    pub secondary_window_minutes: Option<i64>,
    pub secondary_window_present: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoragePaths {
    pub app_data_dir: String,
    pub database_path: String,
    pub auth_json_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MigrationStatus {
    pub pending_plaintext_accounts: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OperationLog {
    pub id: i64,
    pub level: String,
    pub action: String,
    pub account_id: Option<i64>,
    pub account_name: String,
    pub account_identifier: String,
    pub stage: String,
    pub message: String,
    pub details: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CodexAppSpeed {
    Standard,
    Fast,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexAppSpeedConfig {
    pub speed: CodexAppSpeed,
    pub config_path: String,
    pub global_state_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexOfficialModeIssue {
    pub line: usize,
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexFeatureStatus {
    pub config_path: String,
    pub global_state_path: String,
    pub goals_db_path: String,
    pub goals_enabled: bool,
    pub goals_db_present: bool,
    pub memory_generate_enabled: bool,
    pub memory_use_enabled: bool,
    pub official_mode_ok: bool,
    pub official_mode_issues: Vec<CodexOfficialModeIssue>,
    pub config_speed: CodexAppSpeed,
    pub config_service_tier: Option<String>,
    pub global_state_speed: CodexAppSpeed,
    pub global_state_service_tier: Option<String>,
    pub global_state_user_changed_tier: bool,
    pub fast_state_synced: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexProxyState {
    pub enabled: bool,
    pub port: u16,
    pub base_url: String,
    pub active_account_id: Option<i64>,
    pub active_account_name: String,
    pub config_installed: bool,
    pub config_path: String,
    pub last_error: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexProjectVisibilityStatus {
    pub project_path: String,
    pub config_path: String,
    pub is_trusted: bool,
    pub changed: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexSessionVisibilityStatus {
    pub codex_home: String,
    pub state_db_path: String,
    pub session_index_path: String,
    pub target_provider: String,
    pub scanned_rollout_files: usize,
    pub mismatched_rollout_files: usize,
    pub mismatched_sqlite_records: usize,
    pub missing_sqlite_records: usize,
    pub missing_session_index_entries: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexSessionVisibilityRepairFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexSessionVisibilityRepairReport {
    pub codex_home: String,
    pub state_db_path: String,
    pub session_index_path: String,
    pub target_provider: String,
    pub backup_dir: String,
    pub scanned_rollout_files: usize,
    pub rewritten_rollout_files: usize,
    pub sqlite_records_updated: usize,
    pub sqlite_records_inserted: usize,
    pub session_index_entries_added: usize,
    pub failed_rollout_files: Vec<CodexSessionVisibilityRepairFailure>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountHealthItem {
    pub key: String,
    pub label: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountHealthReport {
    pub account_id: i64,
    pub checked_at: String,
    pub summary_status: String,
    pub items: Vec<AccountHealthItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CodexUsageRollup {
    pub request_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub non_cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub api_cost_usd: f64,
    pub codex_credits: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexModelUsage {
    pub model: String,
    pub usage: CodexUsageRollup,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexUsageFailure {
    pub ts: i64,
    pub model: String,
    pub turn_id: String,
    pub response_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexUsageSummary {
    pub log_path: String,
    pub today_start_ts: i64,
    pub today_end_ts: i64,
    pub total: CodexUsageRollup,
    pub today: CodexUsageRollup,
    pub by_model: Vec<CodexModelUsage>,
    pub recent_failures: Vec<CodexUsageFailure>,
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub activation_date: String,
    pub has_json_info: bool,
    pub account_id: Option<String>,
    pub plan_type: String,
    pub primary_used_percent: i32,
    pub primary_reset_at: i64,
    pub primary_window_minutes: Option<i64>,
    pub primary_window_present: bool,
    pub secondary_used_percent: i32,
    pub secondary_reset_at: i64,
    pub secondary_window_minutes: Option<i64>,
    pub secondary_window_present: bool,
    pub last_quota_checked_at: String,
    pub last_quota_error: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodexHotSwitchResult {
    pub status: String,
    pub message: String,
    pub detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwitchAccountResult {
    pub restarted: bool,
    pub auth_json_path: String,
    pub hot_switch: CodexHotSwitchResult,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupAccount {
    name: String,
    activation_date: String,
    json_info: String,
    plan_type: String,
    primary_used_percent: i32,
    primary_reset_at: i64,
    #[serde(default)]
    primary_window_minutes: Option<i64>,
    #[serde(default = "default_quota_window_present")]
    primary_window_present: bool,
    secondary_used_percent: i32,
    secondary_reset_at: i64,
    #[serde(default)]
    secondary_window_minutes: Option<i64>,
    #[serde(default = "default_quota_window_present")]
    secondary_window_present: bool,
    #[serde(default)]
    last_quota_checked_at: String,
    #[serde(default)]
    last_quota_error: String,
}

fn default_quota_window_present() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupPayload {
    version: u32,
    accounts: Vec<BackupAccount>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackupPreview {
    pub version: u32,
    pub total_accounts: usize,
    pub duplicate_accounts: usize,
    pub new_accounts: usize,
    pub account_names: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImportBackupResult {
    pub imported: usize,
    pub skipped: usize,
    pub updated: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedBackup {
    format: String,
    version: u32,
    kdf: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Clone)]
struct OAuthState {
    login_id: String,
    auth_url: String,
    state: String,
    code_verifier: String,
    redirect_uri: String,
    expires_at: i64,
    code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStartResponse {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OAuthSaveResult {
    pub id: i64,
    pub created: bool,
    pub name: String,
    pub account_id: String,
}

#[derive(Debug, Clone)]
struct AccountIdentity {
    email: Option<String>,
    account_id: Option<String>,
    plan_type: Option<String>,
    account_name: Option<String>,
}

// ── Helper functions ──────────────────────────────────────────────────

const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_AUTH_ENDPOINT: &str = "https://auth.openai.com/oauth/authorize";
const CODEX_TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const CODEX_ACCOUNT_CHECK_ENDPOINT: &str = "https://chatgpt.com/backend-api/wham/accounts/check";
const CODEX_OAUTH_SCOPES: &str = "openid profile email offline_access";
const CODEX_OAUTH_ORIGINATOR: &str = "codex_vscode";
const CODEX_OAUTH_CALLBACK_PORT: u16 = 1455;
const TOKEN_REFRESH_SKEW_SECONDS: i64 = 300;
const CODEX_CONFIG_FILE: &str = "config.toml";
const CODEX_GLOBAL_STATE_FILE: &str = ".codex-global-state.json";
const CODEX_GOALS_DB_FILE: &str = "goals_1.sqlite";
const CODEX_STATE_DB_FILE: &str = "state_5.sqlite";
const CODEX_SESSION_INDEX_FILE: &str = "session_index.jsonl";
const CODEX_SESSIONS_DIR: &str = "sessions";
const CODEX_ARCHIVED_SESSIONS_DIR: &str = "archived_sessions";
const CODEX_FEATURES_SECTION: &str = "features";
const CODEX_MEMORIES_SECTION: &str = "memories";
const CODEX_DESKTOP_SECTION: &str = "desktop";
const CODEX_SERVICE_TIER_KEY: &str = "default-service-tier";
const CODEX_PRIORITY_SERVICE_TIER: &str = "priority";
const CODEX_ATOM_STATE_KEY: &str = "electron-persisted-atom-state";
const CODEX_USER_CHANGED_TIER_KEY: &str = "has-user-changed-service-tier";
const CODEX_PROJECTS_SECTION_PREFIX: &str = "projects.";
const CODEX_TRUST_LEVEL_KEY: &str = "trust_level";
const CODEX_TRUSTED_LEVEL: &str = "trusted";
const CODEX_PROXY_PROVIDER_ID: &str = "codex_account_manager_proxy";
const CODEX_PROXY_PROVIDER_NAME: &str = "Codex Account Manager Proxy";
const CODEX_PROXY_DEFAULT_PORT: u16 = 14998;
const CODEX_PROXY_UPSTREAM_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const CODEX_PROXY_SETTING_ENABLED: &str = "codex_proxy_enabled";
const CODEX_PROXY_SETTING_PORT: &str = "codex_proxy_port";
const CODEX_PROXY_SETTING_ACTIVE_ACCOUNT_ID: &str = "codex_proxy_active_account_id";
const CODEX_PROXY_SETTING_CONFIG_BACKUP: &str = "codex_proxy_config_backup";
const CODEX_PROXY_DEFAULT_USER_AGENT: &str =
    "codex-tui/0.135.0 (Mac OS; arm64) CodexAccountManagerProxy/0.1.0";
const CODEX_PROXY_DEFAULT_ORIGINATOR: &str = "codex-tui";

static OAUTH_STATE: Mutex<Option<OAuthState>> = Mutex::new(None);

struct CodexProxyRuntime {
    port: u16,
    stop: Arc<AtomicBool>,
    last_error: Arc<Mutex<String>>,
}

static CODEX_PROXY_RUNTIME: Mutex<Option<CodexProxyRuntime>> = Mutex::new(None);

fn get_home_dir() -> Result<String, String> {
    if cfg!(target_os = "windows") {
        std::env::var("USERPROFILE").map_err(|_| "Cannot find home directory".to_string())
    } else {
        std::env::var("HOME").map_err(|_| "Cannot find home directory".to_string())
    }
}

fn get_auth_path() -> Result<std::path::PathBuf, String> {
    let home = get_home_dir()?;
    Ok(std::path::PathBuf::from(home)
        .join(".codex")
        .join("auth.json"))
}

fn get_codex_home_path() -> Result<std::path::PathBuf, String> {
    Ok(std::path::PathBuf::from(get_home_dir()?).join(".codex"))
}

fn get_codex_config_path() -> Result<std::path::PathBuf, String> {
    Ok(get_codex_home_path()?.join(CODEX_CONFIG_FILE))
}

fn get_codex_global_state_path() -> Result<std::path::PathBuf, String> {
    Ok(get_codex_home_path()?.join(CODEX_GLOBAL_STATE_FILE))
}

fn get_codex_logs_path() -> Result<std::path::PathBuf, String> {
    Ok(get_codex_home_path()?.join("logs_2.sqlite"))
}

fn local_day_bounds_ts() -> Result<(i64, i64), String> {
    let now = chrono::Local::now();
    let today = now.date_naive();
    let start_naive = today
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "Failed to build local day start".to_string())?;
    let end_naive = (today + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "Failed to build local day end".to_string())?;
    let start = match start_naive.and_local_timezone(chrono::Local) {
        chrono::LocalResult::Single(value) => value.timestamp(),
        chrono::LocalResult::Ambiguous(a, b) => a.timestamp().min(b.timestamp()),
        chrono::LocalResult::None => now.timestamp(),
    };
    let end = match end_naive.and_local_timezone(chrono::Local) {
        chrono::LocalResult::Single(value) => value.timestamp(),
        chrono::LocalResult::Ambiguous(a, b) => a.timestamp().max(b.timestamp()),
        chrono::LocalResult::None => start + 86_400,
    };
    Ok((start, end.max(start)))
}

fn extract_log_value<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let start = body.find(key)? + key.len();
    let rest = &body[start..];
    let end = rest
        .find(|ch: char| ch.is_whitespace() || ch == '}' || ch == ',' || ch == ')')
        .unwrap_or(rest.len());
    let value = rest[..end].trim_matches('"').trim();
    (!value.is_empty()).then_some(value)
}

fn extract_json_string(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":\"", key);
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let end = rest.find('"')?;
    let value = rest[..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn extract_log_i64(body: &str, key: &str) -> i64 {
    extract_log_value(body, key)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0)
}

fn normalized_model(model: &str) -> String {
    model.trim().to_ascii_lowercase()
}

fn model_api_prices_per_1m(model: &str, input_tokens: i64) -> Option<(f64, f64, f64)> {
    let model = normalized_model(model);
    if model.starts_with("gpt-5.5-pro") {
        return if input_tokens >= 270_000 {
            Some((60.0, 60.0, 270.0))
        } else {
            Some((30.0, 30.0, 180.0))
        };
    }
    if model == "gpt-5.5" || model.starts_with("gpt-5.5-") {
        return if input_tokens >= 270_000 {
            Some((10.0, 1.0, 45.0))
        } else {
            Some((5.0, 0.5, 30.0))
        };
    }
    if model.starts_with("gpt-5.4-pro") {
        return if input_tokens >= 270_000 {
            Some((60.0, 60.0, 270.0))
        } else {
            Some((30.0, 30.0, 180.0))
        };
    }
    if model == "gpt-5.4" || model.starts_with("gpt-5.4-") {
        if model.starts_with("gpt-5.4-mini") {
            return Some((0.75, 0.075, 4.5));
        }
        if model.starts_with("gpt-5.4-nano") {
            return Some((0.2, 0.02, 1.25));
        }
        return if input_tokens >= 270_000 {
            Some((5.0, 0.5, 22.5))
        } else {
            Some((2.5, 0.25, 15.0))
        };
    }
    if model.starts_with("gpt-5-codex-mini") || model.starts_with("gpt-5.1-codex-mini") {
        return Some((0.25, 0.025, 2.0));
    }
    if model.starts_with("gpt-5-codex")
        || model.starts_with("gpt-5.1-codex")
        || model == "gpt-5"
        || model.starts_with("gpt-5-")
    {
        return Some((1.25, 0.125, 10.0));
    }
    None
}

fn model_codex_credit_prices_per_1m(model: &str) -> Option<(f64, f64, f64)> {
    let model = normalized_model(model);
    if model.starts_with("gpt-5.5-pro") {
        return Some((750.0, 750.0, 4_500.0));
    }
    if model == "gpt-5.5" || model.starts_with("gpt-5.5-") {
        return Some((125.0, 12.5, 750.0));
    }
    if model.starts_with("gpt-5.4-pro") {
        return Some((750.0, 750.0, 4_500.0));
    }
    if model == "gpt-5.4" || model.starts_with("gpt-5.4-") {
        return Some((62.5, 6.25, 375.0));
    }
    None
}

fn estimate_weighted_cost(
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    prices_per_1m: Option<(f64, f64, f64)>,
) -> f64 {
    let Some((input_price, cached_price, output_price)) = prices_per_1m else {
        return 0.0;
    };
    let input = input_tokens.max(0) as f64;
    let cached = (cached_input_tokens.max(0) as f64).min(input);
    let billable_input = (input - cached).max(0.0);
    let output = output_tokens.max(0) as f64;
    billable_input / 1_000_000.0 * input_price
        + cached / 1_000_000.0 * cached_price
        + output / 1_000_000.0 * output_price
}

#[derive(Debug, Clone)]
struct ParsedCodexTurnUsage {
    ts: i64,
    turn_id: String,
    model: String,
    input_tokens: i64,
    cached_input_tokens: i64,
    non_cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
}

fn parse_codex_turn_usage(ts: i64, body: &str) -> Option<ParsedCodexTurnUsage> {
    if !body.contains("codex.turn.token_usage.") {
        return None;
    }
    let turn_id = extract_log_value(body, "turn.id=")
        .or_else(|| extract_log_value(body, "submission.id="))
        .unwrap_or("unknown-turn")
        .to_string();
    let model = extract_log_value(body, "model=")
        .map(str::to_string)
        .unwrap_or_else(|| "unknown".to_string());
    let input_tokens = extract_log_i64(body, "codex.turn.token_usage.input_tokens=");
    let cached_input_tokens = extract_log_i64(body, "codex.turn.token_usage.cached_input_tokens=");
    let non_cached_input_tokens =
        extract_log_i64(body, "codex.turn.token_usage.non_cached_input_tokens=");
    let output_tokens = extract_log_i64(body, "codex.turn.token_usage.output_tokens=");
    let reasoning_output_tokens =
        extract_log_i64(body, "codex.turn.token_usage.reasoning_output_tokens=");
    let total_tokens = extract_log_i64(body, "codex.turn.token_usage.total_tokens=");
    if input_tokens == 0
        && cached_input_tokens == 0
        && non_cached_input_tokens == 0
        && output_tokens == 0
        && reasoning_output_tokens == 0
        && total_tokens == 0
    {
        return None;
    }
    Some(ParsedCodexTurnUsage {
        ts,
        turn_id,
        model,
        input_tokens,
        cached_input_tokens,
        non_cached_input_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
    })
}

fn parse_codex_failure(ts: i64, body: &str) -> Option<CodexUsageFailure> {
    let failed = body.contains("response.failed")
        || body.contains("\"status\":\"failed\"")
        || body.contains("\"status\":\"incomplete\"");
    if !failed {
        return None;
    }
    let response_id = extract_json_string(body, "id")
        .or_else(|| extract_log_value(body, "response.id=").map(str::to_string))
        .unwrap_or_default();
    let turn_id = extract_log_value(body, "turn.id=")
        .or_else(|| extract_log_value(body, "submission.id="))
        .unwrap_or_default()
        .to_string();
    let model = extract_log_value(body, "model=")
        .map(str::to_string)
        .or_else(|| extract_json_string(body, "model"))
        .unwrap_or_else(|| "unknown".to_string());
    let status = if body.contains("\"status\":\"incomplete\"") {
        "incomplete"
    } else {
        "failed"
    }
    .to_string();
    let message = extract_json_string(body, "message")
        .or_else(|| extract_json_string(body, "code"))
        .unwrap_or_else(|| truncate_log_text(body, 240));
    Some(CodexUsageFailure {
        ts,
        model,
        turn_id,
        response_id,
        status,
        message,
    })
}

fn add_usage_to_rollup(rollup: &mut CodexUsageRollup, usage: &ParsedCodexTurnUsage) {
    rollup.success_count += 1;
    rollup.request_count += 1;
    rollup.input_tokens += usage.input_tokens;
    rollup.cached_input_tokens += usage.cached_input_tokens;
    rollup.non_cached_input_tokens += if usage.non_cached_input_tokens > 0 {
        usage.non_cached_input_tokens
    } else {
        (usage.input_tokens - usage.cached_input_tokens).max(0)
    };
    rollup.output_tokens += usage.output_tokens;
    rollup.reasoning_output_tokens += usage.reasoning_output_tokens;
    rollup.total_tokens += if usage.total_tokens > 0 {
        usage.total_tokens
    } else {
        usage.input_tokens + usage.output_tokens
    };
    rollup.api_cost_usd += estimate_weighted_cost(
        usage.input_tokens,
        usage.cached_input_tokens,
        usage.output_tokens,
        model_api_prices_per_1m(&usage.model, usage.input_tokens),
    );
    rollup.codex_credits += estimate_weighted_cost(
        usage.input_tokens,
        usage.cached_input_tokens,
        usage.output_tokens,
        model_codex_credit_prices_per_1m(&usage.model),
    );
}

fn strip_toml_comment(line: &str) -> &str {
    line.split('#').next().unwrap_or(line).trim()
}

fn toml_section_name(line: &str) -> Option<&str> {
    let trimmed = strip_toml_comment(line);
    trimmed
        .strip_prefix('[')
        .and_then(|item| item.strip_suffix(']'))
        .map(str::trim)
}

fn toml_string_value_for_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let trimmed = strip_toml_comment(line);
    let (left, right) = trimmed.split_once('=')?;
    if left.trim() != key {
        return None;
    }
    Some(right.trim().trim_matches('"'))
}

fn normalize_service_tier_speed(value: Option<&str>) -> CodexAppSpeed {
    match value {
        Some("fast") | Some("priority") | Some("flex") => CodexAppSpeed::Fast,
        _ => CodexAppSpeed::Standard,
    }
}

fn read_service_tier_from_config(content: &str) -> Option<String> {
    let mut in_desktop = false;
    for line in content.lines() {
        if let Some(section) = toml_section_name(line) {
            in_desktop = section == CODEX_DESKTOP_SECTION;
            continue;
        }
        if in_desktop {
            if let Some(value) = toml_string_value_for_key(line, CODEX_SERVICE_TIER_KEY) {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn service_tier_line() -> String {
    format!(
        "{} = \"{}\"",
        CODEX_SERVICE_TIER_KEY, CODEX_PRIORITY_SERVICE_TIER
    )
}

fn trusted_project_line() -> String {
    format!("{} = \"{}\"", CODEX_TRUST_LEVEL_KEY, CODEX_TRUSTED_LEVEL)
}

fn project_section_header(project_path: &str) -> String {
    format!("[projects.'{}']", project_path.replace('\'', "\\'"))
}

fn normalize_project_path_for_match(path: &str) -> String {
    let normalized = path
        .replace('/', "\\")
        .trim()
        .trim_end_matches('\\')
        .to_string();
    let looks_like_windows_drive_path = normalized
        .as_bytes()
        .get(1)
        .is_some_and(|byte| *byte == b':');
    if cfg!(target_os = "windows") || looks_like_windows_drive_path {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn section_project_path(section: &str) -> Option<String> {
    let raw = section.strip_prefix(CODEX_PROJECTS_SECTION_PREFIX)?.trim();
    let value = if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        raw[1..raw.len() - 1].to_string()
    } else if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        raw[1..raw.len() - 1].replace("\\\\", "\\")
    } else {
        raw.to_string()
    };
    Some(value)
}

fn toml_value_for_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let trimmed = strip_toml_comment(line);
    let (left, right) = trimmed.split_once('=')?;
    if left.trim() != key {
        return None;
    }
    Some(right.trim().trim_matches('"'))
}

fn toml_bool_value_for_key(line: &str, key: &str) -> Option<bool> {
    match toml_value_for_key(line, key)?.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn read_toml_bool_in_section(content: &str, section_name: &str, key: &str) -> bool {
    let mut in_section = false;
    for line in content.lines() {
        if let Some(section) = toml_section_name(line) {
            in_section = section == section_name;
            continue;
        }
        if in_section {
            if let Some(value) = toml_bool_value_for_key(line, key) {
                return value;
            }
        }
    }
    false
}

fn is_official_mode_provider_section(section: &str) -> bool {
    let normalized = section.trim().to_ascii_lowercase();
    normalized == "provider"
        || normalized == "providers"
        || normalized == "model_provider"
        || normalized == "model-providers"
        || normalized == "model_providers"
        || normalized.starts_with("provider.")
        || normalized.starts_with("providers.")
        || normalized.starts_with("model_provider.")
        || normalized.starts_with("model-providers.")
        || normalized.starts_with("model_providers.")
}

fn is_official_mode_forbidden_key(key: &str) -> bool {
    matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "provider"
            | "model_provider"
            | "model-provider"
            | "base_url"
            | "base-url"
            | "api_base"
            | "api-base"
            | "api_base_url"
            | "api-base-url"
            | "proxy"
            | "http_proxy"
            | "https_proxy"
    )
}

fn official_mode_issues_from_config(content: &str) -> Vec<CodexOfficialModeIssue> {
    let mut issues = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        if let Some(section) = toml_section_name(line) {
            if is_official_mode_provider_section(section) {
                issues.push(CodexOfficialModeIssue {
                    line: line_number,
                    label: format!("[{}]", section),
                });
            }
            continue;
        }

        let trimmed = strip_toml_comment(line);
        if let Some((left, _)) = trimmed.split_once('=') {
            let key = left.trim();
            if is_official_mode_forbidden_key(key) {
                issues.push(CodexOfficialModeIssue {
                    line: line_number,
                    label: key.to_string(),
                });
            }
        }
    }
    issues
}

fn is_project_trusted_in_config(content: &str, project_path: &str) -> bool {
    let target = normalize_project_path_for_match(project_path);
    let mut in_target = false;

    for line in content.lines() {
        if let Some(section) = toml_section_name(line) {
            in_target = section_project_path(section)
                .map(|path| normalize_project_path_for_match(&path) == target)
                .unwrap_or(false);
            continue;
        }

        if in_target {
            if let Some(value) = toml_value_for_key(line, CODEX_TRUST_LEVEL_KEY) {
                return value == CODEX_TRUSTED_LEVEL;
            }
        }
    }

    false
}

fn codex_config_toml_with_trusted_project(content: &str, project_path: &str) -> (String, bool) {
    // Guardrail: project visibility repair is only allowed to add or set
    // [projects.'...'].trust_level = "trusted" for the requested path.
    if is_project_trusted_in_config(content, project_path) {
        return (content.to_string(), false);
    }

    let target = normalize_project_path_for_match(project_path);
    let mut output: Vec<String> = Vec::new();
    let mut in_target = false;
    let mut target_found = false;
    let mut trust_written = false;

    for line in content.lines() {
        if let Some(section) = toml_section_name(line) {
            if in_target && !trust_written {
                output.push(trusted_project_line());
                trust_written = true;
            }
            in_target = section_project_path(section)
                .map(|path| normalize_project_path_for_match(&path) == target)
                .unwrap_or(false);
            target_found |= in_target;
            output.push(line.to_string());
            continue;
        }

        if in_target && toml_value_for_key(line, CODEX_TRUST_LEVEL_KEY).is_some() {
            output.push(trusted_project_line());
            trust_written = true;
            continue;
        }

        output.push(line.to_string());
    }

    if in_target && !trust_written {
        output.push(trusted_project_line());
    }

    if !target_found {
        if !output.is_empty() && output.last().is_some_and(|line| !line.trim().is_empty()) {
            output.push(String::new());
        }
        output.push(project_section_header(project_path));
        output.push(trusted_project_line());
    }

    let mut next = output.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }
    (next, true)
}

// Guardrail: this app manages official Codex accounts. Do not add provider,
// proxy, base_url, or model-provider settings here. Fast/standard may only
// touch [desktop].default-service-tier and must preserve all other config.toml
// content such as model, MCP servers, memories, features, plugins, and projects.
fn codex_config_toml_with_speed(content: &str, speed: &CodexAppSpeed) -> String {
    let mut output: Vec<String> = Vec::new();
    let mut in_desktop = false;
    let mut desktop_found = false;
    let mut tier_written = false;

    for line in content.lines() {
        if let Some(section) = toml_section_name(line) {
            if in_desktop && matches!(speed, CodexAppSpeed::Fast) && !tier_written {
                output.push(service_tier_line());
                tier_written = true;
            }
            in_desktop = section == CODEX_DESKTOP_SECTION;
            desktop_found |= in_desktop;
            output.push(line.to_string());
            continue;
        }

        if in_desktop && toml_string_value_for_key(line, CODEX_SERVICE_TIER_KEY).is_some() {
            if matches!(speed, CodexAppSpeed::Fast) && !tier_written {
                output.push(service_tier_line());
                tier_written = true;
            }
            continue;
        }

        output.push(line.to_string());
    }

    if in_desktop && matches!(speed, CodexAppSpeed::Fast) && !tier_written {
        output.push(service_tier_line());
    }

    if !desktop_found && matches!(speed, CodexAppSpeed::Fast) {
        if !output.is_empty() && output.last().is_some_and(|line| !line.trim().is_empty()) {
            output.push(String::new());
        }
        output.push(format!("[{}]", CODEX_DESKTOP_SECTION));
        output.push(service_tier_line());
    }

    let mut next = output.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }
    next
}

fn codex_proxy_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{}/v1", port)
}

fn root_toml_string_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        if toml_section_name(line).is_some() {
            return None;
        }
        if let Some(value) = toml_string_value_for_key(line, key) {
            return Some(value.to_string());
        }
    }
    None
}

fn remove_root_toml_keys(content: &str, keys: &[&str]) -> String {
    let mut output = Vec::new();
    let mut in_section = false;
    for line in content.lines() {
        if toml_section_name(line).is_some() {
            in_section = true;
            output.push(line.to_string());
            continue;
        }

        if !in_section {
            let trimmed = strip_toml_comment(line);
            if let Some((left, _)) = trimmed.split_once('=') {
                if keys.iter().any(|key| left.trim() == *key) {
                    continue;
                }
            }
        }

        output.push(line.to_string());
    }
    output.join("\n")
}

fn remove_toml_section(content: &str, section_name: &str) -> String {
    let mut output = Vec::new();
    let mut skip = false;
    for line in content.lines() {
        if let Some(section) = toml_section_name(line) {
            skip = section == section_name;
            if skip {
                continue;
            }
        }
        if !skip {
            output.push(line.to_string());
        }
    }
    output.join("\n")
}

fn codex_proxy_provider_section_name() -> String {
    format!("model_providers.{}", CODEX_PROXY_PROVIDER_ID)
}

fn codex_proxy_config_installed(content: &str) -> bool {
    root_toml_string_value(content, "model_provider").as_deref() == Some(CODEX_PROXY_PROVIDER_ID)
        && content.contains(&format!("[model_providers.{}]", CODEX_PROXY_PROVIDER_ID))
}

fn codex_proxy_config_toml(content: &str, port: u16) -> String {
    let without_root = remove_root_toml_keys(content, &["model_provider", "openai_base_url"]);
    let without_provider =
        remove_toml_section(&without_root, codex_proxy_provider_section_name().as_str());
    let preserved = without_provider.trim();
    let provider = format!(
        "model_provider = \"{}\"\n\n[model_providers.{}]\nname = \"{}\"\nbase_url = \"{}\"\nwire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\n",
        CODEX_PROXY_PROVIDER_ID,
        CODEX_PROXY_PROVIDER_ID,
        CODEX_PROXY_PROVIDER_NAME,
        codex_proxy_base_url(port),
    );

    if preserved.is_empty() {
        provider
    } else {
        format!("{}\n\n{}\n", provider.trim_end(), preserved)
    }
}

fn remove_codex_proxy_config_toml(content: &str) -> String {
    let mut next = remove_toml_section(content, codex_proxy_provider_section_name().as_str());
    if root_toml_string_value(&next, "model_provider").as_deref() == Some(CODEX_PROXY_PROVIDER_ID) {
        next = remove_root_toml_keys(&next, &["model_provider"]);
    }
    let trimmed = next.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}\n", trimmed)
    }
}

fn install_codex_proxy_config_to_path(
    config_path: &std::path::Path,
    conn: &Connection,
    port: u16,
) -> Result<(), String> {
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("读取 Codex config.toml 失败: {}", e)),
    };

    if read_setting_from_conn(conn, CODEX_PROXY_SETTING_CONFIG_BACKUP)?.is_none() {
        write_setting_to_conn(conn, CODEX_PROXY_SETTING_CONFIG_BACKUP, &content)?;
    }

    let next = codex_proxy_config_toml(&content, port);
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 Codex 配置目录失败: {}", e))?;
    }
    std::fs::write(config_path, next).map_err(|e| format!("写入 Codex config.toml 失败: {}", e))
}

fn restore_codex_proxy_config_from_backup(
    config_path: &std::path::Path,
    conn: &Connection,
) -> Result<(), String> {
    if let Some(backup) = read_setting_from_conn(conn, CODEX_PROXY_SETTING_CONFIG_BACKUP)? {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建 Codex 配置目录失败: {}", e))?;
        }
        std::fs::write(config_path, backup)
            .map_err(|e| format!("恢复 Codex config.toml 失败: {}", e))?;
        delete_setting_from_conn(conn, CODEX_PROXY_SETTING_CONFIG_BACKUP)?;
        return Ok(());
    }

    let content = std::fs::read_to_string(config_path).unwrap_or_default();
    let next = remove_codex_proxy_config_toml(&content);
    std::fs::write(config_path, next).map_err(|e| format!("清理 Codex 代理配置失败: {}", e))
}

fn read_codex_app_speed_from_path(path: &std::path::Path) -> Result<CodexAppSpeed, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(CodexAppSpeed::Standard),
        Err(e) => return Err(format!("读取 Codex config.toml 失败: {}", e)),
    };
    Ok(normalize_service_tier_speed(
        read_service_tier_from_config(&content).as_deref(),
    ))
}

fn read_codex_global_state_tier(path: &std::path::Path) -> Result<(Option<String>, bool), String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((None, false)),
        Err(e) => return Err(format!("读取 Codex 全局状态失败: {}", e)),
    };
    let value = serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|e| format!("解析 Codex 全局状态失败: {}", e))?;
    let atom_state = value
        .get(CODEX_ATOM_STATE_KEY)
        .and_then(|item| item.as_object());
    let service_tier = atom_state
        .and_then(|item| item.get(CODEX_SERVICE_TIER_KEY))
        .and_then(|item| item.as_str())
        .map(str::to_string);
    let user_changed_tier = atom_state
        .and_then(|item| item.get(CODEX_USER_CHANGED_TIER_KEY))
        .and_then(|item| item.as_bool())
        .unwrap_or(false);
    Ok((service_tier, user_changed_tier))
}

fn sync_codex_global_state(path: &std::path::Path, speed: &CodexAppSpeed) -> Result<(), String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(e) => return Err(format!("读取 Codex 全局状态失败: {}", e)),
    };
    let mut state = serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let atom_state = state
        .entry(CODEX_ATOM_STATE_KEY.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !atom_state.is_object() {
        *atom_state = serde_json::Value::Object(serde_json::Map::new());
    }
    let atom_state = atom_state
        .as_object_mut()
        .ok_or_else(|| "Codex 全局状态格式异常".to_string())?;
    let tier_value = match speed {
        CodexAppSpeed::Fast => serde_json::Value::String(CODEX_PRIORITY_SERVICE_TIER.to_string()),
        CodexAppSpeed::Standard => serde_json::Value::Null,
    };
    atom_state.insert(CODEX_SERVICE_TIER_KEY.to_string(), tier_value);
    atom_state.insert(
        CODEX_USER_CHANGED_TIER_KEY.to_string(),
        serde_json::Value::Bool(true),
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 Codex 配置目录失败: {}", e))?;
    }
    let next = serde_json::to_string_pretty(&serde_json::Value::Object(state))
        .map_err(|e| format!("序列化 Codex 全局状态失败: {}", e))?;
    std::fs::write(path, next).map_err(|e| format!("写入 Codex 全局状态失败: {}", e))
}

fn write_codex_app_speed_to_path(
    config_path: &std::path::Path,
    global_state_path: &std::path::Path,
    speed: CodexAppSpeed,
) -> Result<CodexAppSpeedConfig, String> {
    // Guardrail: keep this as a surgical [desktop].default-service-tier update.
    // Do not rewrite official-mode config.toml into a provider/proxy config.
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("读取 Codex config.toml 失败: {}", e)),
    };
    let current_service_tier = read_service_tier_from_config(&content);
    let should_update_config = match speed {
        CodexAppSpeed::Fast => {
            normalize_service_tier_speed(current_service_tier.as_deref()) != CodexAppSpeed::Fast
        }
        CodexAppSpeed::Standard => current_service_tier.is_some(),
    };
    if should_update_config {
        let next = codex_config_toml_with_speed(&content, &speed);
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建 Codex 配置目录失败: {}", e))?;
        }
        std::fs::write(config_path, next)
            .map_err(|e| format!("写入 Codex config.toml 失败: {}", e))?;
    }
    sync_codex_global_state(global_state_path, &speed)?;
    Ok(CodexAppSpeedConfig {
        speed,
        config_path: config_path.to_string_lossy().to_string(),
        global_state_path: global_state_path.to_string_lossy().to_string(),
    })
}

fn read_codex_feature_status_from_paths(
    config_path: &std::path::Path,
    global_state_path: &std::path::Path,
) -> Result<CodexFeatureStatus, String> {
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("读取 Codex config.toml 失败: {}", e)),
    };
    let config_service_tier = read_service_tier_from_config(&content);
    let config_speed = normalize_service_tier_speed(config_service_tier.as_deref());
    let (global_state_service_tier, global_state_user_changed_tier) =
        read_codex_global_state_tier(global_state_path)?;
    let global_state_speed = normalize_service_tier_speed(global_state_service_tier.as_deref());
    let official_mode_issues = official_mode_issues_from_config(&content);
    let goals_db_path = get_codex_home_path()?.join(CODEX_GOALS_DB_FILE);
    let fast_state_synced = match config_speed {
        CodexAppSpeed::Fast => global_state_speed == CodexAppSpeed::Fast,
        CodexAppSpeed::Standard => global_state_service_tier.is_none(),
    };

    Ok(CodexFeatureStatus {
        config_path: config_path.to_string_lossy().to_string(),
        global_state_path: global_state_path.to_string_lossy().to_string(),
        goals_db_path: goals_db_path.to_string_lossy().to_string(),
        goals_enabled: read_toml_bool_in_section(&content, CODEX_FEATURES_SECTION, "goals"),
        goals_db_present: goals_db_path.exists(),
        memory_generate_enabled: read_toml_bool_in_section(
            &content,
            CODEX_MEMORIES_SECTION,
            "generate_memories",
        ),
        memory_use_enabled: read_toml_bool_in_section(
            &content,
            CODEX_MEMORIES_SECTION,
            "use_memories",
        ),
        official_mode_ok: official_mode_issues.is_empty(),
        official_mode_issues,
        config_speed,
        config_service_tier,
        global_state_speed,
        global_state_service_tier,
        global_state_user_changed_tier,
        fast_state_synced,
    })
}

fn get_database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot find app data directory: {}", e))?;
    Ok(app_data_dir.join("codex_accounts.db"))
}

fn open_accounts_db(app: &AppHandle) -> Result<Connection, String> {
    let db_path = get_database_path(app)?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    }

    let conn = Connection::open(db_path).map_err(|e| format!("Failed to open database: {}", e))?;
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS accounts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            activation_date TEXT DEFAULT '',
            json_info TEXT NOT NULL DEFAULT '',
            plan_type TEXT DEFAULT 'unknown',
            primary_used_percent INTEGER DEFAULT 0,
            primary_reset_at INTEGER DEFAULT 0,
            secondary_used_percent INTEGER DEFAULT 0,
            secondary_reset_at INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS operation_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            level TEXT NOT NULL,
            action TEXT NOT NULL,
            account_id INTEGER,
            account_name TEXT DEFAULT '',
            account_identifier TEXT DEFAULT '',
            stage TEXT NOT NULL,
            message TEXT NOT NULL,
            details TEXT DEFAULT '',
            created_at TEXT DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_operation_logs_created_at ON operation_logs(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_operation_logs_account_id ON operation_logs(account_id);
        ",
    )
    .map_err(|e| format!("Failed to initialize database: {}", e))?;
    ensure_column(
        &conn,
        "accounts",
        "credential_key",
        "ALTER TABLE accounts ADD COLUMN credential_key TEXT DEFAULT ''",
    )?;
    ensure_column(
        &conn,
        "accounts",
        "last_quota_checked_at",
        "ALTER TABLE accounts ADD COLUMN last_quota_checked_at TEXT DEFAULT ''",
    )?;
    ensure_column(
        &conn,
        "accounts",
        "last_quota_error",
        "ALTER TABLE accounts ADD COLUMN last_quota_error TEXT DEFAULT ''",
    )?;
    ensure_column(
        &conn,
        "accounts",
        "primary_window_minutes",
        "ALTER TABLE accounts ADD COLUMN primary_window_minutes INTEGER",
    )?;
    ensure_column(
        &conn,
        "accounts",
        "primary_window_present",
        "ALTER TABLE accounts ADD COLUMN primary_window_present INTEGER DEFAULT 1",
    )?;
    ensure_column(
        &conn,
        "accounts",
        "secondary_window_minutes",
        "ALTER TABLE accounts ADD COLUMN secondary_window_minutes INTEGER",
    )?;
    ensure_column(
        &conn,
        "accounts",
        "secondary_window_present",
        "ALTER TABLE accounts ADD COLUMN secondary_window_present INTEGER DEFAULT 1",
    )?;

    Ok(conn)
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| format!("Failed to inspect database schema: {}", e))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("Failed to inspect database columns: {}", e))?;

    for item in columns {
        if item.map_err(|e| format!("Failed to read database column: {}", e))? == column {
            return Ok(());
        }
    }

    conn.execute(alter_sql, [])
        .map_err(|e| format!("Failed to migrate database schema: {}", e))?;
    Ok(())
}

fn read_setting_from_conn(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    let result = conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to read setting: {}", e)),
    }
}

fn write_setting_to_conn(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "
        INSERT INTO app_settings (key, value)
        VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        ",
        params![key, value],
    )
    .map_err(|e| format!("Failed to save setting: {}", e))?;
    Ok(())
}

fn delete_setting_from_conn(conn: &Connection, key: &str) -> Result<(), String> {
    conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])
        .map_err(|e| format!("Failed to delete setting: {}", e))?;
    Ok(())
}

fn account_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let account_id: String = row.get(17)?;
    Ok(Account {
        id: row.get(0)?,
        name: row.get(1)?,
        activation_date: row.get(2)?,
        has_json_info: row.get::<_, i64>(3)? == 1,
        account_id: if account_id.is_empty() {
            None
        } else {
            Some(account_id)
        },
        plan_type: row.get(4)?,
        primary_used_percent: row.get(5)?,
        primary_reset_at: row.get(6)?,
        primary_window_minutes: row.get(7)?,
        primary_window_present: row.get::<_, i64>(8)? != 0,
        secondary_used_percent: row.get(9)?,
        secondary_reset_at: row.get(10)?,
        secondary_window_minutes: row.get(11)?,
        secondary_window_present: row.get::<_, i64>(12)? != 0,
        last_quota_checked_at: row.get(13)?,
        last_quota_error: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn operation_log_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OperationLog> {
    Ok(OperationLog {
        id: row.get(0)?,
        level: row.get(1)?,
        action: row.get(2)?,
        account_id: row.get(3)?,
        account_name: row.get(4)?,
        account_identifier: row.get(5)?,
        stage: row.get(6)?,
        message: row.get(7)?,
        details: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn truncate_log_text(value: &str, max_chars: usize) -> String {
    let mut output: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

fn account_log_context(conn: &Connection, id: i64) -> (String, String) {
    conn.query_row(
        "
        SELECT name,
               CASE
                   WHEN json_valid(json_info)
                   THEN COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
                   ELSE ''
               END
        FROM accounts
        WHERE id = ?1
        ",
        params![id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .unwrap_or_else(|_| (format!("#{}", id), String::new()))
}

fn insert_operation_log(
    conn: &Connection,
    level: &str,
    action: &str,
    account_id: Option<i64>,
    account_name: &str,
    account_identifier: &str,
    stage: &str,
    message: &str,
    details: &str,
) -> Result<(), String> {
    conn.execute(
        "
        INSERT INTO operation_logs (
            level, action, account_id, account_name, account_identifier,
            stage, message, details
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ",
        params![
            level,
            action,
            account_id,
            account_name,
            account_identifier,
            stage,
            truncate_log_text(message, 1000),
            truncate_log_text(details, 6000),
        ],
    )
    .map_err(|e| format!("Failed to write operation log: {}", e))?;
    Ok(())
}

fn quota_log_details(
    status: &str,
    content_type: &str,
    content_encoding: &str,
    elapsed_ms: u128,
    body_preview: &str,
) -> String {
    serde_json::json!({
        "status": status,
        "content_type": content_type,
        "content_encoding": content_encoding,
        "elapsed_ms": elapsed_ms,
        "body_preview": body_preview,
    })
    .to_string()
}

fn missing_account_credential_message() -> String {
    "该账号没有保存在本地账号库的 auth.json，请重新 OAuth 授权，或编辑该账号重新粘贴 auth.json/token。"
        .to_string()
}

fn extract_account_id(json_info: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json_info)
        .ok()
        .and_then(|value| {
            value
                .pointer("/tokens/account_id")
                .and_then(|id| id.as_str())
                .map(ToString::to_string)
        })
}

#[cfg(test)]
fn account_stub_from_json(json_info: &str) -> String {
    extract_account_id(json_info)
        .map(|account_id| serde_json::json!({ "tokens": { "account_id": account_id } }).to_string())
        .unwrap_or_else(|| "{}".to_string())
}

fn json_info_has_credential(json_info: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json_info)
        .ok()
        .and_then(|value| {
            value
                .pointer("/tokens/access_token")
                .and_then(|item| item.as_str())
                .map(|item| !item.trim().is_empty())
        })
        .unwrap_or(false)
}

fn save_account_json_info(conn: &Connection, id: i64, json_info: &str) -> Result<(), String> {
    conn.execute(
        "
        UPDATE accounts
        SET credential_key = '',
            json_info = ?1,
            updated_at = datetime('now')
        WHERE id = ?2
        ",
        params![json_info, id],
    )
    .map_err(|e| format!("Failed to save account credential: {}", e))?;
    Ok(())
}

fn health_item(
    key: &str,
    label: &str,
    status: &str,
    message: impl Into<String>,
) -> AccountHealthItem {
    AccountHealthItem {
        key: key.to_string(),
        label: label.to_string(),
        status: status.to_string(),
        message: message.into(),
    }
}

fn health_summary_status(items: &[AccountHealthItem]) -> String {
    if items.iter().any(|item| item.status == "error") {
        "error".to_string()
    } else if items.iter().any(|item| item.status == "warn") {
        "warn".to_string()
    } else {
        "ok".to_string()
    }
}

fn jwt_expiration_message(token: &str) -> String {
    decode_jwt_payload_value(token)
        .and_then(|payload| payload.get("exp").and_then(|value| value.as_i64()))
        .map(|exp| {
            let remaining = exp - chrono_like_now_timestamp();
            if remaining <= 0 {
                "已过期".to_string()
            } else {
                format!("约 {} 分钟后过期", remaining / 60)
            }
        })
        .unwrap_or_else(|| "无法读取过期时间".to_string())
}

fn require_json_string(
    value: &serde_json::Value,
    pointer: &str,
    label: &str,
) -> Result<String, String> {
    value
        .pointer(pointer)
        .and_then(|item| item.as_str())
        .filter(|item| !item.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("Auth JSON missing required field: {}", label))
}

fn parse_auth_json(json_info: &str) -> Result<serde_json::Value, String> {
    let value = serde_json::from_str::<serde_json::Value>(json_info)
        .map_err(|e| format!("Invalid JSON: {}", e))?;
    require_json_string(&value, "/tokens/access_token", "tokens.access_token")?;
    require_json_string(&value, "/tokens/account_id", "tokens.account_id")?;
    Ok(value)
}

fn extract_access_token(json_info: &str) -> Result<String, String> {
    let value = parse_auth_json(json_info)?;
    require_json_string(&value, "/tokens/access_token", "tokens.access_token")
}

fn normalize_json_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn normalize_email_for_match(value: Option<&str>) -> Option<String> {
    normalize_json_string(value).map(|item| item.to_ascii_lowercase())
}

fn first_json_string(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for key in *path {
            current = match current {
                serde_json::Value::Array(items) => items.get(key.parse::<usize>().ok()?)?,
                _ => current.get(*key)?,
            };
        }
        normalize_json_string(current.as_str())
    })
}

fn decode_jwt_payload_value(token: &str) -> Option<serde_json::Value> {
    let mut parts = token.split('.');
    parts.next()?;
    let payload = parts.next()?;
    if parts.next().is_none() {
        return None;
    }
    let payload_bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&payload_bytes).ok()
}

fn is_token_expired(access_token: &str) -> bool {
    let payload = match decode_jwt_payload_value(access_token) {
        Some(value) => value,
        None => return true,
    };
    let exp = match payload.get("exp").and_then(|value| value.as_i64()) {
        Some(value) => value,
        None => return true,
    };

    exp < chrono_like_now_timestamp() + TOKEN_REFRESH_SKEW_SECONDS
}

fn extract_identity_from_tokens(id_token: &str, access_token: &str) -> AccountIdentity {
    let id_payload = decode_jwt_payload_value(id_token);
    let access_payload = decode_jwt_payload_value(access_token);
    let auth_data = access_payload
        .as_ref()
        .and_then(|payload| payload.get("https://api.openai.com/auth"));

    let email = id_payload
        .as_ref()
        .and_then(|payload| {
            first_json_string(
                payload,
                &[
                    &["email"],
                    &["https://api.openai.com/profile", "email"],
                    &["https://api.openai.com/auth", "email"],
                ],
            )
        })
        .or_else(|| {
            access_payload.as_ref().and_then(|payload| {
                first_json_string(
                    payload,
                    &[&["email"], &["https://api.openai.com/profile", "email"]],
                )
            })
        });
    let account_id = auth_data
        .and_then(|value| first_json_string(value, &[&["chatgpt_account_id"], &["account_id"]]))
        .or_else(|| {
            id_payload.as_ref().and_then(|payload| {
                first_json_string(
                    payload,
                    &[
                        &["https://api.openai.com/auth", "chatgpt_account_id"],
                        &["https://api.openai.com/auth", "account_id"],
                    ],
                )
            })
        });
    let plan_type = auth_data.and_then(|value| first_json_string(value, &[&["chatgpt_plan_type"]]));

    AccountIdentity {
        email,
        account_id,
        plan_type,
        account_name: None,
    }
}

fn extract_email_from_auth_json(json_info: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(json_info).ok()?;
    let id_token = value
        .pointer("/tokens/id_token")
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    let access_token = value
        .pointer("/tokens/access_token")
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    extract_identity_from_tokens(id_token, access_token).email
}

fn oauth_existing_account_matches_identity(
    existing_name: &str,
    existing_json_info: &str,
    target_email: Option<&str>,
) -> bool {
    let Some(target_email) = normalize_email_for_match(target_email) else {
        return false;
    };

    extract_email_from_auth_json(existing_json_info)
        .and_then(|email| normalize_email_for_match(Some(&email)))
        .or_else(|| normalize_email_for_match(Some(existing_name)))
        .as_deref()
        == Some(target_email.as_str())
}

fn chrono_like_now_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn codex_last_refresh_string() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.6fZ")
        .to_string()
}

fn codex_auth_json(
    id_token: &str,
    access_token: &str,
    refresh_token: &str,
    account_id: &str,
) -> Result<String, String> {
    serde_json::to_string_pretty(&serde_json::json!({
        "OPENAI_API_KEY": serde_json::Value::Null,
        "last_refresh": codex_last_refresh_string(),
        "tokens": {
            "access_token": access_token,
            "account_id": account_id,
            "id_token": id_token,
            "refresh_token": refresh_token
        }
    }))
    .map_err(|e| format!("Failed to serialize auth JSON: {}", e))
}

fn random_base64url_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn code_challenge(code_verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn percent_encode(input: &str) -> String {
    let mut output = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            _ => output.push_str(&format!("%{:02X}", byte)),
        }
    }
    output
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(&input[index + 1..index + 3], 16) {
                output.push(value);
                index += 3;
                continue;
            }
        }
        output.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8(output).unwrap_or_else(|_| input.to_string())
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1.split_once('#').map_or_else(
        || url.split_once('?').map(|(_, query)| query).unwrap_or(""),
        |(query, _)| query,
    );
    query.split('&').find_map(|pair| {
        let (raw_key, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        if percent_decode(raw_key) == key {
            Some(percent_decode(raw_value))
        } else {
            None
        }
    })
}

fn build_codex_oauth_url(
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    force_account_selection: bool,
) -> String {
    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true&state={}&originator={}",
        CODEX_AUTH_ENDPOINT,
        percent_encode(CODEX_OAUTH_CLIENT_ID),
        percent_encode(redirect_uri),
        percent_encode(CODEX_OAUTH_SCOPES),
        percent_encode(code_challenge),
        percent_encode(state),
        percent_encode(CODEX_OAUTH_ORIGINATOR),
    );

    if force_account_selection {
        url.push_str("&prompt=login&max_age=0");
    }

    url
}

fn callback_response_html() -> &'static str {
    r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>授权成功</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; display: grid; place-items: center; min-height: 100vh; margin: 0; background: #111827; color: white; }
    main { text-align: center; padding: 32px; }
    h1 { margin: 0 0 10px; font-size: 24px; }
    p { margin: 0; color: #cbd5e1; }
  </style>
</head>
<body>
  <main>
    <h1>授权成功</h1>
    <p>可以关闭此窗口并返回 Codex Account Manager。</p>
  </main>
</body>
</html>"#
}

fn write_http_response(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        body.as_bytes().len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn start_oauth_callback_listener(
    app: AppHandle,
    listener: TcpListener,
    login_id: String,
    expected_state: String,
) {
    std::thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(300);
        let mut completed = false;

        while started.elapsed() < timeout {
            let should_stop = OAUTH_STATE
                .lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|state| state.login_id != login_id))
                .unwrap_or(true);
            if should_stop {
                break;
            }

            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0u8; 4096];
                    let bytes = stream.read(&mut buffer).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..bytes]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("");
                    if path.starts_with("/cancel") {
                        write_http_response(&mut stream, "200 OK", "OAuth cancelled");
                        break;
                    }
                    if !path.starts_with("/auth/callback") {
                        write_http_response(&mut stream, "404 Not Found", "Not Found");
                        continue;
                    }
                    let callback_url =
                        format!("http://localhost:{}{}", CODEX_OAUTH_CALLBACK_PORT, path);
                    let code = query_param(&callback_url, "code").unwrap_or_default();
                    let state = query_param(&callback_url, "state").unwrap_or_default();
                    if code.is_empty() || state != expected_state {
                        write_http_response(
                            &mut stream,
                            "400 Bad Request",
                            "OAuth callback invalid",
                        );
                        continue;
                    }

                    let mut accepted = false;
                    if let Ok(mut guard) = OAUTH_STATE.lock() {
                        if let Some(current) = guard.as_mut() {
                            if current.login_id == login_id && current.state == expected_state {
                                current.code = Some(code);
                                accepted = true;
                            }
                        }
                    }

                    if accepted {
                        write_http_response(&mut stream, "200 OK", callback_response_html());
                        let _ = app.emit(
                            "codex-oauth-login-completed",
                            serde_json::json!({ "loginId": login_id }),
                        );
                        completed = true;
                    } else {
                        write_http_response(&mut stream, "409 Conflict", "OAuth state changed");
                    }
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("OAuth callback listener error: {}", e);
                    break;
                }
            }
        }

        if !completed {
            let _ = app.emit(
                "codex-oauth-login-timeout",
                serde_json::json!({ "loginId": login_id, "timeoutSeconds": timeout.as_secs() }),
            );
        }
    });
}

fn open_url_in_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    Ok(())
}

async fn fetch_remote_account_identity(
    access_token: &str,
    account_id: Option<&str>,
) -> Result<AccountIdentity, String> {
    let auth_header = HeaderValue::from_str(&format!("Bearer {}", access_token))
        .map_err(|e| format!("Invalid access token for account check: {}", e))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", error_chain(&e)))?;
    let mut request = client
        .get(CODEX_ACCOUNT_CHECK_ENDPOINT)
        .header(AUTHORIZATION, auth_header)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(account_id) = normalize_json_string(account_id) {
        request = request.header("ChatGPT-Account-Id", account_id);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Account check request failed: {}", error_chain(&e)))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read account check response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "Account check failed: status={}, body_len={}",
            status,
            body.len()
        ));
    }

    let payload: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid account check JSON: {}", e))?;
    Ok(AccountIdentity {
        email: first_json_string(
            &payload,
            &[&["email"], &["user", "email"], &["account", "email"]],
        ),
        account_id: first_json_string(
            &payload,
            &[
                &["id"],
                &["account_id"],
                &["chatgpt_account_id"],
                &["account", "id"],
                &["account", "account_id"],
                &["accounts", "0", "id"],
            ],
        ),
        plan_type: first_json_string(
            &payload,
            &[
                &["plan_type"],
                &["planType"],
                &["account", "plan_type"],
                &["account", "planType"],
            ],
        ),
        account_name: first_json_string(
            &payload,
            &[
                &["name"],
                &["display_name"],
                &["account_name"],
                &["account", "name"],
                &["account", "display_name"],
            ],
        ),
    })
}

async fn refresh_auth_json_if_needed(
    json_info: &str,
    force: bool,
) -> Result<(String, bool), String> {
    let mut value = parse_auth_json(json_info)?;
    let access_token = require_json_string(&value, "/tokens/access_token", "tokens.access_token")?;
    if !force && !is_token_expired(&access_token) {
        return Ok((json_info.to_string(), false));
    }

    let refresh_token =
        require_json_string(&value, "/tokens/refresh_token", "tokens.refresh_token")?;
    let current_id_token = value
        .pointer("/tokens/id_token")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", error_chain(&e)))?;
    let response = client
        .post(CODEX_TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", CODEX_OAUTH_CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {}", error_chain(&e)))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read token refresh response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "Token refresh failed: status={}, body_len={}",
            status,
            body.len()
        ));
    }

    let token_response: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse token refresh response: {}", e))?;
    let new_access_token = token_response
        .get("access_token")
        .and_then(|item| item.as_str())
        .filter(|item| !item.trim().is_empty())
        .ok_or_else(|| "Token refresh response missing access_token".to_string())?;
    let new_refresh_token = token_response
        .get("refresh_token")
        .and_then(|item| item.as_str())
        .filter(|item| !item.trim().is_empty())
        .unwrap_or(refresh_token.as_str());
    let new_id_token = token_response
        .get("id_token")
        .and_then(|item| item.as_str())
        .filter(|item| !item.trim().is_empty())
        .or(current_id_token.as_deref());

    let tokens = value
        .get_mut("tokens")
        .and_then(|item| item.as_object_mut())
        .ok_or_else(|| "Auth JSON missing tokens object".to_string())?;
    tokens.insert(
        "access_token".to_string(),
        serde_json::Value::String(new_access_token.to_string()),
    );
    tokens.insert(
        "refresh_token".to_string(),
        serde_json::Value::String(new_refresh_token.to_string()),
    );
    if let Some(id_token) = new_id_token {
        tokens.insert(
            "id_token".to_string(),
            serde_json::Value::String(id_token.to_string()),
        );
    }
    value["OPENAI_API_KEY"] = serde_json::Value::Null;
    if let Some(object) = value.as_object_mut() {
        object.remove("auth_mode");
    }
    value["last_refresh"] = serde_json::Value::String(codex_last_refresh_string());

    let updated = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to serialize refreshed auth JSON: {}", e))?;
    Ok((updated, true))
}

fn extract_token_string(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    first_json_string(value, paths)
}

fn access_token_account_id(access_token: &str) -> Option<String> {
    extract_identity_from_tokens("", access_token).account_id
}

async fn refresh_token_to_auth_json(refresh_token: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", error_chain(&e)))?;
    let response = client
        .post(CODEX_TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CODEX_OAUTH_CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {}", error_chain(&e)))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read token refresh response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "Token refresh failed: status={}, body_len={}",
            status,
            body.len()
        ));
    }

    let token_response: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse token refresh response: {}", e))?;
    let id_token = token_response
        .get("id_token")
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .to_string();
    let access_token = require_json_string(&token_response, "/access_token", "access_token")?;
    let next_refresh_token = token_response
        .get("refresh_token")
        .and_then(|item| item.as_str())
        .filter(|item| !item.trim().is_empty())
        .unwrap_or(refresh_token);
    let account_id = access_token_account_id(&access_token)
        .ok_or_else(|| "Cannot detect account_id from refreshed access_token".to_string())?;

    codex_auth_json(&id_token, &access_token, next_refresh_token, &account_id)
}

fn access_token_to_auth_json(access_token: &str) -> Result<String, String> {
    let account_id = access_token_account_id(access_token)
        .ok_or_else(|| "Cannot detect account_id from access_token".to_string())?;
    codex_auth_json("", access_token, "", &account_id)
}

// Guardrail: account switching is auth-only. Keep the on-disk shape compatible
// with official Codex ~/.codex/auth.json and never use this path to change
// ~/.codex/config.toml, providers, proxies, models, MCP, memories, or projects.
fn canonicalize_auth_json(json_info: &str) -> Result<String, String> {
    let value = parse_auth_json(json_info)?;
    let id_token = value
        .pointer("/tokens/id_token")
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    let access_token = require_json_string(&value, "/tokens/access_token", "tokens.access_token")?;
    let refresh_token = value
        .pointer("/tokens/refresh_token")
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    let account_id = require_json_string(&value, "/tokens/account_id", "tokens.account_id")?;

    codex_auth_json(id_token, &access_token, refresh_token, &account_id)
}

async fn normalize_auth_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        if decode_jwt_payload_value(trimmed).is_some() {
            return access_token_to_auth_json(trimmed);
        }
        return refresh_token_to_auth_json(trimmed).await;
    }

    let mut value = serde_json::from_str::<serde_json::Value>(trimmed)
        .map_err(|e| format!("Invalid JSON: {}", e))?;
    if let Some(tokens) = value
        .get_mut("tokens")
        .and_then(|item| item.as_object_mut())
    {
        if let Some(access_token) = tokens
            .get("access_token")
            .and_then(|item| item.as_str())
            .map(ToString::to_string)
        {
            if !tokens
                .get("account_id")
                .and_then(|item| item.as_str())
                .map(|item| !item.trim().is_empty())
                .unwrap_or(false)
            {
                if let Some(account_id) = access_token_account_id(&access_token) {
                    tokens.insert(
                        "account_id".to_string(),
                        serde_json::Value::String(account_id),
                    );
                }
            }
            return canonicalize_auth_json(&serde_json::to_string(&value).unwrap_or_default());
        }
    }

    if let Some(refresh_token) = extract_token_string(
        &value,
        &[
            &["refresh_token"],
            &["refreshToken"],
            &["tokens", "refresh_token"],
            &["tokens", "refreshToken"],
        ],
    ) {
        return refresh_token_to_auth_json(&refresh_token).await;
    }
    if let Some(access_token) = extract_token_string(
        &value,
        &[
            &["access_token"],
            &["accessToken"],
            &["tokens", "access_token"],
            &["tokens", "accessToken"],
            &["token"],
        ],
    ) {
        return access_token_to_auth_json(&access_token);
    }

    Err("未找到可导入的 Codex token 或 auth.json".to_string())
}

fn account_json_info(conn: &Connection, id: i64) -> Result<String, String> {
    let (_key, json_info): (String, String) = conn
        .query_row(
            "SELECT credential_key, json_info FROM accounts WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("Failed to find account credential: {}", e))?;

    if json_info_has_credential(&json_info) {
        return Ok(json_info);
    }

    Err(
        "该账号没有保存在本地账号库的 auth.json，请重新 OAuth 授权或编辑账号粘贴 auth.json/token。"
            .to_string(),
    )
}

fn read_proxy_active_account_id(conn: &Connection) -> Result<Option<i64>, String> {
    read_setting_from_conn(conn, CODEX_PROXY_SETTING_ACTIVE_ACCOUNT_ID).map(|value| {
        value.and_then(|item| item.trim().parse::<i64>().ok().filter(|parsed| *parsed > 0))
    })
}

fn write_proxy_active_account_id(conn: &Connection, id: i64) -> Result<(), String> {
    write_setting_to_conn(conn, CODEX_PROXY_SETTING_ACTIVE_ACCOUNT_ID, &id.to_string())
}

fn fallback_proxy_account_id(conn: &Connection) -> Result<Option<i64>, String> {
    let result = conn.query_row(
        "
        SELECT id
        FROM accounts
        WHERE CASE
                  WHEN json_valid(json_info)
                  THEN COALESCE(json_extract(json_info, '$.tokens.access_token'), '')
                  ELSE ''
              END != ''
        ORDER BY id DESC
        LIMIT 1
        ",
        [],
        |row| row.get::<_, i64>(0),
    );

    match result {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to find proxy account: {}", e)),
    }
}

fn proxy_account_credentials(conn: &Connection) -> Result<(i64, String, String, String), String> {
    let id = read_proxy_active_account_id(conn)?
        .or_else(|| fallback_proxy_account_id(conn).ok().flatten())
        .ok_or_else(|| "代理没有可用账号，请先选择一个带 auth.json 的账号。".to_string())?;
    let (account_name, _identifier) = account_log_context(conn, id);
    let json_info = account_json_info(conn, id)?;
    let (json_info, changed) =
        tauri::async_runtime::block_on(refresh_auth_json_if_needed(&json_info, false))?;
    if changed {
        save_account_json_info(conn, id, &json_info)?;
    }

    let value = parse_auth_json(&json_info)?;
    let access_token = require_json_string(&value, "/tokens/access_token", "tokens.access_token")?;
    let account_id = require_json_string(&value, "/tokens/account_id", "tokens.account_id")?;
    Ok((id, account_name, account_id, access_token))
}

#[derive(Debug)]
struct ProxyHttpRequest {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn proxy_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn proxy_content_length(header_text: &str) -> Result<usize, String> {
    for line in header_text.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            return value
                .trim()
                .parse::<usize>()
                .map_err(|e| format!("Content-Length 无效: {}", e));
        }
        if name.trim().eq_ignore_ascii_case("transfer-encoding")
            && value.to_ascii_lowercase().contains("chunked")
        {
            return Err("暂不支持客户端 chunked 请求体".to_string());
        }
    }
    Ok(0)
}

fn parse_proxy_http_request(raw: &[u8]) -> Result<ProxyHttpRequest, String> {
    let Some(header_end) = proxy_header_end(raw) else {
        return Err("代理请求缺少 HTTP 头".to_string());
    };
    let header_text = String::from_utf8_lossy(&raw[..header_end]);
    let mut lines = header_text.lines();
    let request_line = lines.next().ok_or_else(|| "代理请求行为空".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "代理请求缺少 method".to_string())?
        .to_string();
    let target = parts
        .next()
        .ok_or_else(|| "代理请求缺少 target".to_string())?
        .to_string();
    let mut headers = HashMap::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    Ok(ProxyHttpRequest {
        method,
        target,
        headers,
        body: raw[header_end..].to_vec(),
    })
}

fn read_proxy_http_request(stream: &mut TcpStream) -> Result<ProxyHttpRequest, String> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(120)))
        .map_err(|e| format!("设置读取超时失败: {}", e))?;
    let mut buffer = Vec::with_capacity(8192);
    let mut chunk = [0u8; 8192];
    let mut header_end = None;
    let mut content_length = 0usize;

    loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|e| format!("读取代理请求失败: {}", e))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > 64 * 1024 * 1024 {
            return Err("代理请求体过大".to_string());
        }

        if header_end.is_none() {
            if let Some(end) = proxy_header_end(&buffer) {
                let header_text = String::from_utf8_lossy(&buffer[..end]);
                content_length = proxy_content_length(&header_text)?;
                header_end = Some(end);
            }
        }

        if let Some(end) = header_end {
            if buffer.len() >= end + content_length {
                return parse_proxy_http_request(&buffer[..end + content_length]);
            }
        }
    }

    Err("代理请求不完整".to_string())
}

fn normalize_proxy_target(target: &str) -> Result<String, String> {
    let target = target.trim();
    if target.starts_with("http://") || target.starts_with("https://") {
        let after_scheme = target
            .split_once("://")
            .map(|(_, rest)| rest)
            .ok_or_else(|| "代理请求地址无效".to_string())?;
        let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
        let path = &after_scheme[path_start..];
        return Ok(if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        });
    }

    if target.starts_with('/') {
        Ok(target.to_string())
    } else {
        Err("代理请求路径必须以 / 开头".to_string())
    }
}

fn resolve_codex_proxy_upstream_target(target: &str) -> Result<String, String> {
    let target = normalize_proxy_target(target)?;
    let trimmed = if let Some(rest) = target.strip_prefix("/v1") {
        rest
    } else if let Some(rest) = target.strip_prefix("/backend-api/codex") {
        rest
    } else {
        return Err("代理只支持 /v1 或 /backend-api/codex 路径".to_string());
    };

    if trimmed.is_empty() {
        Ok("/".to_string())
    } else if trimmed.starts_with('/') {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("/{}", trimmed))
    }
}

fn proxy_request_is_stream(headers: &HashMap<String, String>, body: &[u8]) -> bool {
    headers
        .get("accept")
        .map(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        .unwrap_or(false)
        || serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|value| value.get("stream").and_then(|item| item.as_bool()))
            .unwrap_or(false)
}

fn write_proxy_json_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        body.as_bytes().len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn write_proxy_error(stream: &mut TcpStream, status: &str, message: &str) {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "codex_account_manager_proxy_error"
        }
    })
    .to_string();
    write_proxy_json_response(stream, status, &body);
}

fn write_proxy_options_response(stream: &mut TcpStream) {
    let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, OpenAI-Beta, X-API-Key, X-Codex-Turn-State, X-Codex-Turn-Metadata, X-Client-Request-Id, ChatGPT-Account-Id\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn codex_proxy_models_response() -> String {
    serde_json::json!({
        "object": "list",
        "data": [
            { "id": "gpt-5-codex", "object": "model", "created": 0, "owned_by": "openai" },
            { "id": "gpt-5-codex-mini", "object": "model", "created": 0, "owned_by": "openai" },
            { "id": "gpt-5", "object": "model", "created": 0, "owned_by": "openai" }
        ]
    })
    .to_string()
}

fn random_proxy_session_id() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    BASE64.encode(bytes).replace(['/', '+', '='], "")
}

fn forward_codex_proxy_request(
    stream: &mut TcpStream,
    db_path: &std::path::Path,
    request: ProxyHttpRequest,
) -> Result<(), String> {
    let normalized_target = normalize_proxy_target(&request.target)?;
    if request.method.eq_ignore_ascii_case("OPTIONS") {
        write_proxy_options_response(stream);
        return Ok(());
    }

    if normalized_target == "/v1/models" || normalized_target.starts_with("/v1/models?") {
        write_proxy_json_response(stream, "200 OK", &codex_proxy_models_response());
        return Ok(());
    }

    let upstream_target = resolve_codex_proxy_upstream_target(&normalized_target)?;
    let upstream_url = format!("{}{}", CODEX_PROXY_UPSTREAM_BASE_URL, upstream_target);
    let conn = Connection::open(db_path).map_err(|e| format!("代理打开账号库失败: {}", e))?;
    let (_id, _name, account_id, access_token) = proxy_account_credentials(&conn)?;
    let method = reqwest::Method::from_bytes(request.method.as_bytes())
        .map_err(|e| format!("代理请求方法无效: {}", e))?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("创建代理 HTTP 客户端失败: {}", e))?;
    let mut upstream = client.request(method, upstream_url);

    for (name, value) in &request.headers {
        if matches!(
            name.as_str(),
            "authorization"
                | "host"
                | "content-length"
                | "connection"
                | "proxy-connection"
                | "accept-encoding"
                | "x-api-key"
        ) {
            continue;
        }
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|e| format!("代理请求头无效 {}: {}", name, e))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|e| format!("代理请求头值无效 {}: {}", name, e))?;
        upstream = upstream.header(header_name, header_value);
    }

    upstream = upstream
        .header(AUTHORIZATION, format!("Bearer {}", access_token))
        .header("ChatGPT-Account-Id", account_id)
        .header("Originator", CODEX_PROXY_DEFAULT_ORIGINATOR)
        .header("Connection", "Keep-Alive");

    if !request.headers.contains_key("user-agent") {
        upstream = upstream.header(USER_AGENT, CODEX_PROXY_DEFAULT_USER_AGENT);
    }
    if !request.headers.contains_key("accept") {
        upstream = upstream.header(
            ACCEPT,
            if proxy_request_is_stream(&request.headers, &request.body) {
                "text/event-stream"
            } else {
                "application/json"
            },
        );
    }
    if !request.headers.contains_key("content-type") && !request.body.is_empty() {
        upstream = upstream.header(CONTENT_TYPE, "application/json");
    }
    if upstream_target.starts_with("/responses") {
        if !request.headers.contains_key("x-codex-turn-state") {
            upstream = upstream.header("x-codex-turn-state", "");
        }
        if !request.headers.contains_key("x-codex-turn-metadata") {
            upstream = upstream.header("x-codex-turn-metadata", "");
        }
        if !request.headers.contains_key("session_id")
            && !request.headers.contains_key("session-id")
        {
            upstream = upstream.header("Session_id", random_proxy_session_id());
        }
    }
    if !request.body.is_empty() {
        upstream = upstream.body(request.body);
    }

    let mut response = upstream
        .send()
        .map_err(|e| format!("代理上游请求失败: {}", e))?;
    let status = response.status();
    let reason = status.canonical_reason().unwrap_or("OK");
    let mut head = format!("HTTP/1.1 {} {}\r\n", status.as_u16(), reason);
    for (name, value) in response.headers() {
        let name_text = name.as_str().to_ascii_lowercase();
        if matches!(
            name_text.as_str(),
            "content-length" | "transfer-encoding" | "connection" | "content-encoding"
        ) {
            continue;
        }
        if let Ok(value_text) = value.to_str() {
            head.push_str(name.as_str());
            head.push_str(": ");
            head.push_str(value_text);
            head.push_str("\r\n");
        }
    }
    head.push_str("Access-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n");
    stream
        .write_all(head.as_bytes())
        .map_err(|e| format!("写入代理响应头失败: {}", e))?;
    std::io::copy(&mut response, stream).map_err(|e| format!("转发代理响应失败: {}", e))?;
    let _ = stream.flush();
    Ok(())
}

fn handle_codex_proxy_connection(
    mut stream: TcpStream,
    db_path: std::path::PathBuf,
    last_error: Arc<Mutex<String>>,
) {
    let result = read_proxy_http_request(&mut stream)
        .and_then(|request| forward_codex_proxy_request(&mut stream, &db_path, request));
    match result {
        Ok(()) => {
            if let Ok(mut guard) = last_error.lock() {
                guard.clear();
            }
        }
        Err(error) => {
            if let Ok(mut guard) = last_error.lock() {
                *guard = error.clone();
            }
            write_proxy_error(&mut stream, "502 Bad Gateway", &error);
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

fn stop_codex_proxy_runtime() -> Result<(), String> {
    let mut guard = CODEX_PROXY_RUNTIME
        .lock()
        .map_err(|_| "Codex proxy runtime lock is poisoned".to_string())?;
    if let Some(runtime) = guard.take() {
        runtime.stop.store(true, Ordering::Relaxed);
    }
    Ok(())
}

fn start_codex_proxy_runtime(db_path: std::path::PathBuf, port: u16) -> Result<(), String> {
    {
        let guard = CODEX_PROXY_RUNTIME
            .lock()
            .map_err(|_| "Codex proxy runtime lock is poisoned".to_string())?;
        if let Some(runtime) = guard.as_ref() {
            if runtime.port == port && !runtime.stop.load(Ordering::Relaxed) {
                return Ok(());
            }
        }
    }

    stop_codex_proxy_runtime()?;
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("启动 Codex 代理失败，端口 {} 不可用: {}", port, e))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("设置 Codex 代理监听失败: {}", e))?;
    let stop = Arc::new(AtomicBool::new(false));
    let last_error = Arc::new(Mutex::new(String::new()));
    let stop_for_thread = stop.clone();
    let last_error_for_thread = last_error.clone();
    std::thread::spawn(move || {
        while !stop_for_thread.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    if let Err(e) = stream.set_nonblocking(false) {
                        if let Ok(mut guard) = last_error_for_thread.lock() {
                            *guard = format!("Codex 代理连接初始化失败: {}", e);
                        }
                        continue;
                    }
                    let db_path = db_path.clone();
                    let last_error = last_error_for_thread.clone();
                    std::thread::spawn(move || {
                        handle_codex_proxy_connection(stream, db_path, last_error);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(80));
                }
                Err(e) => {
                    if let Ok(mut guard) = last_error_for_thread.lock() {
                        *guard = format!("Codex 代理监听失败: {}", e);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
        }
    });

    let mut guard = CODEX_PROXY_RUNTIME
        .lock()
        .map_err(|_| "Codex proxy runtime lock is poisoned".to_string())?;
    *guard = Some(CodexProxyRuntime {
        port,
        stop,
        last_error,
    });
    Ok(())
}

fn hot_switch_result(
    status: &str,
    message: impl Into<String>,
    detail: impl Into<String>,
) -> CodexHotSwitchResult {
    CodexHotSwitchResult {
        status: status.to_string(),
        message: message.into(),
        detail: detail.into(),
    }
}

fn skipped_hot_switch_result(message: impl Into<String>) -> CodexHotSwitchResult {
    hot_switch_result("skipped", message, "")
}

fn unavailable_hot_switch_result(
    message: impl Into<String>,
    detail: impl Into<String>,
) -> CodexHotSwitchResult {
    hot_switch_result("unavailable", message, detail)
}

fn failed_hot_switch_result(
    message: impl Into<String>,
    detail: impl Into<String>,
) -> CodexHotSwitchResult {
    hot_switch_result("failed", message, detail)
}

fn applied_hot_switch_result(
    message: impl Into<String>,
    detail: impl Into<String>,
) -> CodexHotSwitchResult {
    hot_switch_result("applied", message, detail)
}

fn codex_app_server_socket_path() -> Result<std::path::PathBuf, String> {
    Ok(get_codex_home_path()?
        .join("app-server-control")
        .join("app-server-control.sock"))
}

fn app_server_auth_params_from_json(
    json_info: &str,
) -> Result<(String, String, Option<String>), String> {
    let value = parse_auth_json(json_info)?;
    let access_token = require_json_string(&value, "/tokens/access_token", "tokens.access_token")?;
    let account_id = require_json_string(&value, "/tokens/account_id", "tokens.account_id")?;
    let id_token = value
        .pointer("/tokens/id_token")
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    let identity = extract_identity_from_tokens(id_token, &access_token);
    let plan_type = identity.plan_type.filter(|item| {
        let trimmed = item.trim();
        !trimmed.is_empty() && trimmed != "unknown"
    });

    Ok((access_token, account_id, plan_type))
}

#[cfg(unix)]
fn read_http_upgrade_response(
    stream: &mut std::os::unix::net::UnixStream,
) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 1];
    while bytes.len() < 16 * 1024 {
        stream.read_exact(&mut buf)?;
        bytes.push(buf[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(String::from_utf8_lossy(&bytes).to_string());
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "app-server websocket handshake response is too large",
    ))
}

#[cfg(unix)]
fn websocket_send_text(
    stream: &mut std::os::unix::net::UnixStream,
    text: &str,
) -> std::io::Result<()> {
    let payload = text.as_bytes();
    let mut frame = Vec::with_capacity(payload.len() + 16);
    frame.push(0x81);
    if payload.len() <= 125 {
        frame.push(0x80 | payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }

    let mut mask = [0u8; 4];
    OsRng.fill_bytes(&mut mask);
    frame.extend_from_slice(&mask);
    for (index, byte) in payload.iter().enumerate() {
        frame.push(byte ^ mask[index % mask.len()]);
    }

    stream.write_all(&frame)?;
    stream.flush()
}

#[cfg(unix)]
fn websocket_read_text(stream: &mut std::os::unix::net::UnixStream) -> std::io::Result<String> {
    loop {
        let mut header = [0u8; 2];
        stream.read_exact(&mut header)?;
        let opcode = header[0] & 0x0f;
        let masked = (header[1] & 0x80) != 0;
        let mut len = (header[1] & 0x7f) as u64;

        if len == 126 {
            let mut extended = [0u8; 2];
            stream.read_exact(&mut extended)?;
            len = u16::from_be_bytes(extended) as u64;
        } else if len == 127 {
            let mut extended = [0u8; 8];
            stream.read_exact(&mut extended)?;
            len = u64::from_be_bytes(extended);
        }

        if len > 8 * 1024 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "app-server websocket frame is too large",
            ));
        }

        let mut mask = [0u8; 4];
        if masked {
            stream.read_exact(&mut mask)?;
        }

        let mut payload = vec![0u8; len as usize];
        stream.read_exact(&mut payload)?;
        if masked {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % mask.len()];
            }
        }

        match opcode {
            0x1 => {
                return String::from_utf8(payload).map_err(|err| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
                });
            }
            0x8 => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    "app-server closed websocket",
                ));
            }
            0x9 | 0xA => continue,
            _ => continue,
        }
    }
}

#[cfg(unix)]
fn websocket_read_response(
    stream: &mut std::os::unix::net::UnixStream,
    id: i64,
) -> Result<serde_json::Value, String> {
    for _ in 0..50 {
        let text = websocket_read_text(stream)
            .map_err(|e| format!("failed to read app-server response: {}", e))?;
        let value = serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| format!("invalid app-server JSON-RPC message: {}", e))?;
        if value.get("id").and_then(|item| item.as_i64()) == Some(id) {
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(|item| item.as_str())
                    .unwrap_or("app-server returned an error");
                let code = error.get("code").and_then(|item| item.as_i64());
                return Err(match code {
                    Some(code) => format!("{} (code {})", message, code),
                    None => message.to_string(),
                });
            }
            return Ok(value
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null));
        }
    }

    Err(format!("app-server did not return response id {}", id))
}

#[cfg(unix)]
fn try_hot_switch_codex_app_server_inner(json_info: &str) -> Result<String, String> {
    let (access_token, account_id, plan_type) = app_server_auth_params_from_json(json_info)?;
    let socket_path = codex_app_server_socket_path()?;
    if !socket_path.exists() {
        return Err(format!(
            "APP_SERVER_SOCKET_MISSING:{}",
            socket_path.to_string_lossy()
        ));
    }

    let mut stream = std::os::unix::net::UnixStream::connect(&socket_path)
        .map_err(|e| format!("APP_SERVER_CONNECT_FAILED:{}:{}", socket_path.display(), e))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| format!("failed to set app-server read timeout: {}", e))?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| format!("failed to set app-server write timeout: {}", e))?;

    let mut websocket_key_bytes = [0u8; 16];
    OsRng.fill_bytes(&mut websocket_key_bytes);
    let websocket_key = BASE64.encode(websocket_key_bytes);
    let request = format!(
        "GET / HTTP/1.1\r\n\
         Host: localhost\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: {}\r\n\
         \r\n",
        websocket_key
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("failed to send app-server websocket handshake: {}", e))?;
    let response = read_http_upgrade_response(&mut stream)
        .map_err(|e| format!("failed to read app-server websocket handshake: {}", e))?;
    if !response.starts_with("HTTP/1.1 101") && !response.starts_with("HTTP/1.0 101") {
        return Err(format!(
            "app-server websocket handshake was rejected: {}",
            response.lines().next().unwrap_or("unknown response")
        ));
    }

    let initialize = serde_json::json!({
        "method": "initialize",
        "id": 1,
        "params": {
            "clientInfo": {
                "name": "codex-account-manager",
                "title": "Codex Account Manager",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "experimentalApi": true
            }
        }
    });
    websocket_send_text(&mut stream, &initialize.to_string())
        .map_err(|e| format!("failed to send app-server initialize: {}", e))?;
    websocket_read_response(&mut stream, 1)?;

    let initialized = serde_json::json!({
        "method": "initialized"
    });
    websocket_send_text(&mut stream, &initialized.to_string())
        .map_err(|e| format!("failed to send app-server initialized notification: {}", e))?;

    let login = serde_json::json!({
        "method": "account/login/start",
        "id": 2,
        "params": {
            "type": "chatgptAuthTokens",
            "accessToken": access_token,
            "chatgptAccountId": account_id,
            "chatgptPlanType": plan_type
        }
    });
    websocket_send_text(&mut stream, &login.to_string())
        .map_err(|e| format!("failed to send app-server account switch: {}", e))?;
    websocket_read_response(&mut stream, 2)?;

    Ok(socket_path.to_string_lossy().to_string())
}

#[cfg(unix)]
fn try_hot_switch_codex_app_server(json_info: &str) -> CodexHotSwitchResult {
    match try_hot_switch_codex_app_server_inner(json_info) {
        Ok(socket_path) => applied_hot_switch_result(
            "已通知正在运行的 Codex 热更新账号",
            format!("socket={socket_path}"),
        ),
        Err(err) if err.starts_with("APP_SERVER_SOCKET_MISSING:") => {
            let path = err.trim_start_matches("APP_SERVER_SOCKET_MISSING:");
            unavailable_hot_switch_result(
                "未发现正在运行的 Codex app-server，已写入 auth.json，重启 Codex 后生效",
                format!("socket={path}"),
            )
        }
        Err(err) if err.starts_with("APP_SERVER_CONNECT_FAILED:") => unavailable_hot_switch_result(
            "无法连接正在运行的 Codex app-server，已写入 auth.json，重启 Codex 后生效",
            err,
        ),
        Err(err) => failed_hot_switch_result(
            "Codex app-server 拒绝热切号，已写入 auth.json，必要时请重启 Codex",
            err,
        ),
    }
}

#[cfg(not(unix))]
fn try_hot_switch_codex_app_server(_json_info: &str) -> CodexHotSwitchResult {
    unavailable_hot_switch_result(
        "当前系统暂不支持通过本工具热切号，已写入 auth.json，重启 Codex 后生效",
        "app-server unix socket is unavailable on this platform",
    )
}

fn mark_quota_error(conn: &Connection, id: i64, error: &str) -> Result<(), String> {
    let friendly = friendly_account_error(error);
    let message = if friendly.chars().count() > 500 {
        format!("{}...", friendly.chars().take(500).collect::<String>())
    } else {
        friendly
    };
    conn.execute(
        "
        UPDATE accounts
        SET last_quota_checked_at = datetime('now'),
            last_quota_error = ?1
        WHERE id = ?2
        ",
        params![message, id],
    )
    .map_err(|e| format!("Failed to update quota error: {}", e))?;
    Ok(())
}

fn friendly_account_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("no matching entry found in secure storage")
        || lower.contains("账号凭据在系统凭据库中不存在")
    {
        return missing_account_credential_message();
    }
    if lower.contains("refresh_token") && lower.contains("missing") {
        return "该账号没有 refresh_token，access_token 过期后无法自动刷新，请重新 OAuth 授权。"
            .to_string();
    }
    if lower.contains("token refresh failed")
        || lower.contains("invalid_grant")
        || lower.contains("refresh_token")
    {
        return format!("登录凭据刷新失败，请重新授权。详情：{}", error);
    }
    if lower.contains("timed out") || lower.contains("operation timed out") {
        return format!("请求超时，请检查网络或代理后重试。详情：{}", error);
    }
    if lower.contains("proxy") || lower.contains("dns") || lower.contains("connect") {
        return format!(
            "网络连接失败，请检查代理、DNS 或网络连通性。详情：{}",
            error
        );
    }
    if lower.contains("401") || lower.contains("unauthorized") {
        return "登录已失效，请重新 OAuth 授权。".to_string();
    }
    if lower.contains("403") || lower.contains("forbidden") {
        return format!("接口拒绝访问，请确认账号权限或重新授权。详情：{}", error);
    }
    error.to_string()
}

fn error_chain(error: &dyn Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();

    while let Some(err) = source {
        message.push_str(": ");
        message.push_str(&err.to_string());
        source = err.source();
    }

    message
}

fn derive_backup_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let params = Params::new(19_456, 2, 1, Some(32))
        .map_err(|e| format!("Failed to configure backup encryption: {}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Failed to derive backup key: {}", e))?;
    Ok(key)
}

fn encrypt_backup_payload(payload: &BackupPayload, password: &str) -> Result<String, String> {
    if password.len() < 8 {
        return Err("Backup password must be at least 8 characters".to_string());
    }

    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);

    let key = derive_backup_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("Failed to initialize backup encryption: {}", e))?;
    let plaintext = serde_json::to_vec(payload)
        .map_err(|e| format!("Failed to serialize backup payload: {}", e))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|e| format!("Failed to encrypt backup: {}", e))?;

    let backup = EncryptedBackup {
        format: "codex-account-manager-backup".to_string(),
        version: 1,
        kdf: "argon2id:m=19456,t=2,p=1".to_string(),
        salt: BASE64.encode(salt),
        nonce: BASE64.encode(nonce),
        ciphertext: BASE64.encode(ciphertext),
    };

    serde_json::to_string_pretty(&backup)
        .map_err(|e| format!("Failed to serialize encrypted backup: {}", e))
}

fn decrypt_backup_payload(backup_text: &str, password: &str) -> Result<BackupPayload, String> {
    if password.len() < 8 {
        return Err("Backup password must be at least 8 characters".to_string());
    }

    let backup: EncryptedBackup =
        serde_json::from_str(backup_text).map_err(|e| format!("Invalid backup file: {}", e))?;
    if backup.format != "codex-account-manager-backup" || backup.version != 1 {
        return Err("Unsupported backup format".to_string());
    }

    let salt = BASE64
        .decode(backup.salt)
        .map_err(|e| format!("Invalid backup salt: {}", e))?;
    let nonce = BASE64
        .decode(backup.nonce)
        .map_err(|e| format!("Invalid backup nonce: {}", e))?;
    let ciphertext = BASE64
        .decode(backup.ciphertext)
        .map_err(|e| format!("Invalid backup ciphertext: {}", e))?;
    if nonce.len() != 12 {
        return Err("Invalid backup nonce length".to_string());
    }

    let key = derive_backup_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("Failed to initialize backup decryption: {}", e))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| "Failed to decrypt backup. Check the password.".to_string())?;

    serde_json::from_slice(&plaintext)
        .map_err(|e| format!("Invalid decrypted backup payload: {}", e))
}

fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open path: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open path: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open path: {}", e))?;
    }

    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Codex Account Manager")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(&tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn kill_codex_process() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "-i", "codex"])
            .output();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/IM", "codex.exe", "/F"])
            .output();
    }

    std::thread::sleep(std::time::Duration::from_millis(500));
    Ok(())
}

fn restart_codex_process() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let result = std::process::Command::new("open")
            .args(["-a", "Codex"])
            .output();

        if result.is_err() || result.unwrap().status.code() != Some(0) {
            std::process::Command::new("codex")
                .spawn()
                .map_err(|e| format!("Failed to restart Codex: {}", e))?;
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Codex desktop is a packaged Windows app. Do not fall back to
        // codex.exe here: that starts the CLI and leaves the desktop app closed.
        std::process::Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{}", WINDOWS_CODEX_APP_ID))
            .spawn()
            .map_err(|e| format!("Failed to restart Codex desktop app: {}", e))?;
    }

    Ok(())
}

// ── Tauri commands ────────────────────────────────────────────────────

#[derive(Debug)]
struct QuotaFetchError {
    message: String,
    details: String,
}

fn header_to_string(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

fn body_preview(bytes: &[u8], max_chars: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    truncate_log_text(&text, max_chars)
}

fn quota_used_percent(window: Option<&Window>) -> i32 {
    window
        .and_then(|item| item.used_percent)
        .unwrap_or(0)
        .clamp(0, 100)
}

fn quota_reset_time(window: Option<&Window>) -> i64 {
    let Some(window) = window else {
        return 0;
    };
    if let Some(reset_at) = window.reset_at {
        return reset_at.max(0);
    }
    if let Some(reset_after_seconds) = window.reset_after_seconds {
        if reset_after_seconds >= 0 {
            return chrono_like_now_timestamp() + reset_after_seconds;
        }
    }
    0
}

fn quota_window_minutes(window: Option<&Window>) -> Option<i64> {
    let seconds = window?.limit_window_seconds?;
    if seconds <= 0 {
        return None;
    }
    Some((seconds + 59) / 60)
}

async fn fetch_quota_internal(access_token: String) -> Result<QuotaInfo, QuotaFetchError> {
    let token = access_token.trim();
    if token.is_empty() {
        return Err(QuotaFetchError {
            message: "Access token is empty".to_string(),
            details: String::new(),
        });
    }
    let auth_header =
        HeaderValue::from_str(&format!("Bearer {}", token)).map_err(|e| QuotaFetchError {
            message: format!("Invalid access token for Authorization header: {}", e),
            details: String::new(),
        })?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| QuotaFetchError {
            message: format!("Failed to create HTTP client: {}", error_chain(&e)),
            details: String::new(),
        })?;
    let started_at = std::time::Instant::now();
    let mut request = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .header(AUTHORIZATION, auth_header)
        .header(ACCEPT, "application/json")
        .header(REFERER, "https://chatgpt.com/")
        .header(USER_AGENT, "Mozilla/5.0");
    if let Some(account_id) = access_token_account_id(token) {
        request = request.header("ChatGPT-Account-Id", account_id);
    }
    let response = request.send().await.map_err(|e| QuotaFetchError {
        message: format!("Request failed: {}", error_chain(&e)),
        details: serde_json::json!({
            "elapsed_ms": started_at.elapsed().as_millis(),
        })
        .to_string(),
    })?;

    let status = response.status();
    let content_type = header_to_string(response.headers(), reqwest::header::CONTENT_TYPE);
    let content_encoding = header_to_string(response.headers(), reqwest::header::CONTENT_ENCODING);
    let elapsed_ms = started_at.elapsed().as_millis();
    let status_text = status.to_string();
    let body = response.bytes().await.map_err(|e| QuotaFetchError {
        message: format!("Failed to read response body: {}", error_chain(&e)),
        details: quota_log_details(
            &status_text,
            &content_type,
            &content_encoding,
            elapsed_ms,
            "",
        ),
    })?;
    let preview = body_preview(&body, 1200);
    let details = quota_log_details(
        &status_text,
        &content_type,
        &content_encoding,
        elapsed_ms,
        &preview,
    );

    if !status.is_success() {
        return Err(QuotaFetchError {
            message: format!("API returned status: {}", status),
            details,
        });
    }

    let api_response: ApiResponse = serde_json::from_slice(&body).map_err(|e| QuotaFetchError {
        message: format!("Failed to parse response JSON: {}", e),
        details,
    })?;
    let rate_limit = api_response.rate_limit.as_ref();
    let primary_window = rate_limit.and_then(|item| item.primary_window.as_ref());
    let secondary_window = rate_limit.and_then(|item| item.secondary_window.as_ref());

    Ok(QuotaInfo {
        plan_type: api_response
            .plan_type
            .unwrap_or_else(|| "unknown".to_string()),
        primary_used_percent: quota_used_percent(primary_window),
        primary_reset_at: quota_reset_time(primary_window),
        primary_window_minutes: quota_window_minutes(primary_window),
        primary_window_present: primary_window.is_some(),
        secondary_used_percent: quota_used_percent(secondary_window),
        secondary_reset_at: quota_reset_time(secondary_window),
        secondary_window_minutes: quota_window_minutes(secondary_window),
        secondary_window_present: secondary_window.is_some(),
    })
}

#[command]
async fn fetch_quota(access_token: String) -> Result<QuotaInfo, String> {
    fetch_quota_internal(access_token)
        .await
        .map_err(|e| e.message)
}

#[command]
fn start_codex_oauth_login(
    app: AppHandle,
    open_browser: Option<bool>,
    force_account_selection: Option<bool>,
) -> Result<OAuthStartResponse, String> {
    if let Some(existing) = OAUTH_STATE
        .lock()
        .map_err(|_| "OAuth state lock is poisoned".to_string())?
        .as_ref()
        .filter(|state| state.expires_at > chrono_like_now_timestamp())
        .cloned()
    {
        if open_browser.unwrap_or(true) {
            open_url_in_browser(&existing.auth_url)?;
        }
        return Ok(OAuthStartResponse {
            login_id: existing.login_id,
            auth_url: existing.auth_url,
        });
    }

    let login_id = random_base64url_token();
    let state = random_base64url_token();
    let code_verifier = random_base64url_token();
    let redirect_uri = format!(
        "http://localhost:{}/auth/callback",
        CODEX_OAUTH_CALLBACK_PORT
    );
    let challenge = code_challenge(&code_verifier);
    let auth_url = build_codex_oauth_url(
        &redirect_uri,
        &challenge,
        &state,
        force_account_selection.unwrap_or(true),
    );
    let listener = TcpListener::bind(("127.0.0.1", CODEX_OAUTH_CALLBACK_PORT)).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            format!("CODEX_OAUTH_PORT_IN_USE:{}", CODEX_OAUTH_CALLBACK_PORT)
        } else {
            format!("Failed to start OAuth callback listener: {}", e)
        }
    })?;

    let mut guard = OAUTH_STATE
        .lock()
        .map_err(|_| "OAuth state lock is poisoned".to_string())?;
    *guard = Some(OAuthState {
        login_id: login_id.clone(),
        auth_url: auth_url.clone(),
        state: state.clone(),
        code_verifier,
        redirect_uri,
        expires_at: chrono_like_now_timestamp() + 300,
        code: None,
    });
    drop(guard);

    start_oauth_callback_listener(app, listener, login_id.clone(), state.clone());

    if open_browser.unwrap_or(true) {
        open_url_in_browser(&auth_url)?;
    }

    Ok(OAuthStartResponse { login_id, auth_url })
}

#[command]
fn open_codex_oauth_url(login_id: String) -> Result<(), String> {
    let auth_url = {
        let guard = OAUTH_STATE
            .lock()
            .map_err(|_| "OAuth state lock is poisoned".to_string())?;
        guard
            .as_ref()
            .filter(|state| state.login_id == login_id)
            .filter(|state| state.expires_at > chrono_like_now_timestamp())
            .map(|state| state.auth_url.clone())
            .ok_or_else(|| "OAuth login state not found, please start login again".to_string())?
    };

    open_url_in_browser(&auth_url)
}

#[command]
fn cancel_codex_oauth_login(login_id: Option<String>) -> Result<(), String> {
    let should_cancel = {
        let guard = OAUTH_STATE
            .lock()
            .map_err(|_| "OAuth state lock is poisoned".to_string())?;
        match (guard.as_ref(), login_id.as_deref()) {
            (Some(_), None) => true,
            (Some(state), Some(id)) => state.login_id == id,
            _ => false,
        }
    };

    if !should_cancel {
        return Ok(());
    }

    {
        let mut guard = OAUTH_STATE
            .lock()
            .map_err(|_| "OAuth state lock is poisoned".to_string())?;
        *guard = None;
    }

    if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", CODEX_OAUTH_CALLBACK_PORT)) {
        let _ = stream
            .write_all(b"GET /cancel HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let _ = stream.flush();
    }

    Ok(())
}

fn save_oauth_account(
    app: &AppHandle,
    auth_json: &str,
    identity: &AccountIdentity,
) -> Result<OAuthSaveResult, String> {
    parse_auth_json(auth_json)?;
    let conn = open_accounts_db(app)?;
    let account_id = identity
        .account_id
        .as_deref()
        .ok_or_else(|| "OAuth login did not return a ChatGPT account id".to_string())?;
    let name = identity
        .email
        .as_deref()
        .or(identity.account_name.as_deref())
        .unwrap_or("Codex OAuth Account");
    let plan_type = identity.plan_type.as_deref().unwrap_or("unknown");
    let candidates = {
        let mut stmt = conn
            .prepare(
                "
                SELECT id, name, json_info
                FROM accounts
                WHERE CASE
                          WHEN json_valid(json_info)
                          THEN COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
                          ELSE ''
                      END = ?1
                ORDER BY id DESC
                ",
            )
            .map_err(|e| format!("Failed to prepare existing OAuth account query: {}", e))?;
        let rows = stmt
            .query_map(params![account_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("Failed to find existing OAuth account: {}", e))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read existing OAuth account: {}", e))?
    };
    let existing = candidates
        .into_iter()
        .find(|(_, existing_name, json_info)| {
            oauth_existing_account_matches_identity(
                existing_name,
                json_info,
                identity.email.as_deref(),
            )
        });

    if let Some((id, _existing_name, _json_info)) = existing {
        conn.execute(
            "
            UPDATE accounts
            SET credential_key = '',
                json_info = ?1,
                plan_type = CASE WHEN ?2 != 'unknown' THEN ?2 ELSE plan_type END,
                last_quota_error = '',
                updated_at = datetime('now')
            WHERE id = ?3
            ",
            params![auth_json, plan_type, id],
        )
        .map_err(|e| format!("Failed to restore OAuth account credential: {}", e))?;

        let (account_name, account_identifier) = account_log_context(&conn, id);
        let _ = insert_operation_log(
            &conn,
            "info",
            "oauth_login",
            Some(id),
            &account_name,
            &account_identifier,
            "updated",
            "OAuth 账号凭据已更新",
            &format!(
                "email={}, account_id={}",
                identity.email.as_deref().unwrap_or(""),
                account_id
            ),
        );

        return Ok(OAuthSaveResult {
            id,
            created: false,
            name: account_name,
            account_id: account_id.to_string(),
        });
    }

    conn.execute(
        "
        INSERT INTO accounts (name, activation_date, credential_key, json_info, plan_type, updated_at)
        VALUES (?1, '', '', ?2, ?3, datetime('now'))
        ",
        params![name, auth_json, plan_type],
    )
    .map_err(|e| format!("Failed to add OAuth account: {}", e))?;
    let id = conn.last_insert_rowid();

    let _ = insert_operation_log(
        &conn,
        "info",
        "oauth_login",
        Some(id),
        name,
        account_id,
        "created",
        "OAuth 账号已新增",
        &format!(
            "email={}, account_id={}",
            identity.email.as_deref().unwrap_or(""),
            account_id
        ),
    );

    Ok(OAuthSaveResult {
        id,
        created: true,
        name: name.to_string(),
        account_id: account_id.to_string(),
    })
}

async fn exchange_oauth_code(
    state: &OAuthState,
    code: &str,
) -> Result<(String, String, String), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", error_chain(&e)))?;
    let response = client
        .post(CODEX_TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", state.redirect_uri.as_str()),
            ("client_id", CODEX_OAUTH_CLIENT_ID),
            ("code_verifier", state.code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("OAuth token request failed: {}", error_chain(&e)))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read OAuth token response: {}", e))?;
    if !status.is_success() {
        return Err(format!(
            "OAuth token exchange failed: status={}, body_len={}",
            status,
            body.len()
        ));
    }

    let token_response: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid OAuth token JSON: {}", e))?;
    let id_token = require_json_string(&token_response, "/id_token", "id_token")?;
    let access_token = require_json_string(&token_response, "/access_token", "access_token")?;
    let refresh_token = require_json_string(&token_response, "/refresh_token", "refresh_token")?;
    Ok((id_token, access_token, refresh_token))
}

async fn save_oauth_tokens(
    app: AppHandle,
    id_token: String,
    access_token: String,
    refresh_token: String,
) -> Result<OAuthSaveResult, String> {
    let mut identity = extract_identity_from_tokens(&id_token, &access_token);
    if let Ok(remote) =
        fetch_remote_account_identity(&access_token, identity.account_id.as_deref()).await
    {
        identity.email = remote.email.or(identity.email);
        identity.account_id = remote.account_id.or(identity.account_id);
        identity.plan_type = remote.plan_type.or(identity.plan_type);
        identity.account_name = remote.account_name.or(identity.account_name);
    }
    let account_id = identity
        .account_id
        .clone()
        .ok_or_else(|| "OAuth login succeeded, but account id could not be detected".to_string())?;

    let auth_json = codex_auth_json(&id_token, &access_token, &refresh_token, &account_id)?;
    let saved = save_oauth_account(&app, &auth_json, &identity)?;
    if let Err(e) = refresh_account_quota(app, saved.id).await {
        eprintln!("Failed to fetch initial OAuth quota: {}", e);
    }

    Ok(saved)
}

#[command]
async fn complete_codex_oauth_login(
    app: AppHandle,
    login_id: String,
    callback_url: Option<String>,
) -> Result<OAuthSaveResult, String> {
    let (state, code) = {
        let mut guard = OAUTH_STATE
            .lock()
            .map_err(|_| "OAuth state lock is poisoned".to_string())?;
        let state_ref = guard
            .as_ref()
            .filter(|state| state.login_id == login_id)
            .ok_or_else(|| "OAuth login state not found, please start login again".to_string())?;

        let code = if let Some(callback_url) = callback_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let callback_state = query_param(callback_url, "state")
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Callback URL missing state parameter".to_string())?;
            if callback_state != state_ref.state {
                return Err(
                    "OAuth state mismatch, please paste the latest callback URL".to_string()
                );
            }
            query_param(callback_url, "code")
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Callback URL missing code parameter".to_string())?
        } else {
            state_ref
                .code
                .clone()
                .ok_or_else(|| "OAuth authorization has not completed yet".to_string())?
        };

        let state = state_ref.clone();
        *guard = None;
        (state, code)
    };

    let (id_token, access_token, refresh_token) = exchange_oauth_code(&state, &code).await?;
    save_oauth_tokens(app, id_token, access_token, refresh_token).await
}

#[command]
fn list_accounts(app: AppHandle) -> Result<Vec<Account>, String> {
    let conn = open_accounts_db(&app)?;
    let mut stmt = conn
        .prepare(
            "
            SELECT id, name, activation_date,
                   CASE
                       WHEN json_valid(json_info)
                       THEN CASE
                           WHEN COALESCE(json_extract(json_info, '$.tokens.access_token'), '') != ''
                           THEN 1 ELSE 0
                       END
                       ELSE 0
                   END AS has_json_info,
                   plan_type,
                   primary_used_percent, primary_reset_at,
                   primary_window_minutes, primary_window_present,
                   secondary_used_percent, secondary_reset_at,
                   secondary_window_minutes, secondary_window_present,
                   last_quota_checked_at, last_quota_error,
                   created_at, updated_at,
                   CASE
                       WHEN json_valid(json_info)
                       THEN COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
                       ELSE ''
                   END
            FROM accounts
            ORDER BY id DESC
            ",
        )
        .map_err(|e| format!("Failed to prepare account query: {}", e))?;

    let rows = stmt
        .query_map([], account_from_row)
        .map_err(|e| format!("Failed to query accounts: {}", e))?;

    let mut accounts = Vec::new();
    for row in rows {
        accounts.push(row.map_err(|e| format!("Failed to read account: {}", e))?);
    }
    Ok(accounts)
}

#[command]
fn get_account_auth_json(app: AppHandle, id: i64) -> Result<String, String> {
    let conn = open_accounts_db(&app)?;
    let json_info: String = conn
        .query_row(
            "SELECT json_info FROM accounts WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to read account auth.json: {}", e))?;

    if json_info_has_credential(&json_info) {
        Ok(json_info)
    } else {
        Ok(String::new())
    }
}

#[command]
fn list_operation_logs(
    app: AppHandle,
    account_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<OperationLog>, String> {
    let conn = open_accounts_db(&app)?;
    let limit = limit.unwrap_or(200).clamp(1, 1000);

    let mut logs = Vec::new();
    if let Some(account_id) = account_id {
        let mut stmt = conn
            .prepare(
                "
                SELECT id, level, action, account_id, account_name, account_identifier,
                       stage, message, details, created_at
                FROM operation_logs
                WHERE account_id = ?1
                ORDER BY id DESC
                LIMIT ?2
                ",
            )
            .map_err(|e| format!("Failed to prepare operation log query: {}", e))?;
        let rows = stmt
            .query_map(params![account_id, limit], operation_log_from_row)
            .map_err(|e| format!("Failed to query operation logs: {}", e))?;
        for row in rows {
            logs.push(row.map_err(|e| format!("Failed to read operation log: {}", e))?);
        }
    } else {
        let mut stmt = conn
            .prepare(
                "
                SELECT id, level, action, account_id, account_name, account_identifier,
                       stage, message, details, created_at
                FROM operation_logs
                ORDER BY id DESC
                LIMIT ?1
                ",
            )
            .map_err(|e| format!("Failed to prepare operation log query: {}", e))?;
        let rows = stmt
            .query_map(params![limit], operation_log_from_row)
            .map_err(|e| format!("Failed to query operation logs: {}", e))?;
        for row in rows {
            logs.push(row.map_err(|e| format!("Failed to read operation log: {}", e))?);
        }
    }

    Ok(logs)
}

#[command]
fn clear_operation_logs(app: AppHandle) -> Result<(), String> {
    let conn = open_accounts_db(&app)?;
    conn.execute("DELETE FROM operation_logs", [])
        .map_err(|e| format!("Failed to clear operation logs: {}", e))?;
    Ok(())
}

#[command]
async fn refresh_account_profile(app: AppHandle, id: i64) -> Result<Account, String> {
    let conn = open_accounts_db(&app)?;
    let json_info = account_json_info(&conn, id)?;
    let (json_info, changed) = refresh_auth_json_if_needed(&json_info, false).await?;
    if changed {
        save_account_json_info(&conn, id, &json_info)?;
    }

    let mut value = parse_auth_json(&json_info)?;
    let access_token = require_json_string(&value, "/tokens/access_token", "tokens.access_token")?;
    let local_identity = extract_identity_from_tokens(
        value
            .pointer("/tokens/id_token")
            .and_then(|item| item.as_str())
            .unwrap_or_default(),
        &access_token,
    );
    let remote_identity =
        fetch_remote_account_identity(&access_token, local_identity.account_id.as_deref())
            .await
            .unwrap_or(local_identity);

    if let Some(account_id) = remote_identity.account_id.as_deref() {
        if let Some(tokens) = value
            .get_mut("tokens")
            .and_then(|item| item.as_object_mut())
        {
            tokens.insert(
                "account_id".to_string(),
                serde_json::Value::String(account_id.to_string()),
            );
        }
    }

    let updated_json = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to serialize refreshed account profile: {}", e))?;
    save_account_json_info(&conn, id, &updated_json)?;

    let plan_type = remote_identity
        .plan_type
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    conn.execute(
        "
        UPDATE accounts
        SET json_info = ?1,
            plan_type = CASE WHEN ?2 != 'unknown' THEN ?2 ELSE plan_type END,
            last_quota_error = '',
            updated_at = datetime('now')
        WHERE id = ?3
        ",
        params![updated_json, plan_type, id],
    )
    .map_err(|e| format!("Failed to update account profile: {}", e))?;

    let mut stmt = conn
        .prepare(
            "
            SELECT id, name, activation_date,
                   CASE
                       WHEN json_valid(json_info)
                       THEN CASE
                           WHEN COALESCE(json_extract(json_info, '$.tokens.access_token'), '') != ''
                           THEN 1 ELSE 0
                       END
                       ELSE 0
                   END AS has_json_info,
                   plan_type,
                   primary_used_percent, primary_reset_at,
                   primary_window_minutes, primary_window_present,
                   secondary_used_percent, secondary_reset_at,
                   secondary_window_minutes, secondary_window_present,
                   last_quota_checked_at, last_quota_error,
                   created_at, updated_at,
                   CASE
                       WHEN json_valid(json_info)
                       THEN COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
                       ELSE ''
                   END
            FROM accounts
            WHERE id = ?1
            ",
        )
        .map_err(|e| format!("Failed to prepare account query: {}", e))?;
    stmt.query_row(params![id], account_from_row)
        .map_err(|e| format!("Failed to read refreshed account: {}", e))
}

#[command]
fn get_migration_status(app: AppHandle) -> Result<MigrationStatus, String> {
    let conn = open_accounts_db(&app)?;
    Ok(MigrationStatus {
        pending_plaintext_accounts: pending_plaintext_account_count(&conn)?,
    })
}

#[command]
fn migrate_plaintext_accounts(app: AppHandle) -> Result<MigrationStatus, String> {
    let conn = open_accounts_db(&app)?;
    migrate_plaintext_credentials(&conn)?;
    Ok(MigrationStatus {
        pending_plaintext_accounts: pending_plaintext_account_count(&conn)?,
    })
}

fn pending_plaintext_account_count(conn: &Connection) -> Result<i64, String> {
    let _ = conn;
    Ok(0)
}

fn migrate_plaintext_credentials(conn: &Connection) -> Result<usize, String> {
    let _ = conn;
    Ok(0)
}

#[command]
async fn add_account(
    app: AppHandle,
    name: String,
    activation_date: String,
    json_info: String,
) -> Result<i64, String> {
    if name.trim().is_empty() {
        return Err("Account name is required".to_string());
    }
    let json_info = normalize_auth_input(&json_info).await?;

    let conn = open_accounts_db(&app)?;
    let stored_json = if json_info.trim().is_empty() {
        "{}"
    } else {
        json_info.trim()
    };
    conn.execute(
        "
        INSERT INTO accounts (name, activation_date, credential_key, json_info, updated_at)
        VALUES (?1, ?2, '', ?3, datetime('now'))
        ",
        params![name.trim(), activation_date, stored_json],
    )
    .map_err(|e| format!("Failed to add account: {}", e))?;

    let id = conn.last_insert_rowid();
    Ok(id)
}

#[command]
async fn update_account(
    app: AppHandle,
    id: i64,
    name: String,
    activation_date: String,
    json_info: String,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Account name is required".to_string());
    }
    let should_update_secret = !json_info.trim().is_empty();
    let json_info = if should_update_secret {
        normalize_auth_input(&json_info).await?
    } else {
        String::new()
    };
    if should_update_secret {
        parse_auth_json(&json_info)?;
    }

    let conn = open_accounts_db(&app)?;
    let exists = match conn.query_row("SELECT 1 FROM accounts WHERE id = ?1", params![id], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(_) => true,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err("Account not found".to_string()),
        Err(e) => return Err(format!("Failed to find account: {}", e)),
    };
    if !exists {
        return Err("Account not found".to_string());
    }

    if should_update_secret {
        conn.execute(
            "
            UPDATE accounts
            SET name = ?1,
                activation_date = ?2,
                credential_key = '',
                json_info = ?3,
                updated_at = datetime('now')
            WHERE id = ?4
            ",
            params![name.trim(), activation_date, json_info.trim(), id],
        )
        .map_err(|e| format!("Failed to update account credential: {}", e))?;

        return Ok(());
    }

    conn.execute(
        "
        UPDATE accounts
        SET name = ?1, activation_date = ?2, updated_at = datetime('now')
        WHERE id = ?3
        ",
        params![name.trim(), activation_date, id],
    )
    .map_err(|e| format!("Failed to update account: {}", e))?;

    Ok(())
}

#[command]
fn delete_account(app: AppHandle, id: i64) -> Result<(), String> {
    let conn = open_accounts_db(&app)?;
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])
        .map_err(|e| format!("Failed to delete account: {}", e))?;
    Ok(())
}

#[command]
fn export_encrypted_backup(
    app: AppHandle,
    password: String,
    account_ids: Option<Vec<i64>>,
) -> Result<String, String> {
    let conn = open_accounts_db(&app)?;
    let filter_ids: Option<std::collections::HashSet<i64>> =
        account_ids.map(|items| items.into_iter().collect());

    let mut stmt = conn
        .prepare(
            "
            SELECT id, name, activation_date, plan_type,
                   primary_used_percent, primary_reset_at,
                   primary_window_minutes, primary_window_present,
                   secondary_used_percent, secondary_reset_at,
                   secondary_window_minutes, secondary_window_present,
                   last_quota_checked_at, last_quota_error,
                   json_info
            FROM accounts
            ORDER BY id ASC
            ",
        )
        .map_err(|e| format!("Failed to prepare backup export: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)? != 0,
                row.get::<_, i32>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, i64>(11)? != 0,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
            ))
        })
        .map_err(|e| format!("Failed to query backup accounts: {}", e))?;

    let mut accounts = Vec::new();
    for row in rows {
        let (
            id,
            name,
            activation_date,
            plan_type,
            primary_used_percent,
            primary_reset_at,
            primary_window_minutes,
            primary_window_present,
            secondary_used_percent,
            secondary_reset_at,
            secondary_window_minutes,
            secondary_window_present,
            last_quota_checked_at,
            last_quota_error,
            stored_json_info,
        ) = row.map_err(|e| format!("Failed to read backup account: {}", e))?;
        if let Some(filter_ids) = &filter_ids {
            if !filter_ids.contains(&id) {
                continue;
            }
        }
        let json_info = if json_info_has_credential(&stored_json_info) {
            stored_json_info
        } else {
            continue;
        };
        accounts.push(BackupAccount {
            name,
            activation_date,
            json_info,
            plan_type,
            primary_used_percent,
            primary_reset_at,
            primary_window_minutes,
            primary_window_present,
            secondary_used_percent,
            secondary_reset_at,
            secondary_window_minutes,
            secondary_window_present,
            last_quota_checked_at,
            last_quota_error,
        });
    }

    encrypt_backup_payload(
        &BackupPayload {
            version: 1,
            accounts,
        },
        &password,
    )
}

fn backup_file_name() -> String {
    let now = chrono::Local::now();
    format!(
        "codex-accounts-backup-{}.json",
        now.format("%Y-%m-%d-%H-%M")
    )
}

fn default_backup_export_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    if let Ok(path) = app.path().download_dir() {
        return Ok(path);
    }
    if let Ok(path) = app.path().desktop_dir() {
        return Ok(path);
    }
    Ok(std::path::PathBuf::from(get_home_dir()?))
}

fn unique_backup_path(dir: std::path::PathBuf, file_name: &str) -> std::path::PathBuf {
    let base = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|item| item.to_str())
        .unwrap_or("codex-accounts-backup");
    let ext = std::path::Path::new(file_name)
        .extension()
        .and_then(|item| item.to_str())
        .unwrap_or("json");
    let first = dir.join(file_name);
    if !first.exists() {
        return first;
    }
    for index in 2..1000 {
        let candidate = dir.join(format!("{}-{}.{}", base, index, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!(
        "{}-{}.{}",
        base,
        chrono::Local::now().timestamp(),
        ext
    ))
}

#[command]
fn export_encrypted_backup_file(
    app: AppHandle,
    password: String,
    account_ids: Option<Vec<i64>>,
) -> Result<String, String> {
    let backup_text = export_encrypted_backup(app.clone(), password, account_ids)?;
    let export_dir = default_backup_export_dir(&app)?;
    std::fs::create_dir_all(&export_dir).map_err(|e| format!("创建备份目录失败: {}", e))?;
    let path = unique_backup_path(export_dir, &backup_file_name());
    std::fs::write(&path, backup_text).map_err(|e| format!("写入备份文件失败: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

async fn normalized_backup_accounts(
    backup_text: &str,
    password: &str,
) -> Result<(u32, Vec<BackupAccount>), String> {
    let payload = decrypt_backup_payload(&backup_text, &password)?;
    if payload.version != 1 {
        return Err("Unsupported backup payload version".to_string());
    }

    let mut normalized_accounts = Vec::new();
    for mut account in payload.accounts {
        account.json_info = normalize_auth_input(&account.json_info)
            .await
            .map_err(|e| format!("Invalid account JSON in backup: {}", e))?;
        parse_auth_json(&account.json_info)
            .map_err(|e| format!("Invalid account JSON in backup: {}", e))?;
        normalized_accounts.push(account);
    }
    Ok((payload.version, normalized_accounts))
}

fn existing_account_ids(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, i64>, String> {
    let mut stmt = conn
        .prepare(
            "
            SELECT id,
                   CASE
                       WHEN json_valid(json_info)
                       THEN COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
                       ELSE ''
                   END
            FROM accounts
            WHERE CASE
                      WHEN json_valid(json_info)
                      THEN COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
                      ELSE ''
                  END != ''
            ORDER BY id ASC
            ",
        )
        .map_err(|e| format!("Failed to prepare existing account lookup: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query existing accounts: {}", e))?;

    let mut existing = std::collections::HashMap::new();
    for row in rows {
        let (id, account_id) =
            row.map_err(|e| format!("Failed to read existing account lookup: {}", e))?;
        existing.entry(account_id).or_insert(id);
    }
    Ok(existing)
}

fn insert_backup_account(
    conn: &Connection,
    account: &BackupAccount,
    imported_credentials: &mut Vec<(i64, String)>,
) -> Result<(), String> {
    conn.execute(
        "
        INSERT INTO accounts (
            name, activation_date, json_info, plan_type,
            primary_used_percent, primary_reset_at,
            primary_window_minutes, primary_window_present,
            secondary_used_percent, secondary_reset_at,
            secondary_window_minutes, secondary_window_present,
            last_quota_checked_at, last_quota_error,
            updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, datetime('now'))
        ",
        params![
            account.name,
            account.activation_date,
            account.json_info,
            account.plan_type,
            account.primary_used_percent,
            account.primary_reset_at,
            account.primary_window_minutes,
            if account.primary_window_present { 1 } else { 0 },
            account.secondary_used_percent,
            account.secondary_reset_at,
            account.secondary_window_minutes,
            if account.secondary_window_present {
                1
            } else {
                0
            },
            account.last_quota_checked_at,
            account.last_quota_error,
        ],
    )
    .map_err(|e| format!("Failed to import account: {}", e))?;

    let id = conn.last_insert_rowid();
    imported_credentials.push((id, String::new()));
    Ok(())
}

fn merge_backup_account(conn: &Connection, id: i64, account: &BackupAccount) -> Result<(), String> {
    conn.execute(
        "
        UPDATE accounts
        SET name = ?1,
            activation_date = ?2,
            credential_key = '',
            json_info = ?3,
            plan_type = ?4,
            primary_used_percent = ?5,
            primary_reset_at = ?6,
            primary_window_minutes = ?7,
            primary_window_present = ?8,
            secondary_used_percent = ?9,
            secondary_reset_at = ?10,
            secondary_window_minutes = ?11,
            secondary_window_present = ?12,
            last_quota_checked_at = ?13,
            last_quota_error = ?14,
            updated_at = datetime('now')
        WHERE id = ?15
        ",
        params![
            account.name,
            account.activation_date,
            account.json_info,
            account.plan_type,
            account.primary_used_percent,
            account.primary_reset_at,
            account.primary_window_minutes,
            if account.primary_window_present { 1 } else { 0 },
            account.secondary_used_percent,
            account.secondary_reset_at,
            account.secondary_window_minutes,
            if account.secondary_window_present {
                1
            } else {
                0
            },
            account.last_quota_checked_at,
            account.last_quota_error,
            id,
        ],
    )
    .map_err(|e| format!("Failed to merge account: {}", e))?;
    Ok(())
}

#[command]
async fn preview_encrypted_backup(
    app: AppHandle,
    backup_text: String,
    password: String,
) -> Result<BackupPreview, String> {
    let (version, accounts) = normalized_backup_accounts(&backup_text, &password).await?;
    let conn = open_accounts_db(&app)?;
    let existing = existing_account_ids(&conn)?;
    let duplicate_accounts = accounts
        .iter()
        .filter(|account| {
            extract_account_id(&account.json_info)
                .as_ref()
                .map(|account_id| existing.contains_key(account_id))
                .unwrap_or(false)
        })
        .count();
    let account_names = accounts
        .iter()
        .take(8)
        .map(|account| account.name.clone())
        .collect();

    Ok(BackupPreview {
        version,
        total_accounts: accounts.len(),
        duplicate_accounts,
        new_accounts: accounts.len().saturating_sub(duplicate_accounts),
        account_names,
    })
}

#[command]
async fn import_encrypted_backup(
    app: AppHandle,
    backup_text: String,
    password: String,
    strategy: Option<String>,
) -> Result<ImportBackupResult, String> {
    let (_, normalized_accounts) = normalized_backup_accounts(&backup_text, &password).await?;

    let conn = open_accounts_db(&app)?;
    let mut existing = existing_account_ids(&conn)?;
    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut updated = 0usize;
    let mut imported_credentials: Vec<(i64, String)> = Vec::new();
    let strategy = strategy.unwrap_or_else(|| "add".to_string());

    for account in normalized_accounts {
        let account_id = extract_account_id(&account.json_info);
        let existing_id = account_id
            .as_ref()
            .and_then(|item| existing.get(item).copied());
        let result = match (strategy.as_str(), existing_id) {
            ("skip_duplicates", Some(_)) => {
                skipped += 1;
                Ok(())
            }
            ("merge_by_account_id", Some(id)) => {
                merge_backup_account(&conn, id, &account).map(|_| {
                    updated += 1;
                })
            }
            ("add" | "skip_duplicates" | "merge_by_account_id", _) => {
                insert_backup_account(&conn, &account, &mut imported_credentials).map(|_| {
                    imported += 1;
                    if let Some(account_id) = account_id {
                        existing
                            .entry(account_id)
                            .or_insert(conn.last_insert_rowid());
                    }
                })
            }
            _ => Err("Unsupported import strategy".to_string()),
        };

        if let Err(e) = result {
            cleanup_imported_accounts(&conn, &imported_credentials);
            return Err(e);
        }
    }

    Ok(ImportBackupResult {
        imported,
        skipped,
        updated,
    })
}

fn cleanup_imported_accounts(conn: &Connection, imported_credentials: &[(i64, String)]) {
    for (id, _key) in imported_credentials {
        let _ = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id]);
    }
}

#[command]
async fn refresh_account_quota(app: AppHandle, id: i64) -> Result<QuotaInfo, String> {
    let (account_name, account_identifier, access_token) = {
        let conn = open_accounts_db(&app)?;
        let (account_name, account_identifier) = account_log_context(&conn, id);
        let _ = insert_operation_log(
            &conn,
            "info",
            "refresh_quota",
            Some(id),
            &account_name,
            &account_identifier,
            "start",
            "开始刷新额度",
            "",
        );
        let json_info = account_json_info(&conn, id)?;
        let refreshed_json = match refresh_auth_json_if_needed(&json_info, false).await {
            Ok((updated_json, changed)) => {
                if changed {
                    save_account_json_info(&conn, id, &updated_json)?;
                }
                updated_json
            }
            Err(e) => {
                let _ = insert_operation_log(
                    &conn,
                    "error",
                    "refresh_quota",
                    Some(id),
                    &account_name,
                    &account_identifier,
                    "refresh_token",
                    &e,
                    "",
                );
                mark_quota_error(&conn, id, &e)?;
                return Err(e);
            }
        };
        match extract_access_token(&refreshed_json) {
            Ok(token) => (account_name, account_identifier, token),
            Err(e) => {
                let _ = insert_operation_log(
                    &conn,
                    "error",
                    "refresh_quota",
                    Some(id),
                    &account_name,
                    &account_identifier,
                    "extract_access_token",
                    &e,
                    "",
                );
                mark_quota_error(&conn, id, &e)?;
                return Err(e);
            }
        }
    };

    match fetch_quota_internal(access_token).await {
        Ok(quota) => {
            update_account_quota(app.clone(), id, quota.clone())?;
            let conn = open_accounts_db(&app)?;
            let details = serde_json::json!({
                "plan_type": quota.plan_type,
                "primary_used_percent": quota.primary_used_percent,
                "primary_reset_at": quota.primary_reset_at,
                "primary_window_minutes": quota.primary_window_minutes,
                "primary_window_present": quota.primary_window_present,
                "secondary_used_percent": quota.secondary_used_percent,
                "secondary_reset_at": quota.secondary_reset_at,
                "secondary_window_minutes": quota.secondary_window_minutes,
                "secondary_window_present": quota.secondary_window_present,
            })
            .to_string();
            let _ = insert_operation_log(
                &conn,
                "info",
                "refresh_quota",
                Some(id),
                &account_name,
                &account_identifier,
                "quota_api",
                "额度刷新成功",
                &details,
            );
            Ok(quota)
        }
        Err(e) => {
            let conn = open_accounts_db(&app)?;
            let _ = insert_operation_log(
                &conn,
                "error",
                "refresh_quota",
                Some(id),
                &account_name,
                &account_identifier,
                "quota_api",
                &e.message,
                &e.details,
            );
            mark_quota_error(&conn, id, &e.message)?;
            Err(e.message)
        }
    }
}

#[command]
async fn check_account_health(app: AppHandle, id: i64) -> Result<AccountHealthReport, String> {
    let mut items = Vec::new();
    let mut can_check_quota = true;

    let secret_result = {
        let conn = open_accounts_db(&app)?;
        account_json_info(&conn, id)
    };

    match secret_result {
        Ok(json_info) => {
            items.push(health_item(
                "credential",
                "凭据读取",
                "ok",
                "本地数据库可读取",
            ));

            match serde_json::from_str::<serde_json::Value>(&json_info) {
                Ok(value) => {
                    items.push(health_item("json", "JSON 结构", "ok", "auth.json 可解析"));

                    let access_token = value
                        .pointer("/tokens/access_token")
                        .and_then(|item| item.as_str())
                        .filter(|item| !item.trim().is_empty());
                    let refresh_token = value
                        .pointer("/tokens/refresh_token")
                        .and_then(|item| item.as_str())
                        .filter(|item| !item.trim().is_empty());
                    let account_id = value
                        .pointer("/tokens/account_id")
                        .and_then(|item| item.as_str())
                        .filter(|item| !item.trim().is_empty());
                    let id_token = value
                        .pointer("/tokens/id_token")
                        .and_then(|item| item.as_str())
                        .filter(|item| !item.trim().is_empty());

                    if let Some(access_token) = access_token {
                        items.push(health_item("access_token", "Access Token", "ok", "存在"));
                        if is_token_expired(access_token) {
                            items.push(health_item(
                                "access_token_expiry",
                                "Access Token 有效期",
                                "warn",
                                jwt_expiration_message(access_token),
                            ));
                        } else {
                            items.push(health_item(
                                "access_token_expiry",
                                "Access Token 有效期",
                                "ok",
                                jwt_expiration_message(access_token),
                            ));
                        }
                    } else {
                        can_check_quota = false;
                        items.push(health_item("access_token", "Access Token", "error", "缺失"));
                    }

                    if refresh_token.is_some() {
                        items.push(health_item(
                            "refresh_token",
                            "Refresh Token",
                            "ok",
                            "存在，可在过期时刷新",
                        ));
                    } else {
                        items.push(health_item(
                            "refresh_token",
                            "Refresh Token",
                            "warn",
                            "缺失，access token 过期后需要重新授权",
                        ));
                    }

                    if let Some(account_id) = account_id {
                        items.push(health_item("account_id", "Account ID", "ok", account_id));
                    } else {
                        can_check_quota = false;
                        items.push(health_item("account_id", "Account ID", "error", "缺失"));
                    }

                    if id_token.is_some() {
                        items.push(health_item("id_token", "ID Token", "ok", "存在"));
                    } else {
                        items.push(health_item(
                            "id_token",
                            "ID Token",
                            "warn",
                            "缺失，不影响切换但资料识别可能不完整",
                        ));
                    }
                }
                Err(e) => {
                    can_check_quota = false;
                    items.push(health_item(
                        "json",
                        "JSON 结构",
                        "error",
                        format!("解析失败: {}", e),
                    ));
                }
            }
        }
        Err(e) => {
            can_check_quota = false;
            items.push(health_item("credential", "凭据读取", "error", e));
        }
    }

    if can_check_quota {
        match refresh_account_quota(app.clone(), id).await {
            Ok(_) => items.push(health_item(
                "quota_api",
                "Quota/API",
                "ok",
                "额度接口调用成功",
            )),
            Err(e) => items.push(health_item("quota_api", "Quota/API", "error", e)),
        }
    } else {
        items.push(health_item(
            "quota_api",
            "Quota/API",
            "warn",
            "本地凭据不完整，已跳过网络检查",
        ));
    }

    Ok(AccountHealthReport {
        account_id: id,
        checked_at: codex_last_refresh_string(),
        summary_status: health_summary_status(&items),
        items,
    })
}

#[command]
async fn switch_account_by_id(
    app: AppHandle,
    id: i64,
    restart_codex: Option<bool>,
) -> Result<SwitchAccountResult, String> {
    let conn = open_accounts_db(&app)?;
    let _ = write_proxy_active_account_id(&conn, id);
    let (account_name, account_identifier) = account_log_context(&conn, id);
    let json_info = account_json_info(&conn, id)?;
    let (json_info, changed) = refresh_auth_json_if_needed(&json_info, false).await?;
    if changed {
        save_account_json_info(&conn, id, &json_info)?;
    }
    let result = switch_account(json_info, restart_codex).await?;
    let (level, stage) = match result.hot_switch.status.as_str() {
        "failed" => ("warn", "hot_switch_failed"),
        "unavailable" => ("info", "hot_switch_unavailable"),
        "applied" => ("info", "hot_switch_applied"),
        _ => ("info", "completed"),
    };
    let _ = insert_operation_log(
        &conn,
        level,
        "switch_account",
        Some(id),
        &account_name,
        &account_identifier,
        stage,
        &result.hot_switch.message,
        &result.hot_switch.detail,
    );
    Ok(result)
}

#[command]
fn update_account_quota(app: AppHandle, id: i64, quota: QuotaInfo) -> Result<(), String> {
    let conn = open_accounts_db(&app)?;
    let changed = conn
        .execute(
            "
            UPDATE accounts SET
                plan_type = ?1,
                primary_used_percent = ?2,
                primary_reset_at = ?3,
                primary_window_minutes = ?4,
                primary_window_present = ?5,
                secondary_used_percent = ?6,
                secondary_reset_at = ?7,
                secondary_window_minutes = ?8,
                secondary_window_present = ?9,
                last_quota_checked_at = datetime('now'),
                last_quota_error = '',
                updated_at = datetime('now')
            WHERE id = ?10
            ",
            params![
                quota.plan_type,
                quota.primary_used_percent,
                quota.primary_reset_at,
                quota.primary_window_minutes,
                if quota.primary_window_present { 1 } else { 0 },
                quota.secondary_used_percent,
                quota.secondary_reset_at,
                quota.secondary_window_minutes,
                if quota.secondary_window_present { 1 } else { 0 },
                id
            ],
        )
        .map_err(|e| format!("Failed to update quota: {}", e))?;

    if changed == 0 {
        return Err("Account not found".to_string());
    }
    Ok(())
}

fn saved_codex_proxy_port(conn: &Connection) -> Result<u16, String> {
    Ok(read_setting_from_conn(conn, CODEX_PROXY_SETTING_PORT)?
        .and_then(|value| value.trim().parse::<u16>().ok())
        .filter(|port| *port > 0)
        .unwrap_or(CODEX_PROXY_DEFAULT_PORT))
}

fn runtime_proxy_snapshot() -> Result<(bool, Option<u16>, String), String> {
    let guard = CODEX_PROXY_RUNTIME
        .lock()
        .map_err(|_| "Codex proxy runtime lock is poisoned".to_string())?;
    if let Some(runtime) = guard.as_ref() {
        let enabled = !runtime.stop.load(Ordering::Relaxed);
        let last_error = runtime
            .last_error
            .lock()
            .map(|item| item.clone())
            .unwrap_or_default();
        Ok((enabled, Some(runtime.port), last_error))
    } else {
        Ok((false, None, String::new()))
    }
}

fn codex_proxy_state(app: &AppHandle, conn: &Connection) -> Result<CodexProxyState, String> {
    let (enabled, runtime_port, last_error) = runtime_proxy_snapshot()?;
    let port = runtime_port.unwrap_or(saved_codex_proxy_port(conn)?);
    let active_account_id = read_proxy_active_account_id(conn)?;
    let active_account_name = active_account_id
        .map(|id| account_log_context(conn, id).0)
        .unwrap_or_default();
    let config_path = get_codex_config_path()?;
    let config_text = std::fs::read_to_string(&config_path).unwrap_or_default();
    let _ = app;

    Ok(CodexProxyState {
        enabled,
        port,
        base_url: codex_proxy_base_url(port),
        active_account_id,
        active_account_name,
        config_installed: codex_proxy_config_installed(&config_text),
        config_path: config_path.to_string_lossy().to_string(),
        last_error,
    })
}

fn start_codex_proxy_for_app(app: &AppHandle, port: u16) -> Result<(), String> {
    let db_path = get_database_path(app)?;
    start_codex_proxy_runtime(db_path, port)
}

fn restore_codex_proxy_on_startup(app: AppHandle) {
    let result = (|| -> Result<(), String> {
        let conn = open_accounts_db(&app)?;
        let enabled = read_setting_from_conn(&conn, CODEX_PROXY_SETTING_ENABLED)?
            .map(|value| value == "true")
            .unwrap_or(false);
        if enabled {
            let port = saved_codex_proxy_port(&conn)?;
            start_codex_proxy_for_app(&app, port)?;
        }
        Ok(())
    })();

    if let Err(error) = result {
        eprintln!("Failed to restore Codex proxy: {}", error);
    }
}

#[command]
fn get_codex_proxy_state(app: AppHandle) -> Result<CodexProxyState, String> {
    let conn = open_accounts_db(&app)?;
    codex_proxy_state(&app, &conn)
}

#[command]
fn activate_codex_proxy(
    app: AppHandle,
    account_id: Option<i64>,
    port: Option<u16>,
) -> Result<CodexProxyState, String> {
    let conn = open_accounts_db(&app)?;
    let port = port.unwrap_or(saved_codex_proxy_port(&conn)?);
    let account_id = account_id
        .or(read_proxy_active_account_id(&conn)?)
        .or(fallback_proxy_account_id(&conn)?)
        .ok_or_else(|| "没有可用于代理的账号，请先添加 OAuth 账号。".to_string())?;
    account_json_info(&conn, account_id)?;
    write_proxy_active_account_id(&conn, account_id)?;
    write_setting_to_conn(&conn, CODEX_PROXY_SETTING_PORT, &port.to_string())?;
    write_setting_to_conn(&conn, CODEX_PROXY_SETTING_ENABLED, "true")?;
    install_codex_proxy_config_to_path(&get_codex_config_path()?, &conn, port)?;
    start_codex_proxy_for_app(&app, port)?;
    codex_proxy_state(&app, &conn)
}

#[command]
fn deactivate_codex_proxy(app: AppHandle) -> Result<CodexProxyState, String> {
    let conn = open_accounts_db(&app)?;
    stop_codex_proxy_runtime()?;
    write_setting_to_conn(&conn, CODEX_PROXY_SETTING_ENABLED, "false")?;
    restore_codex_proxy_config_from_backup(&get_codex_config_path()?, &conn)?;
    codex_proxy_state(&app, &conn)
}

#[command]
fn set_codex_proxy_account(app: AppHandle, account_id: i64) -> Result<CodexProxyState, String> {
    let conn = open_accounts_db(&app)?;
    let (was_running, _runtime_port, _last_error) = runtime_proxy_snapshot()?;
    let config_path = get_codex_config_path()?;
    let config_text = std::fs::read_to_string(&config_path).unwrap_or_default();
    let should_keep_enabled = was_running
        || read_setting_from_conn(&conn, CODEX_PROXY_SETTING_ENABLED)?
            .map(|value| value == "true")
            .unwrap_or(false)
        || codex_proxy_config_installed(&config_text);

    account_json_info(&conn, account_id)?;
    write_proxy_active_account_id(&conn, account_id)?;

    if should_keep_enabled {
        let port = saved_codex_proxy_port(&conn)?;
        write_setting_to_conn(&conn, CODEX_PROXY_SETTING_ENABLED, "true")?;
        install_codex_proxy_config_to_path(&config_path, &conn, port)?;
        start_codex_proxy_for_app(&app, port)?;
    }

    codex_proxy_state(&app, &conn)
}

#[command]
fn get_setting(app: AppHandle, key: String) -> Result<Option<String>, String> {
    let conn = open_accounts_db(&app)?;
    read_setting_from_conn(&conn, &key)
}

#[command]
fn set_setting(app: AppHandle, key: String, value: String) -> Result<(), String> {
    let conn = open_accounts_db(&app)?;
    write_setting_to_conn(&conn, &key, &value)
}

#[command]
async fn switch_account(
    json_info: String,
    restart_codex: Option<bool>,
) -> Result<SwitchAccountResult, String> {
    // Guardrail: switching accounts writes only ~/.codex/auth.json. Any
    // config.toml repair or speed setting must stay behind explicit commands.
    if json_info.trim().is_empty() {
        return Err("JSON info is empty, aborting switch".to_string());
    }

    let json_info = canonicalize_auth_json(&json_info)?;

    let should_restart = restart_codex.unwrap_or(true);
    if should_restart {
        kill_codex_process()?;
    }

    let auth_path = get_auth_path()?;
    if let Some(parent) = auth_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .codex directory: {}", e))?;
    }
    std::fs::write(&auth_path, &json_info)
        .map_err(|e| format!("Failed to write auth.json: {}", e))?;

    if should_restart {
        restart_codex_process()?;
    }

    let hot_switch = if should_restart {
        skipped_hot_switch_result("已重启 Codex，不需要单独热切号")
    } else {
        try_hot_switch_codex_app_server(&json_info)
    };

    Ok(SwitchAccountResult {
        restarted: should_restart,
        auth_json_path: auth_path.to_string_lossy().to_string(),
        hot_switch,
    })
}

#[command]
async fn write_auth_json(json_info: String) -> Result<(), String> {
    // Guardrail: direct auth writes must not mutate ~/.codex/config.toml.
    if json_info.trim().is_empty() {
        return Err("JSON info is empty".to_string());
    }

    let json_info = canonicalize_auth_json(&json_info)?;

    let auth_path = get_auth_path()?;
    if let Some(parent) = auth_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .codex directory: {}", e))?;
    }
    std::fs::write(&auth_path, &json_info)
        .map_err(|e| format!("Failed to write auth.json: {}", e))?;

    Ok(())
}

#[command]
async fn read_auth_json() -> Result<String, String> {
    let auth_path = get_auth_path()?;

    if !auth_path.exists() {
        return Ok("{}".to_string());
    }

    std::fs::read_to_string(&auth_path).map_err(|e| format!("Failed to read auth.json: {}", e))
}

#[command]
async fn get_current_account_record_id(app: AppHandle) -> Result<Option<i64>, String> {
    let current_json = read_auth_json().await?;
    let current_value = serde_json::from_str::<serde_json::Value>(&current_json)
        .map_err(|e| format!("Invalid current auth.json: {}", e))?;
    let current_access_token = current_value
        .pointer("/tokens/access_token")
        .and_then(|item| item.as_str())
        .filter(|item| !item.trim().is_empty());
    let current_account_id = current_value
        .pointer("/tokens/account_id")
        .and_then(|item| item.as_str())
        .filter(|item| !item.trim().is_empty());

    let Some(current_access_token) = current_access_token else {
        return Ok(None);
    };

    let conn = open_accounts_db(&app)?;
    let mut stmt = conn
        .prepare(
            "
            SELECT id, json_info,
                   CASE
                       WHEN json_valid(json_info)
                       THEN COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
                       ELSE ''
                   END
            FROM accounts
            WHERE CASE
                      WHEN json_valid(json_info)
                      THEN COALESCE(json_extract(json_info, '$.tokens.access_token'), '')
                      ELSE ''
                  END != ''
            ORDER BY id DESC
            ",
        )
        .map_err(|e| format!("Failed to prepare current account query: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("Failed to query current account candidates: {}", e))?;

    let mut account_id_matches = Vec::new();
    for row in rows {
        let (id, stored_json_info, account_id) =
            row.map_err(|e| format!("Failed to read current account candidate: {}", e))?;
        if current_account_id == Some(account_id.as_str()) {
            account_id_matches.push(id);
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&stored_json_info) else {
            continue;
        };
        let Some(access_token) = value
            .pointer("/tokens/access_token")
            .and_then(|item| item.as_str())
        else {
            continue;
        };
        if access_token == current_access_token {
            return Ok(Some(id));
        }
    }

    if account_id_matches.len() == 1 {
        Ok(account_id_matches.first().copied())
    } else {
        Ok(None)
    }
}

#[command]
async fn get_codex_auth_path() -> Result<String, String> {
    let auth_path = get_auth_path()?;
    Ok(auth_path.to_string_lossy().to_string())
}

#[command]
async fn get_storage_paths(app: AppHandle) -> Result<StoragePaths, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot find app data directory: {}", e))?;
    let database_path = get_database_path(&app)?;
    let auth_json_path = get_auth_path()?;

    Ok(StoragePaths {
        app_data_dir: app_data_dir.to_string_lossy().to_string(),
        database_path: database_path.to_string_lossy().to_string(),
        auth_json_path: auth_json_path.to_string_lossy().to_string(),
    })
}

#[command]
async fn get_codex_app_speed_config() -> Result<CodexAppSpeedConfig, String> {
    let config_path = get_codex_config_path()?;
    let global_state_path = get_codex_global_state_path()?;
    let speed = read_codex_app_speed_from_path(&config_path)?;
    Ok(CodexAppSpeedConfig {
        speed,
        config_path: config_path.to_string_lossy().to_string(),
        global_state_path: global_state_path.to_string_lossy().to_string(),
    })
}

#[command]
async fn set_codex_app_speed(speed: CodexAppSpeed) -> Result<CodexAppSpeedConfig, String> {
    let config_path = get_codex_config_path()?;
    let global_state_path = get_codex_global_state_path()?;
    write_codex_app_speed_to_path(&config_path, &global_state_path, speed)
}

#[command]
async fn get_codex_feature_status() -> Result<CodexFeatureStatus, String> {
    let config_path = get_codex_config_path()?;
    let global_state_path = get_codex_global_state_path()?;
    read_codex_feature_status_from_paths(&config_path, &global_state_path)
}

#[command]
async fn repair_codex_app_speed_state() -> Result<CodexFeatureStatus, String> {
    let config_path = get_codex_config_path()?;
    let global_state_path = get_codex_global_state_path()?;
    let speed = read_codex_app_speed_from_path(&config_path)?;
    sync_codex_global_state(&global_state_path, &speed)?;
    read_codex_feature_status_from_paths(&config_path, &global_state_path)
}

fn default_project_visibility_path() -> String {
    std::env::current_dir()
        .ok()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[command]
async fn get_codex_project_visibility_status(
    project_path: Option<String>,
) -> Result<CodexProjectVisibilityStatus, String> {
    let project_path = project_path
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(default_project_visibility_path);
    if project_path.trim().is_empty() {
        return Err("Project path is empty".to_string());
    }

    let config_path = get_codex_config_path()?;
    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("读取 Codex config.toml 失败: {}", e)),
    };

    let is_trusted = is_project_trusted_in_config(&content, &project_path);
    Ok(CodexProjectVisibilityStatus {
        project_path,
        config_path: config_path.to_string_lossy().to_string(),
        is_trusted,
        changed: false,
    })
}

#[command]
async fn repair_codex_project_visibility(
    project_path: String,
) -> Result<CodexProjectVisibilityStatus, String> {
    if project_path.trim().is_empty() {
        return Err("Project path is empty".to_string());
    }

    let config_path = get_codex_config_path()?;
    let content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("读取 Codex config.toml 失败: {}", e)),
    };
    let (next, changed) = codex_config_toml_with_trusted_project(&content, &project_path);

    if changed {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建 Codex 配置目录失败: {}", e))?;
        }
        std::fs::write(&config_path, next)
            .map_err(|e| format!("写入 Codex config.toml 失败: {}", e))?;
    }

    Ok(CodexProjectVisibilityStatus {
        project_path,
        config_path: config_path.to_string_lossy().to_string(),
        is_trusted: true,
        changed,
    })
}

#[derive(Debug, Clone)]
struct CodexRolloutFile {
    path: PathBuf,
    relative_path: PathBuf,
    archived: bool,
}

#[derive(Debug, Clone)]
struct CodexRolloutThreadMetadata {
    path: PathBuf,
    relative_path: PathBuf,
    archived: bool,
    id: String,
    model_provider: String,
    cwd: String,
    created_at: i64,
    created_at_ms: i64,
    updated_at: i64,
    updated_at_ms: i64,
    source: String,
    thread_source: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    cli_version: String,
    title: String,
    preview: String,
    sandbox_policy: String,
    approval_mode: String,
    tokens_used: i64,
    first_user_message: String,
    memory_mode: String,
}

#[derive(Debug, Clone)]
struct SqliteVisibilityState {
    exists: bool,
    provider: String,
}

const USER_MESSAGE_BEGIN_MARKER: &str = "## My request for Codex:";

fn codex_state_db_path_for_home(codex_home: &Path) -> PathBuf {
    codex_home.join(CODEX_STATE_DB_FILE)
}

fn codex_session_index_path_for_home(codex_home: &Path) -> PathBuf {
    codex_home.join(CODEX_SESSION_INDEX_FILE)
}

fn collect_codex_rollout_files(codex_home: &Path) -> Result<Vec<CodexRolloutFile>, String> {
    let mut files = Vec::new();
    for (subdir, archived) in [
        (CODEX_SESSIONS_DIR, false),
        (CODEX_ARCHIVED_SESSIONS_DIR, true),
    ] {
        let root = codex_home.join(subdir);
        if !root.exists() {
            continue;
        }
        collect_codex_rollout_files_recursive(codex_home, &root, archived, &mut files)?;
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

fn collect_codex_rollout_files_recursive(
    codex_home: &Path,
    dir: &Path,
    archived: bool,
    files: &mut Vec<CodexRolloutFile>,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("读取 Codex 会话目录失败: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取 Codex 会话文件失败: {}", e))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取 Codex 会话文件类型失败: {}", e))?;
        if file_type.is_dir() {
            collect_codex_rollout_files_recursive(codex_home, &path, archived, files)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|item| item.to_str()) else {
            continue;
        };
        if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
            continue;
        }
        let relative_path = path.strip_prefix(codex_home).unwrap_or(&path).to_path_buf();
        files.push(CodexRolloutFile {
            path,
            relative_path,
            archived,
        });
    }
    Ok(())
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::to_string)
        .filter(|item| !item.trim().is_empty())
}

fn json_state_string(value: &serde_json::Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    if let Some(text) = value.as_str() {
        return (!text.trim().is_empty()).then(|| text.to_string());
    }
    serde_json::to_string(value)
        .ok()
        .filter(|item| !item.trim().is_empty())
}

fn parse_rfc3339_epoch(value: &str) -> Option<(i64, i64)> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value).ok()?;
    Some((parsed.timestamp(), parsed.timestamp_millis()))
}

fn file_modified_epoch(path: &Path) -> Option<(i64, i64)> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = i64::try_from(duration.as_secs()).ok()?;
    let millis = seconds
        .saturating_mul(1000)
        .saturating_add(i64::from(duration.subsec_millis()));
    Some((seconds, millis))
}

fn strip_user_message_prefix(text: &str) -> String {
    match text.find(USER_MESSAGE_BEGIN_MARKER) {
        Some(index) => text[index + USER_MESSAGE_BEGIN_MARKER.len()..]
            .trim()
            .to_string(),
        None => text.trim().to_string(),
    }
}

fn default_rollout_title(id: &str) -> String {
    format!("Codex session {}", id)
}

fn parse_codex_rollout_metadata(
    file: &CodexRolloutFile,
) -> Result<Option<CodexRolloutThreadMetadata>, String> {
    let content = std::fs::read_to_string(&file.path)
        .map_err(|e| format!("读取 rollout 文件失败 {}: {}", file.path.display(), e))?;
    let file_time = file_modified_epoch(&file.path).unwrap_or((0, 0));

    let mut id = String::new();
    let mut model_provider = String::new();
    let mut cwd = String::new();
    let mut created_at = file_time.0;
    let mut created_at_ms = file_time.1;
    let mut source = String::new();
    let mut thread_source = None;
    let mut cli_version = String::new();
    let mut memory_mode = "enabled".to_string();
    let mut model = None;
    let mut reasoning_effort = None;
    let mut title = String::new();
    let mut preview = String::new();
    let mut sandbox_policy = r#"{"type":"read-only"}"#.to_string();
    let mut approval_mode = "on-request".to_string();
    let mut tokens_used = 0;
    let mut first_user_message = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        match value.get("type").and_then(|item| item.as_str()) {
            Some("session_meta") if id.is_empty() => {
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                id = json_string(payload, "id").unwrap_or_default();
                model_provider = json_string(payload, "model_provider").unwrap_or_default();
                cwd = json_string(payload, "cwd").unwrap_or_default();
                source = payload
                    .get("source")
                    .and_then(json_state_string)
                    .unwrap_or_else(|| "vscode".to_string());
                thread_source = json_string(payload, "thread_source");
                cli_version = json_string(payload, "cli_version").unwrap_or_default();
                memory_mode =
                    json_string(payload, "memory_mode").unwrap_or_else(|| "enabled".to_string());
                if let Some(timestamp) = json_string(payload, "timestamp") {
                    if let Some((seconds, millis)) = parse_rfc3339_epoch(&timestamp) {
                        created_at = seconds;
                        created_at_ms = millis;
                    }
                }
            }
            Some("turn_context") => {
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                model = json_string(payload, "model").or(model);
                reasoning_effort = json_string(payload, "effort").or(reasoning_effort);
                if let Some(policy) = payload
                    .get("permission_profile")
                    .or_else(|| payload.get("sandbox_policy"))
                    .and_then(json_state_string)
                {
                    sandbox_policy = policy;
                }
                if let Some(approval) = payload.get("approval_policy").and_then(json_state_string)
                {
                    approval_mode = approval;
                }
                if cwd.is_empty() {
                    cwd = json_string(payload, "cwd").unwrap_or_default();
                }
            }
            Some("event_msg") => {
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                match payload.get("type").and_then(|item| item.as_str()) {
                    Some("user_message") => {
                        if let Some(message) = json_string(payload, "message") {
                            let clean = strip_user_message_prefix(&message);
                            if !clean.is_empty() {
                                if first_user_message.is_empty() {
                                    first_user_message = clean.clone();
                                }
                                if preview.is_empty() {
                                    preview = clean.clone();
                                }
                                if title.is_empty() {
                                    title = clean;
                                }
                            }
                        }
                    }
                    Some("token_count") => {
                        tokens_used = payload
                            .pointer("/info/total_token_usage/total_tokens")
                            .and_then(|item| item.as_i64())
                            .unwrap_or(tokens_used)
                            .max(0);
                    }
                    Some("thread_goal_updated") => {
                        if preview.is_empty() {
                            if let Some(objective) =
                                payload.pointer("/goal/objective").and_then(|item| item.as_str())
                            {
                                let objective = objective.trim();
                                if !objective.is_empty() {
                                    preview = objective.to_string();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if id.is_empty() {
        return Ok(None);
    }
    if title.trim().is_empty() {
        title = if preview.trim().is_empty() {
            default_rollout_title(&id)
        } else {
            preview.clone()
        };
    }
    if preview.trim().is_empty() {
        preview = title.clone();
    }

    Ok(Some(CodexRolloutThreadMetadata {
        path: file.path.clone(),
        relative_path: file.relative_path.clone(),
        archived: file.archived,
        id,
        model_provider,
        cwd,
        created_at,
        created_at_ms,
        updated_at: file_time.0.max(created_at),
        updated_at_ms: file_time.1.max(created_at_ms),
        source,
        thread_source,
        model,
        reasoning_effort,
        cli_version,
        title,
        preview,
        sandbox_policy,
        approval_mode,
        tokens_used,
        first_user_message,
        memory_mode,
    }))
}

fn detect_session_visibility_target_provider(
    codex_home: &Path,
    target_provider: Option<String>,
    metadata: &[CodexRolloutThreadMetadata],
) -> String {
    if let Some(provider) = target_provider
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
    {
        return provider;
    }

    if let Some(provider) = metadata
        .iter()
        .filter(|item| !item.model_provider.trim().is_empty())
        .max_by_key(|item| item.updated_at_ms)
        .map(|item| item.model_provider.clone())
    {
        return provider;
    }

    let config_path = codex_home.join(CODEX_CONFIG_FILE);
    let config = std::fs::read_to_string(config_path).unwrap_or_default();
    root_toml_string_value(&config, "model_provider").unwrap_or_else(|| "openai".to_string())
}

fn read_session_index_ids(path: &Path) -> Result<HashSet<String>, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(e) => return Err(format!("读取 session_index.jsonl 失败: {}", e)),
    };
    let mut ids = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if let Some(id) = json_string(&value, "id") {
            ids.insert(id);
        }
    }
    Ok(ids)
}

fn sqlite_visibility_state(
    conn: &Connection,
    thread_id: &str,
) -> Result<SqliteVisibilityState, String> {
    match conn.query_row(
        "SELECT model_provider FROM threads WHERE id = ?",
        params![thread_id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(provider) => Ok(SqliteVisibilityState {
            exists: true,
            provider,
        }),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(SqliteVisibilityState {
            exists: false,
            provider: String::new(),
        }),
        Err(e) => Err(format!("读取 Codex state_5.sqlite 失败: {}", e)),
    }
}

fn scan_codex_session_visibility(
    codex_home: &Path,
    target_provider: Option<String>,
) -> Result<
    (
        String,
        Vec<CodexRolloutThreadMetadata>,
        usize,
        usize,
        usize,
        usize,
    ),
    String,
> {
    let rollout_files = collect_codex_rollout_files(codex_home)?;
    let mut metadata = Vec::new();
    for file in &rollout_files {
        if let Some(item) = parse_codex_rollout_metadata(file)? {
            metadata.push(item);
        }
    }
    let target_provider =
        detect_session_visibility_target_provider(codex_home, target_provider, &metadata);
    let mismatched_rollout_files = metadata
        .iter()
        .filter(|item| item.model_provider != target_provider)
        .count();

    let state_db_path = codex_state_db_path_for_home(codex_home);
    let mut mismatched_sqlite_records = 0;
    let mut missing_sqlite_records = 0;
    if state_db_path.exists() {
        let conn = Connection::open_with_flags(
            &state_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("打开 Codex state_5.sqlite 失败: {}", e))?;
        for item in &metadata {
            let state = sqlite_visibility_state(&conn, &item.id)?;
            if !state.exists {
                missing_sqlite_records += 1;
            } else if state.provider != target_provider {
                mismatched_sqlite_records += 1;
            }
        }
    } else {
        missing_sqlite_records = metadata.len();
    }

    let session_index_path = codex_session_index_path_for_home(codex_home);
    let index_ids = read_session_index_ids(&session_index_path)?;
    let missing_session_index_entries = metadata
        .iter()
        .filter(|item| !index_ids.contains(&item.id))
        .count();

    Ok((
        target_provider,
        metadata,
        rollout_files.len(),
        mismatched_rollout_files,
        mismatched_sqlite_records,
        missing_sqlite_records + missing_session_index_entries,
    ))
}

fn get_codex_session_visibility_status_for_home(
    codex_home: &Path,
    target_provider: Option<String>,
) -> Result<CodexSessionVisibilityStatus, String> {
    let (
        target_provider,
        metadata,
        scanned_rollout_files,
        mismatched_rollout_files,
        mismatched_sqlite_records,
        missing_total,
    ) = scan_codex_session_visibility(codex_home, target_provider)?;
    let state_db_path = codex_state_db_path_for_home(codex_home);
    let session_index_path = codex_session_index_path_for_home(codex_home);

    let missing_sqlite_records = if state_db_path.exists() {
        let conn = Connection::open_with_flags(
            &state_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("打开 Codex state_5.sqlite 失败: {}", e))?;
        let mut count = 0;
        for item in &metadata {
            if !sqlite_visibility_state(&conn, &item.id)?.exists {
                count += 1;
            }
        }
        count
    } else {
        metadata.len()
    };
    let session_index_ids = read_session_index_ids(&session_index_path)?;
    let missing_session_index_entries = missing_total.saturating_sub(missing_sqlite_records);
    let missing_session_index_entries = if missing_session_index_entries == 0 {
        metadata
            .iter()
            .filter(|item| !session_index_ids.contains(&item.id))
            .count()
    } else {
        missing_session_index_entries
    };

    Ok(CodexSessionVisibilityStatus {
        codex_home: codex_home.to_string_lossy().to_string(),
        state_db_path: state_db_path.to_string_lossy().to_string(),
        session_index_path: session_index_path.to_string_lossy().to_string(),
        target_provider,
        scanned_rollout_files,
        mismatched_rollout_files,
        mismatched_sqlite_records,
        missing_sqlite_records,
        missing_session_index_entries,
    })
}

fn ensure_parent_dir(path: &Path, label: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建{}目录失败: {}", label, e))?;
    }
    Ok(())
}

fn copy_file_to_backup(source: &Path, destination: &Path) -> Result<bool, String> {
    if !source.exists() {
        return Ok(false);
    }
    ensure_parent_dir(destination, "备份")?;
    std::fs::copy(source, destination).map_err(|e| {
        format!(
            "备份文件失败 {} -> {}: {}",
            source.display(),
            destination.display(),
            e
        )
    })?;
    Ok(true)
}

fn create_session_visibility_backup(
    codex_home: &Path,
    target_provider: &str,
    rollout_files: &[PathBuf],
    include_sqlite: bool,
    include_session_index: bool,
) -> Result<String, String> {
    if rollout_files.is_empty() && !include_sqlite && !include_session_index {
        return Ok(String::new());
    }

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let backup_dir = codex_home.join(format!(
        "backup-{}-session-visibility-repair",
        stamp
    ));
    let files_dir = backup_dir.join("files");
    std::fs::create_dir_all(&files_dir).map_err(|e| format!("创建修复备份目录失败: {}", e))?;

    let mut rollout_relatives = Vec::new();
    for relative_path in rollout_files {
        let source = codex_home.join(relative_path);
        let destination = files_dir.join(relative_path);
        copy_file_to_backup(&source, &destination)?;
        rollout_relatives.push(relative_path.to_string_lossy().to_string());
    }

    let state_db_path = codex_state_db_path_for_home(codex_home);
    let has_sqlite_backup = if include_sqlite {
        copy_file_to_backup(&state_db_path, &backup_dir.join(CODEX_STATE_DB_FILE))?
    } else {
        false
    };
    for suffix in ["-wal", "-shm"] {
        let source = PathBuf::from(format!("{}{}", state_db_path.display(), suffix));
        if source.exists() {
            let destination = backup_dir.join(format!("{}{}", CODEX_STATE_DB_FILE, suffix));
            let _ = copy_file_to_backup(&source, &destination)?;
        }
    }

    let session_index_path = codex_session_index_path_for_home(codex_home);
    let has_session_index_backup = if include_session_index {
        copy_file_to_backup(&session_index_path, &backup_dir.join(CODEX_SESSION_INDEX_FILE))?
    } else {
        false
    };

    let manifest = serde_json::json!({
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "instanceRoot": codex_home.to_string_lossy(),
        "targetProvider": target_provider,
        "rolloutFiles": rollout_relatives,
        "hasSqliteBackup": has_sqlite_backup,
        "hasSessionIndexBackup": has_session_index_backup,
    });
    std::fs::write(
        backup_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("生成修复备份清单失败: {}", e))?,
    )
    .map_err(|e| format!("写入修复备份清单失败: {}", e))?;

    Ok(backup_dir.to_string_lossy().to_string())
}

fn rewrite_rollout_model_provider(path: &Path, target_provider: &str) -> Result<bool, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 rollout 文件失败 {}: {}", path.display(), e))?;
    let had_trailing_newline = content.ends_with('\n');
    let mut changed = false;
    let mut lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if !changed && !trimmed.is_empty() {
            if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if value.get("type").and_then(|item| item.as_str()) == Some("session_meta") {
                    if let Some(payload) = value.get_mut("payload").and_then(|item| item.as_object_mut())
                    {
                        let current = payload
                            .get("model_provider")
                            .and_then(|item| item.as_str())
                            .unwrap_or_default();
                        if current != target_provider {
                            payload.insert(
                                "model_provider".to_string(),
                                serde_json::Value::String(target_provider.to_string()),
                            );
                            lines.push(
                                serde_json::to_string(&value)
                                    .map_err(|e| format!("序列化 rollout 元数据失败: {}", e))?,
                            );
                            changed = true;
                            continue;
                        }
                    }
                }
            }
        }
        lines.push(line.to_string());
    }

    if changed {
        let mut next = lines.join("\n");
        if had_trailing_newline {
            next.push('\n');
        }
        std::fs::write(path, next)
            .map_err(|e| format!("写入 rollout 文件失败 {}: {}", path.display(), e))?;
    }
    Ok(changed)
}

fn update_sqlite_thread_provider(
    conn: &Connection,
    thread_id: &str,
    target_provider: &str,
) -> Result<usize, String> {
    conn.execute(
        "UPDATE threads SET model_provider = ? WHERE id = ? AND model_provider != ?",
        params![target_provider, thread_id, target_provider],
    )
    .map_err(|e| format!("更新 Codex state_5.sqlite 失败: {}", e))
}

fn insert_sqlite_thread_metadata(
    conn: &Connection,
    item: &CodexRolloutThreadMetadata,
    target_provider: &str,
) -> Result<usize, String> {
    let archived = if item.archived { 1 } else { 0 };
    let archived_at = item.archived.then_some(item.updated_at);
    conn.execute(
        r#"
        INSERT INTO threads (
            id,
            rollout_path,
            created_at,
            updated_at,
            created_at_ms,
            updated_at_ms,
            source,
            thread_source,
            model_provider,
            model,
            reasoning_effort,
            cwd,
            cli_version,
            title,
            preview,
            sandbox_policy,
            approval_mode,
            tokens_used,
            first_user_message,
            archived,
            archived_at,
            memory_mode
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO NOTHING
        "#,
        params![
            item.id,
            item.path.to_string_lossy().to_string(),
            item.created_at,
            item.updated_at,
            item.created_at_ms,
            item.updated_at_ms,
            item.source,
            item.thread_source,
            target_provider,
            item.model,
            item.reasoning_effort,
            item.cwd,
            item.cli_version,
            item.title,
            item.preview,
            item.sandbox_policy,
            item.approval_mode,
            item.tokens_used,
            item.first_user_message,
            archived,
            archived_at,
            item.memory_mode,
        ],
    )
    .map_err(|e| format!("补写 Codex state_5.sqlite 失败: {}", e))
}

fn append_session_index_entry(
    path: &Path,
    thread_id: &str,
    thread_name: &str,
) -> Result<(), String> {
    ensure_parent_dir(path, "session_index")?;
    let entry = serde_json::json!({
        "id": thread_id,
        "thread_name": thread_name,
        "updated_at": chrono::Utc::now().to_rfc3339(),
    });
    let mut line = serde_json::to_string(&entry)
        .map_err(|e| format!("序列化 session_index 条目失败: {}", e))?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("打开 session_index.jsonl 失败: {}", e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| format!("写入 session_index.jsonl 失败: {}", e))
}

fn repair_codex_session_visibility_for_home(
    codex_home: &Path,
    target_provider: Option<String>,
) -> Result<CodexSessionVisibilityRepairReport, String> {
    let rollout_files = collect_codex_rollout_files(codex_home)?;
    let mut metadata = Vec::new();
    let mut failed_rollout_files = Vec::new();
    for file in &rollout_files {
        match parse_codex_rollout_metadata(file) {
            Ok(Some(item)) => metadata.push(item),
            Ok(None) => {}
            Err(error) => failed_rollout_files.push(CodexSessionVisibilityRepairFailure {
                path: file.path.to_string_lossy().to_string(),
                error,
            }),
        }
    }
    let target_provider =
        detect_session_visibility_target_provider(codex_home, target_provider, &metadata);

    let state_db_path = codex_state_db_path_for_home(codex_home);
    let session_index_path = codex_session_index_path_for_home(codex_home);
    let index_ids = read_session_index_ids(&session_index_path)?;

    let mut rollout_relatives_to_backup = metadata
        .iter()
        .filter(|item| item.model_provider != target_provider)
        .map(|item| item.relative_path.clone())
        .collect::<Vec<_>>();
    rollout_relatives_to_backup.sort();
    rollout_relatives_to_backup.dedup();

    let mut needs_sqlite_backup = false;
    let mut sqlite_existing_states = HashMap::<String, SqliteVisibilityState>::new();
    if state_db_path.exists() {
        let conn = Connection::open_with_flags(
            &state_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("打开 Codex state_5.sqlite 失败: {}", e))?;
        for item in &metadata {
            let state = sqlite_visibility_state(&conn, &item.id)?;
            if !state.exists || state.provider != target_provider {
                needs_sqlite_backup = true;
            }
            sqlite_existing_states.insert(item.id.clone(), state);
        }
    }

    let needs_session_index_backup = metadata.iter().any(|item| !index_ids.contains(&item.id));
    let backup_dir = create_session_visibility_backup(
        codex_home,
        &target_provider,
        &rollout_relatives_to_backup,
        needs_sqlite_backup,
        needs_session_index_backup,
    )?;

    let mut ready_metadata = Vec::new();
    let mut rewritten_rollout_files = 0;
    for mut item in metadata {
        if item.model_provider != target_provider {
            match rewrite_rollout_model_provider(&item.path, &target_provider) {
                Ok(true) => {
                    rewritten_rollout_files += 1;
                    item.model_provider = target_provider.clone();
                }
                Ok(false) => {}
                Err(error) => {
                    failed_rollout_files.push(CodexSessionVisibilityRepairFailure {
                        path: item.path.to_string_lossy().to_string(),
                        error,
                    });
                    continue;
                }
            }
        }
        ready_metadata.push(item);
    }

    let mut sqlite_records_updated = 0;
    let mut sqlite_records_inserted = 0;
    if state_db_path.exists() {
        let conn = Connection::open(&state_db_path)
            .map_err(|e| format!("打开 Codex state_5.sqlite 失败: {}", e))?;
        for item in &ready_metadata {
            let state = sqlite_existing_states
                .get(&item.id)
                .cloned()
                .unwrap_or(SqliteVisibilityState {
                    exists: false,
                    provider: String::new(),
                });
            if state.exists {
                sqlite_records_updated +=
                    update_sqlite_thread_provider(&conn, &item.id, &target_provider)?;
            } else {
                sqlite_records_inserted +=
                    insert_sqlite_thread_metadata(&conn, item, &target_provider)?;
            }
        }
    }

    let mut refreshed_index_ids = index_ids;
    let mut session_index_entries_added = 0;
    for item in &ready_metadata {
        if refreshed_index_ids.insert(item.id.clone()) {
            append_session_index_entry(&session_index_path, &item.id, &item.title)?;
            session_index_entries_added += 1;
        }
    }

    Ok(CodexSessionVisibilityRepairReport {
        codex_home: codex_home.to_string_lossy().to_string(),
        state_db_path: state_db_path.to_string_lossy().to_string(),
        session_index_path: session_index_path.to_string_lossy().to_string(),
        target_provider,
        backup_dir,
        scanned_rollout_files: rollout_files.len(),
        rewritten_rollout_files,
        sqlite_records_updated,
        sqlite_records_inserted,
        session_index_entries_added,
        failed_rollout_files,
    })
}

#[command]
async fn get_codex_session_visibility_status(
    target_provider: Option<String>,
) -> Result<CodexSessionVisibilityStatus, String> {
    let codex_home = get_codex_home_path()?;
    get_codex_session_visibility_status_for_home(&codex_home, target_provider)
}

#[command]
async fn repair_codex_session_visibility(
    target_provider: Option<String>,
) -> Result<CodexSessionVisibilityRepairReport, String> {
    let codex_home = get_codex_home_path()?;
    repair_codex_session_visibility_for_home(&codex_home, target_provider)
}

#[command]
fn get_codex_usage_summary() -> Result<CodexUsageSummary, String> {
    let log_path = get_codex_logs_path()?;
    let (today_start_ts, today_end_ts) = local_day_bounds_ts()?;
    if !log_path.exists() {
        return Ok(CodexUsageSummary {
            log_path: log_path.to_string_lossy().to_string(),
            today_start_ts,
            today_end_ts,
            total: CodexUsageRollup::default(),
            today: CodexUsageRollup::default(),
            by_model: Vec::new(),
            recent_failures: Vec::new(),
            note: "未找到 Codex 本地日志库，启动 Codex 后才会产生统计数据。".to_string(),
        });
    }

    let conn = Connection::open_with_flags(
        &log_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_URI
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("打开 Codex 日志失败: {}", e))?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT ts, feedback_log_body
            FROM logs
            WHERE feedback_log_body LIKE '%codex.turn.token_usage.%'
               OR feedback_log_body LIKE '%response.failed%'
               OR feedback_log_body LIKE '%"status":"failed"%'
               OR feedback_log_body LIKE '%"status":"incomplete"%'
            ORDER BY ts ASC, ts_nanos ASC
            "#,
        )
        .map_err(|e| format!("读取 Codex 日志结构失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })
        .map_err(|e| format!("查询 Codex 日志失败: {}", e))?;

    let mut usage_by_turn: HashMap<String, ParsedCodexTurnUsage> = HashMap::new();
    let mut failures_by_key: HashMap<String, CodexUsageFailure> = HashMap::new();

    for row in rows {
        let (ts, body) = row.map_err(|e| format!("读取 Codex 日志行失败: {}", e))?;
        if let Some(usage) = parse_codex_turn_usage(ts, &body) {
            usage_by_turn
                .entry(usage.turn_id.clone())
                .and_modify(|existing| {
                    if usage.total_tokens > existing.total_tokens
                        || (usage.total_tokens == existing.total_tokens && usage.ts > existing.ts)
                    {
                        *existing = usage.clone();
                    }
                })
                .or_insert(usage);
        }
        if let Some(failure) = parse_codex_failure(ts, &body) {
            let key = if !failure.response_id.is_empty() {
                failure.response_id.clone()
            } else if !failure.turn_id.is_empty() {
                format!("turn:{}:{}", failure.turn_id, failure.status)
            } else {
                format!("{}:{}:{}", failure.ts, failure.status, failure.message)
            };
            failures_by_key.entry(key).or_insert(failure);
        }
    }

    let success_turn_ids = usage_by_turn.keys().cloned().collect::<HashSet<_>>();
    let mut total = CodexUsageRollup::default();
    let mut today = CodexUsageRollup::default();
    let mut by_model = HashMap::<String, CodexUsageRollup>::new();

    for usage in usage_by_turn.values() {
        add_usage_to_rollup(&mut total, usage);
        add_usage_to_rollup(by_model.entry(usage.model.clone()).or_default(), usage);
        if usage.ts >= today_start_ts && usage.ts < today_end_ts {
            add_usage_to_rollup(&mut today, usage);
        }
    }

    let mut recent_failures = failures_by_key.into_values().collect::<Vec<_>>();
    recent_failures.sort_by(|a, b| b.ts.cmp(&a.ts));
    for failure in &recent_failures {
        if !failure.turn_id.is_empty() && success_turn_ids.contains(&failure.turn_id) {
            continue;
        }
        total.error_count += 1;
        total.request_count += 1;
        let model_usage = by_model.entry(failure.model.clone()).or_default();
        model_usage.error_count += 1;
        model_usage.request_count += 1;
        if failure.ts >= today_start_ts && failure.ts < today_end_ts {
            today.error_count += 1;
            today.request_count += 1;
        }
    }
    recent_failures.truncate(10);

    let mut by_model = by_model
        .into_iter()
        .map(|(model, usage)| CodexModelUsage { model, usage })
        .collect::<Vec<_>>();
    by_model.sort_by(|a, b| {
        b.usage
            .total_tokens
            .cmp(&a.usage.total_tokens)
            .then_with(|| a.model.cmp(&b.model))
    });

    Ok(CodexUsageSummary {
        log_path: log_path.to_string_lossy().to_string(),
        today_start_ts,
        today_end_ts,
        total,
        today,
        by_model,
        recent_failures,
        note: "统计来自本机 Codex logs_2.sqlite；token 按 turn.id 去重，费用为本地价格表估算，不代表官方账单。".to_string(),
    })
}

#[command]
async fn open_storage_folder(app: AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot find app data directory: {}", e))?;
    std::fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    open_in_file_manager(&app_data_dir)
}

#[command]
async fn open_codex_auth_folder() -> Result<(), String> {
    let auth_path = get_auth_path()?;
    let auth_dir = auth_path
        .parent()
        .ok_or_else(|| "Cannot find auth.json parent directory".to_string())?;
    std::fs::create_dir_all(auth_dir)
        .map_err(|e| format!("Failed to create .codex directory: {}", e))?;
    open_in_file_manager(auth_dir)
}

#[command]
async fn is_codex_running() -> Result<bool, String> {
    let running = {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("pgrep")
                .args(["-f", "-i", "codex"])
                .output()
                .map_err(|e| e.to_string())?;
            !output.stdout.is_empty()
        }

        #[cfg(target_os = "windows")]
        {
            let output = std::process::Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq codex.exe"])
                .output()
                .map_err(|e| e.to_string())?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains("codex.exe")
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            false
        }
    };

    Ok(running)
}

// ── App entry point ───────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            setup_tray(app.handle())?;
            restore_codex_proxy_on_startup(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            #[cfg(target_os = "windows")]
            WindowEvent::Resized(size) if size.width == 0 && size.height == 0 => {
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            fetch_quota,
            start_codex_oauth_login,
            open_codex_oauth_url,
            cancel_codex_oauth_login,
            complete_codex_oauth_login,
            list_accounts,
            get_account_auth_json,
            list_operation_logs,
            clear_operation_logs,
            refresh_account_profile,
            add_account,
            update_account,
            delete_account,
            export_encrypted_backup,
            export_encrypted_backup_file,
            preview_encrypted_backup,
            import_encrypted_backup,
            get_migration_status,
            migrate_plaintext_accounts,
            refresh_account_quota,
            check_account_health,
            switch_account_by_id,
            get_codex_proxy_state,
            activate_codex_proxy,
            deactivate_codex_proxy,
            set_codex_proxy_account,
            update_account_quota,
            get_setting,
            set_setting,
            switch_account,
            write_auth_json,
            read_auth_json,
            get_current_account_record_id,
            get_codex_auth_path,
            get_storage_paths,
            get_codex_app_speed_config,
            set_codex_app_speed,
            get_codex_feature_status,
            repair_codex_app_speed_state,
            get_codex_project_visibility_status,
            repair_codex_project_visibility,
            get_codex_session_visibility_status,
            repair_codex_session_visibility,
            get_codex_usage_summary,
            open_storage_folder,
            open_codex_auth_folder,
            is_codex_running,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_jwt_with_exp(exp: i64) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{}}}"#, exp));
        format!("{}.{}.signature", header, payload)
    }

    fn sample_jwt_payload(payload: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_string(&payload).unwrap());
        format!("{}.{}.signature", header, payload)
    }

    fn sample_auth_json() -> String {
        serde_json::json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "access_token": "access-token",
                "refresh_token": "refresh-token",
                "account_id": "account-123"
            }
        })
        .to_string()
    }

    fn sample_auth_json_with_email(email: &str, account_id: &str) -> String {
        serde_json::json!({
            "OPENAI_API_KEY": serde_json::Value::Null,
            "tokens": {
                "id_token": sample_jwt_payload(serde_json::json!({ "email": email })),
                "access_token": "access-token",
                "refresh_token": "refresh-token",
                "account_id": account_id
            }
        })
        .to_string()
    }

    #[test]
    fn parse_auth_json_requires_tokens() {
        assert!(parse_auth_json(&sample_auth_json()).is_ok());

        let missing_access = serde_json::json!({
            "tokens": {
                "refresh_token": "refresh-token",
                "account_id": "account-123"
            }
        })
        .to_string();
        let err = parse_auth_json(&missing_access).unwrap_err();

        assert!(err.contains("tokens.access_token"));
    }

    #[test]
    fn account_stub_keeps_only_account_id() {
        let stub = account_stub_from_json(&sample_auth_json());
        let parsed: serde_json::Value = serde_json::from_str(&stub).unwrap();

        assert_eq!(parsed.pointer("/tokens/account_id").unwrap(), "account-123");
        assert!(parsed.pointer("/tokens/access_token").is_none());
        assert!(parsed.pointer("/tokens/refresh_token").is_none());
    }

    #[test]
    fn canonical_auth_json_matches_codex_oauth_shape() {
        let canonical = canonicalize_auth_json(&sample_auth_json()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&canonical).unwrap();

        assert!(parsed.get("auth_mode").is_none());
        assert!(parsed.get("OPENAI_API_KEY").unwrap().is_null());
        assert!(parsed
            .get("last_refresh")
            .and_then(|item| item.as_str())
            .unwrap()
            .ends_with('Z'));
        assert_eq!(parsed.pointer("/tokens/account_id").unwrap(), "account-123");
        assert_eq!(
            parsed.pointer("/tokens/access_token").unwrap(),
            "access-token"
        );
        assert_eq!(
            parsed.pointer("/tokens/refresh_token").unwrap(),
            "refresh-token"
        );
    }

    #[test]
    fn token_expiration_uses_jwt_exp_with_refresh_skew() {
        let now = chrono_like_now_timestamp();

        assert!(!is_token_expired(&sample_jwt_with_exp(
            now + TOKEN_REFRESH_SKEW_SECONDS + 60
        )));
        assert!(is_token_expired(&sample_jwt_with_exp(now - 60)));
        assert!(is_token_expired("not-a-jwt"));
    }

    #[test]
    fn oauth_url_can_force_account_selection() {
        let auth_url = build_codex_oauth_url(
            "http://localhost:1455/auth/callback",
            "challenge",
            "state",
            true,
        );

        assert!(auth_url.contains("prompt=login"));
        assert!(auth_url.contains("max_age=0"));

        let reusable_url = build_codex_oauth_url(
            "http://localhost:1455/auth/callback",
            "challenge",
            "state",
            false,
        );

        assert!(!reusable_url.contains("prompt=login"));
        assert!(!reusable_url.contains("max_age=0"));
    }

    #[test]
    fn oauth_existing_match_requires_same_email_when_token_email_exists() {
        let existing = sample_auth_json_with_email("old@example.com", "shared-account-id");

        assert!(oauth_existing_account_matches_identity(
            "Renamed Account",
            &existing,
            Some("OLD@example.com")
        ));
        assert!(!oauth_existing_account_matches_identity(
            "new@example.com",
            &existing,
            Some("new@example.com")
        ));
        assert!(!oauth_existing_account_matches_identity(
            "old@example.com",
            &existing,
            None
        ));
    }

    #[test]
    fn oauth_existing_match_can_fallback_to_name_for_legacy_rows() {
        assert!(oauth_existing_account_matches_identity(
            "legacy@example.com",
            &sample_auth_json(),
            Some("LEGACY@example.com")
        ));
        assert!(!oauth_existing_account_matches_identity(
            "legacy@example.com",
            &sample_auth_json(),
            Some("other@example.com")
        ));
    }

    #[test]
    fn codex_proxy_config_installs_managed_provider() {
        let original = r#"model = "gpt-5"
openai_base_url = "https://legacy.example.com/v1"

[features]
goals = true
"#;
        let next = codex_proxy_config_toml(original, 14998);

        assert!(codex_proxy_config_installed(&next));
        assert!(next.contains("model_provider = \"codex_account_manager_proxy\""));
        assert!(next.contains("[model_providers.codex_account_manager_proxy]"));
        assert!(next.contains("base_url = \"http://127.0.0.1:14998/v1\""));
        assert!(next.contains("wire_api = \"responses\""));
        assert!(next.contains("[features]\ngoals = true"));
        assert!(!next.contains("openai_base_url"));
    }

    #[test]
    fn codex_proxy_config_removal_only_cleans_managed_provider() {
        let installed = codex_proxy_config_toml(
            r#"[model_providers.manual]
name = "Manual"
base_url = "https://manual.example.com/v1"
"#,
            14998,
        );
        let cleaned = remove_codex_proxy_config_toml(&installed);

        assert!(!codex_proxy_config_installed(&cleaned));
        assert!(!cleaned.contains("codex_account_manager_proxy"));
        assert!(cleaned.contains("[model_providers.manual]"));
        assert!(cleaned.contains("https://manual.example.com/v1"));
    }

    #[test]
    fn codex_proxy_resolves_v1_and_backend_targets() {
        assert_eq!(
            resolve_codex_proxy_upstream_target("/v1/responses?stream=true").unwrap(),
            "/responses?stream=true"
        );
        assert_eq!(
            resolve_codex_proxy_upstream_target("/backend-api/codex/responses").unwrap(),
            "/responses"
        );
        assert!(resolve_codex_proxy_upstream_target("/other/responses").is_err());
    }

    #[test]
    fn codex_speed_config_toggles_desktop_service_tier() {
        let original = r#"[model]
name = "gpt-5"

[desktop]
theme = "system"
default-service-tier = "priority"

[other]
enabled = true
"#;
        let standard = codex_config_toml_with_speed(original, &CodexAppSpeed::Standard);

        assert!(standard.contains("[desktop]"));
        assert!(standard.contains("theme = \"system\""));
        assert!(!standard.contains("default-service-tier"));
        assert!(standard.contains("[other]"));

        let fast = codex_config_toml_with_speed(&standard, &CodexAppSpeed::Fast);

        let tier_index = fast.find("default-service-tier = \"priority\"").unwrap();
        let other_index = fast.find("[other]").unwrap();
        assert!(tier_index < other_index);
        assert_eq!(
            read_service_tier_from_config(&fast).as_deref(),
            Some(CODEX_PRIORITY_SERVICE_TIER)
        );
    }

    #[test]
    fn project_visibility_repair_only_touches_target_project() {
        let original = r#"model = "gpt-5.5"

[features]
goals = true

[projects.'D:\project\old']
trust_level = "trusted"

[projects.'D:\project\rust\codex_account_manager']
trust_level = "untrusted"

[memories]
use_memories = true
"#;
        let (next, changed) = codex_config_toml_with_trusted_project(
            original,
            r#"d:\project\rust\codex_account_manager"#,
        );

        assert!(changed);
        assert!(next.contains("model = \"gpt-5.5\""));
        assert!(next.contains("[features]\ngoals = true"));
        assert!(next.contains("[memories]\nuse_memories = true"));
        assert!(next.contains("[projects.'D:\\project\\old']\ntrust_level = \"trusted\""));
        assert!(is_project_trusted_in_config(
            &next,
            r#"D:\project\rust\codex_account_manager"#
        ));
        assert!(!next.contains("provider"));
        assert!(!next.contains("base_url"));
    }

    #[test]
    fn project_visibility_repair_appends_missing_project() {
        let original = r#"[features]
goals = true
"#;
        let (next, changed) = codex_config_toml_with_trusted_project(original, r#"D:\project\new"#);

        assert!(changed);
        assert!(next.contains("[features]\ngoals = true"));
        assert!(next.contains("[projects.'D:\\project\\new']\ntrust_level = \"trusted\""));
    }

    fn temp_codex_home(label: &str) -> PathBuf {
        let unique = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "codex-account-manager-{}-{}-{}",
            label,
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_test_rollout(
        codex_home: &Path,
        thread_id: &str,
        provider: &str,
        cwd: &str,
    ) -> PathBuf {
        let path = codex_home
            .join(CODEX_SESSIONS_DIR)
            .join("2026")
            .join("06")
            .join("08")
            .join(format!("rollout-2026-06-08T12-30-00-{}.jsonl", thread_id));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let session_meta = serde_json::json!({
            "timestamp": "2026-06-08T04:30:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": thread_id,
                "timestamp": "2026-06-08T04:30:00.000Z",
                "source": "vscode",
                "model_provider": provider,
                "cwd": cwd,
                "cli_version": "0.137.0",
                "thread_source": "user"
            }
        });
        let turn_context = serde_json::json!({
            "timestamp": "2026-06-08T04:30:01.000Z",
            "type": "turn_context",
            "payload": {
                "cwd": cwd,
                "model": "gpt-5.5",
                "effort": "medium",
                "approval_policy": "never",
                "permission_profile": { "type": "disabled" }
            }
        });
        let user_message = serde_json::json!({
            "timestamp": "2026-06-08T04:30:02.000Z",
            "type": "event_msg",
            "payload": {
                "type": "user_message",
                "message": "帮我修一下历史会话"
            }
        });
        let content = format!(
            "{}\n{}\n{}\n",
            serde_json::to_string(&session_meta).unwrap(),
            serde_json::to_string(&turn_context).unwrap(),
            serde_json::to_string(&user_message).unwrap()
        );
        std::fs::write(&path, content).unwrap();
        path
    }

    fn create_test_state_db(codex_home: &Path) -> Connection {
        let path = codex_state_db_path_for_home(codex_home);
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                rollout_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                source TEXT NOT NULL,
                model_provider TEXT NOT NULL,
                cwd TEXT NOT NULL,
                title TEXT NOT NULL,
                sandbox_policy TEXT NOT NULL,
                approval_mode TEXT NOT NULL,
                tokens_used INTEGER NOT NULL DEFAULT 0,
                has_user_event INTEGER NOT NULL DEFAULT 0,
                archived INTEGER NOT NULL DEFAULT 0,
                archived_at INTEGER,
                git_sha TEXT,
                git_branch TEXT,
                git_origin_url TEXT,
                cli_version TEXT NOT NULL DEFAULT '',
                first_user_message TEXT NOT NULL DEFAULT '',
                agent_nickname TEXT,
                agent_role TEXT,
                memory_mode TEXT NOT NULL DEFAULT 'enabled',
                model TEXT,
                reasoning_effort TEXT,
                agent_path TEXT,
                created_at_ms INTEGER,
                updated_at_ms INTEGER,
                thread_source TEXT,
                preview TEXT NOT NULL DEFAULT ''
            );
            "#,
        )
        .unwrap();
        conn
    }

    fn insert_test_thread_row(
        conn: &Connection,
        thread_id: &str,
        rollout_path: &Path,
        provider: &str,
        cwd: &str,
    ) {
        conn.execute(
            r#"
            INSERT INTO threads (
                id, rollout_path, created_at, updated_at, source, model_provider, cwd,
                title, sandbox_policy, approval_mode, preview, created_at_ms, updated_at_ms
            ) VALUES (?, ?, 1780893000, 1780893001, 'vscode', ?, ?, '旧会话', '{"type":"disabled"}', 'never', '旧会话', 1780893000000, 1780893001000)
            "#,
            params![
                thread_id,
                rollout_path.to_string_lossy().to_string(),
                provider,
                cwd
            ],
        )
        .unwrap();
    }

    fn rollout_provider(path: &Path) -> String {
        let content = std::fs::read_to_string(path).unwrap();
        for line in content.lines() {
            let value = serde_json::from_str::<serde_json::Value>(line).unwrap();
            if value.get("type").and_then(|item| item.as_str()) == Some("session_meta") {
                return value
                    .pointer("/payload/model_provider")
                    .and_then(|item| item.as_str())
                    .unwrap()
                    .to_string();
            }
        }
        String::new()
    }

    #[test]
    fn session_visibility_repair_rewrites_rollout_and_sqlite_provider() {
        let codex_home = temp_codex_home("session-visibility-rewrite");
        let thread_id = "019ea57e-a382-7461-9043-d0bd81d86f2f";
        let cwd = "/Users/shorlyn/Documents/project/rust/codex_account_manager";
        let rollout_path = write_test_rollout(&codex_home, thread_id, "openai", cwd);
        let conn = create_test_state_db(&codex_home);
        insert_test_thread_row(&conn, thread_id, &rollout_path, "openai", cwd);
        drop(conn);

        let report =
            repair_codex_session_visibility_for_home(&codex_home, Some("custom".to_string()))
                .unwrap();

        assert_eq!(report.rewritten_rollout_files, 1);
        assert_eq!(report.sqlite_records_updated, 1);
        assert_eq!(report.session_index_entries_added, 1);
        assert!(PathBuf::from(&report.backup_dir).join("manifest.json").exists());
        assert_eq!(rollout_provider(&rollout_path), "custom");

        let conn = Connection::open(codex_state_db_path_for_home(&codex_home)).unwrap();
        let provider: String = conn
            .query_row(
                "SELECT model_provider FROM threads WHERE id = ?",
                params![thread_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(provider, "custom");
        let index = std::fs::read_to_string(codex_session_index_path_for_home(&codex_home)).unwrap();
        assert!(index.contains(thread_id));

        let _ = std::fs::remove_dir_all(codex_home);
    }

    #[test]
    fn session_visibility_repair_updates_stale_sqlite_when_rollout_is_already_fixed() {
        let codex_home = temp_codex_home("session-visibility-sqlite-only");
        let thread_id = "019ea4f3-f763-73e0-9722-17c177a0b64b";
        let cwd = "/Users/shorlyn/Documents/project/core/campus-all";
        let rollout_path = write_test_rollout(&codex_home, thread_id, "custom", cwd);
        let conn = create_test_state_db(&codex_home);
        insert_test_thread_row(&conn, thread_id, &rollout_path, "openai", cwd);
        drop(conn);

        let report =
            repair_codex_session_visibility_for_home(&codex_home, Some("custom".to_string()))
                .unwrap();

        assert_eq!(report.rewritten_rollout_files, 0);
        assert_eq!(report.sqlite_records_updated, 1);
        assert_eq!(rollout_provider(&rollout_path), "custom");

        let conn = Connection::open(codex_state_db_path_for_home(&codex_home)).unwrap();
        let provider: String = conn
            .query_row(
                "SELECT model_provider FROM threads WHERE id = ?",
                params![thread_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(provider, "custom");

        let _ = std::fs::remove_dir_all(codex_home);
    }

    #[test]
    fn encrypted_backup_round_trips_payload() {
        let payload = BackupPayload {
            version: 1,
            accounts: vec![BackupAccount {
                name: "Main".to_string(),
                activation_date: "2026-06-06".to_string(),
                json_info: sample_auth_json(),
                plan_type: "plus".to_string(),
                primary_used_percent: 12,
                primary_reset_at: 123,
                primary_window_minutes: Some(300),
                primary_window_present: true,
                secondary_used_percent: 34,
                secondary_reset_at: 456,
                secondary_window_minutes: Some(10080),
                secondary_window_present: true,
                last_quota_checked_at: "2026-06-06 12:00:00".to_string(),
                last_quota_error: String::new(),
            }],
        };

        let encrypted = encrypt_backup_payload(&payload, "strong-password").unwrap();
        let decrypted = decrypt_backup_payload(&encrypted, "strong-password").unwrap();

        assert_eq!(decrypted.version, 1);
        assert_eq!(decrypted.accounts.len(), 1);
        assert_eq!(decrypted.accounts[0].name, "Main");
        assert_eq!(decrypted.accounts[0].json_info, sample_auth_json());
    }

    #[test]
    fn encrypted_backup_rejects_wrong_password() {
        let payload = BackupPayload {
            version: 1,
            accounts: Vec::new(),
        };

        let encrypted = encrypt_backup_payload(&payload, "strong-password").unwrap();
        let err = decrypt_backup_payload(&encrypted, "wrong-password").unwrap_err();

        assert!(err.contains("Failed to decrypt backup"));
    }

    #[test]
    fn backup_password_must_be_long_enough() {
        let payload = BackupPayload {
            version: 1,
            accounts: Vec::new(),
        };

        assert!(encrypt_backup_payload(&payload, "short").is_err());
        assert!(decrypt_backup_payload("{}", "short").is_err());
    }

    #[test]
    fn keyring_missing_entry_gets_actionable_account_message() {
        let friendly = friendly_account_error(
            "Failed to read account credential: No matching entry found in secure storage",
        );

        assert!(friendly.contains("本地账号库"));
        assert!(friendly.contains("重新粘贴 auth.json/token"));
    }
}
