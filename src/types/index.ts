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

export interface CodexHotSwitchResult {
  status: 'applied' | 'unavailable' | 'failed' | 'skipped' | string;
  message: string;
  detail: string;
}

export interface SwitchAccountResult {
  restarted: boolean;
  auth_json_path: string;
  hot_switch: CodexHotSwitchResult;
}

export interface OAuthSaveResult {
  id: number;
  created: boolean;
  name: string;
  accountId: string;
}

export interface CodexProxyState {
  enabled: boolean;
  port: number;
  base_url: string;
  auth_token: string;
  config_snippet: string;
  active_account_id: number | null;
  active_account_name: string;
  config_installed: boolean;
  auth_enabled: boolean;
  active_requests: number;
  max_concurrent_requests: number;
  config_path: string;
  last_error: string;
}

export interface CodexProxyAccountChangedEvent {
  previousAccountId: number | null;
  activeAccountId: number;
  activeAccountName: string;
  activeAccountIdentifier: string;
  reason: string;
  reasonLabel: string;
  stage: string;
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

export interface CodexOfficialModeIssue {
  line: number;
  label: string;
}

export interface CodexFeatureStatus {
  config_path: string;
  global_state_path: string;
  goals_db_path: string;
  goals_enabled: boolean;
  goals_db_present: boolean;
  memory_generate_enabled: boolean;
  memory_use_enabled: boolean;
  official_mode_ok: boolean;
  official_mode_issues: CodexOfficialModeIssue[];
  config_speed: CodexAppSpeed;
  config_service_tier: string | null;
  global_state_speed: CodexAppSpeed;
  global_state_service_tier: string | null;
  global_state_user_changed_tier: boolean;
  fast_state_synced: boolean;
}

export interface CodexProjectVisibilityStatus {
  project_path: string;
  config_path: string;
  is_trusted: boolean;
  changed: boolean;
}

export interface CodexSessionVisibilityStatus {
  codex_home: string;
  state_db_path: string;
  session_index_path: string;
  target_provider: string;
  scanned_rollout_files: number;
  mismatched_rollout_files: number;
  mismatched_sqlite_records: number;
  missing_sqlite_records: number;
  missing_session_index_entries: number;
}

export interface CodexSessionVisibilityRepairFailure {
  path: string;
  error: string;
}

export interface CodexSessionVisibilityRepairReport {
  codex_home: string;
  state_db_path: string;
  session_index_path: string;
  target_provider: string;
  backup_dir: string;
  scanned_rollout_files: number;
  rewritten_rollout_files: number;
  sqlite_records_updated: number;
  sqlite_records_inserted: number;
  session_index_entries_added: number;
  failed_rollout_files: CodexSessionVisibilityRepairFailure[];
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
