<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useAccounts } from './composables/useAccounts';
import AccountList from './components/AccountList.vue';
import AccountDialog from './components/AccountDialog.vue';
import type {
  Account,
  AccountHealthReport,
  BackupPreview,
  CodexAppSpeed,
  CodexAppSpeedConfig,
  CodexProjectVisibilityStatus,
  CodexUsageRollup,
  CodexUsageSummary,
  BatchRefreshFailure,
  BatchRefreshProgress,
  ImportBackupResult,
  ImportBackupStrategy,
  MigrationStatus,
  OperationLog,
  StoragePaths,
} from './types';

const {
  accounts, loading, switchingId, currentAccountRecordId, refreshInterval, accountViewMode,
  restartCodexOnSwitch,
  loadAccounts, loadCurrentAccount, addAccount, updateAccount, deleteAccount,
  refreshQuota, refreshProfile, refreshQuotaBatch, switchAccount, loadRefreshInterval, setRefreshInterval,
  loadRestartCodexOnSwitch, setRestartCodexOnSwitch, loadAccountViewMode, setAccountViewMode,
} = useAccounts();

const showDialog = ref(false);
const editingAccount = ref<Account | null>(null);
const message = ref('');
const messageType = ref<'success' | 'error'>('success');
const storagePaths = ref<StoragePaths | null>(null);
const migrationStatus = ref<MigrationStatus | null>(null);
const showStorageDetails = ref(false);
const savingAccount = ref(false);
const migratingAccounts = ref(false);
const oauthAdding = ref(false);
const showToolsMenu = ref(false);
const codexAppSpeed = ref<CodexAppSpeed>('standard');
const codexSpeedSaving = ref(false);
const codexUsage = ref<CodexUsageSummary | null>(null);
const codexUsageLoading = ref(false);
const batchRefreshing = ref(false);
const batchRefreshProgress = ref<BatchRefreshProgress | null>(null);
const batchRefreshFailures = ref<BatchRefreshFailure[]>([]);
const detailAccountId = ref<number | null>(null);
const healthCheckingId = ref<number | null>(null);
const healthReports = ref<Record<number, AccountHealthReport>>({});
const showOauthDialog = ref(false);
const oauthLoginId = ref('');
const oauthUrl = ref('');
const oauthCallbackUrl = ref('');
const oauthError = ref('');
const oauthUrlCopied = ref(false);
const oauthTimedOut = ref(false);
const importInput = ref<HTMLInputElement | null>(null);
const importBackupText = ref('');
const importPassword = ref('');
const importPreview = ref<BackupPreview | null>(null);
const importStrategy = ref<ImportBackupStrategy>('add');
const importingBackup = ref(false);
const showImportPreviewDialog = ref(false);
const showOperationLogs = ref(false);
const operationLogs = ref<OperationLog[]>([]);
const operationLogsLoading = ref(false);
const operationLogAccountId = ref<number | null>(null);
const operationLogErrorsOnly = ref(false);
const operationLogActionFilter = ref('all');
let messageTimer: ReturnType<typeof setTimeout> | null = null;
let unlistenOauth: UnlistenFn | null = null;
let unlistenOauthTimeout: UnlistenFn | null = null;

const intervalOptions = [
  { label: '关闭', value: 0 },
  { label: '5 分钟', value: 5 },
  { label: '10 分钟', value: 10 },
  { label: '15 分钟', value: 15 },
  { label: '30 分钟', value: 30 },
];

type AccountFilterStatus = 'all' | 'current' | 'usable' | 'unavailable' | 'authInvalid' | 'quotaLimited' | 'queryFailed' | 'empty';
type AccountSortBy = 'created_at' | 'name' | 'primary_remaining' | 'secondary_remaining' | 'primary_reset' | 'secondary_reset' | 'last_checked';
type AccountSortDirection = 'asc' | 'desc';
type ErrorReasonKey = 'missing_json' | 'auth' | 'billing' | 'forbidden' | 'rate_limit' | 'network' | 'parse' | 'other';

interface ErrorReason {
  key: ErrorReasonKey;
  label: string;
}

const searchQuery = ref('');
const accountFilterStatus = ref<AccountFilterStatus>('all');
const accountSortBy = ref<AccountSortBy>('created_at');
const accountSortDirection = ref<AccountSortDirection>('desc');

const filterOptions: Array<{ label: string; value: AccountFilterStatus }> = [
  { label: '全部', value: 'all' },
  { label: '当前', value: 'current' },
  { label: '可用', value: 'usable' },
  { label: '不可用', value: 'unavailable' },
  { label: '授权无效', value: 'authInvalid' },
  { label: '额度受限', value: 'quotaLimited' },
  { label: '查询失败', value: 'queryFailed' },
  { label: '无凭据', value: 'empty' },
];

const sortOptions: Array<{ label: string; value: AccountSortBy }> = [
  { label: '创建时间', value: 'created_at' },
  { label: '账号名称', value: 'name' },
  { label: '额度一剩余', value: 'primary_remaining' },
  { label: '额度二剩余', value: 'secondary_remaining' },
  { label: '额度一重置', value: 'primary_reset' },
  { label: '额度二重置', value: 'secondary_reset' },
  { label: '最近检查', value: 'last_checked' },
];

function quotaRemaining(used: number): number {
  return Math.max(0, Math.min(100, 100 - used));
}

function quotaWindowLabel(minutes: number | null, fallback: 'primary' | 'secondary', compact = false): string {
  if (!minutes || minutes <= 0) {
    return fallback === 'secondary' ? (compact ? '周' : '周额度') : (compact ? '5h' : '5 小时额度');
  }

  const hourMinutes = 60;
  const dayMinutes = 24 * hourMinutes;
  const weekMinutes = 7 * dayMinutes;
  if (minutes >= 28 * dayMinutes && minutes <= 31 * dayMinutes) return compact ? '月' : '月额度';
  if (minutes >= weekMinutes - 1) {
    const weeks = Math.ceil(minutes / weekMinutes);
    return weeks <= 1 ? (compact ? '周' : '周额度') : `${weeks} 周额度`;
  }
  if (minutes >= dayMinutes - 1) return `${Math.ceil(minutes / dayMinutes)}d 额度`;
  if (minutes >= hourMinutes) return `${Math.ceil(minutes / hourMinutes)}h 额度`;
  return `${Math.ceil(minutes)}m 额度`;
}

function quotaSummaryLabel(accounts: Account[], field: 'primary' | 'secondary'): string {
  const minutes = Array.from(new Set(
    accounts
      .map(account => field === 'primary' ? account.primary_window_minutes : account.secondary_window_minutes)
      .filter((value): value is number => typeof value === 'number' && value > 0),
  ));
  if (minutes.length === 1) return quotaWindowLabel(minutes[0], field, true);
  return field === 'primary' ? '额度一' : '额度二';
}

function formatResetTime(timestamp: number): string {
  if (!timestamp) return '-';
  const d = new Date(timestamp * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function formatCheckedTime(value: string): string {
  if (!value) return '尚未检查';
  return value.replace('T', ' ');
}

function formatTokenAmount(value: number | null | undefined): string {
  const normalized = typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
  if (normalized >= 100_000_000) return `${(normalized / 100_000_000).toFixed(2).replace(/\.00$/, '')}亿`;
  if (normalized >= 10_000) return `${(normalized / 10_000).toFixed(1).replace(/\.0$/, '')}万`;
  return Math.round(normalized).toLocaleString('zh-CN');
}

function formatExactNumber(value: number | null | undefined): string {
  const normalized = typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
  return Math.round(normalized).toLocaleString('zh-CN');
}

function formatUsd(value: number | null | undefined): string {
  const normalized = typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: normalized >= 100 ? 2 : 4,
  }).format(normalized);
}

function formatCredits(value: number | null | undefined): string {
  const normalized = typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
  return normalized.toLocaleString('zh-CN', {
    maximumFractionDigits: normalized >= 100 ? 1 : 3,
  });
}

function usageSuccessRate(usage: CodexUsageRollup | null | undefined): string {
  if (!usage || usage.request_count <= 0) return '-';
  return `${((usage.success_count / usage.request_count) * 100).toFixed(1)}%`;
}

function hasQuotaError(account: Account): boolean {
  return Boolean(account.last_quota_error?.trim());
}

function extractHttpStatus(error: string): number | null {
  const explicitMatch = error.match(/\b(?:http\s*status|status(?:\s*code)?|code|http)\D*(\d{3})\b/i);
  if (explicitMatch) return Number(explicitMatch[1]);
  const standaloneMatch = error.match(/\b([45]\d{2})\b/);
  return standaloneMatch ? Number(standaloneMatch[1]) : null;
}

function accountErrorReason(account: Account): ErrorReason | null {
  if (!account.has_json_info) {
    return { key: 'missing_json', label: '无凭据' };
  }

  const rawError = account.last_quota_error?.trim();
  if (!rawError) return null;

  const error = rawError.toLowerCase();
  const status = extractHttpStatus(rawError);

  if (
    status === 401
    || error.includes('unauthorized')
    || error.includes('auth')
    || error.includes('token')
    || error.includes('refresh')
    || error.includes('授权')
    || error.includes('登录')
  ) {
    return { key: 'auth', label: '401 授权无效' };
  }

  if (
    status === 402
    || error.includes('payment required')
    || error.includes('billing')
    || error.includes('insufficient_quota')
    || error.includes('quota exceeded')
    || error.includes('额度')
    || error.includes('付款')
  ) {
    return { key: 'billing', label: '402 额度/付款' };
  }

  if (status === 403 || error.includes('forbidden') || error.includes('permission') || error.includes('权限')) {
    return { key: 'forbidden', label: '403 权限拒绝' };
  }

  if (status === 429 || error.includes('rate limit') || error.includes('too many requests') || error.includes('频率')) {
    return { key: 'rate_limit', label: '429 频率限制' };
  }

  if (
    error.includes('timeout')
    || error.includes('network')
    || error.includes('connection')
    || error.includes('dns')
    || error.includes('timed out')
    || error.includes('网络')
    || error.includes('超时')
  ) {
    return { key: 'network', label: '网络/超时' };
  }

  if (error.includes('json') || error.includes('parse') || error.includes('解析')) {
    return { key: 'parse', label: '数据解析' };
  }

  return { key: 'other', label: status ? `${status} 其他错误` : '其他错误' };
}

function isAuthInvalid(account: Account): boolean {
  const reason = accountErrorReason(account);
  return reason?.key === 'auth';
}

function isQuotaLimited(account: Account): boolean {
  const reason = accountErrorReason(account);
  if (reason?.key === 'billing' || reason?.key === 'forbidden' || reason?.key === 'rate_limit') return true;
  if (!account.has_json_info || hasQuotaError(account)) return false;
  const primaryEmpty = account.primary_window_present && quotaRemaining(account.primary_used_percent) <= 0;
  const secondaryEmpty = account.secondary_window_present && quotaRemaining(account.secondary_used_percent) <= 0;
  return primaryEmpty || secondaryEmpty;
}

function isQuotaQueryFailed(account: Account): boolean {
  const reason = accountErrorReason(account);
  return reason?.key === 'network' || reason?.key === 'parse' || reason?.key === 'other';
}

function isAccountUnavailable(account: Account): boolean {
  const reason = accountErrorReason(account);
  return reason?.key === 'missing_json' || reason?.key === 'auth';
}

function isAccountUsable(account: Account): boolean {
  return account.has_json_info && !isAccountUnavailable(account) && !isQuotaLimited(account);
}

