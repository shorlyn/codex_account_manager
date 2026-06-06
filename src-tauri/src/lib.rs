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
use reqwest::header::{HeaderValue, AUTHORIZATION};
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

// ── API response structs ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ApiResponse {
    plan_type: String,
    rate_limit: RateLimit,
}

#[derive(Debug, Deserialize)]
struct RateLimit {
    primary_window: Window,
    secondary_window: Window,
}

#[derive(Debug, Deserialize)]
struct Window {
    used_percent: i32,
    reset_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuotaInfo {
    pub plan_type: String,
    pub primary_used_percent: i32,
    pub primary_reset_at: i64,
    pub secondary_used_percent: i32,
    pub secondary_reset_at: i64,
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
pub struct Account {
    pub id: i64,
    pub name: String,
    pub activation_date: String,
    pub has_json_info: bool,
    pub account_id: Option<String>,
    pub plan_type: String,
    pub primary_used_percent: i32,
    pub primary_reset_at: i64,
    pub secondary_used_percent: i32,
    pub secondary_reset_at: i64,
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
    secondary_used_percent: i32,
    secondary_reset_at: i64,
    #[serde(default)]
    last_quota_checked_at: String,
    #[serde(default)]
    last_quota_error: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupPayload {
    version: u32,
    accounts: Vec<BackupAccount>,
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
    let account_id: String = row.get(13)?;
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
        secondary_used_percent: row.get(7)?,
        secondary_reset_at: row.get(8)?,
        last_quota_checked_at: row.get(9)?,
        last_quota_error: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
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
    value["last_refresh"] =
        serde_json::Value::Number(serde_json::Number::from(chrono_like_now_timestamp()));

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

    serde_json::to_string_pretty(&serde_json::json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": id_token,
            "access_token": access_token,
            "refresh_token": next_refresh_token,
            "account_id": account_id
        },
        "last_refresh": chrono_like_now_timestamp()
    }))
    .map_err(|e| format!("Failed to serialize auth JSON: {}", e))
}

fn access_token_to_auth_json(access_token: &str) -> Result<String, String> {
    let account_id = access_token_account_id(access_token)
        .ok_or_else(|| "Cannot detect account_id from access_token".to_string())?;
    serde_json::to_string_pretty(&serde_json::json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "",
            "access_token": access_token,
            "refresh_token": "",
            "account_id": account_id
        },
        "last_refresh": chrono_like_now_timestamp()
    }))
    .map_err(|e| format!("Failed to serialize auth JSON: {}", e))
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
            parse_auth_json(&serde_json::to_string(&value).unwrap_or_default())?;
            return serde_json::to_string_pretty(&value)
                .map_err(|e| format!("Failed to serialize auth JSON: {}", e));
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
        let result = std::process::Command::new("cmd")
            .args(["/C", "start", "", "Codex"])
            .output();

        if result.is_err() || result.unwrap().status.code() != Some(0) {
            std::process::Command::new("codex.exe")
                .spawn()
                .map_err(|e| format!("Failed to restart Codex: {}", e))?;
        }
    }

    Ok(())
}

// ── Tauri commands ────────────────────────────────────────────────────

#[command]
async fn fetch_quota(access_token: String) -> Result<QuotaInfo, String> {
    let token = access_token.trim();
    if token.is_empty() {
        return Err("Access token is empty".to_string());
    }
    let auth_header = HeaderValue::from_str(&format!("Bearer {}", token))
        .map_err(|e| format!("Invalid access token for Authorization header: {}", e))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", error_chain(&e)))?;
    let response = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .header(AUTHORIZATION, auth_header)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", error_chain(&e)))?;

    if !response.status().is_success() {
        return Err(format!("API returned status: {}", response.status()));
    }

    let api_response: ApiResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(QuotaInfo {
        plan_type: api_response.plan_type,
        primary_used_percent: api_response.rate_limit.primary_window.used_percent,
        primary_reset_at: api_response.rate_limit.primary_window.reset_at,
        secondary_used_percent: api_response.rate_limit.secondary_window.used_percent,
        secondary_reset_at: api_response.rate_limit.secondary_window.reset_at,
    })
}

async fn fetch_quota_with_token(access_token: &str) -> Result<QuotaInfo, String> {
    fetch_quota(access_token.to_string()).await
}

