export interface Account {
  id: number;
  name: string;
  activation_date: string;
  has_json_info: boolean;
  account_id: string | null;
  plan_type: string;
  primary_used_percent: number;
  primary_reset_at: number;
  secondary_used_percent: number;
  secondary_reset_at: number;
  last_quota_checked_at: string;
  last_quota_error: string;
  created_at: string;
  updated_at: string;
}

export interface QuotaInfo {
  plan_type: string;
  primary_used_percent: number;
  primary_reset_at: number;
  secondary_used_percent: number;
  secondary_reset_at: number;
}

export interface AuthJson {
  auth_mode: string;
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
