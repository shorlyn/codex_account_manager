import { ref, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { Account, QuotaInfo } from '../types';

const REFRESH_INTERVAL_SETTING = 'refreshInterval';
const RESTART_CODEX_SETTING = 'restartCodexOnSwitch';

const accounts = ref<Account[]>([]);
const loading = ref(false);
const switchingId = ref<number | null>(null);
const currentAccountId = ref<string | null>(null);
const restartCodexOnSwitch = ref(true);
let refreshTimer: ReturnType<typeof setInterval> | null = null;
const refreshInterval = ref(0);

async function loadAccounts(): Promise<void> {
  loading.value = true;
  try {
    accounts.value = await invoke<Account[]>('list_accounts');
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
  const id = await invoke<number>('add_account', { name, activationDate, jsonInfo });

  if (extractAccessToken(jsonInfo)) {
    try {
      await refreshQuotaById(id);
    } catch (e) {
      console.warn('Failed to fetch initial quota:', e);
    }
  }

  await loadAccounts();
}

async function updateAccount(id: number, name: string, activationDate: string, jsonInfo: string): Promise<void> {
  await invoke('update_account', { id, name, activationDate, jsonInfo });

  if (extractAccessToken(jsonInfo)) {
    try {
      await refreshQuotaById(id);
    } catch (e) {
      console.warn('Failed to refresh quota:', e);
    }
  }

  await loadAccounts();
}

async function deleteAccount(id: number): Promise<void> {
  await invoke('delete_account', { id });
  await loadAccounts();
}

async function refreshQuotaById(id: number | bigint): Promise<void> {
  await invoke<QuotaInfo>('refresh_account_quota', { id: Number(id) });
}

async function refreshQuota(accountId: number): Promise<void> {
  const account = accounts.value.find(a => a.id === accountId);
  if (!account) return;

  if (!account.has_json_info) {
    throw new Error('Account JSON info is empty');
  }

  await refreshQuotaById(accountId);
  await loadAccounts();
}

async function refreshAllQuotas(): Promise<void> {
  for (const account of accounts.value) {
    if (!account.has_json_info) continue;
    try {
      await refreshQuotaById(account.id);
    } catch (e) {
      console.warn(`Failed to refresh quota for account ${account.name}:`, e);
    }
  }
  await loadAccounts();
}

async function switchAccount(accountId: number, restartCodex = restartCodexOnSwitch.value): Promise<void> {
  const account = accounts.value.find(a => a.id === accountId);
  if (!account) throw new Error('Account not found');

  if (!account.has_json_info) {
    throw new Error('Account JSON info is empty');
  }

  switchingId.value = accountId;
  try {
    await invoke('switch_account_by_id', { id: account.id, restartCodex });
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

async function loadRefreshInterval(defaultValue = 10): Promise<void> {
  try {
    const saved = await invoke<string | null>('get_setting', { key: REFRESH_INTERVAL_SETTING });
    const minutes = Number(saved ?? defaultValue);
    startAutoRefresh(Number.isFinite(minutes) ? minutes : defaultValue);
  } catch (e) {
    console.warn('Failed to load refresh interval setting:', e);
    startAutoRefresh(defaultValue);
  }
}

async function setRefreshInterval(intervalMinutes: number): Promise<void> {
  startAutoRefresh(intervalMinutes);
  try {
    await invoke('set_setting', {
      key: REFRESH_INTERVAL_SETTING,
      value: String(intervalMinutes),
    });
  } catch (e) {
    console.warn('Failed to save refresh interval setting:', e);
  }
}

async function loadRestartCodexOnSwitch(defaultValue = true): Promise<void> {
  try {
    const saved = await invoke<string | null>('get_setting', { key: RESTART_CODEX_SETTING });
    restartCodexOnSwitch.value = saved === null ? defaultValue : saved === 'true';
  } catch (e) {
    console.warn('Failed to load restart setting:', e);
    restartCodexOnSwitch.value = defaultValue;
  }
}

async function setRestartCodexOnSwitch(value: boolean): Promise<void> {
  restartCodexOnSwitch.value = value;
  try {
    await invoke('set_setting', {
      key: RESTART_CODEX_SETTING,
      value: String(value),
    });
  } catch (e) {
    console.warn('Failed to save restart setting:', e);
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
    restartCodexOnSwitch,
    refreshInterval,
    loadAccounts,
    loadCurrentAccount,
    addAccount,
    updateAccount,
    deleteAccount,
    refreshQuota,
    refreshAllQuotas,
    switchAccount,
    loadRefreshInterval,
    setRefreshInterval,
    loadRestartCodexOnSwitch,
    setRestartCodexOnSwitch,
    startAutoRefresh,
    stopAutoRefresh,
  };
}
