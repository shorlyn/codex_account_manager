use serde::{Deserialize, Serialize};
use tauri::command;

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
    Ok(std::path::PathBuf::from(home).join(".codex").join("auth.json"))
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

#[command]
async fn switch_account(json_info: String) -> Result<(), String> {
    if json_info.trim().is_empty() {
        return Err("JSON info is empty, aborting switch".to_string());
    }

    serde_json::from_str::<serde_json::Value>(&json_info)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    kill_codex_process()?;

    let auth_path = get_auth_path()?;
    if let Some(parent) = auth_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .codex directory: {}", e))?;
    }
    std::fs::write(&auth_path, &json_info)
        .map_err(|e| format!("Failed to write auth.json: {}", e))?;

    restart_codex_process()?;

    Ok(())
}

#[command]
async fn write_auth_json(json_info: String) -> Result<(), String> {
    if json_info.trim().is_empty() {
        return Err("JSON info is empty".to_string());
    }

    serde_json::from_str::<serde_json::Value>(&json_info)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

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

    std::fs::read_to_string(&auth_path)
        .map_err(|e| format!("Failed to read auth.json: {}", e))
}

#[command]
async fn get_codex_auth_path() -> Result<String, String> {
    let auth_path = get_auth_path()?;
    Ok(auth_path.to_string_lossy().to_string())
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
        .plugin(tauri_plugin_sql::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            fetch_quota,
            switch_account,
            write_auth_json,
            read_auth_json,
            get_codex_auth_path,
            is_codex_running,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
