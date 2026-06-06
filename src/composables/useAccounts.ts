import { ref, onUnmounted } from 'vue';
import Database from '@tauri-apps/plugin-sql';
import { invoke } from '@tauri-apps/api/core';
import type { Account, QuotaInfo } from '../types';

const DB_NAME = 'sqlite:codex_accounts.db';

const accounts = ref<Account[]>([]);
const loading = ref(false);
const switchingId = ref<number | null>(null);
const currentAccountId = ref<string | null>(null);
let db: Database | null = null;
let refreshTimer: ReturnType<typeof setInterval> | null = null;
const refreshInterval = ref(0);

async function getDb(): Promise<Database> {
  if (!db) {
    db = await Database.load(DB_NAME);
    await db.execute(`
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
      )
    `);
  }
  return db;
}

async function loadAccounts(): Promise<void> {
  loading.value = true;
  try {
    const database = await getDb();
    const rows = await database.select<Account[]>('SELECT * FROM accounts ORDER BY id DESC');
    accounts.value = rows;
  } catch (e) {
    console.error('Failed to load accounts:', e);
  } finally {
    loading.value = false;
  }
}

async function loadCurrentAccount(): Promise<void> {
  try {
    const authJson = await invoke<string>('read_auth_json');
    const parsed = JSON.parse(authJson);
    currentAccountId.value = parsed.tokens?.account_id || null;
  } catch {
    currentAccountId.value = null;
  }
}

function extractAccessToken(jsonInfo: string): string | null {
  try {
    const parsed = JSON.parse(jsonInfo);
    return parsed.tokens?.access_token || null;
  } catch {
    return null;
  }
}

async function addAccount(name: string, activationDate: string, jsonInfo: string): Promise<void> {
  const database = await getDb();
  const result = await database.execute(
    `INSERT INTO accounts (name, activation_date, json_info, updated_at)
     VALUES ($1, $2, $3, datetime('now'))`,
    [name, activationDate, jsonInfo]
  );
  const id = result.lastInsertId;
  if (id === undefined) {
    await loadAccounts();
    return;
  }

  // Try to fetch quota info
  const accessToken = extractAccessToken(jsonInfo);
  if (accessToken) {
    try {
      await refreshQuotaById(id, accessToken);
    } catch (e) {
      console.warn('Failed to fetch initial quota:', e);
    }
  }

  await loadAccounts();
}

async function updateAccount(id: number, name: string, activationDate: string, jsonInfo: string): Promise<void> {
  const database = await getDb();
  await database.execute(
    `UPDATE accounts SET name = $1, activation_date = $2, json_info = $3, updated_at = datetime('now')
     WHERE id = $4`,
    [name, activationDate, jsonInfo, id]
  );

  // Try to refresh quota
  const accessToken = extractAccessToken(jsonInfo);
  if (accessToken) {
    try {
      await refreshQuotaById(id, accessToken);
    } catch (e) {
      console.warn('Failed to refresh quota:', e);
    }
  }

  await loadAccounts();
}

async function deleteAccount(id: number): Promise<void> {
  const database = await getDb();
  await database.execute('DELETE FROM accounts WHERE id = $1', [id]);
  await loadAccounts();
}

async function refreshQuotaById(id: number | bigint, accessToken: string): Promise<void> {
  const quota = await invoke<QuotaInfo>('fetch_quota', { accessToken });
  const database = await getDb();
  await database.execute(
    `UPDATE accounts SET
       plan_type = $1,
       primary_used_percent = $2,
       primary_reset_at = $3,
       secondary_used_percent = $4,
       secondary_reset_at = $5,
       updated_at = datetime('now')
     WHERE id = $6`,
    [
      quota.plan_type,
      quota.primary_used_percent,
      quota.primary_reset_at,
      quota.secondary_used_percent,
      quota.secondary_reset_at,
      Number(id),
    ]
  );
}

async function refreshQuota(accountId: number): Promise<void> {
  const account = accounts.value.find(a => a.id === accountId);
  if (!account) return;

  const accessToken = extractAccessToken(account.json_info);
  if (!accessToken) {
    throw new Error('No access token found in account JSON');
  }

  await refreshQuotaById(accountId, accessToken);
  await loadAccounts();
}

async function refreshAllQuotas(): Promise<void> {
  for (const account of accounts.value) {
    const accessToken = extractAccessToken(account.json_info);
    if (!accessToken) continue;
    try {
      await refreshQuotaById(account.id, accessToken);
    } catch (e) {
      console.warn(`Failed to refresh quota for account ${account.name}:`, e);
    }
  }
  await loadAccounts();
}

async function switchAccount(accountId: number): Promise<void> {
  const account = accounts.value.find(a => a.id === accountId);
  if (!account) throw new Error('Account not found');

  if (!account.json_info || account.json_info.trim() === '') {
    throw new Error('Account JSON info is empty');
  }

  switchingId.value = accountId;
  try {
    await invoke('switch_account', { jsonInfo: account.json_info });
    await loadCurrentAccount();
  } finally {
    switchingId.value = null;
  }
}

function startAutoRefresh(intervalMinutes: number): void {
  stopAutoRefresh();
  refreshInterval.value = intervalMinutes;
  if (intervalMinutes > 0) {
    refreshTimer = setInterval(() => {
      refreshAllQuotas();
    }, intervalMinutes * 60 * 1000);
  }
}

function stopAutoRefresh(): void {
  if (refreshTimer) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }
}

onUnmounted(() => {
  stopAutoRefresh();
});

export function useAccounts() {
  return {
    accounts,
    loading,
    switchingId,
    currentAccountId,
    refreshInterval,
    loadAccounts,
    loadCurrentAccount,
    addAccount,
    updateAccount,
    deleteAccount,
    refreshQuota,
    refreshAllQuotas,
    switchAccount,
    startAutoRefresh,
    stopAutoRefresh,
  };
}