function accountMatchesStatus(account: Account, status: AccountFilterStatus): boolean {
  if (status === 'all') return true;
  if (status === 'current') return currentAccountRecordId.value === account.id;
  if (status === 'usable') return isAccountUsable(account);
  if (status === 'unavailable') return isAccountUnavailable(account);
  if (status === 'authInvalid') return isAuthInvalid(account);
  if (status === 'quotaLimited') return isQuotaLimited(account);
  if (status === 'queryFailed') return isQuotaQueryFailed(account);
  if (status === 'empty') return !account.has_json_info;
  return true;
}

function dateValue(value: string): number {
  if (!value) return 0;
  const parsed = Date.parse(value.includes('T') ? value : value.replace(' ', 'T'));
  return Number.isFinite(parsed) ? parsed : 0;
}

function sortValue(account: Account, sortBy: AccountSortBy): number | string {
  if (sortBy === 'name') return account.name.toLowerCase();
  if (sortBy === 'primary_remaining') return quotaRemaining(account.primary_used_percent);
  if (sortBy === 'secondary_remaining') return quotaRemaining(account.secondary_used_percent);
  if (sortBy === 'primary_reset') return account.primary_reset_at || 0;
  if (sortBy === 'secondary_reset') return account.secondary_reset_at || 0;
  if (sortBy === 'last_checked') return dateValue(account.last_quota_checked_at);
  return dateValue(account.created_at);
}

const currentAccount = computed(() => {
  if (currentAccountRecordId.value === null) return null;
  return accounts.value.find(account => account.id === currentAccountRecordId.value) ?? null;
});

const detailAccount = computed(() => {
  if (detailAccountId.value === null) return null;
  return accounts.value.find(account => account.id === detailAccountId.value) ?? null;
});

const accountStats = computed(() => {
  const usableAccounts = accounts.value.filter(isAccountUsable);
  const current = currentAccount.value;
  const errorReasonCounts = new Map<string, { label: string; count: number }>();
  const unavailable = accounts.value.filter(isAccountUnavailable).length;
  const quotaLimited = accounts.value.filter(isQuotaLimited).length;
  const queryFailed = accounts.value.filter(isQuotaQueryFailed).length;

  accounts.value.forEach((account) => {
    const reason = accountErrorReason(account);
    if (!reason) return;
    const existing = errorReasonCounts.get(reason.key);
    if (existing) {
      existing.count += 1;
    } else {
      errorReasonCounts.set(reason.key, { label: reason.label, count: 1 });
    }
  });

  return {
    total: accounts.value.length,
    usable: usableAccounts.length,
    unavailable,
    quotaLimited,
    queryFailed,
    issueCount: unavailable + quotaLimited + queryFailed,
    authInvalid: accounts.value.filter(isAuthInvalid).length,
    errorReasons: Array.from(errorReasonCounts.values()),
    currentName: current?.name ?? '未切换',
    currentPrimaryRemaining: current ? quotaRemaining(current.primary_used_percent) : null,
    currentSecondaryRemaining: current ? quotaRemaining(current.secondary_used_percent) : null,
    currentPrimaryLabel: current ? quotaWindowLabel(current.primary_window_minutes, 'primary', true) : '额度一',
    currentSecondaryLabel: current ? quotaWindowLabel(current.secondary_window_minutes, 'secondary', true) : '额度二',
    currentSecondaryVisible: current?.secondary_window_present ?? true,
    totalPrimaryLabel: quotaSummaryLabel(usableAccounts, 'primary'),
    totalSecondaryLabel: quotaSummaryLabel(usableAccounts, 'secondary'),
    totalPrimaryRemaining: usableAccounts
      .filter(account => account.primary_window_present)
      .reduce((sum, account) => sum + quotaRemaining(account.primary_used_percent), 0),
    totalSecondaryRemaining: usableAccounts
      .filter(account => account.secondary_window_present)
      .reduce((sum, account) => sum + quotaRemaining(account.secondary_used_percent), 0),
    hasSecondaryQuota: usableAccounts.some(account => account.secondary_window_present),
  };
});

const filteredAccounts = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  const direction = accountSortDirection.value === 'asc' ? 1 : -1;

  return accounts.value
    .filter((account) => {
      if (!accountMatchesStatus(account, accountFilterStatus.value)) return false;
      if (!query) return true;
      return [
        account.name,
        account.account_id ?? '',
        account.plan_type,
        String(account.id),
      ].some(value => value.toLowerCase().includes(query));
    })
    .slice()
    .sort((a, b) => {
      const left = sortValue(a, accountSortBy.value);
      const right = sortValue(b, accountSortBy.value);
      if (typeof left === 'string' || typeof right === 'string') {
        return String(left).localeCompare(String(right), 'zh-Hans-CN') * direction;
      }
      if (left === right) return b.id - a.id;
      return (left - right) * direction;
    });
});

const filteredRefreshableIds = computed(() =>
  filteredAccounts.value
    .filter(account => account.has_json_info)
    .map(account => account.id),
);

const hasActiveAccountFilters = computed(() =>
  Boolean(searchQuery.value.trim()) || accountFilterStatus.value !== 'all',
);

const detailHealthItems = computed(() => {
  const account = detailAccount.value;
  if (!account) return [];
  return [
    {
      label: '凭据',
      ok: account.has_json_info,
      text: account.has_json_info ? '已保存' : '缺失',
    },
    {
      label: '账号 ID',
      ok: Boolean(account.account_id),
      text: account.account_id ? '已识别' : '未识别',
    },
    {
      label: '额度接口',
      ok: !account.last_quota_error,
      text: account.last_quota_error ? '最近失败' : '正常',
    },
    {
      label: '当前运行',
      ok: currentAccountRecordId.value === account.id,
      text: currentAccountRecordId.value === account.id ? '是' : '否',
    },
  ];
});

const detailHealthReport = computed(() => {
  const account = detailAccount.value;
  if (!account) return null;
  return healthReports.value[account.id] ?? null;
});

const operationLogActionOptions = computed(() => {
  const actions = Array.from(new Set(operationLogs.value.map(log => log.action).filter(Boolean)));
  return actions.sort((a, b) => logActionLabel(a).localeCompare(logActionLabel(b), 'zh-Hans-CN'));
});

const topCodexUsageModels = computed(() => codexUsage.value?.by_model.slice(0, 4) ?? []);

const visibleOperationLogs = computed(() =>
  operationLogs.value.filter((log) => {
    if (operationLogErrorsOnly.value && log.level !== 'error') return false;
    if (operationLogActionFilter.value !== 'all' && log.action !== operationLogActionFilter.value) return false;
    return true;
  }),
);

onMounted(async () => {
  unlistenOauth = await listen<{ loginId?: string }>('codex-oauth-login-completed', async (event) => {
    if (!showOauthDialog.value || !event.payload?.loginId) return;
    if (event.payload.loginId !== oauthLoginId.value) return;
    await completeOauthLogin(true);
  });
  unlistenOauthTimeout = await listen<{ loginId?: string; timeoutSeconds?: number }>('codex-oauth-login-timeout', async (event) => {
    if (!showOauthDialog.value || !event.payload?.loginId) return;
    if (event.payload.loginId !== oauthLoginId.value) return;
    oauthTimedOut.value = true;
    oauthAdding.value = false;
    oauthError.value = `授权已超时，请刷新授权链接后重试。`;
  });
  await loadAccounts();
  await loadCurrentAccount();
  await loadStoragePaths();
  await loadMigrationStatus();
  await loadRefreshInterval(10);
  await loadRestartCodexOnSwitch(true);
  await loadAccountViewMode('cards');
  await loadCodexAppSpeed();
  await loadCodexUsage();
});

onUnmounted(() => {
  if (unlistenOauth) {
    unlistenOauth();
    unlistenOauth = null;
  }
  if (unlistenOauthTimeout) {
    unlistenOauthTimeout();
    unlistenOauthTimeout = null;
  }
});

function showMessage(text: string, type: 'success' | 'error' = 'success') {
  message.value = text;
  messageType.value = type;
  if (messageTimer) clearTimeout(messageTimer);
  messageTimer = setTimeout(() => { message.value = ''; }, 3000);
}

function openAddDialog() { editingAccount.value = null; showDialog.value = true; }
function openEditDialog(account: Account) { editingAccount.value = account; showDialog.value = true; }
function closeDialog() { showDialog.value = false; editingAccount.value = null; }

async function addAccountWithOAuth() {
  oauthAdding.value = true;
  try {
    const started = await invoke<{ loginId: string; authUrl: string }>('start_codex_oauth_login', {
      openBrowser: false,
      forceAccountSelection: true,
    });
    oauthLoginId.value = started.loginId;
    oauthUrl.value = started.authUrl;
    oauthCallbackUrl.value = '';
    oauthError.value = '';
    oauthUrlCopied.value = false;
    oauthTimedOut.value = false;
    showOauthDialog.value = true;
  } catch (e) {
    const message = String(e).replace(/^Error:\s*/, '');
    if (message.includes('CODEX_OAUTH_PORT_IN_USE')) {
      showMessage('OAuth 回调端口 1455 被占用，请关闭占用程序后重试', 'error');
    } else {
      showMessage(`OAuth 添加失败: ${message}`, 'error');
    }
  } finally {
    oauthAdding.value = false;
  }
}

async function cancelOauthSession() {
  if (!oauthLoginId.value) return;
  await invoke('cancel_codex_oauth_login', { loginId: oauthLoginId.value }).catch(() => {});
}

async function closeOauthDialog() {
  await cancelOauthSession();
  showOauthDialog.value = false;
  oauthAdding.value = false;
  oauthLoginId.value = '';
  oauthUrl.value = '';
  oauthCallbackUrl.value = '';
  oauthError.value = '';
  oauthUrlCopied.value = false;
  oauthTimedOut.value = false;
}

async function retryOauthLogin() {
  await closeOauthDialog();
  await addAccountWithOAuth();
}

async function copyOauthUrl() {
  if (!oauthUrl.value) return;
  try {
    await navigator.clipboard.writeText(oauthUrl.value);
    oauthUrlCopied.value = true;
    setTimeout(() => { oauthUrlCopied.value = false; }, 1200);
  } catch {
    oauthError.value = '复制失败，请手动选中授权链接复制';
  }
}

async function openOauthUrl() {
  try {
    window.open(oauthUrl.value, '_blank', 'noopener,noreferrer');
  } catch {
    await copyOauthUrl();
  }
}

async function completeOauthLogin(auto = false) {
  if (!auto && !oauthCallbackUrl.value.trim()) {
    oauthError.value = '请粘贴完整回调地址';
    return;
  }
  oauthAdding.value = true;
  oauthError.value = '';
  try {
    await invoke<number>('complete_codex_oauth_login', {
      loginId: oauthLoginId.value,
      callbackUrl: auto ? null : oauthCallbackUrl.value.trim(),
    });
    await loadAccounts();
    await closeOauthDialog();
    showMessage('OAuth 账号已添加');
  } catch (e) {
    oauthError.value = String(e).replace(/^Error:\s*/, '');
  } finally {
    oauthAdding.value = false;
  }
}

async function loadStoragePaths() {
  try {
    storagePaths.value = await invoke<StoragePaths>('get_storage_paths');
  } catch (e) {
    showMessage(`读取数据位置失败: ${e}`, 'error');
  }
}

async function loadCodexAppSpeed() {
  try {
    const config = await invoke<CodexAppSpeedConfig>('get_codex_app_speed_config');
    codexAppSpeed.value = config.speed;
  } catch (e) {
    showMessage(`读取 Codex 速度失败: ${e}`, 'error');
  }
}

