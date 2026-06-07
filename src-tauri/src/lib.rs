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
use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION, REFERER, USER_AGENT};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
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
pub struct CodexProjectVisibilityStatus {
    pub project_path: String,
    pub config_path: String,
    pub is_trusted: bool,
    pub changed: bool,
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

#[derive(Debug, Serialize, Deserialize)]
struct CredentialManifest {
    format: String,
    version: u32,
    part_prefix: String,
    parts: usize,
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

#[derive(Debug, Clone)]
struct AccountIdentity {
    email: Option<String>,
    account_id: Option<String>,
    plan_type: Option<String>,
    account_name: Option<String>,
}

// ── Helper functions ──────────────────────────────────────────────────

const CREDENTIAL_CHUNK_CHARS: usize = 500;
const CREDENTIAL_MANIFEST_FORMAT: &str = "codex-account-manager-secret-chunks";
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
const CODEX_DESKTOP_SECTION: &str = "desktop";
const CODEX_SERVICE_TIER_KEY: &str = "default-service-tier";
const CODEX_PRIORITY_SERVICE_TIER: &str = "priority";
const CODEX_ATOM_STATE_KEY: &str = "electron-persisted-atom-state";
const CODEX_USER_CHANGED_TIER_KEY: &str = "has-user-changed-service-tier";
const CODEX_PROJECTS_SECTION_PREFIX: &str = "projects.";
const CODEX_TRUST_LEVEL_KEY: &str = "trust_level";
const CODEX_TRUSTED_LEVEL: &str = "trusted";

static OAUTH_STATE: Mutex<Option<OAuthState>> = Mutex::new(None);

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
    if cfg!(target_os = "windows") {
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
        SELECT name, COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
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

fn credential_key(id: i64) -> String {
    format!("account-{}", id)
}

fn credential_entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("codex-account-manager", key)
        .map_err(|e| format!("Failed to open credential store: {}", e))
}

fn credential_manifest(text: &str) -> Option<CredentialManifest> {
    let manifest = serde_json::from_str::<CredentialManifest>(text).ok()?;
    if manifest.format == CREDENTIAL_MANIFEST_FORMAT && manifest.version == 1 {
        Some(manifest)
    } else {
        None
    }
}

fn chunk_secret(secret: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in secret.chars() {
        if current.chars().count() >= CREDENTIAL_CHUNK_CHARS {
            chunks.push(current);
            current = String::new();
        }
        current.push(ch);
    }

    if !current.is_empty() || secret.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn cleanup_secret_parts(prefix: &str, parts: usize) {
    for index in 0..parts {
        delete_account_secret_entry(&format!("{}.{}", prefix, index));
    }
}

fn delete_account_secret_entry(key: &str) {
    let _ = credential_entry(key).and_then(|entry| {
        entry
            .delete_credential()
            .map_err(|e| format!("Failed to delete account credential: {}", e))
    });
}

fn save_account_secret(key: &str, json_info: &str) -> Result<(), String> {
    let previous_manifest = credential_entry(key)
        .and_then(|entry| {
            entry
                .get_password()
                .map_err(|e| format!("Failed to read account credential: {}", e))
        })
        .ok()
        .and_then(|text| credential_manifest(&text));

    let generation = {
        let mut bytes = [0u8; 8];
        OsRng.fill_bytes(&mut bytes);
        BASE64.encode(bytes).replace(['/', '+', '='], "")
    };
    let part_prefix = format!("{}.part.{}", key, generation);
    let chunks = chunk_secret(json_info);

    for (index, chunk) in chunks.iter().enumerate() {
        if let Err(e) = credential_entry(&format!("{}.{}", part_prefix, index)).and_then(|entry| {
            entry
                .set_password(chunk)
                .map_err(|e| format!("Failed to save account credential: {}", e))
        }) {
            cleanup_secret_parts(&part_prefix, index);
            return Err(e);
        }
    }

    let manifest = CredentialManifest {
        format: CREDENTIAL_MANIFEST_FORMAT.to_string(),
        version: 1,
        part_prefix: part_prefix.clone(),
        parts: chunks.len(),
    };
    let manifest_text = serde_json::to_string(&manifest)
        .map_err(|e| format!("Failed to serialize credential manifest: {}", e))?;

    if let Err(e) = credential_entry(key)?.set_password(&manifest_text) {
        cleanup_secret_parts(&part_prefix, chunks.len());
        return Err(format!("Failed to save account credential: {}", e));
    }

    if let Some(previous_manifest) = previous_manifest {
        cleanup_secret_parts(&previous_manifest.part_prefix, previous_manifest.parts);
    }

    Ok(())
}

fn read_account_secret(key: &str) -> Result<String, String> {
    let stored = credential_entry(key)?
        .get_password()
        .map_err(|e| format!("Failed to read account credential: {}", e))?;

    let Some(manifest) = credential_manifest(&stored) else {
        return Ok(stored);
    };

    let mut secret = String::new();
    for index in 0..manifest.parts {
        let chunk = credential_entry(&format!("{}.{}", manifest.part_prefix, index))?
            .get_password()
            .map_err(|e| format!("Failed to read account credential part: {}", e))?;
        secret.push_str(&chunk);
    }
    Ok(secret)
}

fn delete_account_secret(key: &str) {
    if let Ok(stored) = credential_entry(key).and_then(|entry| {
        entry
            .get_password()
            .map_err(|e| format!("Failed to read account credential: {}", e))
    }) {
        if let Some(manifest) = credential_manifest(&stored) {
            cleanup_secret_parts(&manifest.part_prefix, manifest.parts);
        }
    }
    delete_account_secret_entry(key);
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

fn account_stub_from_json(json_info: &str) -> String {
    extract_account_id(json_info)
        .map(|account_id| serde_json::json!({ "tokens": { "account_id": account_id } }).to_string())
        .unwrap_or_else(|| "{}".to_string())
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

fn account_secret_key(conn: &Connection, id: i64) -> Result<String, String> {
    let key: String = conn
        .query_row(
            "SELECT credential_key FROM accounts WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to find account credential: {}", e))?;
    if key.is_empty() {
        Err("Account credential is missing".to_string())
    } else {
        Ok(key)
    }
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
) -> Result<i64, String> {
    parse_auth_json(auth_json)?;
    let conn = open_accounts_db(app)?;
    if identity.account_id.as_deref().is_none() {
        return Err("OAuth login did not return a ChatGPT account id".to_string());
    }
    let name = identity
        .email
        .as_deref()
        .or(identity.account_name.as_deref())
        .unwrap_or("Codex OAuth Account");
    let plan_type = identity.plan_type.as_deref().unwrap_or("unknown");

    conn.execute(
        "
        INSERT INTO accounts (name, activation_date, json_info, plan_type, updated_at)
        VALUES (?1, '', '{}', ?2, datetime('now'))
        ",
        params![name, plan_type],
    )
    .map_err(|e| format!("Failed to add OAuth account: {}", e))?;
    let id = conn.last_insert_rowid();

    let key = credential_key(id);
    save_account_secret(&key, auth_json)?;
    conn.execute(
        "
        UPDATE accounts
        SET name = ?1,
            credential_key = ?2,
            json_info = ?3,
            plan_type = ?4,
            updated_at = datetime('now')
        WHERE id = ?5
        ",
        params![name, key, account_stub_from_json(auth_json), plan_type, id],
    )
    .map_err(|e| format!("Failed to save OAuth account: {}", e))?;

    Ok(id)
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
) -> Result<i64, String> {
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
    let id = save_oauth_account(&app, &auth_json, &identity)?;
    if let Err(e) = refresh_account_quota(app, id).await {
        eprintln!("Failed to fetch initial OAuth quota: {}", e);
    }

    Ok(id)
}

#[command]
async fn complete_codex_oauth_login(
    app: AppHandle,
    login_id: String,
    callback_url: Option<String>,
) -> Result<i64, String> {
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
                   CASE WHEN credential_key IS NOT NULL AND credential_key != '' THEN 1 ELSE 0 END AS has_json_info,
                   plan_type,
                   primary_used_percent, primary_reset_at,
                   primary_window_minutes, primary_window_present,
                   secondary_used_percent, secondary_reset_at,
                   secondary_window_minutes, secondary_window_present,
                   last_quota_checked_at, last_quota_error,
                   created_at, updated_at,
                   COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
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
    let key = account_secret_key(&conn, id)?;
    let json_info = read_account_secret(&key)?;
    let (json_info, changed) = refresh_auth_json_if_needed(&json_info, false).await?;
    if changed {
        save_account_secret(&key, &json_info)?;
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
    save_account_secret(&key, &updated_json)?;

    let plan_type = remote_identity
        .plan_type
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    let account_stub = account_stub_from_json(&updated_json);
    conn.execute(
        "
        UPDATE accounts
        SET json_info = ?1,
            plan_type = CASE WHEN ?2 != 'unknown' THEN ?2 ELSE plan_type END,
            last_quota_error = '',
            updated_at = datetime('now')
        WHERE id = ?3
        ",
        params![account_stub, plan_type, id],
    )
    .map_err(|e| format!("Failed to update account profile: {}", e))?;

    let mut stmt = conn
        .prepare(
            "
            SELECT id, name, activation_date,
                   CASE WHEN credential_key IS NOT NULL AND credential_key != '' THEN 1 ELSE 0 END AS has_json_info,
                   plan_type,
                   primary_used_percent, primary_reset_at,
                   primary_window_minutes, primary_window_present,
                   secondary_used_percent, secondary_reset_at,
                   secondary_window_minutes, secondary_window_present,
                   last_quota_checked_at, last_quota_error,
                   created_at, updated_at,
                   COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
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
    conn.query_row(
        "
        SELECT COUNT(*)
        FROM accounts
        WHERE json_info IS NOT NULL
          AND trim(json_info) != ''
          AND (credential_key IS NULL OR credential_key = '')
        ",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|e| format!("Failed to inspect credential migration status: {}", e))
}

fn migrate_plaintext_credentials(conn: &Connection) -> Result<usize, String> {
    let mut stmt = conn
        .prepare(
            "
            SELECT id, json_info
            FROM accounts
            WHERE json_info IS NOT NULL
              AND trim(json_info) != ''
              AND (credential_key IS NULL OR credential_key = '')
            ",
        )
        .map_err(|e| format!("Failed to prepare credential migration: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query credential migration rows: {}", e))?;

    let mut migrated = Vec::new();
    for row in rows {
        let (id, json_info) =
            row.map_err(|e| format!("Failed to read credential migration row: {}", e))?;
        let key = credential_key(id);
        save_account_secret(&key, &json_info)?;
        migrated.push((id, key, extract_account_id(&json_info).unwrap_or_default()));
    }
    drop(stmt);

    let migrated_count = migrated.len();
    for (id, key, account_id) in migrated {
        let account_stub = account_id
            .is_empty()
            .then(|| "{}".to_string())
            .unwrap_or_else(|| {
                serde_json::json!({ "tokens": { "account_id": account_id } }).to_string()
            });
        conn.execute(
            "
            UPDATE accounts
            SET credential_key = ?1, json_info = ?2, updated_at = datetime('now')
            WHERE id = ?3
            ",
            params![key, account_stub, id],
        )
        .map_err(|e| format!("Failed to finish credential migration: {}", e))?;
    }

    Ok(migrated_count)
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
    conn.execute(
        "
        INSERT INTO accounts (name, activation_date, json_info, updated_at)
        VALUES (?1, ?2, '{}', datetime('now'))
        ",
        params![name.trim(), activation_date],
    )
    .map_err(|e| format!("Failed to add account: {}", e))?;

    let id = conn.last_insert_rowid();
    if !json_info.trim().is_empty() {
        let key = credential_key(id);
        if let Err(e) = save_account_secret(&key, json_info.trim()) {
            let _ = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id]);
            return Err(e);
        }
        if let Err(e) = conn.execute(
            "
            UPDATE accounts
            SET credential_key = ?1, json_info = ?2, updated_at = datetime('now')
            WHERE id = ?3
            ",
            params![key, account_stub_from_json(json_info.trim()), id],
        ) {
            delete_account_secret(&key);
            let _ = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id]);
            return Err(format!("Failed to attach account credential: {}", e));
        }
    }

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
    let stored_key = match conn.query_row(
        "SELECT credential_key FROM accounts WHERE id = ?1",
        params![id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(key) => key,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err("Account not found".to_string()),
        Err(e) => return Err(format!("Failed to find account: {}", e)),
    };

    if should_update_secret {
        let key = if stored_key.is_empty() {
            credential_key(id)
        } else {
            stored_key
        };
        let previous_secret = read_account_secret(&key).ok();
        if let Err(e) = save_account_secret(&key, json_info.trim()) {
            return Err(e);
        }

        if let Err(e) = conn.execute(
            "
            UPDATE accounts
            SET name = ?1,
                activation_date = ?2,
                credential_key = ?3,
                json_info = ?4,
                updated_at = datetime('now')
            WHERE id = ?5
            ",
            params![
                name.trim(),
                activation_date,
                key,
                account_stub_from_json(json_info.trim()),
                id
            ],
        ) {
            if let Some(previous_secret) = previous_secret {
                let _ = save_account_secret(&key, &previous_secret);
            } else {
                delete_account_secret(&key);
            }
            return Err(format!("Failed to update account credential: {}", e));
        }

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
    if let Ok(key) = account_secret_key(&conn, id) {
        delete_account_secret(&key);
    }
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
    migrate_plaintext_credentials(&conn)?;
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
                   credential_key
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
            key,
        ) = row.map_err(|e| format!("Failed to read backup account: {}", e))?;
        if let Some(filter_ids) = &filter_ids {
            if !filter_ids.contains(&id) {
                continue;
            }
        }
        if key.is_empty() {
            continue;
        }
        let json_info = read_account_secret(&key)?;
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
            SELECT id, COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
            FROM accounts
            WHERE COALESCE(json_extract(json_info, '$.tokens.account_id'), '') != ''
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
        VALUES (?1, ?2, '{}', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, datetime('now'))
        ",
        params![
            account.name,
            account.activation_date,
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
    let key = credential_key(id);
    if let Err(e) = save_account_secret(&key, &account.json_info) {
        let _ = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id]);
        return Err(e);
    }
    imported_credentials.push((id, key.clone()));

    if let Err(e) = conn.execute(
        "
        UPDATE accounts
        SET credential_key = ?1, json_info = ?2, updated_at = datetime('now')
        WHERE id = ?3
        ",
        params![key, account_stub_from_json(&account.json_info), id],
    ) {
        delete_account_secret(&key);
        let _ = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id]);
        return Err(format!("Failed to attach imported credential: {}", e));
    }
    Ok(())
}

fn merge_backup_account(conn: &Connection, id: i64, account: &BackupAccount) -> Result<(), String> {
    let key = match conn.query_row(
        "SELECT credential_key FROM accounts WHERE id = ?1",
        params![id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(key) if !key.is_empty() => key,
        Ok(_) => credential_key(id),
        Err(e) => return Err(format!("Failed to find account for merge: {}", e)),
    };
    save_account_secret(&key, &account.json_info)?;
    conn.execute(
        "
        UPDATE accounts
        SET name = ?1,
            activation_date = ?2,
            credential_key = ?3,
            json_info = ?4,
            plan_type = ?5,
            primary_used_percent = ?6,
            primary_reset_at = ?7,
            primary_window_minutes = ?8,
            primary_window_present = ?9,
            secondary_used_percent = ?10,
            secondary_reset_at = ?11,
            secondary_window_minutes = ?12,
            secondary_window_present = ?13,
            last_quota_checked_at = ?14,
            last_quota_error = ?15,
            updated_at = datetime('now')
        WHERE id = ?16
        ",
        params![
            account.name,
            account.activation_date,
            key,
            account_stub_from_json(&account.json_info),
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
    for (id, key) in imported_credentials {
        delete_account_secret(key);
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
        let key = account_secret_key(&conn, id)?;
        let json_info = read_account_secret(&key)?;
        let refreshed_json = match refresh_auth_json_if_needed(&json_info, false).await {
            Ok((updated_json, changed)) => {
                if changed {
                    save_account_secret(&key, &updated_json)?;
                    let account_stub = account_stub_from_json(&updated_json);
                    conn.execute(
                        "
                        UPDATE accounts
                        SET json_info = ?1, updated_at = datetime('now')
                        WHERE id = ?2
                        ",
                        params![account_stub, id],
                    )
                    .map_err(|e| format!("Failed to update refreshed account credential: {}", e))?;
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
        match account_secret_key(&conn, id) {
            Ok(key) => read_account_secret(&key),
            Err(e) => Err(e),
        }
    };

    match secret_result {
        Ok(json_info) => {
            items.push(health_item(
                "credential",
                "凭据读取",
                "ok",
                "系统凭据库可读取",
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
) -> Result<(), String> {
    let conn = open_accounts_db(&app)?;
    let key = account_secret_key(&conn, id)?;
    let json_info = read_account_secret(&key)?;
    let (json_info, changed) = refresh_auth_json_if_needed(&json_info, false).await?;
    if changed {
        save_account_secret(&key, &json_info)?;
        let account_stub = account_stub_from_json(&json_info);
        conn.execute(
            "
            UPDATE accounts
            SET json_info = ?1, updated_at = datetime('now')
            WHERE id = ?2
            ",
            params![account_stub, id],
        )
        .map_err(|e| format!("Failed to update refreshed account credential: {}", e))?;
    }
    switch_account(json_info, restart_codex).await
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

#[command]
fn get_setting(app: AppHandle, key: String) -> Result<Option<String>, String> {
    let conn = open_accounts_db(&app)?;
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

#[command]
fn set_setting(app: AppHandle, key: String, value: String) -> Result<(), String> {
    let conn = open_accounts_db(&app)?;
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

#[command]
async fn switch_account(json_info: String, restart_codex: Option<bool>) -> Result<(), String> {
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

    Ok(())
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
            SELECT id, credential_key,
                   COALESCE(json_extract(json_info, '$.tokens.account_id'), '')
            FROM accounts
            WHERE credential_key IS NOT NULL AND credential_key != ''
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
        let (id, key, account_id) =
            row.map_err(|e| format!("Failed to read current account candidate: {}", e))?;
        if current_account_id == Some(account_id.as_str()) {
            account_id_matches.push(id);
        }
        let Ok(secret) = read_account_secret(&key) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&secret) else {
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
            cancel_codex_oauth_login,
            complete_codex_oauth_login,
            list_accounts,
            list_operation_logs,
            clear_operation_logs,
            refresh_account_profile,
            add_account,
            update_account,
            delete_account,
            export_encrypted_backup,
            preview_encrypted_backup,
            import_encrypted_backup,
            get_migration_status,
            migrate_plaintext_accounts,
            refresh_account_quota,
            check_account_health,
            switch_account_by_id,
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
            get_codex_project_visibility_status,
            repair_codex_project_visibility,
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
    fn long_secret_is_split_into_safe_chunks() {
        let secret = "a".repeat(CREDENTIAL_CHUNK_CHARS * 2 + 17);
        let chunks = chunk_secret(&secret);

        assert_eq!(chunks.len(), 3);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.chars().count() <= CREDENTIAL_CHUNK_CHARS));
        assert_eq!(chunks.concat(), secret);
    }
}
