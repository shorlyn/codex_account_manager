export interface Account {
  id: number;
  name: string;
  activation_date: string;
  has_json_info: boolean;
  account_id: string | null;
  plan_type: string;
  primary_used_percent: number;
  primary_reset_at: number;
  primary_window_minutes: number | null;
  primary_window_present: boolean;
  secondary_used_percent: number;
  secondary_reset_at: number;
  secondary_window_minutes: number | null;
  secondary_window_present: boolean;
  last_quota_checked_at: string;
  last_quota_error: string;
  created_at: string;
  updated_at: string;
}

export interface QuotaInfo {
  plan_type: string;
  primary_used_percent: number;
  primary_reset_at: number;
  primary_window_minutes: number | null;
  primary_window_present: boolean;
  secondary_used_percent: number;
  secondary_reset_at: number;
  secondary_window_minutes: number | null;
  secondary_window_present: boolean;
}

export interface AuthJson {
  auth_mode?: string;
  OPENAI_API_KEY: string | null;
  tokens: {
    id_token: string;
    access_token: string;
    refresh_token: string;
    account_id: string;
  };
  last_refresh: string;
}

export interface StoragePaths {
  app_data_dir: string;
  database_path: string;
  auth_json_path: string;
}

export interface MigrationStatus {
  pending_plaintext_accounts: number;
}

export interface OperationLog {
  id: number;
  level: 'info' | 'warn' | 'error' | string;
  action: string;
  account_id: number | null;
  account_name: string;
  account_identifier: string;
  stage: string;
  message: string;
  details: string;
  created_at: string;
}

export interface BatchRefreshFailure {
  id: number;
  name: string;
  error: string;
}

export interface BatchRefreshProgress {
  done: number;
  total: number;
  currentName: string;
}

export interface BatchRefreshResult {
  success: number;
  failed: number;
  skipped: number;
  failures: BatchRefreshFailure[];
}

export type AccountViewMode = 'cards' | 'compact' | 'table';

export type CodexAppSpeed = 'standard' | 'fast';

export interface CodexAppSpeedConfig {
  speed: CodexAppSpeed;
  config_path: string;
  global_state_path: string;
}

export interface CodexProjectVisibilityStatus {
  project_path: string;
  config_path: string;
  is_trusted: boolean;
  changed: boolean;
}

export interface AccountHealthItem {
  key: string;
  label: string;
  status: 'ok' | 'warn' | 'error';
  message: string;
}

export interface AccountHealthReport {
  account_id: number;
  checked_at: string;
  summary_status: 'ok' | 'warn' | 'error';
  items: AccountHealthItem[];
}

export interface CodexUsageRollup {
  request_count: number;
  success_count: number;
  error_count: number;
  input_tokens: number;
  cached_input_tokens: number;
  non_cached_input_tokens: number;
  output_tokens: number;
  reasoning_output_tokens: number;
  total_tokens: number;
  api_cost_usd: number;
  codex_credits: number;
}

export interface CodexModelUsage {
  model: string;
  usage: CodexUsageRollup;
}

export interface CodexUsageFailure {
  ts: number;
  model: string;
  turn_id: string;
  response_id: string;
  status: string;
  message: string;
}

export interface CodexUsageSummary {
  log_path: string;
  today_start_ts: number;
  today_end_ts: number;
  total: CodexUsageRollup;
  today: CodexUsageRollup;
  by_model: CodexModelUsage[];
  recent_failures: CodexUsageFailure[];
  note: string;
}

export interface BackupPreview {
  version: number;
  total_accounts: number;
  duplicate_accounts: number;
  new_accounts: number;
  account_names: string[];
}

export interface ImportBackupResult {
  imported: number;
  skipped: number;
  updated: number;
}

export type ImportBackupStrategy = 'add' | 'skip_duplicates' | 'merge_by_account_id';