async function loadCodexUsage(showSuccess = false) {
  codexUsageLoading.value = true;
  try {
    codexUsage.value = await invoke<CodexUsageSummary>('get_codex_usage_summary');
    if (showSuccess) showMessage('Codex 使用统计已刷新');
  } catch (e) {
    showMessage(`读取 Codex 使用统计失败: ${e}`, 'error');
  } finally {
    codexUsageLoading.value = false;
  }
}

async function changeCodexAppSpeed(speed: CodexAppSpeed) {
  if (codexAppSpeed.value === speed || codexSpeedSaving.value) return;
  codexSpeedSaving.value = true;
  try {
    const config = await invoke<CodexAppSpeedConfig>('set_codex_app_speed', { speed });
    codexAppSpeed.value = config.speed;
    showMessage(config.speed === 'fast' ? '已切换为 Fast 模式' : '已切换为标准模式');
  } catch (e) {
    showMessage(`切换速度失败: ${e}`, 'error');
  } finally {
    codexSpeedSaving.value = false;
  }
}

async function copyText(text: string, label: string) {
  try {
    await navigator.clipboard.writeText(text);
    showMessage(`${label}已复制`);
  } catch {
    const el = document.createElement('textarea');
    el.value = text;
    el.style.position = 'fixed';
    el.style.opacity = '0';
    document.body.appendChild(el);
    el.select();
    const copied = document.execCommand('copy');
    document.body.removeChild(el);
    showMessage(copied ? `${label}已复制` : '复制失败，请手动选中路径复制', copied ? 'success' : 'error');
  }
}

async function openStorageFolder() {
  showToolsMenu.value = false;
  try {
    await invoke('open_storage_folder');
    showMessage('已打开账号库目录');
  } catch (e) {
    showMessage(`打开账号库目录失败: ${e}`, 'error');
  }
}

async function openAuthFolder() {
  showToolsMenu.value = false;
  try {
    await invoke('open_codex_auth_folder');
    showMessage('已打开当前账号目录');
  } catch (e) {
    showMessage(`打开当前账号目录失败: ${e}`, 'error');
  }
}

async function handleSave(data: { name: string; activationDate: string; jsonInfo: string }) {
  savingAccount.value = true;
  try {
    if (editingAccount.value) {
      await updateAccount(editingAccount.value.id, data.name, data.activationDate, data.jsonInfo);
      showMessage('账号已更新');
    } else {
      await addAccount(data.name, data.activationDate, data.jsonInfo);
      showMessage('账号已添加');
    }
    showDialog.value = false;
    editingAccount.value = null;
  } catch (e) {
    showMessage(`保存失败: ${e}`, 'error');
  } finally {
    savingAccount.value = false;
  }
}

async function repairProjectVisibility() {
  showToolsMenu.value = false;
  try {
    const status = await invoke<CodexProjectVisibilityStatus>('get_codex_project_visibility_status', {
      projectPath: null,
    });
    const projectPath = prompt(
      '请输入要修复为 trusted 的 Codex 项目路径。此操作只会新增或修正该项目的 trust_level，不会修改 provider、MCP、模型或内存配置。',
      status.project_path,
    );
    if (!projectPath?.trim()) return;

    const repaired = await invoke<CodexProjectVisibilityStatus>('repair_codex_project_visibility', {
      projectPath: projectPath.trim(),
    });
    showMessage(
      repaired.changed
        ? `项目可见性已修复: ${repaired.project_path}`
        : `项目已经是 trusted: ${repaired.project_path}`,
    );
  } catch (e) {
    showMessage(`修复项目可见性失败: ${e}`, 'error');
  }
}

async function loadMigrationStatus() {
  try {
    migrationStatus.value = await invoke<MigrationStatus>('get_migration_status');
  } catch (e) {
    showMessage(`读取迁移状态失败: ${e}`, 'error');
  }
}

function backupFileName(): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  const d = new Date();
  return [
    'codex-accounts-backup',
    d.getFullYear(),
    pad(d.getMonth() + 1),
    pad(d.getDate()),
    pad(d.getHours()),
    pad(d.getMinutes()),
  ].join('-') + '.json';
}

