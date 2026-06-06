export interface Account {
  id: number;
  name: string;
  activation_date: string;
  json_info: string;
  plan_type: string;
  primary_used_percent: number;
  primary_reset_at: number;
  secondary_used_percent: number;
  secondary_reset_at: number;
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