#[command]
fn start_codex_oauth_login(
    app: AppHandle,
    open_browser: Option<bool>,
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
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true&state={}&originator={}",
        CODEX_AUTH_ENDPOINT,
        percent_encode(CODEX_OAUTH_CLIENT_ID),
        percent_encode(&redirect_uri),
        percent_encode(CODEX_OAUTH_SCOPES),
        percent_encode(&challenge),
        percent_encode(&state),
        percent_encode(CODEX_OAUTH_ORIGINATOR),
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

    let existing_id = conn
        .query_row(
            "
            SELECT id
            FROM accounts
            WHERE COALESCE(json_extract(json_info, '$.tokens.account_id'), '') = ?1
            LIMIT 1
            ",
            params![account_id],
            |row| row.get::<_, i64>(0),
        )
        .ok();

    let id = if let Some(id) = existing_id {
        id
    } else {
        conn.execute(
            "
            INSERT INTO accounts (name, activation_date, json_info, plan_type, updated_at)
            VALUES (?1, '', '{}', ?2, datetime('now'))
            ",
            params![name, plan_type],
        )
        .map_err(|e| format!("Failed to add OAuth account: {}", e))?;
        conn.last_insert_rowid()
    };

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

    let auth_json = serde_json::json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": id_token,
            "access_token": access_token,
            "refresh_token": refresh_token,
            "account_id": account_id
        },
        "last_refresh": chrono_like_now_timestamp()
    })
    .to_string();
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
                   secondary_used_percent, secondary_reset_at,
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
                   secondary_used_percent, secondary_reset_at,
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
fn export_encrypted_backup(app: AppHandle, password: String) -> Result<String, String> {
    let conn = open_accounts_db(&app)?;
    migrate_plaintext_credentials(&conn)?;

    let mut stmt = conn
        .prepare(
            "
            SELECT id, name, activation_date, plan_type,
                   primary_used_percent, primary_reset_at,
                   secondary_used_percent, secondary_reset_at,
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
                row.get::<_, i32>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(|e| format!("Failed to query backup accounts: {}", e))?;

    let mut accounts = Vec::new();
    for row in rows {
        let (
            _id,
            name,
            activation_date,
            plan_type,
            primary_used_percent,
            primary_reset_at,
            secondary_used_percent,
            secondary_reset_at,
            last_quota_checked_at,
            last_quota_error,
            key,
        ) = row.map_err(|e| format!("Failed to read backup account: {}", e))?;
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
            secondary_used_percent,
            secondary_reset_at,
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

#[command]
async fn import_encrypted_backup(
    app: AppHandle,
    backup_text: String,
    password: String,
) -> Result<usize, String> {
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

    let conn = open_accounts_db(&app)?;
    let mut imported = 0usize;
    let mut imported_credentials: Vec<(i64, String)> = Vec::new();

    for account in normalized_accounts {
        if let Err(e) = conn.execute(
            "
            INSERT INTO accounts (
                name, activation_date, json_info, plan_type,
                primary_used_percent, primary_reset_at,
                secondary_used_percent, secondary_reset_at,
                last_quota_checked_at, last_quota_error,
                updated_at
            )
            VALUES (?1, ?2, '{}', ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))
            ",
            params![
                account.name,
                account.activation_date,
                account.plan_type,
                account.primary_used_percent,
                account.primary_reset_at,
                account.secondary_used_percent,
                account.secondary_reset_at,
                account.last_quota_checked_at,
                account.last_quota_error,
            ],
        ) {
            cleanup_imported_accounts(&conn, &imported_credentials);
            return Err(format!("Failed to import account: {}", e));
        }

        let id = conn.last_insert_rowid();
        let key = credential_key(id);
        if let Err(e) = save_account_secret(&key, &account.json_info) {
            let _ = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id]);
            cleanup_imported_accounts(&conn, &imported_credentials);
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
            cleanup_imported_accounts(&conn, &imported_credentials);
            return Err(format!("Failed to attach imported credential: {}", e));
        }
        imported += 1;
    }

    Ok(imported)
}

fn cleanup_imported_accounts(conn: &Connection, imported_credentials: &[(i64, String)]) {
    for (id, key) in imported_credentials {
        delete_account_secret(key);
        let _ = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id]);
    }
}

#[command]
async fn refresh_account_quota(app: AppHandle, id: i64) -> Result<QuotaInfo, String> {
    let access_token = {
        let conn = open_accounts_db(&app)?;
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
                mark_quota_error(&conn, id, &e)?;
                return Err(e);
            }
        };
        match extract_access_token(&refreshed_json) {
            Ok(token) => token,
            Err(e) => {
                mark_quota_error(&conn, id, &e)?;
                return Err(e);
            }
        }
    };

    match fetch_quota_with_token(&access_token).await {
        Ok(quota) => {
            update_account_quota(app, id, quota.clone())?;
            Ok(quota)
        }
        Err(e) => {
            let conn = open_accounts_db(&app)?;
            mark_quota_error(&conn, id, &e)?;
            Err(e)
        }
    }
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
                secondary_used_percent = ?4,
                secondary_reset_at = ?5,
                last_quota_checked_at = datetime('now'),
                last_quota_error = '',
                updated_at = datetime('now')
            WHERE id = ?6
            ",
            params![
                quota.plan_type,
                quota.primary_used_percent,
                quota.primary_reset_at,
                quota.secondary_used_percent,
                quota.secondary_reset_at,
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
    if json_info.trim().is_empty() {
        return Err("JSON info is empty, aborting switch".to_string());
    }

    parse_auth_json(&json_info)?;

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
    if json_info.trim().is_empty() {
        return Err("JSON info is empty".to_string());
    }

    parse_auth_json(&json_info)?;

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
            refresh_account_profile,
            add_account,
            update_account,
            delete_account,
            export_encrypted_backup,
            import_encrypted_backup,
            get_migration_status,
            migrate_plaintext_accounts,
            refresh_account_quota,
            switch_account_by_id,
            update_account_quota,
            get_setting,
            set_setting,
            switch_account,
            write_auth_json,
            read_auth_json,
            get_codex_auth_path,
            get_storage_paths,
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
    fn token_expiration_uses_jwt_exp_with_refresh_skew() {
        let now = chrono_like_now_timestamp();

        assert!(!is_token_expired(&sample_jwt_with_exp(
            now + TOKEN_REFRESH_SKEW_SECONDS + 60
        )));
        assert!(is_token_expired(&sample_jwt_with_exp(now - 60)));
        assert!(is_token_expired("not-a-jwt"));
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
                secondary_used_percent: 34,
                secondary_reset_at: 456,
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