function downloadTextFile(fileName: string, text: string) {
  const blob = new Blob([text], { type: 'application/json;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

async function exportBackup(accountIds?: number[]) {
  showToolsMenu.value = false;
  const password = prompt('请输入备份密码（至少 8 位）。导入时需要同一个密码。');
  if (!password) return;
  try {
    const backupText = await invoke<string>('export_encrypted_backup', {
      password,
      accountIds: accountIds && accountIds.length > 0 ? accountIds : null,
    });
    downloadTextFile(backupFileName(), backupText);
    showMessage(`加密备份已导出（${accountIds?.length || accounts.value.length} 个账号）`);
  } catch (e) {
    showMessage(`导出备份失败: ${e}`, 'error');
  }
}

async function exportFilteredBackup() {
  const ids = filteredAccounts.value.filter(account => account.has_json_info).map(account => account.id);
  if (ids.length === 0) {
    showMessage('当前筛选结果里没有可导出的账号', 'error');
    return;
  }
  await exportBackup(ids);
}

function openImportBackup() {
  showToolsMenu.value = false;
  importInput.value?.click();
}

async function importBackup(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;

  const password = prompt('请输入备份密码');
  if (!password) return;

  try {
    const backupText = await file.text();
    const preview = await invoke<BackupPreview>('preview_encrypted_backup', { backupText, password });
    importBackupText.value = backupText;
    importPassword.value = password;
    importPreview.value = preview;
    importStrategy.value = preview.duplicate_accounts > 0 ? 'skip_duplicates' : 'add';
    showImportPreviewDialog.value = true;
  } catch (err) {
    showMessage(`读取备份失败: ${err}`, 'error');
  }
}

async function confirmImportBackup() {
  if (!importPreview.value) return;
  importingBackup.value = true;
  try {
    const result = await invoke<ImportBackupResult>('import_encrypted_backup', {
      backupText: importBackupText.value,
      password: importPassword.value,
      strategy: importStrategy.value,
    });
    await loadAccounts();
    await loadMigrationStatus();
    closeImportPreviewDialog();
    showMessage(`导入完成：新增 ${result.imported}，更新 ${result.updated}，跳过 ${result.skipped}`);
  } catch (err) {
    showMessage(`导入备份失败: ${err}`, 'error');
  } finally {
    importingBackup.value = false;
  }
}

function closeImportPreviewDialog() {
  showImportPreviewDialog.value = false;
  importBackupText.value = '';
  importPassword.value = '';
  importPreview.value = null;
  importStrategy.value = 'add';
  importingBackup.value = false;
}

async function migrateOldAccounts() {
  showToolsMenu.value = false;
  const pending = migrationStatus.value?.pending_plaintext_accounts ?? 0;
  if (pending <= 0) return;
  if (!confirm(`检测到 ${pending} 个旧账号仍在数据库明文保存。现在迁移到系统凭据库吗？`)) return;

  migratingAccounts.value = true;
  try {
    const status = await invoke<MigrationStatus>('migrate_plaintext_accounts');
    migrationStatus.value = status;
    await loadAccounts();
    showMessage(status.pending_plaintext_accounts === 0 ? '旧账号已迁移到系统凭据库' : `仍有 ${status.pending_plaintext_accounts} 个账号待迁移`, status.pending_plaintext_accounts === 0 ? 'success' : 'error');
  } catch (e) {
    showMessage(`迁移旧账号失败: ${e}`, 'error');
  } finally {
    migratingAccounts.value = false;
  }
}

async function handleRun(id: number) {
  try {
    await switchAccount(id, restartCodexOnSwitch.value);
    showMessage(restartCodexOnSwitch.value ? '账号已切换，Codex 已重启' : '账号已切换，未重启 Codex');
  }
  catch (e) { showMessage(`切换失败: ${e}`, 'error'); }
}

async function handleDelete(id: number) {
  const account = accounts.value.find(a => a.id === id);
  if (!account || !confirm(`确定要删除「${account.name}」吗？`)) return;
  try { await deleteAccount(id); showMessage('账号已删除'); }
  catch (e) { showMessage(`删除失败: ${e}`, 'error'); }
}

async function handleRefresh(id: number) {
  try { await refreshQuota(id); showMessage('额度已刷新'); }
  catch (e) { showMessage(`刷新失败: ${e}`, 'error'); }
}

async function handleRefreshProfile(id: number) {
  try { await refreshProfile(id); showMessage('账号资料已刷新'); }
  catch (e) { showMessage(`资料刷新失败: ${e}`, 'error'); }
}

function resetAccountFilters() {
  searchQuery.value = '';
  accountFilterStatus.value = 'all';
  accountSortBy.value = 'created_at';
  accountSortDirection.value = 'desc';
}

function toggleSortDirection() {
  accountSortDirection.value = accountSortDirection.value === 'desc' ? 'asc' : 'desc';
}

async function refreshFilteredAccounts() {
  const ids = filteredRefreshableIds.value;
  if (ids.length === 0) {
    showMessage('当前筛选结果里没有可刷新的账号', 'error');
    return;
  }

  batchRefreshing.value = true;
  batchRefreshFailures.value = [];
  batchRefreshProgress.value = { done: 0, total: ids.length, currentName: '' };
  try {
    const result = await refreshQuotaBatch(ids, (progress) => {
      batchRefreshProgress.value = progress;
    });
    batchRefreshFailures.value = result.failures;
    showMessage(
      `批量刷新完成：成功 ${result.success}，失败 ${result.failed}，跳过 ${result.skipped}`,
      result.failed > 0 ? 'error' : 'success',
    );
  } catch (e) {
    showMessage(`批量刷新失败: ${e}`, 'error');
  } finally {
    batchRefreshing.value = false;
    batchRefreshProgress.value = null;
  }
}

async function runAccountHealthCheck(accountId: number) {
  healthCheckingId.value = accountId;
  try {
    const report = await invoke<AccountHealthReport>('check_account_health', { id: accountId });
    healthReports.value = {
      ...healthReports.value,
      [accountId]: report,
    };
    await loadAccounts();
    showMessage(
      report.summary_status === 'ok'
        ? '账号健康检查通过'
        : report.summary_status === 'warn'
          ? '账号健康检查完成，有警告'
          : '账号健康检查发现问题',
      report.summary_status === 'error' ? 'error' : 'success',
    );
  } catch (e) {
    showMessage(`账号健康检查失败: ${e}`, 'error');
  } finally {
    healthCheckingId.value = null;
  }
}

async function handleIntervalChange(e: Event) {
  await setRefreshInterval(Number((e.target as HTMLSelectElement).value));
}

async function handleRestartToggle(e: Event) {
  await setRestartCodexOnSwitch((e.target as HTMLInputElement).checked);
}

function toggleStorageDetails() {
  showToolsMenu.value = false;
  showStorageDetails.value = !showStorageDetails.value;
}

function formatLogDetails(details: string): string {
  if (!details.trim()) return '';
  try {
    return JSON.stringify(JSON.parse(details), null, 2);
  } catch {
    return details;
  }
}

function logLevelLabel(level: string): string {
  if (level === 'error') return '错误';
  if (level === 'warn') return '警告';
  return '信息';
}

function logActionLabel(action: string): string {
  if (action === 'refresh_quota') return '刷新额度';
  if (action === 'switch_account') return '切换账号';
  if (action === 'oauth_login') return 'OAuth 登录';
  return action;
}

function logAccountLabel(log: OperationLog): string {
  const name = log.account_name || (log.account_id ? `#${log.account_id}` : '系统');
  return log.account_identifier ? `${name} · ${log.account_identifier}` : name;
}

function openFailureDetail(failure: BatchRefreshFailure) {
  const account = accounts.value.find(item => item.id === failure.id);
  if (account) {
    openDetailDrawer(account);
  } else {
    detailAccountId.value = failure.id;
  }
}

async function openFailureLogs(failure: BatchRefreshFailure) {
  await openOperationLogs(failure.id);
}

function failureSummaryText(): string {
  return batchRefreshFailures.value
    .map(failure => `#${failure.id} ${failure.name}\n${failure.error}`)
    .join('\n\n');
}

async function copyBatchRefreshFailures() {
  if (batchRefreshFailures.value.length === 0) {
    showMessage('没有可复制的失败摘要', 'error');
    return;
  }
  await copyText(failureSummaryText(), '失败摘要');
}

function logCopyText(log: OperationLog): string {
  const details = formatLogDetails(log.details);
  return [
    `[${logLevelLabel(log.level)}] ${logActionLabel(log.action)} · ${log.stage}`,
    `时间: ${formatCheckedTime(log.created_at)}`,
    `账号: ${logAccountLabel(log)}`,
    `消息: ${log.message}`,
    details ? `详情:\n${details}` : '',
  ].filter(Boolean).join('\n');
}

async function copyOperationLog(log: OperationLog) {
  await copyText(logCopyText(log), '日志详情');
}

async function copyVisibleOperationLogs() {
  if (visibleOperationLogs.value.length === 0) {
    showMessage('没有可复制的日志', 'error');
    return;
  }
  await copyText(visibleOperationLogs.value.map(logCopyText).join('\n\n---\n\n'), '当前日志');
}

function openLogAccountDetail(log: OperationLog) {
  if (!log.account_id) return;
  detailAccountId.value = log.account_id;
  showOperationLogs.value = false;
}

async function loadOperationLogs() {
  operationLogsLoading.value = true;
  try {
    operationLogs.value = await invoke<OperationLog[]>('list_operation_logs', {
      accountId: operationLogAccountId.value,
      limit: 200,
    });
  } catch (e) {
    showMessage(`读取日志失败: ${e}`, 'error');
  } finally {
    operationLogsLoading.value = false;
  }
}

async function openOperationLogs(accountId: number | null = null) {
  showToolsMenu.value = false;
  operationLogAccountId.value = accountId;
  showOperationLogs.value = true;
  await loadOperationLogs();
}

function closeOperationLogs() {
  showOperationLogs.value = false;
}

async function changeOperationLogAccount(e: Event) {
  const value = (e.target as HTMLSelectElement).value;
  operationLogAccountId.value = value ? Number(value) : null;
  await loadOperationLogs();
}

async function clearOperationLogs() {
  if (!confirm('确定清空所有操作日志吗？')) return;
  try {
    await invoke('clear_operation_logs');
    await loadOperationLogs();
    showMessage('操作日志已清空');
  } catch (e) {
    showMessage(`清空日志失败: ${e}`, 'error');
  }
}

function openDetailDrawer(account: Account) {
  detailAccountId.value = account.id;
}

function closeDetailDrawer() {
  detailAccountId.value = null;
}

function editDetailAccount(account: Account) {
  closeDetailDrawer();
  openEditDialog(account);
}
</script>

<template>
  <div class="app" @click="showToolsMenu = false">
    <!-- Header -->
    <header class="header">
      <div class="header-bg"></div>
      <div class="header-inner">
        <div class="header-left">
          <div class="logo">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
              <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
            </svg>
          </div>
          <div class="header-text">
            <h1>Codex Manager</h1>
            <span class="header-count">{{ accounts.length }} 个账号</span>
          </div>
        </div>
        <div class="header-right">
          <div class="interval-wrap">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
            </svg>
            <select :value="refreshInterval" @change="handleIntervalChange">
              <option v-for="opt in intervalOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
            </select>
          </div>
          <label class="restart-toggle" title="开启：写入 auth.json 并重启 Codex，当前运行环境立即生效。关闭：仅写入 auth.json，下次启动生效。">
            <input type="checkbox" :checked="restartCodexOnSwitch" @change="handleRestartToggle" />
            <span>立即生效（重启 Codex）</span>
          </label>
          <button class="btn-add btn-oauth" :disabled="oauthAdding" @click="addAccountWithOAuth">
            <svg v-if="oauthAdding" class="spin" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
            </svg>
            <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M15 3h4a2 2 0 0 1 2 2v4"/>
              <path d="M10 14 21 3"/>
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
            </svg>
            {{ oauthAdding ? '授权中...' : 'OAuth 登录' }}
          </button>
          <button class="btn-add" @click="openAddDialog">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            添加账号
          </button>
        </div>
      </div>
    </header>

    <!-- Toast -->
    <Transition name="toast">
      <div v-if="message" :class="['toast', `toast-${messageType}`]">
        <svg v-if="messageType === 'success'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
        </svg>
        <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
          <circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        {{ message }}
      </div>
    </Transition>

    <!-- Content -->
    <main class="content">
      <section v-if="storagePaths" class="storage-panel">
        <div class="storage-bar">
          <div class="storage-summary">
            <div class="storage-icon">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <ellipse cx="12" cy="5" rx="9" ry="3"/>
                <path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/>
                <path d="M3 12c0 1.66 4.03 3 9 3s9-1.34 9-3"/>
              </svg>
            </div>
            <div>
              <p class="storage-title">账号数据保存在本机</p>
              <span>换电脑请使用加密备份，账号密钥会写入系统凭据库</span>
            </div>
          </div>

          <div class="storage-actions" @click.stop>
            <button
              v-if="migrationStatus && migrationStatus.pending_plaintext_accounts > 0"
              class="btn-storage-warning"
              :disabled="migratingAccounts"
              @click="migrateOldAccounts"
            >
              {{ migratingAccounts ? '迁移中...' : `迁移旧账号 (${migrationStatus.pending_plaintext_accounts})` }}
            </button>
            <input
              ref="importInput"
              class="backup-input"
              type="file"
              accept="application/json,.json"
              @change="importBackup"
            />
            <div class="tools-menu-wrap">
              <button class="btn-storage-primary btn-tools" @click="showToolsMenu = !showToolsMenu">
                <span>工具</span>
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="6 9 12 15 18 9"/>
                </svg>
              </button>
              <div v-if="showToolsMenu" class="tools-menu">
                <button @click="() => exportBackup()">
                  <span>导出全部账号</span>
                </button>
                <button @click="exportFilteredBackup">
                  <span>导出当前筛选结果</span>
                </button>
                <button @click="openImportBackup">
                  <span>导入备份</span>
                </button>
                <button @click="openStorageFolder">
                  <span>打开账号库目录</span>
                </button>
                <button @click="openAuthFolder">
                  <span>打开当前账号目录</span>
                </button>
                <button @click="repairProjectVisibility">
                  <span>修复项目可见性</span>
                </button>
                <button @click="() => openOperationLogs()">
                  <span>查看操作日志</span>
                </button>
                <button @click="toggleStorageDetails">
                  <span>{{ showStorageDetails ? '收起存储详情' : '查看存储详情' }}</span>
                </button>
              </div>
            </div>
          </div>
        </div>

        <div v-if="showStorageDetails" class="storage-details">
          <div class="path-row">
            <span class="path-label">账号库</span>
            <code>{{ storagePaths.database_path }}</code>
            <button @click="copyText(storagePaths.database_path, '账号库路径')">复制</button>
          </div>
          <div class="path-row path-row-muted">
            <span class="path-label">当前生效</span>
            <code>{{ storagePaths.auth_json_path }}</code>
            <button @click="copyText(storagePaths.auth_json_path, 'auth.json 路径')">复制</button>
          </div>
          <p class="storage-note">
            `codex_accounts.db` 只保存账号元数据；完整 auth.json 保存在系统凭据库中。换电脑请使用加密备份导出和导入。
            <span v-if="migrationStatus && migrationStatus.pending_plaintext_accounts > 0">
              当前检测到 {{ migrationStatus.pending_plaintext_accounts }} 个旧账号还未迁移。
            </span>
          </p>
        </div>
      </section>

      <section class="overview-panel">
        <div class="stat-grid">
          <div class="stat-item">
            <span class="stat-label">当前账号</span>
            <strong :title="accountStats.currentName">{{ accountStats.currentName }}</strong>
            <small v-if="accountStats.currentPrimaryRemaining !== null">
              {{ accountStats.currentPrimaryLabel }} {{ accountStats.currentPrimaryRemaining }}%
              <template v-if="accountStats.currentSecondaryVisible">
                · {{ accountStats.currentSecondaryLabel }} {{ accountStats.currentSecondaryRemaining }}%
              </template>
            </small>
            <small v-else>尚未写入当前 auth.json</small>
          </div>
          <div class="stat-item">
            <span class="stat-label">账号数量</span>
            <strong>{{ accountStats.total }}</strong>
            <small>可用 {{ accountStats.usable }} 个</small>
          </div>
          <div class="stat-item">
            <span class="stat-label">总剩余额度</span>
            <strong>{{ accountStats.totalPrimaryLabel }} {{ accountStats.totalPrimaryRemaining }}%</strong>
            <small v-if="accountStats.hasSecondaryQuota">{{ accountStats.totalSecondaryLabel }} {{ accountStats.totalSecondaryRemaining }}%</small>
            <small v-else>无第二额度窗口</small>
          </div>
          <div class="stat-item stat-warn">
            <span class="stat-label">状态问题</span>
            <strong>{{ accountStats.issueCount }}</strong>
            <small
              v-if="accountStats.issueCount > 0"
              class="stat-error-reasons"
              :title="accountStats.errorReasons.map(item => `${item.label} ${item.count} 个`).join(' · ')"
            >
              不可用 {{ accountStats.unavailable }} · 额度受限 {{ accountStats.quotaLimited }} · 查询失败 {{ accountStats.queryFailed }}
            </small>
            <small v-else>没有问题</small>
          </div>
        </div>

        <div class="overview-controls">
          <div class="segmented" title="Codex 桌面速度">
            <button
              :class="{ active: codexAppSpeed === 'standard' }"
              :disabled="codexSpeedSaving"
              @click="changeCodexAppSpeed('standard')"
            >
              标准
            </button>
            <button
              :class="{ active: codexAppSpeed === 'fast' }"
              :disabled="codexSpeedSaving"
              @click="changeCodexAppSpeed('fast')"
            >
              Fast
            </button>
          </div>
          <div class="segmented view-segmented" title="账号展示方式">
            <button :class="{ active: accountViewMode === 'cards' }" @click="setAccountViewMode('cards')">
              卡片
            </button>
            <button :class="{ active: accountViewMode === 'compact' }" @click="setAccountViewMode('compact')">
              紧凑
            </button>
            <button :class="{ active: accountViewMode === 'table' }" @click="setAccountViewMode('table')">
              表格
            </button>
          </div>
        </div>
      </section>

      <section class="usage-panel">
        <div class="usage-head">
          <div>
            <span class="section-kicker">Codex 使用统计</span>
            <h2>Tokens 与估算成本</h2>
          </div>
          <button class="usage-refresh" :disabled="codexUsageLoading" @click="loadCodexUsage(true)">
            <svg v-if="codexUsageLoading" class="spin" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
            </svg>
            <svg v-else width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="23 4 23 10 17 10"/>
              <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
            {{ codexUsageLoading ? '读取中' : '刷新' }}
          </button>
        </div>

        <div v-if="codexUsage" class="usage-layout">
          <div class="usage-primary">
            <div class="usage-total">
              <span>今日 Tokens</span>
              <strong>{{ formatExactNumber(codexUsage.today.total_tokens) }}</strong>
              <small>
                全部 {{ formatTokenAmount(codexUsage.total.total_tokens) }}
                · 成功率 {{ usageSuccessRate(codexUsage.today) }}
              </small>
            </div>
            <div class="usage-mini-grid">
              <div class="usage-mini-card">
                <span>今日请求</span>
                <strong>{{ formatExactNumber(codexUsage.today.request_count) }}</strong>
                <small>成功 {{ codexUsage.today.success_count }} · 失败 {{ codexUsage.today.error_count }}</small>
              </div>
              <div class="usage-mini-card">
                <span>Codex Credits</span>
                <strong>{{ formatCredits(codexUsage.today.codex_credits) }}</strong>
                <small>全部 {{ formatCredits(codexUsage.total.codex_credits) }}</small>
              </div>
              <div class="usage-mini-card">
                <span>API 等价成本</span>
                <strong>{{ formatUsd(codexUsage.today.api_cost_usd) }}</strong>
                <small>全部 {{ formatUsd(codexUsage.total.api_cost_usd) }}</small>
              </div>
            </div>
          </div>

          <div class="usage-breakdown">
            <div class="usage-breakdown-row">
              <span>Input</span>
              <strong>{{ formatTokenAmount(codexUsage.today.input_tokens) }}</strong>
            </div>
            <div class="usage-breakdown-row">
              <span>Cached</span>
              <strong>{{ formatTokenAmount(codexUsage.today.cached_input_tokens) }}</strong>
            </div>
            <div class="usage-breakdown-row">
              <span>Output</span>
              <strong>{{ formatTokenAmount(codexUsage.today.output_tokens) }}</strong>
            </div>
            <div class="usage-breakdown-row">
              <span>Reasoning</span>
              <strong>{{ formatTokenAmount(codexUsage.today.reasoning_output_tokens) }}</strong>
            </div>
          </div>

          <div class="usage-models">
            <div class="usage-subhead">模型</div>
            <div v-if="topCodexUsageModels.length === 0" class="usage-empty">暂无 token 记录</div>
            <div v-for="item in topCodexUsageModels" :key="item.model" class="usage-model-row">
              <span :title="item.model">{{ item.model }}</span>
              <strong>{{ formatTokenAmount(item.usage.total_tokens) }}</strong>
              <small>{{ formatUsd(item.usage.api_cost_usd) }}</small>
            </div>
          </div>
        </div>

        <div v-else class="usage-empty usage-empty-large">
          {{ codexUsageLoading ? '读取 Codex 本地日志中...' : '暂无 Codex 使用统计' }}
        </div>

        <div v-if="codexUsage?.note" class="usage-note">{{ codexUsage.note }}</div>
      </section>

      <section class="account-toolbar">
        <div class="toolbar-main">
          <div class="account-search">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="11" cy="11" r="8"/>
              <path d="m21 21-4.3-4.3"/>
            </svg>
            <input
              v-model="searchQuery"
              type="search"
              placeholder="搜索账号名、account_id、记录 ID"
            />
          </div>

          <div class="toolbar-selects">
            <label>
              <span>状态</span>
              <select v-model="accountFilterStatus">
                <option v-for="option in filterOptions" :key="option.value" :value="option.value">
                  {{ option.label }}
                </option>
              </select>
            </label>
            <label>
              <span>排序</span>
              <select v-model="accountSortBy">
                <option v-for="option in sortOptions" :key="option.value" :value="option.value">
                  {{ option.label }}
                </option>
              </select>
            </label>
            <button class="btn-toolbar-icon" :title="accountSortDirection === 'desc' ? '降序' : '升序'" @click="toggleSortDirection">
              <svg v-if="accountSortDirection === 'desc'" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                <path d="M12 5v14"/>
                <path d="m19 12-7 7-7-7"/>
              </svg>
              <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                <path d="M12 19V5"/>
                <path d="m5 12 7-7 7 7"/>
              </svg>
            </button>
          </div>
        </div>

        <div class="toolbar-actions">
          <span class="toolbar-result">
            {{ filteredAccounts.length }} / {{ accounts.length }} 个账号
          </span>
          <button
            class="btn-toolbar"
            :disabled="batchRefreshing || filteredRefreshableIds.length === 0"
            @click="refreshFilteredAccounts"
          >
            <svg v-if="batchRefreshing" class="spin" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
            </svg>
            <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="23 4 23 10 17 10"/>
              <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
            {{
              batchRefreshing && batchRefreshProgress
                ? `刷新中 ${batchRefreshProgress.done}/${batchRefreshProgress.total}`
                : `刷新筛选结果 (${filteredRefreshableIds.length})`
            }}
          </button>
          <button v-if="hasActiveAccountFilters" class="btn-toolbar btn-toolbar-ghost" @click="resetAccountFilters">
            清空
          </button>
        </div>
      </section>

      <section v-if="batchRefreshFailures.length > 0" class="batch-failures">
        <div class="batch-failures-head">
          <strong>批量刷新失败 {{ batchRefreshFailures.length }} 个</strong>
          <div class="batch-failures-actions">
            <button @click="copyBatchRefreshFailures">复制摘要</button>
            <button @click="batchRefreshFailures = []">清除</button>
          </div>
        </div>
        <div class="batch-failure-list">
          <div v-for="failure in batchRefreshFailures" :key="failure.id" class="batch-failure-item">
            <span :title="failure.name">{{ failure.name }}</span>
            <code :title="failure.error">{{ failure.error }}</code>
            <div class="batch-failure-actions">
              <button @click="openFailureDetail(failure)">详情</button>
              <button @click="openFailureLogs(failure)">日志</button>
            </div>
          </div>
        </div>
      </section>

      <AccountList
        :accounts="filteredAccounts"
        :loading="loading"
        :switching-id="switchingId"
        :current-account-record-id="currentAccountRecordId"
        :view-mode="accountViewMode"
        :empty-title="accounts.length === 0 ? '还没有账号' : '没有匹配的账号'"
        :empty-description="accounts.length === 0 ? '使用 OAuth 登录或导入 auth.json 后，账号会出现在这里' : '调整搜索、筛选或排序条件后再查看'"
        @run="handleRun"
        @edit="openEditDialog"
        @delete="handleDelete"
        @refresh="handleRefresh"
        @profile="handleRefreshProfile"
        @detail="openDetailDrawer"
      />
    </main>

    <Transition name="drawer">
      <div v-if="detailAccount" class="detail-backdrop" @click.self="closeDetailDrawer">
        <aside class="detail-drawer">
          <div class="detail-header">
            <div>
              <span class="detail-kicker">账号详情</span>
              <h2 :title="detailAccount.name">{{ detailAccount.name }}</h2>
            </div>
            <button class="detail-close" @click="closeDetailDrawer" aria-label="关闭详情">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>

          <div class="detail-body">
            <section class="detail-section">
              <div class="detail-section-head">
                <div class="detail-section-title">健康状态</div>
                <button
                  class="detail-mini-action"
                  :disabled="healthCheckingId === detailAccount.id"
                  @click="runAccountHealthCheck(detailAccount.id)"
                >
                  <svg v-if="healthCheckingId === detailAccount.id" class="spin" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                    <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
                  </svg>
                  {{ healthCheckingId === detailAccount.id ? '检查中' : '深度检查' }}
                </button>
              </div>
              <div class="health-grid">
                <div v-for="item in detailHealthItems" :key="item.label" :class="['health-item', { ok: item.ok }]">
                  <span>{{ item.label }}</span>
                  <strong>{{ item.text }}</strong>
                </div>
              </div>
              <div v-if="detailHealthReport" class="health-report">
                <div :class="['health-report-summary', detailHealthReport.summary_status]">
                  <strong>
                    {{
                      detailHealthReport.summary_status === 'ok'
                        ? '深度检查通过'
                        : detailHealthReport.summary_status === 'warn'
                          ? '深度检查有警告'
                          : '深度检查发现问题'
                    }}
                  </strong>
                  <span>{{ formatCheckedTime(detailHealthReport.checked_at) }}</span>
                </div>
                <div class="health-report-list">
                  <div v-for="item in detailHealthReport.items" :key="item.key" :class="['health-report-row', item.status]">
                    <span>{{ item.label }}</span>
                    <strong>{{ item.message }}</strong>
                  </div>
                </div>
              </div>
            </section>

            <section class="detail-section">
              <div class="detail-section-title">额度</div>
              <div class="detail-quota-grid">
                <div v-if="detailAccount.primary_window_present" class="detail-quota-card">
                  <span>{{ quotaWindowLabel(detailAccount.primary_window_minutes, 'primary') }}剩余</span>
                  <strong>{{ quotaRemaining(detailAccount.primary_used_percent) }}%</strong>
                  <small>重置 {{ formatResetTime(detailAccount.primary_reset_at) }}</small>
                </div>
                <div v-if="detailAccount.secondary_window_present" class="detail-quota-card">
                  <span>{{ quotaWindowLabel(detailAccount.secondary_window_minutes, 'secondary') }}剩余</span>
                  <strong>{{ quotaRemaining(detailAccount.secondary_used_percent) }}%</strong>
                  <small>重置 {{ formatResetTime(detailAccount.secondary_reset_at) }}</small>
                </div>
              </div>
            </section>

            <section class="detail-section">
              <div class="detail-section-title">基础信息</div>
              <div class="detail-rows">
                <div>
                  <span>记录 ID</span>
                  <code>#{{ detailAccount.id }}</code>
                </div>
                <div>
                  <span>Account ID</span>
                  <code>{{ detailAccount.account_id || '未识别' }}</code>
                </div>
                <div>
                  <span>套餐</span>
                  <code>{{ detailAccount.plan_type || 'unknown' }}</code>
                </div>
                <div>
                  <span>开通日期</span>
                  <code>{{ detailAccount.activation_date || '-' }}</code>
                </div>
                <div>
                  <span>上次检查</span>
                  <code>{{ formatCheckedTime(detailAccount.last_quota_checked_at) }}</code>
                </div>
                <div>
                  <span>创建时间</span>
                  <code>{{ detailAccount.created_at || '-' }}</code>
                </div>
                <div>
                  <span>更新时间</span>
                  <code>{{ detailAccount.updated_at || '-' }}</code>
                </div>
              </div>
            </section>

            <section v-if="detailAccount.last_quota_error" class="detail-section">
              <div class="detail-section-title">最近错误</div>
              <pre class="detail-error">{{ detailAccount.last_quota_error }}</pre>
            </section>
          </div>

          <div class="detail-footer">
            <button
              class="btn-detail-primary"
              :disabled="switchingId === detailAccount.id || !detailAccount.has_json_info"
              @click="handleRun(detailAccount.id)"
            >
              {{ switchingId === detailAccount.id ? '切换中' : '运行此账号' }}
            </button>
            <button class="btn-detail-secondary" @click="handleRefresh(detailAccount.id)">
              刷新额度
            </button>
            <button class="btn-detail-secondary" @click="openOperationLogs(detailAccount.id)">
              日志
            </button>
            <button class="btn-detail-secondary" @click="editDetailAccount(detailAccount)">
              编辑
            </button>
          </div>
        </aside>
      </div>
    </Transition>

    <Transition name="drawer">
      <div v-if="showOperationLogs" class="detail-backdrop" @click.self="closeOperationLogs">
        <aside class="log-drawer">
          <div class="detail-header">
            <div>
              <span class="detail-kicker">诊断</span>
              <h2>操作日志</h2>
            </div>
            <button class="detail-close" @click="closeOperationLogs" aria-label="关闭日志">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>

          <div class="log-toolbar">
            <select :value="operationLogAccountId ?? ''" @change="changeOperationLogAccount">
              <option value="">全部账号</option>
              <option v-for="account in accounts" :key="account.id" :value="account.id">
                {{ account.name }}
              </option>
            </select>
            <select v-model="operationLogActionFilter" class="log-action-select">
              <option value="all">全部操作</option>
              <option v-for="action in operationLogActionOptions" :key="action" :value="action">
                {{ logActionLabel(action) }}
              </option>
            </select>
            <button :disabled="operationLogsLoading" @click="loadOperationLogs">
              {{ operationLogsLoading ? '刷新中' : '刷新' }}
            </button>
            <button @click="copyVisibleOperationLogs">复制当前</button>
            <label class="log-toggle">
              <input v-model="operationLogErrorsOnly" type="checkbox" />
              <span>只看错误</span>
            </label>
            <button class="log-clear" @click="clearOperationLogs">清空</button>
          </div>

          <div class="log-body">
            <div v-if="operationLogsLoading" class="log-empty">读取日志中...</div>
            <div v-else-if="visibleOperationLogs.length === 0" class="log-empty">
              {{ operationLogs.length === 0 ? '还没有操作日志' : '当前筛选没有日志' }}
            </div>
            <template v-else>
              <article
                v-for="log in visibleOperationLogs"
                :key="log.id"
                :class="['log-item', `log-${log.level}`]"
              >
                <div class="log-item-head">
                  <span class="log-level">{{ logLevelLabel(log.level) }}</span>
                  <strong>{{ logActionLabel(log.action) }} · {{ log.stage }}</strong>
                  <time>{{ formatCheckedTime(log.created_at) }}</time>
                  <button class="log-copy" @click="copyOperationLog(log)">复制</button>
                </div>
                <p>{{ log.message }}</p>
                <div class="log-account">
                  <span>{{ logAccountLabel(log) }}</span>
                  <button v-if="log.account_id" @click="openLogAccountDetail(log)">详情</button>
                </div>
                <pre v-if="log.details" class="log-details">{{ formatLogDetails(log.details) }}</pre>
              </article>
            </template>
          </div>
        </aside>
      </div>
    </Transition>

    <!-- Dialog -->
    <Transition name="dialog">
      <AccountDialog
        v-if="showDialog"
        :account="editingAccount"
        :saving="savingAccount"
        @save="handleSave"
        @close="closeDialog"
      />
    </Transition>

    <Transition name="dialog">
      <div v-if="showImportPreviewDialog && importPreview" class="import-backdrop" @click.self="closeImportPreviewDialog">
        <div class="import-dialog">
          <div class="import-header">
            <div>
              <h2>导入备份预览</h2>
              <p>确认导入策略后再写入账号库，重复判断基于 account_id。</p>
            </div>
            <button class="import-close" @click="closeImportPreviewDialog" aria-label="关闭">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>

          <div class="import-body">
            <div class="import-stats">
              <div>
                <span>备份版本</span>
                <strong>{{ importPreview.version }}</strong>
              </div>
              <div>
                <span>账号总数</span>
                <strong>{{ importPreview.total_accounts }}</strong>
              </div>
              <div>
                <span>新增账号</span>
                <strong>{{ importPreview.new_accounts }}</strong>
              </div>
              <div>
                <span>重复账号</span>
                <strong>{{ importPreview.duplicate_accounts }}</strong>
              </div>
            </div>

            <div class="import-section">
              <div class="import-section-title">导入策略</div>
              <label class="import-strategy">
                <input v-model="importStrategy" type="radio" value="add" />
                <span>
                  <strong>全部新增</strong>
                  <small>即使 account_id 重复，也作为新记录导入。</small>
                </span>
              </label>
              <label class="import-strategy">
                <input v-model="importStrategy" type="radio" value="skip_duplicates" />
                <span>
                  <strong>跳过重复</strong>
                  <small>重复 account_id 不导入，只新增缺失账号。</small>
                </span>
              </label>
              <label class="import-strategy">
                <input v-model="importStrategy" type="radio" value="merge_by_account_id" />
                <span>
                  <strong>合并更新</strong>
                  <small>重复 account_id 会更新现有记录和凭据。</small>
                </span>
              </label>
            </div>

            <div v-if="importPreview.account_names.length > 0" class="import-section">
              <div class="import-section-title">备份内账号</div>
              <div class="import-name-list">
                <span v-for="name in importPreview.account_names" :key="name">{{ name }}</span>
                <small v-if="importPreview.total_accounts > importPreview.account_names.length">
                  还有 {{ importPreview.total_accounts - importPreview.account_names.length }} 个账号
                </small>
              </div>
            </div>
          </div>

          <div class="import-footer">
            <button class="btn-detail-secondary" :disabled="importingBackup" @click="closeImportPreviewDialog">
              取消
            </button>
            <button class="btn-detail-primary" :disabled="importingBackup" @click="confirmImportBackup">
              {{ importingBackup ? '导入中' : '确认导入' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="dialog">
      <div v-if="showOauthDialog" class="oauth-backdrop" @click.self="closeOauthDialog">
        <div class="oauth-dialog">
          <div class="oauth-header">
            <div>
              <h2>OAuth 授权添加账号</h2>
              <p>通过 OpenAI 官方授权登录，成功后会自动保存账号信息。</p>
            </div>
            <button class="oauth-close" @click="closeOauthDialog" aria-label="关闭">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>

          <div class="oauth-body">
            <div class="oauth-field">
              <label>授权链接</label>
              <div class="oauth-url-box">
                <input :value="oauthUrl" readonly />
                <button @click="copyOauthUrl">{{ oauthUrlCopied ? '已复制' : '复制' }}</button>
              </div>
            </div>

            <button class="oauth-primary" @click="openOauthUrl">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                <path d="M15 3h4a2 2 0 0 1 2 2v4"/>
                <path d="M10 14 21 3"/>
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
              </svg>
              在浏览器中打开
            </button>
            <button v-if="oauthTimedOut" class="oauth-secondary" @click="retryOauthLogin">
              刷新授权链接
            </button>

            <div class="oauth-field">
              <label>手动输入回调地址</label>
              <div class="oauth-callback-row">
                <input
                  v-model="oauthCallbackUrl"
                  placeholder="粘贴完整回调地址，例如：http://localhost:1455/auth/callback?code=...&state=..."
                  @keyup.enter="() => completeOauthLogin()"
                />
                <button :disabled="oauthAdding || !oauthCallbackUrl.trim()" @click="() => completeOauthLogin()">
                  <svg v-if="oauthAdding" class="spin" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                    <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
                  </svg>
                  <span>{{ oauthAdding ? '处理中...' : '继续' }}</span>
                </button>
              </div>
            </div>

            <div v-if="oauthError" class="oauth-error">
              {{ oauthError }}
            </div>
            <p class="oauth-hint">
              如果浏览器没有自动打开，先复制授权链接手动打开；授权完成后把地址栏中的回调 URL 粘贴到上方。
            </p>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg);
}

/* ── Header ───────────────────────────── */
.header {
  position: relative;
  flex-shrink: 0;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
}

.header-bg {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 3px;
  background: linear-gradient(90deg, var(--primary), var(--accent), #ec4899);
}

.header-inner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 28px;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 14px;
}

.logo {
  width: 38px;
  height: 38px;
  border-radius: 10px;
  background: linear-gradient(135deg, var(--primary), var(--accent));
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 8px rgba(99, 102, 241, 0.3);
}

.header-text h1 {
  font-size: 17px;
  font-weight: 700;
  color: var(--text);
  margin: 0;
  line-height: 1.2;
  letter-spacing: -0.01em;
}

.header-count {
  font-size: 12px;
  color: var(--text-tertiary);
  font-weight: 500;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 14px;
}

.interval-wrap {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-tertiary);
}

.interval-wrap select {
  padding: 7px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  font-size: 13px;
  color: var(--text);
  background: var(--surface);
  cursor: pointer;
  transition: all 0.2s var(--ease-out);
}

.interval-wrap select:hover {
  border-color: #cbd5e1;
}

.interval-wrap select:focus {
  outline: none;
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

.restart-toggle {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
  cursor: pointer;
}

.restart-toggle input {
  width: 15px;
  height: 15px;
  accent-color: var(--primary);
}

.btn-add {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 9px 18px;
  background: linear-gradient(135deg, var(--primary), var(--primary-hover));
  color: #fff;
  border: none;
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s var(--ease-out);
  box-shadow: 0 2px 8px rgba(99, 102, 241, 0.25);
}

.btn-add:hover {
  transform: translateY(-1px);
  box-shadow: 0 4px 14px rgba(99, 102, 241, 0.35);
}

.btn-add:active {
  transform: translateY(0);
}

.btn-add:disabled {
  opacity: 0.6;
  cursor: not-allowed;
  transform: none;
}

.btn-oauth {
  background: #111827;
  box-shadow: 0 2px 8px rgba(17, 24, 39, 0.2);
}

.btn-oauth:hover:not(:disabled) {
  box-shadow: 0 4px 14px rgba(17, 24, 39, 0.28);
}

/* ── Content ──────────────────────────── */
.content {
  flex: 1;
  padding: 24px 28px;
  overflow-y: auto;
  overflow-x: hidden;
}

/* ── Storage panel ─────────────────────── */
.storage-panel {
  margin-bottom: 14px;
  padding: 10px 12px;
  border: 1px solid rgba(99, 102, 241, 0.14);
  border-radius: var(--radius);
  background: linear-gradient(135deg, #ffffff 0%, #f8fafc 100%);
  box-shadow: var(--shadow-xs);
}

.storage-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}

.storage-summary {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.storage-icon {
  width: 30px;
  height: 30px;
  border-radius: 9px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--primary);
  background: var(--primary-light);
  flex-shrink: 0;
}

.storage-title {
  margin: 0;
  color: var(--text);
  font-size: 13px;
  font-weight: 700;
}

.storage-summary span {
  display: block;
  overflow: hidden;
  color: var(--text-tertiary);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.storage-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.tools-menu-wrap {
  position: relative;
}

.btn-tools {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.tools-menu {
  position: absolute;
  z-index: 30;
  top: calc(100% + 6px);
  right: 0;
  min-width: 178px;
  padding: 6px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  box-shadow: var(--shadow-lg);
}

.tools-menu button {
  width: 100%;
  padding: 9px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 700;
  text-align: left;
  cursor: pointer;
}

.tools-menu button:hover {
  background: #f8fafc;
  color: var(--text);
}

.btn-storage,
.btn-storage-primary,
.btn-storage-warning {
  padding: 6px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 700;
  transition: all 0.2s var(--ease-out);
}

.btn-storage-primary {
  border-color: #c7d2fe;
  background: var(--primary-light);
  color: var(--primary);
}

.btn-storage-warning {
  border-color: #fde68a;
  background: var(--warning-light);
  color: var(--warning);
}

.btn-storage:hover,
.btn-storage-primary:hover,
.btn-storage-warning:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: var(--shadow-xs);
}

.btn-storage-warning:disabled {
  opacity: 0.55;
  cursor: wait;
}

.backup-input {
  display: none;
}

.storage-details {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px dashed var(--border);
}

.path-row {
  display: grid;
  grid-template-columns: 78px minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: rgba(255, 255, 255, 0.78);
}

.path-row-muted {
  opacity: 0.82;
}

.path-label {
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 700;
}

.path-row code {
  min-width: 0;
  overflow: hidden;
  color: var(--text);
  font-family: 'SFMono-Regular', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.path-row button {
  padding: 6px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--primary);
  font-size: 12px;
  font-weight: 700;
  transition: all 0.2s var(--ease-out);
}

.path-row button:hover {
  border-color: #c7d2fe;
  background: var(--primary-light);
}

.storage-note {
  margin: 12px 0 0;
  color: var(--text-tertiary);
  font-size: 12px;
}

/* ── Overview panel ───────────────────── */
.overview-panel {
  display: flex;
  align-items: stretch;
  justify-content: space-between;
  gap: 14px;
  margin-bottom: 14px;
}

.stat-grid {
  flex: 1;
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}

.stat-item {
  min-width: 0;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  box-shadow: var(--shadow-xs);
}

.stat-label {
  display: block;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
}

.stat-item strong {
  display: block;
  overflow: hidden;
  margin-top: 5px;
  color: var(--text);
  font-size: 18px;
  font-weight: 800;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.stat-item small {
  display: block;
  overflow: hidden;
  margin-top: 3px;
  color: var(--text-tertiary);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.stat-warn strong {
  color: var(--danger);
}

.overview-controls {
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex-shrink: 0;
}

.segmented {
  display: inline-grid;
  grid-template-columns: repeat(2, minmax(62px, 1fr));
  gap: 2px;
  padding: 3px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #f8fafc;
}

.segmented button {
  min-height: 30px;
  padding: 5px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
  cursor: pointer;
}

.segmented button.active {
  background: var(--surface);
  color: var(--primary);
  box-shadow: var(--shadow-xs);
}

.segmented button:disabled {
  opacity: 0.55;
  cursor: wait;
}

.view-segmented {
  grid-template-columns: repeat(3, minmax(54px, 1fr));
}

.usage-panel {
  margin-bottom: 14px;
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  box-shadow: var(--shadow-xs);
}

.usage-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.section-kicker {
  display: block;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
}

.usage-head h2 {
  margin: 3px 0 0;
  color: var(--text);
  font-size: 16px;
  letter-spacing: 0;
}

.usage-refresh {
  height: 32px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #f8fafc;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
  cursor: pointer;
}

.usage-refresh:hover:not(:disabled) {
  border-color: #cbd5e1;
  background: #f1f5f9;
}

.usage-refresh:disabled {
  opacity: 0.6;
  cursor: wait;
}

.usage-layout {
  display: grid;
  grid-template-columns: minmax(360px, 1.35fr) minmax(180px, 0.62fr) minmax(220px, 0.78fr);
  gap: 12px;
}

.usage-primary,
.usage-breakdown,
.usage-models {
  min-width: 0;
  padding: 12px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: #f8fafc;
}

.usage-total span,
.usage-mini-card span,
.usage-breakdown-row span,
.usage-subhead {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
}

.usage-total strong {
  display: block;
  margin-top: 6px;
  color: var(--text);
  font-size: 34px;
  line-height: 1.08;
  font-weight: 850;
  font-variant-numeric: tabular-nums;
}

.usage-total small,
.usage-mini-card small,
.usage-model-row small,
.usage-note {
  color: var(--text-tertiary);
  font-size: 11px;
}

.usage-total small {
  display: block;
  margin-top: 5px;
}

.usage-mini-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-top: 12px;
}

.usage-mini-card {
  min-width: 0;
  padding: 10px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: var(--surface);
}

.usage-mini-card strong {
  display: block;
  overflow: hidden;
  margin-top: 5px;
  color: var(--text);
  font-size: 17px;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.usage-mini-card small {
  display: block;
  overflow: hidden;
  margin-top: 3px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.usage-breakdown {
  display: grid;
  align-content: start;
  gap: 8px;
}

.usage-breakdown-row,
.usage-model-row {
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 8px;
}

.usage-breakdown-row strong,
.usage-model-row strong {
  color: var(--text);
  font-size: 13px;
  font-weight: 850;
  font-variant-numeric: tabular-nums;
}

.usage-models {
  display: grid;
  align-content: start;
  gap: 8px;
}

.usage-model-row {
  grid-template-columns: minmax(0, 1fr) auto auto;
  padding-top: 8px;
  border-top: 1px solid var(--border-light);
}

.usage-model-row span {
  overflow: hidden;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.usage-empty {
  color: var(--text-tertiary);
  font-size: 12px;
}

.usage-empty-large {
  padding: 28px 12px;
  border: 1px dashed var(--border);
  border-radius: var(--radius-sm);
  background: #f8fafc;
  text-align: center;
}

.usage-note {
  margin-top: 10px;
  line-height: 1.5;
}

/* ── Account toolbar ──────────────────── */
.account-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  margin-bottom: 14px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  box-shadow: var(--shadow-xs);
}

.toolbar-main {
  min-width: 0;
  flex: 1;
  display: flex;
  align-items: center;
  gap: 10px;
}

.account-search {
  min-width: 260px;
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #f8fafc;
  color: var(--text-tertiary);
}

.account-search input {
  min-width: 0;
  width: 100%;
  height: 34px;
  border: 0;
  outline: none;
  background: transparent;
  color: var(--text);
  font-size: 13px;
}

.account-search:focus-within {
  border-color: var(--primary);
  background: var(--surface);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

.toolbar-selects,
.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.toolbar-selects label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--text-tertiary);
  font-size: 12px;
  font-weight: 800;
  white-space: nowrap;
}

.toolbar-selects select {
  height: 34px;
  padding: 0 28px 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text);
  font-size: 12px;
  font-weight: 700;
}

.toolbar-selects select:focus {
  outline: none;
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

.btn-toolbar,
.btn-toolbar-icon {
  height: 34px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
  cursor: pointer;
  transition: all 0.15s var(--ease-out);
}

.btn-toolbar {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
}

.btn-toolbar-icon {
  width: 34px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.btn-toolbar:hover:not(:disabled),
.btn-toolbar-icon:hover:not(:disabled) {
  border-color: #c7d2fe;
  background: var(--primary-light);
  color: var(--primary);
}

.btn-toolbar:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-toolbar-ghost {
  color: var(--text-tertiary);
}

.toolbar-result {
  color: var(--text-tertiary);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}

.batch-failures {
  margin: -4px 0 14px;
  border: 1px solid #fecaca;
  border-radius: var(--radius-sm);
  background: var(--danger-light);
  overflow: hidden;
}

.batch-failures-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 9px 12px;
  border-bottom: 1px solid #fecaca;
}

.batch-failures-head strong {
  color: var(--danger);
  font-size: 12px;
}

.batch-failures-actions {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}

.batch-failures-head button {
  height: 26px;
  padding: 0 9px;
  border: 1px solid #fecaca;
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--danger);
  font-size: 11px;
  font-weight: 800;
  cursor: pointer;
}

.batch-failure-list {
  max-height: 150px;
  overflow: auto;
}

.batch-failure-item {
  display: grid;
  grid-template-columns: minmax(120px, 0.26fr) minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border-bottom: 1px solid rgba(254, 202, 202, 0.75);
}

.batch-failure-item:last-child {
  border-bottom: 0;
}

.batch-failure-item span,
.batch-failure-item code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
}

.batch-failure-item span {
  color: var(--danger);
  font-weight: 800;
}

.batch-failure-item code {
  color: var(--text-secondary);
  font-family: 'SFMono-Regular', 'Cascadia Code', Consolas, monospace;
}

.batch-failure-actions {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.batch-failure-actions button {
  height: 24px;
  padding: 0 8px;
  border: 1px solid #fecaca;
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--danger);
  font-size: 11px;
  font-weight: 800;
  cursor: pointer;
}

.batch-failure-actions button:hover {
  border-color: #fca5a5;
  background: #fff;
}

/* ── Detail drawer ────────────────────── */
.detail-backdrop {
  position: fixed;
  inset: 0;
  z-index: 115;
  display: flex;
  justify-content: flex-end;
  background: rgba(15, 23, 42, 0.28);
}

.detail-drawer {
  width: 420px;
  max-width: 94vw;
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border-left: 1px solid var(--border);
  box-shadow: var(--shadow-xl);
}

.log-drawer {
  width: 620px;
  max-width: 96vw;
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border-left: 1px solid var(--border);
  box-shadow: var(--shadow-xl);
}

.detail-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 18px 20px;
  border-bottom: 1px solid var(--border-light);
}

.detail-kicker {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
}

.detail-header h2 {
  overflow: hidden;
  max-width: 320px;
  margin: 4px 0 0;
  color: var(--text);
  font-size: 18px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-close {
  width: 32px;
  height: 32px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
}

.detail-close:hover {
  background: var(--border-light);
  color: var(--text);
}

.detail-body {
  flex: 1;
  overflow-y: auto;
  padding: 18px 20px;
}

.detail-section + .detail-section {
  margin-top: 18px;
}

.detail-section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 9px;
}

.detail-section-title {
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
}

.detail-section > .detail-section-title {
  margin-bottom: 9px;
}

.detail-section-head .detail-section-title {
  margin-bottom: 0;
}

.detail-mini-action {
  height: 28px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 0 9px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--primary);
  font-size: 11px;
  font-weight: 800;
  cursor: pointer;
}

.detail-mini-action:hover:not(:disabled) {
  border-color: #c7d2fe;
  background: var(--primary-light);
}

.detail-mini-action:disabled {
  opacity: 0.55;
  cursor: wait;
}

.health-grid,
.detail-quota-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.health-item,
.detail-quota-card {
  min-width: 0;
  padding: 10px;
  border: 1px solid #fecaca;
  border-radius: var(--radius-sm);
  background: var(--danger-light);
}

.health-item.ok {
  border-color: #bbf7d0;
  background: var(--success-light);
}

.health-item span,
.detail-quota-card span {
  display: block;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
}

.health-item strong,
.detail-quota-card strong {
  display: block;
  margin-top: 4px;
  color: var(--text);
  font-size: 14px;
}

.health-report {
  margin-top: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  overflow: hidden;
}

.health-report-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 9px 10px;
  border-bottom: 1px solid var(--border-light);
  background: #f8fafc;
}

.health-report-summary.ok {
  background: var(--success-light);
}

.health-report-summary.warn {
  background: var(--warning-light);
}

.health-report-summary.error {
  background: var(--danger-light);
}

.health-report-summary strong {
  color: var(--text);
  font-size: 12px;
}

.health-report-summary span {
  color: var(--text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.health-report-list {
  display: flex;
  flex-direction: column;
}

.health-report-row {
  display: grid;
  grid-template-columns: 118px minmax(0, 1fr);
  gap: 8px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border-light);
}

.health-report-row:last-child {
  border-bottom: 0;
}

.health-report-row span {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
}

.health-report-row strong {
  overflow: hidden;
  color: var(--text-secondary);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.health-report-row.ok strong {
  color: #047857;
}

.health-report-row.warn strong {
  color: #b45309;
}

.health-report-row.error strong {
  color: var(--danger);
}

.detail-quota-card {
  border-color: var(--border);
  background: #f8fafc;
}

.detail-quota-card strong {
  font-size: 20px;
  font-variant-numeric: tabular-nums;
}

.detail-quota-card small {
  display: block;
  margin-top: 4px;
  color: var(--text-tertiary);
  font-size: 11px;
}

.detail-rows {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  overflow: hidden;
}

.detail-rows div {
  display: grid;
  grid-template-columns: 92px minmax(0, 1fr);
  gap: 10px;
  padding: 9px 10px;
  border-bottom: 1px solid var(--border-light);
}

.detail-rows div:last-child {
  border-bottom: 0;
}

.detail-rows span {
  color: var(--text-tertiary);
  font-size: 12px;
  font-weight: 800;
}

.detail-rows code {
  overflow: hidden;
  color: var(--text);
  font-family: 'SFMono-Regular', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-error {
  margin: 0;
  padding: 10px;
  border: 1px solid #fecaca;
  border-radius: var(--radius-sm);
  background: var(--danger-light);
  color: var(--danger);
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-word;
}

.detail-footer {
  display: flex;
  gap: 8px;
  padding: 14px 20px;
  border-top: 1px solid var(--border-light);
}

.btn-detail-primary,
.btn-detail-secondary {
  height: 36px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  font-weight: 800;
  cursor: pointer;
}

.btn-detail-primary {
  flex: 1;
  border: 0;
  background: var(--primary);
  color: #fff;
}

.btn-detail-primary:hover:not(:disabled) {
  background: var(--primary-hover);
}

.btn-detail-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-detail-secondary {
  padding: 0 12px;
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text-secondary);
}

.log-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-light);
  background: #f8fafc;
}

.log-toolbar select {
  min-width: 0;
  flex: 1;
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text);
  font-size: 12px;
}

