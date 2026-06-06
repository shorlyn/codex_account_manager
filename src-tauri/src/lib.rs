use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand_core::{OsRng, RngCore};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{
    command,
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WindowEvent,
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

// ── Helper functions ──────────────────────────────────────────────────

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

fn save_account_secret(key: &str, json_info: &str) -> Result<(), String> {
    credential_entry(key)?
        .set_password(json_info)
        .map_err(|e| format!("Failed to save account credential: {}", e))
}

fn read_account_secret(key: &str) -> Result<String, String> {
    credential_entry(key)?
        .get_password()
        .map_err(|e| format!("Failed to read account credential: {}", e))
}

fn delete_account_secret(key: &str) {
    let _ = credential_entry(key).and_then(|entry| {
        entry
            .delete_credential()
            .map_err(|e| format!("Failed to delete account credential: {}", e))
    });
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
    require_json_string(&value, "/tokens/refresh_token", "tokens.refresh_token")?;
    require_json_string(&value, "/tokens/account_id", "tokens.account_id")?;
    Ok(value)
}

fn extract_access_token(json_info: &str) -> Result<String, String> {
    let value = parse_auth_json(json_info)?;
    require_json_string(&value, "/tokens/access_token", "tokens.access_token")
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
    let message = if error.chars().count() > 500 {
        format!("{}...", error.chars().take(500).collect::<String>())
    } else {
        error.to_string()
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
    let client = reqwest::Client::new();
    let response = client
        .get("https://chatgpt.com/backend-api/wham/usage")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

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
fn add_account(
    app: AppHandle,
    name: String,
    activation_date: String,
    json_info: String,
) -> Result<i64, String> {
    if name.trim().is_empty() {
        return Err("Account name is required".to_string());
    }
    if !json_info.trim().is_empty() {
        parse_auth_json(&json_info)?;
    }

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
fn update_account(
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
fn import_encrypted_backup(
    app: AppHandle,
    backup_text: String,
    password: String,
) -> Result<usize, String> {
    let payload = decrypt_backup_payload(&backup_text, &password)?;
    if payload.version != 1 {
        return Err("Unsupported backup payload version".to_string());
    }

    for account in &payload.accounts {
        parse_auth_json(&account.json_info)
            .map_err(|e| format!("Invalid account JSON in backup: {}", e))?;
    }

    let conn = open_accounts_db(&app)?;
    let mut imported = 0usize;
    let mut imported_credentials: Vec<(i64, String)> = Vec::new();

    for account in payload.accounts {
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
        match extract_access_token(&json_info) {
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
            list_accounts,
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
}
