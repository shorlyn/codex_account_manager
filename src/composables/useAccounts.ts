import { ref, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { Account, AccountViewMode, BatchRefreshProgress, BatchRefreshResult, QuotaInfo } from '../types';

const REFRESH_INTERVAL_SETTING = 'refreshInterval';
const RESTART_CODEX_SETTING = 'restartCodexOnSwitch';
const ACCOUNT_VIEW_MODE_SETTING = 'accountViewMode';

const accounts = ref<Account[]>([]);
const loading = ref(false);
const switchingId = ref<number | null>(null);
const currentAccountRecordId = ref<number | null>(null);
const restartCodexOnSwitch = ref(true);
const accountViewMode = ref<AccountViewMode>('cards');
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
    currentAccountRecordId.value = await invoke<number | null>('get_current_account_record_id');
  } catch {
    currentAccountRecordId.value = null;
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

async function refreshProfile(accountId: number): Promise<void> {
  await invoke<Account>('refresh_account_profile', { id: accountId });
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

async function refreshQuotaBatch(
  accountIds: number[],
  onProgress?: (progress: BatchRefreshProgress) => void,
): Promise<BatchRefreshResult> {
  const idSet = new Set(accountIds);
  let success = 0;
  let failed = 0;
  let skipped = 0;
  let done = 0;
  const selectedAccounts = accounts.value.filter(account => idSet.has(account.id));
  const total = selectedAccounts.length;
  const failures: BatchRefreshResult['failures'] = [];

  for (const account of selectedAccounts) {
    onProgress?.({ done, total, currentName: account.name });
    if (!account.has_json_info) {
      skipped += 1;
      done += 1;
      onProgress?.({ done, total, currentName: account.name });
      continue;
    }
    try {
      await refreshQuotaById(account.id);
      success += 1;
    } catch (e) {
      const error = String(e);
      failed += 1;
      failures.push({ id: account.id, name: account.name, error });
      console.warn(`Failed to refresh quota for account ${account.name}:`, e);
    }
    done += 1;
    onProgress?.({ done, total, currentName: account.name });
  }

  await loadAccounts();
  return { success, failed, skipped, failures };
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

async function loadAccountViewMode(defaultValue: AccountViewMode = 'cards'): Promise<void> {
  try {
    const saved = await invoke<string | null>('get_setting', { key: ACCOUNT_VIEW_MODE_SETTING });
    accountViewMode.value = saved === 'table' || saved === 'cards' || saved === 'compact' ? saved : defaultValue;
  } catch (e) {
    console.warn('Failed to load account view mode setting:', e);
    accountViewMode.value = defaultValue;
  }
}

async function setAccountViewMode(value: AccountViewMode): Promise<void> {
  accountViewMode.value = value;
  try {
    await invoke('set_setting', {
      key: ACCOUNT_VIEW_MODE_SETTING,
      value,
    });
  } catch (e) {
    console.warn('Failed to save account view mode setting:', e);
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
    currentAccountRecordId,
    restartCodexOnSwitch,
    accountViewMode,
    refreshInterval,
    loadAccounts,
    loadCurrentAccount,
    addAccount,
    updateAccount,
    deleteAccount,
    refreshQuota,
    refreshProfile,
    refreshAllQuotas,
    refreshQuotaBatch,
    switchAccount,
    loadRefreshInterval,
    setRefreshInterval,
    loadRestartCodexOnSwitch,
    setRestartCodexOnSwitch,
    loadAccountViewMode,
    setAccountViewMode,
    startAutoRefresh,
    stopAutoRefresh,
  };
}