.log-toolbar .log-action-select {
  flex: 0 0 118px;
}

.log-toolbar button {
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
  cursor: pointer;
}

.log-toolbar button:hover:not(:disabled) {
  border-color: #cbd5e1;
  background: #f1f5f9;
}

.log-toolbar button:disabled {
  opacity: 0.55;
  cursor: wait;
}

.log-toolbar .log-clear {
  color: var(--danger);
}

.log-toggle {
  height: 34px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
  white-space: nowrap;
}

.log-toggle input {
  width: 14px;
  height: 14px;
  accent-color: var(--primary);
}

.log-body {
  flex: 1;
  overflow-y: auto;
  padding: 14px 16px;
  background: #f8fafc;
}

.log-empty {
  padding: 48px 12px;
  color: var(--text-tertiary);
  font-size: 13px;
  text-align: center;
}

.log-item {
  padding: 12px;
  border: 1px solid var(--border);
  border-left-width: 3px;
  border-radius: var(--radius-sm);
  background: var(--surface);
  box-shadow: var(--shadow-xs);
}

.log-item + .log-item {
  margin-top: 10px;
}

.log-error {
  border-left-color: var(--danger);
}

.log-warn {
  border-left-color: var(--warning);
}

.log-info {
  border-left-color: var(--primary);
}

.log-item-head {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 8px;
}

.log-level {
  padding: 2px 7px;
  border-radius: 999px;
  background: #eef2ff;
  color: var(--primary);
  font-size: 11px;
  font-weight: 800;
}

.log-error .log-level {
  background: var(--danger-light);
  color: var(--danger);
}

.log-warn .log-level {
  background: var(--warning-light);
  color: #b45309;
}

.log-item-head strong {
  overflow: hidden;
  color: var(--text);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log-item-head time {
  color: var(--text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.log-copy {
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: #f8fafc;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
  cursor: pointer;
}

.log-copy:hover {
  border-color: #c7d2fe;
  color: var(--primary);
}

.log-item p {
  margin: 8px 0 0;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.5;
  word-break: break-word;
}

.log-account {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-top: 6px;
  color: var(--text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.log-account span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log-account button {
  flex: 0 0 auto;
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #f8fafc;
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 800;
  cursor: pointer;
}

.log-account button:hover {
  border-color: #cbd5e1;
  background: #f1f5f9;
}

.log-details {
  max-height: 260px;
  overflow: auto;
  margin: 10px 0 0;
  padding: 10px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: #0f172a;
  color: #e2e8f0;
  font-family: 'SFMono-Regular', 'Cascadia Code', Consolas, monospace;
  font-size: 11px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

.drawer-enter-active,
.drawer-leave-active {
  transition: opacity 0.18s ease;
}

.drawer-enter-active .detail-drawer,
.drawer-leave-active .detail-drawer {
  transition: transform 0.18s ease;
}

.drawer-enter-from,
.drawer-leave-to {
  opacity: 0;
}

.drawer-enter-from .detail-drawer,
.drawer-leave-to .detail-drawer {
  transform: translateX(24px);
}

/* ── Import preview ───────────────────── */
.import-backdrop {
  position: fixed;
  inset: 0;
  z-index: 125;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: rgba(15, 23, 42, 0.45);
  backdrop-filter: blur(6px);
}

.import-dialog {
  width: 560px;
  max-width: 94vw;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border-light);
  border-radius: 10px;
  background: var(--surface);
  box-shadow: var(--shadow-xl);
}

.import-header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 18px 22px;
  border-bottom: 1px solid var(--border-light);
}

.import-header h2 {
  margin: 0;
  color: var(--text);
  font-size: 17px;
}

.import-header p {
  margin: 6px 0 0;
  color: var(--text-tertiary);
  font-size: 12px;
}

.import-close {
  width: 32px;
  height: 32px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
}

.import-close:hover {
  background: var(--border-light);
  color: var(--text);
}

.import-body {
  overflow-y: auto;
  padding: 18px 22px;
}

.import-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}

.import-stats div {
  min-width: 0;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #f8fafc;
}

.import-stats span,
.import-section-title {
  display: block;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
}

.import-stats strong {
  display: block;
  margin-top: 5px;
  color: var(--text);
  font-size: 18px;
  font-variant-numeric: tabular-nums;
}

.import-section {
  margin-top: 16px;
}

.import-section-title {
  margin-bottom: 8px;
}

.import-strategy {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.import-strategy + .import-strategy {
  margin-top: 8px;
}

.import-strategy input {
  margin-top: 2px;
  accent-color: var(--primary);
}

.import-strategy strong,
.import-strategy small {
  display: block;
}

.import-strategy strong {
  color: var(--text);
  font-size: 13px;
}

.import-strategy small {
  margin-top: 3px;
  color: var(--text-tertiary);
  font-size: 12px;
}

.import-name-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.import-name-list span,
.import-name-list small {
  padding: 5px 8px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: #f8fafc;
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 700;
}

.import-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 14px 22px;
  border-top: 1px solid var(--border-light);
}

/* ── OAuth dialog ─────────────────────── */
.oauth-backdrop {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: rgba(15, 23, 42, 0.45);
  backdrop-filter: blur(6px);
}

.oauth-dialog {
  width: 640px;
  max-width: 94vw;
  max-height: 90vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border: 1px solid var(--border-light);
  border-radius: 10px;
  box-shadow: var(--shadow-xl), 0 0 0 1px rgba(0, 0, 0, 0.04);
}

.oauth-header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 20px 24px;
  border-bottom: 1px solid var(--border-light);
}

.oauth-header h2 {
  margin: 0;
  font-size: 17px;
  color: var(--text);
}

.oauth-header p {
  margin: 6px 0 0;
  font-size: 13px;
  color: var(--text-tertiary);
}

.oauth-close {
  width: 32px;
  height: 32px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.oauth-close:hover {
  background: var(--border-light);
  color: var(--text);
}

.oauth-body {
  padding: 22px 24px 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow-y: auto;
}

.oauth-field label {
  display: block;
  margin-bottom: 7px;
  font-size: 13px;
  font-weight: 700;
  color: var(--text-secondary);
}

.oauth-url-box,
.oauth-callback-row {
  display: flex;
  gap: 8px;
}

.oauth-url-box input,
.oauth-callback-row input {
  min-width: 0;
  flex: 1;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text);
  font-size: 13px;
}

.oauth-url-box input {
  font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
}

.oauth-url-box input:focus,
.oauth-callback-row input:focus {
  outline: none;
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

.oauth-url-box button,
.oauth-callback-row button,
.oauth-primary,
.oauth-secondary {
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 13px;
  font-weight: 700;
  transition: all 0.15s var(--ease-out);
}

.oauth-url-box button {
  min-width: 72px;
  padding: 0 14px;
  background: var(--border-light);
  color: var(--text-secondary);
}

.oauth-url-box button:hover {
  background: var(--border);
  color: var(--text);
}

.oauth-primary {
  width: 100%;
  min-height: 42px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  background: linear-gradient(135deg, var(--primary), var(--primary-hover));
  color: #fff;
  box-shadow: 0 2px 8px rgba(99, 102, 241, 0.22);
}

.oauth-primary:hover {
  transform: translateY(-1px);
  box-shadow: 0 4px 14px rgba(99, 102, 241, 0.3);
}

.oauth-secondary {
  width: 100%;
  min-height: 40px;
  background: var(--border-light);
  color: var(--text-secondary);
}

.oauth-secondary:hover {
  background: var(--border);
  color: var(--text);
}

.oauth-callback-row button {
  min-width: 92px;
  padding: 0 16px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  background: var(--success);
  color: #fff;
}

.oauth-callback-row button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.oauth-error {
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: rgba(239, 68, 68, 0.08);
  color: var(--danger);
  font-size: 13px;
  font-weight: 600;
}

.oauth-hint {
  margin: 0;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: var(--primary-light);
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.6;
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* ── Toast ────────────────────────────── */
.toast {
  position: fixed;
  top: 20px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 11px 22px;
  border-radius: var(--radius);
  font-size: 14px;
  font-weight: 500;
  z-index: 200;
  box-shadow: var(--shadow-xl);
}

.toast-success {
  background: #ecfdf5;
  color: #059669;
  border: 1px solid #a7f3d0;
}

.toast-error {
  background: #fef2f2;
  color: #dc2626;
  border: 1px solid #fecaca;
}

.toast-enter-active,
.toast-leave-active {
  transition: all 0.35s var(--ease-out);
}

.toast-enter-from {
  opacity: 0;
  transform: translateX(-50%) translateY(-16px) scale(0.95);
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-8px);
}

/* ── Dialog transition ────────────────── */
.dialog-enter-active {
  transition: opacity 0.2s ease;
}

.dialog-leave-active {
  transition: opacity 0.15s ease;
}

.dialog-enter-from,
.dialog-leave-to {
  opacity: 0;
}

@media (max-width: 760px) {
  .header-inner {
    align-items: flex-start;
    flex-direction: column;
    gap: 14px;
  }

  .header-right {
    width: 100%;
    justify-content: space-between;
  }

  .content {
    padding: 18px;
  }

  .storage-bar {
    align-items: stretch;
    flex-direction: column;
  }

  .storage-actions {
    justify-content: flex-end;
    flex-wrap: wrap;
  }

  .overview-panel {
    flex-direction: column;
    min-width: 0;
  }

  .stat-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .overview-controls {
    flex-direction: row;
  }

  .segmented {
    flex: 1;
  }

  .usage-layout {
    grid-template-columns: 1fr;
  }

  .usage-mini-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .account-toolbar,
  .toolbar-main,
  .toolbar-actions {
    align-items: stretch;
    flex-direction: column;
  }

  .account-search {
    min-width: 0;
  }

  .toolbar-selects {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 34px;
  }

  .toolbar-selects label {
    align-items: stretch;
    flex-direction: column;
    gap: 4px;
  }

  .toolbar-selects select,
  .btn-toolbar,
  .btn-toolbar-icon {
    width: 100%;
  }

  .path-row {
    grid-template-columns: 1fr auto;
  }

  .path-label {
    grid-column: 1 / -1;
  }
}

@media (max-width: 520px) {
  .header-right,
  .overview-controls {
    align-items: stretch;
    flex-direction: column;
    min-width: 0;
    width: 100%;
  }

  .overview-controls .segmented {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }

  .overview-controls .view-segmented {
    grid-template-columns: 1fr;
  }

  .overview-controls .segmented button {
    min-width: 0;
    padding-inline: 4px;
  }

  .stat-grid {
    grid-template-columns: 1fr;
  }

  .usage-head {
    align-items: stretch;
    flex-direction: column;
  }

  .usage-refresh {
    justify-content: center;
    width: 100%;
  }

  .usage-total strong {
    font-size: 28px;
  }

  .usage-mini-grid {
    grid-template-columns: 1fr;
  }

  .detail-drawer {
    width: 100vw;
    max-width: 100vw;
  }

  .detail-quota-grid,
  .health-grid {
    grid-template-columns: 1fr;
  }

  .detail-footer {
    flex-direction: column;
  }
}
</style>
