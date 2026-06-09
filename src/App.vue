<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import appLogoUrl from '../src-tauri/icons/128x128.png';
import { useAccounts } from './composables/useAccounts';
import AccountList from './components/AccountList.vue';
import AccountDialog from './components/AccountDialog.vue';
import type {
  Account,
  AccountHealthReport,
  BackupPreview,
  CodexAppSpeed,
  CodexAppSpeedConfig,
  CodexProxyAccountChangedEvent,
  CodexFeatureStatus,
  CodexProxyState,
  CodexSessionVisibilityRepairReport,
  CodexSessionVisibilityStatus,
  CodexUsageRollup,
  CodexUsageSummary,
  BatchRefreshFailure,
  BatchRefreshProgress,
  ImportBackupResult,
  ImportBackupStrategy,
  MigrationStatus,
  OAuthSaveResult,
  OperationLog,
  StoragePaths,
} from './types';

const {
  accounts, loading, switchingId, currentAccountRecordId, refreshInterval, accountViewMode,
  restartCodexOnSwitch,
  loadAccounts, loadCurrentAccount, getAccountAuthJson, addAccount, updateAccount, deleteAccount,
  refreshQuota, refreshProfile, refreshQuotaBatch, switchAccount, loadRefreshInterval, setRefreshInterval,
  loadRestartCodexOnSwitch, setRestartCodexOnSwitch, loadAccountViewMode, setAccountViewMode,
} = useAccounts();

const showDialog = ref(false);
const editingAccount = ref<Account | null>(null);
const editingAccountAuthJson = ref('');
const editingAccountAuthLoading = ref(false);
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
const codexFeatureStatus = ref<CodexFeatureStatus | null>(null);
const codexFeatureLoading = ref(false);
const codexFeatureRepairing = ref(false);
const showCodexFeatureStatus = ref(false);
const codexProxyState = ref<CodexProxyState | null>(null);
const codexProxyBusy = ref(false);
const codexProxySelectedAccountId = ref<number | null>(null);
const codexUsage = ref<CodexUsageSummary | null>(null);
const codexUsageLoading = ref(false);
const showUsageSidebar = ref(false);
const batchRefreshing = ref(false);
const batchRefreshProgress = ref<BatchRefreshProgress | null>(null);
const batchRefreshFailures = ref<BatchRefreshFailure[]>([]);
const detailAccountId = ref<number | null>(null);
const healthCheckingId = ref<number | null>(null);
const healthReports = ref<Record<number, AccountHealthReport>>({});
const pendingDeleteAccount = ref<Account | null>(null);
const deletingAccount = ref(false);
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
const backupPasswordMode = ref<BackupPasswordMode | null>(null);
const backupPassword = ref('');
const backupPasswordError = ref('');
const backupPasswordBusy = ref(false);
const pendingExportAccountIds = ref<number[] | null>(null);
const pendingImportFile = ref<File | null>(null);
const showProjectVisibilityDialog = ref(false);
const sessionVisibilityStatus = ref<CodexSessionVisibilityStatus | null>(null);
const projectVisibilityBusy = ref(false);
const pendingConfirmAction = ref<ConfirmAction | null>(null);
const confirmActionBusy = ref(false);
const showOperationLogs = ref(false);
const operationLogs = ref<OperationLog[]>([]);
const operationLogsLoading = ref(false);
const operationLogAccountId = ref<number | null>(null);
const operationLogErrorsOnly = ref(false);
const operationLogActionFilter = ref('all');
let messageTimer: ReturnType<typeof setTimeout> | null = null;
let unlistenOauth: UnlistenFn | null = null;
let unlistenOauthTimeout: UnlistenFn | null = null;
let unlistenProxyAccountChanged: UnlistenFn | null = null;
let editAuthLoadToken = 0;

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
type BackupPasswordMode = 'export_all' | 'export_filtered' | 'import';
type ConfirmAction = 'migrate_plaintext' | 'clear_logs';

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

const proxyAccountCandidates = computed(() =>
  accounts.value.filter(account => account.has_json_info),
);

const codexProxyStatusLabel = computed(() => {
  if (!codexProxyState.value) return '未读取';
  if (codexProxyState.value.enabled && codexProxyState.value.config_installed) return '运行中';
  if (codexProxyState.value.enabled) return '仅服务运行';
  if (codexProxyState.value.config_installed) return '配置已接管';
  return '未启用';
});

const codexProxySwitchActive = computed(() =>
  Boolean(codexProxyState.value?.enabled || codexProxyState.value?.config_installed),
);

const codexProxySwitchDisabled = computed(() =>
  codexProxyBusy.value
  || (!codexProxySwitchActive.value && proxyAccountCandidates.value.length === 0),
);

const codexSpeedSwitchActive = computed(() => codexAppSpeed.value === 'fast');

const codexSpeedSwitchLabel = computed(() => {
  if (codexSpeedSaving.value) return '处理中';
  return codexSpeedSwitchActive.value ? 'Fast' : '标准';
});

const codexProxyBaseUrlLabel = computed(() => {
  const baseUrl = codexProxyState.value?.base_url ?? '';
  return baseUrl.replace(/^https?:\/\//, '');
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

const codexFeatureIssueCount = computed(() => {
  const status = codexFeatureStatus.value;
  if (!status) return 0;
  let count = 0;
  if (!status.goals_enabled) count += 1;
  if (!status.memory_generate_enabled) count += 1;
  if (!status.memory_use_enabled) count += 1;
  if (!status.official_mode_ok) count += status.official_mode_issues.length || 1;
  if (!status.fast_state_synced) count += 1;
  return count;
});

const codexFeatureStatusLabel = computed(() =>
  codexFeatureIssueCount.value > 0 ? `${codexFeatureIssueCount.value} 项需确认` : '配置正常',
);

const backupPasswordTitle = computed(() => {
  if (backupPasswordMode.value === 'import') return '导入加密备份';
  if (backupPasswordMode.value === 'export_filtered') return '导出筛选账号';
  return '导出全部账号';
});

const backupPasswordDescription = computed(() =>
  backupPasswordMode.value === 'import'
    ? '输入导出时设置的密码，先预览备份内容，再决定导入策略。'
    : '设置一个至少 8 位的备份密码，导入到新机器时需要使用同一个密码。',
);

const backupPasswordConfirmLabel = computed(() =>
  backupPasswordMode.value === 'import'
    ? (backupPasswordBusy.value ? '读取中...' : '读取备份')
    : (backupPasswordBusy.value ? '导出中...' : '导出备份'),
);

const confirmActionTitle = computed(() =>
  pendingConfirmAction.value === 'clear_logs' ? '清空操作日志？' : '迁移旧账号凭据？',
);

const confirmActionDescription = computed(() => {
  if (pendingConfirmAction.value === 'clear_logs') {
    return '这会删除当前账号管理器记录的操作日志，不会影响账号凭据。';
  }
  const pending = migrationStatus.value?.pending_plaintext_accounts ?? 0;
  return `检测到 ${pending} 个旧账号需要确认。本版本会把完整 auth.json 保存在本地账号库。`;
});

const confirmActionButtonLabel = computed(() => {
  if (pendingConfirmAction.value === 'clear_logs') return confirmActionBusy.value ? '清空中...' : '清空日志';
  return confirmActionBusy.value ? '迁移中...' : '开始迁移';
});

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
  unlistenProxyAccountChanged = await listen<CodexProxyAccountChangedEvent>('codex-proxy-account-changed', async (event) => {
    const payload = event.payload;
    await Promise.all([
      loadAccounts(),
      loadCodexProxyState(),
      showOperationLogs.value ? loadOperationLogs() : Promise.resolve(),
    ]);
    codexProxySelectedAccountId.value = payload.activeAccountId;
    showMessage(`代理已自动切换到：${payload.activeAccountName}（${payload.reasonLabel}）`);
  });
  try {
    await loadAccounts();
  } catch (e) {
    showMessage(`加载账号失败: ${e}`, 'error');
  }
  await loadCurrentAccount();
  await loadCodexProxyState();
  await loadStoragePaths();
  await loadMigrationStatus();
  await loadRefreshInterval(10);
  await loadRestartCodexOnSwitch(true);
  await loadAccountViewMode('table');
  await loadCodexAppSpeed();
  await loadCodexFeatureStatus();
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
  if (unlistenProxyAccountChanged) {
    unlistenProxyAccountChanged();
    unlistenProxyAccountChanged = null;
  }
});

function showMessage(text: string, type: 'success' | 'error' = 'success') {
  message.value = text;
  messageType.value = type;
  if (messageTimer) clearTimeout(messageTimer);
  messageTimer = setTimeout(() => { message.value = ''; }, 3000);
}

function openAddDialog() {
  editAuthLoadToken += 1;
  editingAccount.value = null;
  editingAccountAuthJson.value = '';
  editingAccountAuthLoading.value = false;
  showDialog.value = true;
}

async function openEditDialog(account: Account) {
  const loadToken = editAuthLoadToken + 1;
  editAuthLoadToken = loadToken;
  editingAccount.value = account;
  editingAccountAuthJson.value = '';
  editingAccountAuthLoading.value = account.has_json_info;
  showDialog.value = true;

  if (!account.has_json_info) return;

  try {
    const authJson = await getAccountAuthJson(account.id);
    if (editAuthLoadToken !== loadToken) return;
    editingAccountAuthJson.value = authJson;
  } catch (e) {
    if (editAuthLoadToken !== loadToken) return;
    showMessage(`读取账号 auth.json 失败: ${e}`, 'error');
  } finally {
    if (editAuthLoadToken === loadToken) {
      editingAccountAuthLoading.value = false;
    }
  }
}

function closeDialog() {
  editAuthLoadToken += 1;
  showDialog.value = false;
  editingAccount.value = null;
  editingAccountAuthJson.value = '';
  editingAccountAuthLoading.value = false;
}

function resetOauthDialogState() {
  showOauthDialog.value = false;
  oauthAdding.value = false;
  oauthLoginId.value = '';
  oauthUrl.value = '';
  oauthCallbackUrl.value = '';
  oauthError.value = '';
  oauthUrlCopied.value = false;
  oauthTimedOut.value = false;
}

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
  resetOauthDialogState();
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
  if (!oauthLoginId.value) {
    await copyOauthUrl();
    return;
  }
  oauthError.value = '';
  try {
    await invoke('open_codex_oauth_url', { loginId: oauthLoginId.value });
  } catch (e) {
    const message = String(e).replace(/^Error:\s*/, '');
    try {
      await navigator.clipboard.writeText(oauthUrl.value);
      oauthUrlCopied.value = true;
      setTimeout(() => { oauthUrlCopied.value = false; }, 1200);
      oauthError.value = `打开浏览器失败：${message}。授权链接已复制，请手动粘贴到浏览器。`;
    } catch {
      oauthError.value = `打开浏览器失败：${message}。请复制授权链接后手动打开。`;
    }
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
    const result = await invoke<OAuthSaveResult>('complete_codex_oauth_login', {
      loginId: oauthLoginId.value,
      callbackUrl: auto ? null : oauthCallbackUrl.value.trim(),
    });
    await loadAccounts();
    resetAccountFilters();
    const savedAccount = accounts.value.find(account => account.id === result.id);
    if (!savedAccount) {
      throw new Error(`OAuth 账号已写入数据库，但列表刷新后没有找到记录 #${result.id}。请重新打开应用或查看操作日志。`);
    }
    resetOauthDialogState();
    showMessage(result.created ? `OAuth 账号已新增: ${savedAccount.name}` : `OAuth 账号已更新: ${savedAccount.name}`);
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

async function loadCodexFeatureStatus(showSuccess = false) {
  codexFeatureLoading.value = true;
  try {
    codexFeatureStatus.value = await invoke<CodexFeatureStatus>('get_codex_feature_status');
    codexAppSpeed.value = codexFeatureStatus.value.config_speed;
    if (showSuccess) showMessage('Codex 配置检查已刷新');
  } catch (e) {
    showMessage(`读取 Codex 配置检查失败: ${e}`, 'error');
  } finally {
    codexFeatureLoading.value = false;
  }
}

async function loadCodexProxyState(showSuccess = false) {
  try {
    codexProxyState.value = await invoke<CodexProxyState>('get_codex_proxy_state');
    codexProxySelectedAccountId.value =
      codexProxyState.value.active_account_id
      ?? currentAccountRecordId.value
      ?? proxyAccountCandidates.value[0]?.id
      ?? null;
    if (showSuccess) showMessage('Codex 代理状态已刷新');
  } catch (e) {
    showMessage(`读取 Codex 代理状态失败: ${e}`, 'error');
  }
}

async function activateCodexProxy() {
  if (codexProxyBusy.value) return;
  const accountId =
    codexProxySelectedAccountId.value
    ?? currentAccountRecordId.value
    ?? proxyAccountCandidates.value[0]?.id
    ?? null;
  if (accountId === null) {
    showMessage('没有可用于代理的账号', 'error');
    return;
  }
  codexProxyBusy.value = true;
  try {
    codexProxyState.value = await invoke<CodexProxyState>('activate_codex_proxy', {
      accountId,
      port: codexProxyState.value?.port ?? null,
    });
    codexProxySelectedAccountId.value = codexProxyState.value.active_account_id;
    await loadCodexFeatureStatus();
    showMessage('Codex 代理已启用');
  } catch (e) {
    showMessage(`启用 Codex 代理失败: ${e}`, 'error');
  } finally {
    codexProxyBusy.value = false;
  }
}

async function deactivateCodexProxy() {
  if (codexProxyBusy.value) return;
  codexProxyBusy.value = true;
  try {
    codexProxyState.value = await invoke<CodexProxyState>('deactivate_codex_proxy');
    await loadCodexFeatureStatus();
    showMessage('Codex 代理已停用');
  } catch (e) {
    showMessage(`停用 Codex 代理失败: ${e}`, 'error');
  } finally {
    codexProxyBusy.value = false;
  }
}

function toggleCodexProxy() {
  if (codexProxySwitchActive.value) {
    void deactivateCodexProxy();
  } else {
    void activateCodexProxy();
  }
}

function toggleCodexAppSpeed() {
  void changeCodexAppSpeed(codexSpeedSwitchActive.value ? 'standard' : 'fast');
}

async function updateCodexProxyAccount() {
  const accountId = codexProxySelectedAccountId.value;
  if (accountId === null) return;
  const shouldKeepActive = Boolean(codexProxyState.value?.enabled || codexProxyState.value?.config_installed);
  codexProxyBusy.value = true;
  try {
    codexProxyState.value = await invoke<CodexProxyState>('set_codex_proxy_account', {
      accountId,
    });
    if (shouldKeepActive && (!codexProxyState.value.enabled || !codexProxyState.value.config_installed)) {
      codexProxyState.value = await invoke<CodexProxyState>('activate_codex_proxy', {
        accountId,
        port: codexProxyState.value.port,
      });
    }
    showMessage('代理账号已切换');
  } catch (e) {
    showMessage(`切换代理账号失败: ${e}`, 'error');
  } finally {
    codexProxyBusy.value = false;
  }
}

async function repairCodexAppSpeedState() {
  if (codexFeatureRepairing.value) return;
  codexFeatureRepairing.value = true;
  try {
    codexFeatureStatus.value = await invoke<CodexFeatureStatus>('repair_codex_app_speed_state');
    codexAppSpeed.value = codexFeatureStatus.value.config_speed;
    showMessage('Fast 状态已同步');
  } catch (e) {
    showMessage(`同步 Fast 状态失败: ${e}`, 'error');
  } finally {
    codexFeatureRepairing.value = false;
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
  if (codexSpeedSaving.value) return;
  if (codexAppSpeed.value === speed) {
    if (codexFeatureStatus.value && !codexFeatureStatus.value.fast_state_synced) {
      await repairCodexAppSpeedState();
    }
    return;
  }
  codexSpeedSaving.value = true;
  try {
    const config = await invoke<CodexAppSpeedConfig>('set_codex_app_speed', { speed });
    codexAppSpeed.value = config.speed;
    await loadCodexFeatureStatus();
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
      const id = await addAccount(data.name, data.activationDate, data.jsonInfo);
      resetAccountFilters();
      const savedAccount = accounts.value.find(account => account.id === id);
      if (!savedAccount) {
        throw new Error(`账号已写入数据库，但列表刷新后没有找到记录 #${id}。请重新打开应用或查看操作日志。`);
      }
      showMessage(`账号已添加: ${savedAccount.name}`);
    }
    showDialog.value = false;
    editingAccount.value = null;
    editingAccountAuthJson.value = '';
  } catch (e) {
    showMessage(`保存失败: ${e}`, 'error');
  } finally {
    savingAccount.value = false;
  }
}

async function repairProjectVisibility() {
  showToolsMenu.value = false;
  try {
    sessionVisibilityStatus.value = await invoke<CodexSessionVisibilityStatus>('get_codex_session_visibility_status', {
      targetProvider: null,
    });
    showProjectVisibilityDialog.value = true;
  } catch (e) {
    showMessage(`读取历史会话状态失败: ${e}`, 'error');
  }
}

function closeProjectVisibilityDialog(force = false) {
  if (projectVisibilityBusy.value && !force) return;
  showProjectVisibilityDialog.value = false;
  sessionVisibilityStatus.value = null;
}

async function confirmProjectVisibilityRepair() {
  if (projectVisibilityBusy.value) return;
  const targetProvider = sessionVisibilityStatus.value?.target_provider || null;
  projectVisibilityBusy.value = true;
  try {
    const repaired = await invoke<CodexSessionVisibilityRepairReport>('repair_codex_session_visibility', {
      targetProvider,
    });
    closeProjectVisibilityDialog(true);
    const failed = repaired.failed_rollout_files.length;
    const summary = `历史会话可见性已修复: 改写 ${repaired.rewritten_rollout_files} 个 rollout，更新 ${repaired.sqlite_records_updated} 条 SQLite，补写 ${repaired.session_index_entries_added} 条 session_index`;
    showMessage(failed > 0 ? `${summary}，${failed} 个文件失败` : summary, failed > 0 ? 'error' : 'success');
  } catch (e) {
    showMessage(`修复历史会话失败: ${e}`, 'error');
  } finally {
    projectVisibilityBusy.value = false;
  }
}

async function loadMigrationStatus() {
  try {
    migrationStatus.value = await invoke<MigrationStatus>('get_migration_status');
  } catch (e) {
    showMessage(`读取迁移状态失败: ${e}`, 'error');
  }
}

function openBackupPasswordDialog(mode: BackupPasswordMode, accountIds?: number[], file?: File) {
  showToolsMenu.value = false;
  backupPasswordMode.value = mode;
  backupPassword.value = '';
  backupPasswordError.value = '';
  backupPasswordBusy.value = false;
  pendingExportAccountIds.value = accountIds && accountIds.length > 0 ? accountIds : null;
  pendingImportFile.value = file ?? null;
}

function closeBackupPasswordDialog(force = false) {
  if (backupPasswordBusy.value && !force) return;
  backupPasswordMode.value = null;
  backupPassword.value = '';
  backupPasswordError.value = '';
  pendingExportAccountIds.value = null;
  pendingImportFile.value = null;
}

async function runExportBackup(accountIds: number[] | null, password: string) {
  try {
    const backupPath = await invoke<string>('export_encrypted_backup_file', {
      password,
      accountIds,
    });
    showMessage(`加密备份已导出到: ${backupPath}`);
  } catch (e) {
    showMessage(`导出备份失败: ${e}`, 'error');
    throw e;
  }
}

function exportBackup(accountIds?: number[]) {
  openBackupPasswordDialog(accountIds && accountIds.length > 0 ? 'export_filtered' : 'export_all', accountIds);
}

function exportFilteredBackup() {
  const ids = filteredAccounts.value.filter(account => account.has_json_info).map(account => account.id);
  if (ids.length === 0) {
    showMessage('当前筛选结果里没有可导出的账号', 'error');
    return;
  }
  exportBackup(ids);
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

  openBackupPasswordDialog('import', undefined, file);
}

async function runImportPreview(file: File, password: string) {
  try {
    const backupText = await file.text();
    const preview = await invoke<BackupPreview>('preview_encrypted_backup', { backupText, password });
    importBackupText.value = backupText;
    importPassword.value = password;
    importPreview.value = preview;
    importStrategy.value = preview.duplicate_accounts > 0 ? 'merge_by_account_id' : 'add';
    showImportPreviewDialog.value = true;
  } catch (err) {
    showMessage(`读取备份失败: ${err}`, 'error');
    throw err;
  }
}

async function confirmBackupPassword() {
  const mode = backupPasswordMode.value;
  if (!mode || backupPasswordBusy.value) return;
  const password = backupPassword.value;
  if (!password) {
    backupPasswordError.value = '请输入备份密码';
    return;
  }
  if (mode !== 'import' && password.length < 8) {
    backupPasswordError.value = '备份密码至少 8 位';
    return;
  }

  backupPasswordBusy.value = true;
  backupPasswordError.value = '';
  try {
    if (mode === 'import') {
      const file = pendingImportFile.value;
      if (!file) throw new Error('未选择备份文件');
      await runImportPreview(file, password);
    } else {
      await runExportBackup(pendingExportAccountIds.value, password);
    }
    closeBackupPasswordDialog(true);
  } catch (e) {
    backupPasswordError.value = String(e).replace(/^Error:\s*/, '');
  } finally {
    backupPasswordBusy.value = false;
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
    resetAccountFilters();
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
  pendingConfirmAction.value = 'migrate_plaintext';
}

function closeConfirmActionDialog() {
  if (confirmActionBusy.value) return;
  pendingConfirmAction.value = null;
}

async function runMigrateOldAccounts() {
  migratingAccounts.value = true;
  try {
    const status = await invoke<MigrationStatus>('migrate_plaintext_accounts');
    migrationStatus.value = status;
    await loadAccounts();
    showMessage(status.pending_plaintext_accounts === 0 ? '账号库已使用本地保存模式' : `仍有 ${status.pending_plaintext_accounts} 个账号待确认`, status.pending_plaintext_accounts === 0 ? 'success' : 'error');
  } catch (e) {
    showMessage(`迁移旧账号失败: ${e}`, 'error');
  } finally {
    migratingAccounts.value = false;
  }
}

async function confirmDangerAction() {
  if (!pendingConfirmAction.value || confirmActionBusy.value) return;
  const action = pendingConfirmAction.value;
  confirmActionBusy.value = true;
  try {
    if (action === 'migrate_plaintext') {
      await runMigrateOldAccounts();
    } else if (action === 'clear_logs') {
      await invoke('clear_operation_logs');
      await loadOperationLogs();
      showMessage('操作日志已清空');
    }
    pendingConfirmAction.value = null;
  } catch (e) {
    if (action === 'clear_logs') {
      showMessage(`清空日志失败: ${e}`, 'error');
    }
  } finally {
    confirmActionBusy.value = false;
  }
}

async function handleRun(id: number) {
  try {
    const result = await switchAccount(id, restartCodexOnSwitch.value);
    await loadCodexProxyState();
    if (result.restarted) {
      showMessage('账号已切换，Codex 已重启');
      return;
    }
    if (result.hot_switch.status === 'applied') {
      showMessage('账号已切换，正在运行的 Codex 已热更新');
    } else if (result.hot_switch.status === 'unavailable') {
      showMessage('账号已切换；未检测到可热更新的 Codex，重启 Codex 后生效');
    } else if (result.hot_switch.status === 'failed') {
      showMessage(`账号已写入 auth.json，但热切号失败: ${result.hot_switch.message}`, 'error');
    } else {
      showMessage(result.hot_switch.message || '账号已切换');
    }
  }
  catch (e) { showMessage(`切换失败: ${e}`, 'error'); }
}

async function handleDelete(id: number) {
  const account = accounts.value.find(a => a.id === id);
  if (!account) return;
  pendingDeleteAccount.value = account;
}

function closeDeleteConfirm() {
  if (deletingAccount.value) return;
  pendingDeleteAccount.value = null;
}

async function confirmDeleteAccount() {
  const account = pendingDeleteAccount.value;
  if (!account || deletingAccount.value) return;
  deletingAccount.value = true;
  try {
    await deleteAccount(account.id);
    pendingDeleteAccount.value = null;
    showMessage('账号已删除');
  } catch (e) {
    showMessage(`删除失败: ${e}`, 'error');
  } finally {
    deletingAccount.value = false;
  }
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

async function toggleCodexFeatureStatus() {
  showToolsMenu.value = false;
  showCodexFeatureStatus.value = !showCodexFeatureStatus.value;
  if (showCodexFeatureStatus.value) {
    await loadCodexFeatureStatus();
  }
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
  pendingConfirmAction.value = 'clear_logs';
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
            <img :src="appLogoUrl" alt="" />
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
          <label class="restart-toggle" title="开启：写入 auth.json 并重启 Codex。关闭：写入 auth.json 后尝试通知正在运行的 Codex 热切号；连不上时重启后生效。">
            <input type="checkbox" :checked="restartCodexOnSwitch" @change="handleRestartToggle" />
            <span>立即生效（优先重启）</span>
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
      <section class="codex-proxy-strip">
        <div class="proxy-strip-main">
          <span
            class="proxy-status-dot"
            :class="{
              active: codexProxyState?.enabled && codexProxyState?.config_installed,
              partial: codexProxyState?.enabled && !codexProxyState?.config_installed,
              warn: !codexProxyState?.enabled && codexProxyState?.config_installed,
            }"
          ></span>
          <div class="proxy-strip-copy">
            <strong>Codex 代理</strong>
            <span>{{ codexProxyStatusLabel }}</span>
          </div>
          <button
            v-if="codexProxyState?.base_url"
            class="proxy-url-chip"
            :title="`复制代理地址：${codexProxyState.base_url}`"
            @click="copyText(codexProxyState.base_url, '代理地址')"
          >
            {{ codexProxyBaseUrlLabel }}
          </button>
        </div>

        <div class="proxy-strip-controls">
          <select
            v-model.number="codexProxySelectedAccountId"
            :disabled="codexProxyBusy || proxyAccountCandidates.length === 0"
            title="Token 来源：所选账号保存的 OAuth/auth.json"
            @change="updateCodexProxyAccount"
          >
            <option
              v-for="account in proxyAccountCandidates"
              :key="account.id"
              :value="account.id"
            >
              #{{ account.id }} {{ account.name }}
            </option>
          </select>
          <button
            class="btn-toolbar-icon proxy-refresh"
            :disabled="codexProxyBusy"
            title="刷新代理状态"
            @click="loadCodexProxyState(true)"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="23 4 23 10 17 10"/>
              <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
          </button>
          <button
            class="proxy-switch"
            :class="{ active: codexProxySwitchActive }"
            :disabled="codexProxySwitchDisabled"
            title="切换 Codex 代理"
            @click="toggleCodexProxy"
          >
            <span class="proxy-switch-track">
              <span class="proxy-switch-thumb"></span>
            </span>
            <span>{{ codexProxyBusy ? '处理中' : (codexProxySwitchActive ? '启用' : '停用') }}</span>
          </button>
          <button
            class="proxy-switch speed-switch"
            :class="{ active: codexSpeedSwitchActive }"
            :disabled="codexSpeedSaving"
            title="切换 Codex Fast 模式"
            @click="toggleCodexAppSpeed"
          >
            <span class="proxy-switch-track">
              <span class="proxy-switch-thumb"></span>
            </span>
            <span>{{ codexSpeedSwitchLabel }}</span>
          </button>
        </div>
      </section>

      <div v-if="codexProxyState?.last_error" class="proxy-error-strip">
        {{ codexProxyState.last_error }}
      </div>

      <section v-if="showStorageDetails && storagePaths" class="storage-details-panel">
        <div class="storage-details">
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
          <p
            v-if="migrationStatus && migrationStatus.pending_plaintext_accounts > 0"
            class="storage-note"
          >
            当前检测到 {{ migrationStatus.pending_plaintext_accounts }} 个旧账号还未迁移。
          </p>
        </div>
      </section>

      <section v-if="showCodexFeatureStatus" class="codex-feature-panel">
          <div class="codex-feature-head">
            <div>
              <strong>Codex 配置检查</strong>
              <span>{{ codexFeatureStatusLabel }}</span>
            </div>
            <button :disabled="codexFeatureLoading" @click="loadCodexFeatureStatus(true)">
              {{ codexFeatureLoading ? '检查中...' : '刷新' }}
            </button>
          </div>

          <div v-if="codexFeatureStatus" class="codex-feature-grid">
            <div class="codex-feature-item" :class="{ warn: !codexFeatureStatus.goals_enabled }">
              <span>Goal 模式</span>
              <strong>{{ codexFeatureStatus.goals_enabled ? '已开启' : '未开启' }}</strong>
              <small>{{ codexFeatureStatus.goals_db_present ? 'Goals 数据库存在' : '尚未发现 Goals 数据库' }}</small>
            </div>
            <div
              class="codex-feature-item"
              :class="{ warn: !codexFeatureStatus.memory_generate_enabled || !codexFeatureStatus.memory_use_enabled }"
            >
              <span>Memory</span>
              <strong>
                {{ codexFeatureStatus.memory_generate_enabled && codexFeatureStatus.memory_use_enabled ? '已开启' : '需确认' }}
              </strong>
              <small>
                生成 {{ codexFeatureStatus.memory_generate_enabled ? '开' : '关' }} · 使用 {{ codexFeatureStatus.memory_use_enabled ? '开' : '关' }}
              </small>
            </div>
            <div class="codex-feature-item" :class="{ warn: !codexFeatureStatus.official_mode_ok }">
              <span>官方模式</span>
              <strong>{{ codexFeatureStatus.official_mode_ok ? '干净' : '发现 provider 配置' }}</strong>
              <small v-if="codexFeatureStatus.official_mode_ok">未发现 provider / proxy / base_url</small>
              <small v-else :title="codexFeatureStatus.official_mode_issues.map(item => `第 ${item.line} 行 ${item.label}`).join(' · ')">
                {{ codexFeatureStatus.official_mode_issues.length }} 处需人工确认
              </small>
            </div>
            <div class="codex-feature-item" :class="{ warn: !codexFeatureStatus.fast_state_synced }">
              <span>Fast 状态</span>
              <strong>{{ codexFeatureStatus.config_speed === 'fast' ? 'Fast' : '标准' }}</strong>
              <small>
                App 状态 {{ codexFeatureStatus.global_state_service_tier || '空' }}
                <template v-if="!codexFeatureStatus.fast_state_synced"> · 未同步</template>
              </small>
            </div>
          </div>

          <div v-if="codexFeatureStatus" class="codex-feature-actions">
            <button @click="copyText(codexFeatureStatus.config_path, 'config.toml 路径')">复制 config.toml</button>
            <button @click="copyText(codexFeatureStatus.global_state_path, '全局状态路径')">复制 App 状态</button>
            <button
              v-if="!codexFeatureStatus.fast_state_synced"
              :disabled="codexFeatureRepairing"
              @click="repairCodexAppSpeedState"
            >
              {{ codexFeatureRepairing ? '同步中...' : '同步 Fast 状态' }}
            </button>
          </div>

          <div
            v-if="codexFeatureStatus && codexFeatureStatus.official_mode_issues.length > 0"
            class="codex-feature-issues"
          >
            <span
              v-for="issue in codexFeatureStatus.official_mode_issues"
              :key="`${issue.line}-${issue.label}`"
            >
              第 {{ issue.line }} 行 {{ issue.label }}
            </span>
          </div>
      </section>

      <section class="overview-panel">
        <div class="stat-grid">
          <div class="stat-item stat-current">
            <div class="stat-icon">
              <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M12 12a4 4 0 1 0-4-4 4 4 0 0 0 4 4Z"/>
                <path d="M4 21a8 8 0 0 1 11.8-7"/>
                <path d="m16 19 2 2 4-5"/>
              </svg>
            </div>
            <div class="stat-content">
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
            <svg class="stat-sparkline" viewBox="0 0 96 28" preserveAspectRatio="none">
              <polyline points="2,21 16,17 30,18 44,12 58,15 72,8 94,13" />
            </svg>
          </div>
          <div class="stat-item stat-count">
            <div class="stat-icon">
              <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/>
                <circle cx="9" cy="7" r="4"/>
                <path d="M22 21v-2a4 4 0 0 0-3-3.87"/>
                <path d="M16 3.13a4 4 0 0 1 0 7.75"/>
              </svg>
            </div>
            <div class="stat-content">
              <span class="stat-label">账号数量</span>
              <strong>{{ accountStats.total }}</strong>
              <small>可用 {{ accountStats.usable }} 个</small>
            </div>
            <svg class="stat-sparkline" viewBox="0 0 96 28" preserveAspectRatio="none">
              <polyline points="2,20 18,14 34,16 50,10 66,12 82,8 94,9" />
            </svg>
          </div>
          <div class="stat-item stat-quota">
            <div class="stat-icon">
              <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M4 13a8 8 0 1 1 16 0"/>
                <path d="M12 13 16 8"/>
                <path d="M3 17h18"/>
                <path d="M7 21h10"/>
              </svg>
            </div>
            <div class="stat-content">
              <span class="stat-label">剩余额度池</span>
              <strong>{{ accountStats.totalPrimaryRemaining }}%</strong>
              <small>
                {{ accountStats.totalPrimaryLabel }}合计
                <template v-if="accountStats.hasSecondaryQuota">
                  · {{ accountStats.totalSecondaryLabel }} {{ accountStats.totalSecondaryRemaining }}%
                </template>
              </small>
            </div>
            <svg class="stat-sparkline" viewBox="0 0 96 28" preserveAspectRatio="none">
              <polyline points="2,18 16,16 30,12 44,15 58,9 72,11 94,7" />
            </svg>
          </div>
          <div class="stat-item stat-warn">
            <div class="stat-icon">
              <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10"/>
                <path d="M12 7v6"/>
                <path d="M12 17h.01"/>
              </svg>
            </div>
            <div class="stat-content">
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
            <svg class="stat-sparkline" viewBox="0 0 96 28" preserveAspectRatio="none">
              <polyline points="2,12 16,14 30,10 44,15 58,13 72,17 94,19" />
            </svg>
          </div>
        </div>
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
          <button
            class="btn-toolbar btn-usage-toggle"
            :class="{ active: showUsageSidebar }"
            @click="showUsageSidebar = true"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 3v18h18"/>
              <path d="M7 15l3-3 3 2 5-6"/>
            </svg>
            统计
            <span v-if="codexUsage && codexUsage.today.error_count > 0" class="usage-badge">{{ codexUsage.today.error_count }}</span>
          </button>
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
          <button
            v-if="migrationStatus && migrationStatus.pending_plaintext_accounts > 0"
            class="btn-toolbar btn-migration-warning"
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
          <div class="view-mode-actions" title="账号展示方式">
            <button
              class="btn-toolbar-icon view-mode-button"
              :class="{ active: accountViewMode === 'cards' }"
              title="卡片视图"
              aria-label="卡片视图"
              @click="setAccountViewMode('cards')"
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round">
                <rect x="3" y="3" width="7" height="7" rx="1.5"/>
                <rect x="14" y="3" width="7" height="7" rx="1.5"/>
                <rect x="3" y="14" width="7" height="7" rx="1.5"/>
                <rect x="14" y="14" width="7" height="7" rx="1.5"/>
              </svg>
            </button>
            <button
              class="btn-toolbar-icon view-mode-button"
              :class="{ active: accountViewMode === 'compact' }"
              title="紧凑视图"
              aria-label="紧凑视图"
              @click="setAccountViewMode('compact')"
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round">
                <path d="M4 6h16"/>
                <path d="M4 12h16"/>
                <path d="M4 18h16"/>
              </svg>
            </button>
            <button
              class="btn-toolbar-icon view-mode-button"
              :class="{ active: accountViewMode === 'table' }"
              title="表格视图"
              aria-label="表格视图"
              @click="setAccountViewMode('table')"
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round">
                <rect x="3" y="4" width="18" height="16" rx="2"/>
                <path d="M3 10h18"/>
                <path d="M3 15h18"/>
                <path d="M9 4v16"/>
                <path d="M15 4v16"/>
              </svg>
            </button>
          </div>
          <div class="tools-menu-wrap toolbar-tools" @click.stop>
            <button class="btn-toolbar btn-tools" @click="showToolsMenu = !showToolsMenu">
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
                <span>修复历史会话</span>
              </button>
              <button @click="toggleCodexFeatureStatus">
                <span>{{ showCodexFeatureStatus ? '收起 Codex 配置检查' : '检查 Codex 配置' }}</span>
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
      <div v-if="showUsageSidebar" class="detail-backdrop" @click.self="showUsageSidebar = false">
        <aside class="usage-drawer">
          <div class="detail-header">
            <div>
              <span class="detail-kicker">统计</span>
              <h2>Codex 使用统计</h2>
            </div>
            <button class="detail-close" @click="showUsageSidebar = false" aria-label="关闭统计">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>

          <div class="usage-drawer-body">
            <div class="usage-drawer-actions">
              <span>{{ codexUsage ? '今日' : '本地日志' }}</span>
              <button :disabled="codexUsageLoading" @click="loadCodexUsage(true)">
                <svg v-if="codexUsageLoading" class="spin" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                  <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
                </svg>
                {{ codexUsageLoading ? '读取中' : '刷新' }}
              </button>
            </div>

            <div v-if="codexUsage" class="usage-drawer-content">
              <section class="usage-hero">
                <span>今日 Tokens</span>
                <strong>{{ formatExactNumber(codexUsage.today.total_tokens) }}</strong>
                <small>全部 {{ formatTokenAmount(codexUsage.total.total_tokens) }} · 成功率 {{ usageSuccessRate(codexUsage.today) }}</small>
              </section>

              <section class="usage-metric-list">
                <div>
                  <span>请求数</span>
                  <strong>{{ formatExactNumber(codexUsage.today.request_count) }}</strong>
                </div>
                <div>
                  <span>成功 / 失败</span>
                  <strong><em>{{ codexUsage.today.success_count }}</em> / <b>{{ codexUsage.today.error_count }}</b></strong>
                </div>
                <div>
                  <span>Codex Credits</span>
                  <strong>{{ formatCredits(codexUsage.today.codex_credits) }}</strong>
                </div>
                <div>
                  <span>API 等价成本</span>
                  <strong>{{ formatUsd(codexUsage.today.api_cost_usd) }}</strong>
                </div>
              </section>

              <section class="usage-drawer-section">
                <div class="usage-drawer-section-title">按模型统计</div>
                <div v-if="topCodexUsageModels.length === 0" class="usage-empty">暂无 token 记录</div>
                <div v-for="item in topCodexUsageModels" :key="item.model" class="usage-model-row">
                  <span :title="item.model">{{ item.model }}</span>
                  <strong>{{ formatTokenAmount(item.usage.total_tokens) }}</strong>
                  <small>{{ formatUsd(item.usage.api_cost_usd) }}</small>
                </div>
              </section>

              <details class="usage-fold" open>
                <summary>Token 构成</summary>
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
              </details>

              <details class="usage-fold">
                <summary>失败详情</summary>
                <div v-if="codexUsage.recent_failures.length === 0" class="usage-empty">没有失败记录</div>
                <div v-for="failure in codexUsage.recent_failures" :key="`${failure.ts}-${failure.response_id}-${failure.turn_id}`" class="usage-failure-row">
                  <strong>{{ failure.status }}</strong>
                  <span>{{ failure.model }}</span>
                  <small>{{ failure.message }}</small>
                </div>
              </details>

              <p class="usage-note">{{ codexUsage.note }}</p>
            </div>

            <div v-else class="usage-empty usage-drawer-empty">
              {{ codexUsageLoading ? '读取 Codex 本地日志中...' : '暂无 Codex 使用统计' }}
            </div>
          </div>
        </aside>
      </div>
    </Transition>

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
      <div v-if="backupPasswordMode" class="confirm-backdrop" @click.self="closeBackupPasswordDialog()">
        <div class="confirm-dialog">
          <div class="confirm-icon confirm-icon-primary">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <path v-if="backupPasswordMode === 'import'" d="M7 10l5 5 5-5"/>
              <path v-if="backupPasswordMode === 'import'" d="M12 15V3"/>
              <path v-if="backupPasswordMode !== 'import'" d="M7 8l5-5 5 5"/>
              <path v-if="backupPasswordMode !== 'import'" d="M12 3v12"/>
            </svg>
          </div>
          <div class="confirm-content">
            <span>加密备份</span>
            <h2>{{ backupPasswordTitle }}</h2>
            <p>{{ backupPasswordDescription }}</p>
            <label class="confirm-field">
              <span>备份密码</span>
              <input
                v-model="backupPassword"
                type="password"
                autocomplete="off"
                placeholder="输入备份密码"
                @keyup.enter="confirmBackupPassword"
              />
            </label>
            <p v-if="backupPasswordError" class="confirm-error">{{ backupPasswordError }}</p>
          </div>
          <div class="confirm-actions">
            <button class="confirm-cancel" :disabled="backupPasswordBusy" @click="closeBackupPasswordDialog()">取消</button>
            <button class="confirm-primary" :disabled="backupPasswordBusy" @click="confirmBackupPassword">
              {{ backupPasswordConfirmLabel }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="dialog">
      <div v-if="showProjectVisibilityDialog" class="confirm-backdrop" @click.self="closeProjectVisibilityDialog()">
        <div class="confirm-dialog">
          <div class="confirm-icon confirm-icon-primary">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
              <path d="M12 3l7 4v5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V7l7-4z"/>
              <path d="M9 12l2 2 4-4"/>
            </svg>
          </div>
          <div class="confirm-content">
            <span>历史会话</span>
            <h2>修复 Codex 历史会话可见性</h2>
            <p>将旧会话同步到当前 provider，并修复 Codex 本地 SQLite 与 session_index。执行前会自动备份被修改的文件。</p>
            <div v-if="sessionVisibilityStatus" class="repair-stats">
              <div>
                <span>目标 provider</span>
                <strong>{{ sessionVisibilityStatus.target_provider }}</strong>
              </div>
              <div>
                <span>rollout 待改写</span>
                <strong>{{ sessionVisibilityStatus.mismatched_rollout_files }}</strong>
              </div>
              <div>
                <span>SQLite 待更新</span>
                <strong>{{ sessionVisibilityStatus.mismatched_sqlite_records + sessionVisibilityStatus.missing_sqlite_records }}</strong>
              </div>
              <div>
                <span>session_index 待补写</span>
                <strong>{{ sessionVisibilityStatus.missing_session_index_entries }}</strong>
              </div>
            </div>
          </div>
          <div class="confirm-actions">
            <button class="confirm-cancel" :disabled="projectVisibilityBusy" @click="closeProjectVisibilityDialog()">取消</button>
            <button class="confirm-primary" :disabled="projectVisibilityBusy" @click="confirmProjectVisibilityRepair">
              {{ projectVisibilityBusy ? '修复中...' : '开始修复' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="dialog">
      <div v-if="pendingConfirmAction" class="confirm-backdrop" @click.self="closeConfirmActionDialog">
        <div class="confirm-dialog">
          <div class="confirm-icon">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
              <path d="M10.3 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.7 3.86a2 2 0 0 0-3.4 0z"/>
              <path d="M12 9v4"/>
              <path d="M12 17h.01"/>
            </svg>
          </div>
          <div class="confirm-content">
            <span>确认操作</span>
            <h2>{{ confirmActionTitle }}</h2>
            <p>{{ confirmActionDescription }}</p>
          </div>
          <div class="confirm-actions">
            <button class="confirm-cancel" :disabled="confirmActionBusy" @click="closeConfirmActionDialog">取消</button>
            <button class="confirm-danger" :disabled="confirmActionBusy" @click="confirmDangerAction">
              {{ confirmActionButtonLabel }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="dialog">
      <div v-if="pendingDeleteAccount" class="confirm-backdrop" @click.self="closeDeleteConfirm">
        <div class="confirm-dialog">
          <div class="confirm-icon">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 6h18"/>
              <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
              <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
              <path d="M10 11v6"/>
              <path d="M14 11v6"/>
            </svg>
          </div>
          <div class="confirm-content">
            <span>删除账号</span>
            <h2>确认删除「{{ pendingDeleteAccount.name }}」？</h2>
            <p>这会从账号管理器里移除该记录和保存的凭据。当前生效的 auth.json 不会在这里被自动切换。</p>
            <div class="confirm-account">
              <strong>{{ pendingDeleteAccount.name }}</strong>
              <code>{{ pendingDeleteAccount.account_id || `记录 #${pendingDeleteAccount.id}` }}</code>
            </div>
          </div>
          <div class="confirm-actions">
            <button class="confirm-cancel" :disabled="deletingAccount" @click="closeDeleteConfirm">取消</button>
            <button class="confirm-danger" :disabled="deletingAccount" @click="confirmDeleteAccount">
              {{ deletingAccount ? '删除中...' : '删除账号' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="dialog">
      <AccountDialog
        v-if="showDialog"
        :account="editingAccount"
        :initial-json-info="editingAccountAuthJson"
        :loading-json="editingAccountAuthLoading"
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
                  <small>不合并现有账号，备份内账号全部作为新记录导入。</small>
                </span>
              </label>
              <label class="import-strategy">
                <input v-model="importStrategy" type="radio" value="skip_duplicates" />
                <span>
                  <strong>跳过重复</strong>
                  <small>已有完整凭据的账号会跳过；缺 auth.json 的历史记录会自动恢复。</small>
                </span>
              </label>
              <label class="import-strategy">
                <input v-model="importStrategy" type="radio" value="merge_by_account_id" />
                <span>
                  <strong>合并更新</strong>
                  <small>按账号身份更新同一账号，恢复 auth.json 和额度信息。</small>
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
  width: 40px;
  height: 40px;
  border-radius: 12px;
  background: var(--surface);
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(37, 99, 235, 0.18);
}

.logo img {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: cover;
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
  background-color: var(--surface);
  background-image: var(--select-caret);
  background-repeat: no-repeat;
  background-position: right 10px center;
  background-size: 12px 12px;
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
  position: relative;
  z-index: 60;
  overflow: visible;
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

.storage-pills {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}

.storage-pills span {
  max-width: 100%;
  overflow: hidden;
  padding: 4px 8px;
  border: 1px solid var(--border-light);
  border-radius: 999px;
  background: #fff;
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 800;
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
  z-index: 300;
  top: calc(100% + 6px);
  left: 0;
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

.codex-feature-panel,
.codex-proxy-panel {
  margin-top: 10px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #f8fafc;
}

.codex-feature-head,
.codex-feature-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.codex-feature-head strong {
  display: block;
  color: var(--text);
  font-size: 13px;
  font-weight: 850;
}

.codex-feature-head span {
  display: block;
  margin-top: 2px;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 750;
}

.codex-feature-head button,
.codex-feature-actions button {
  min-height: 28px;
  padding: 0 9px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 800;
  cursor: pointer;
}

.codex-feature-head button:hover:not(:disabled),
.codex-feature-actions button:hover:not(:disabled) {
  border-color: #c7d2fe;
  background: var(--primary-light);
  color: var(--primary);
}

.codex-feature-head button:disabled,
.codex-feature-actions button:disabled {
  opacity: 0.55;
  cursor: wait;
}

.codex-feature-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
  margin-top: 10px;
}

.codex-proxy-grid {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.codex-feature-item {
  min-width: 0;
  padding: 9px 10px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: var(--surface);
}

.codex-feature-item span,
.codex-feature-item small {
  display: block;
  overflow: hidden;
  color: var(--text-tertiary);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.codex-feature-item strong {
  display: block;
  overflow: hidden;
  margin-top: 4px;
  color: var(--success);
  font-size: 14px;
  font-weight: 850;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.codex-feature-item small {
  margin-top: 3px;
}

.codex-feature-item.warn strong {
  color: var(--warning);
}

.codex-feature-actions {
  justify-content: flex-start;
  margin-top: 10px;
}

.codex-proxy-actions {
  flex-wrap: wrap;
}

.codex-proxy-actions select {
  min-height: 28px;
  max-width: min(360px, 100%);
  padding: 0 28px 0 9px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background-color: var(--surface);
  background-image: var(--select-caret);
  background-repeat: no-repeat;
  background-position: right 10px center;
  background-size: 12px 12px;
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 800;
}

.codex-feature-issues {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 9px;
}

.codex-feature-issues span {
  max-width: 240px;
  overflow: hidden;
  padding: 4px 7px;
  border: 1px solid #fed7aa;
  border-radius: 999px;
  background: #fff7ed;
  color: var(--warning);
  font-size: 11px;
  font-weight: 800;
  text-overflow: ellipsis;
  white-space: nowrap;
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

.usage-drawer {
  width: 320px;
  max-width: 92vw;
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border-left: 1px solid var(--border);
  box-shadow: var(--shadow-xl);
}

.usage-drawer-body {
  flex: 1;
  overflow-y: auto;
  padding: 14px;
  background: #f8fafc;
}

.usage-drawer-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 12px;
}

.usage-drawer-actions span,
.usage-drawer-section-title {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 850;
}

.usage-drawer-actions button {
  height: 30px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 850;
  cursor: pointer;
}

.usage-drawer-actions button:hover:not(:disabled) {
  border-color: #c7d2fe;
  background: var(--primary-light);
  color: var(--primary);
}

.usage-drawer-actions button:disabled {
  opacity: 0.55;
  cursor: wait;
}

.usage-drawer-content {
  display: grid;
  gap: 10px;
}

.usage-hero,
.usage-metric-list,
.usage-drawer-section,
.usage-fold {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  box-shadow: var(--shadow-xs);
}

.usage-hero {
  padding: 12px;
}

.usage-hero span,
.usage-metric-list span,
.usage-fold summary {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 850;
}

.usage-hero strong {
  display: block;
  margin-top: 5px;
  color: var(--text);
  font-size: 27px;
  line-height: 1.08;
  font-weight: 850;
  font-variant-numeric: tabular-nums;
}

.usage-hero small,
.usage-model-row small,
.usage-note,
.usage-failure-row small {
  color: var(--text-tertiary);
  font-size: 11px;
}

.usage-hero small {
  display: block;
  margin-top: 5px;
}

.usage-metric-list {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  overflow: hidden;
}

.usage-metric-list div {
  min-width: 0;
  padding: 10px;
  border-right: 1px solid var(--border-light);
  border-bottom: 1px solid var(--border-light);
}

.usage-metric-list div:nth-child(2n) {
  border-right: 0;
}

.usage-metric-list div:nth-last-child(-n + 2) {
  border-bottom: 0;
}

.usage-metric-list strong {
  display: block;
  overflow: hidden;
  margin-top: 4px;
  color: var(--text);
  font-size: 14px;
  font-weight: 850;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.usage-metric-list em {
  color: var(--success);
  font-style: normal;
}

.usage-metric-list b {
  color: var(--danger);
}

.usage-drawer-section {
  padding: 10px;
}

.usage-drawer-section-title {
  margin-bottom: 8px;
}

.usage-fold {
  padding: 0 10px 10px;
}

.usage-fold summary {
  padding: 10px 0;
  cursor: pointer;
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

.usage-model-row {
  grid-template-columns: minmax(0, 1fr) auto auto;
  padding-top: 7px;
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

.usage-note {
  margin: 8px 0 0;
  line-height: 1.5;
}

.usage-drawer-empty {
  padding: 34px 12px;
  border: 1px dashed var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  text-align: center;
}

.usage-failure-row {
  display: grid;
  gap: 3px;
  padding: 8px 0;
  border-top: 1px solid var(--border-light);
}

.usage-failure-row strong {
  color: var(--danger);
  font-size: 12px;
}

.usage-failure-row span {
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
}

.usage-badge {
  min-width: 16px;
  height: 16px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0 5px;
  border-radius: 999px;
  background: var(--danger);
  color: #fff;
  font-size: 10px;
  line-height: 1;
}

.btn-usage-toggle.active {
  border-color: #c7d2fe;
  background: var(--primary-light);
  color: var(--primary);
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

.confirm-backdrop {
  position: fixed;
  inset: 0;
  z-index: 130;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 22px;
  background: rgba(15, 23, 42, 0.34);
  backdrop-filter: blur(6px);
}

.confirm-dialog {
  width: min(420px, 100%);
  padding: 18px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--surface);
  box-shadow: var(--shadow-xl);
}

.confirm-icon {
  width: 42px;
  height: 42px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid #fecaca;
  border-radius: 12px;
  background: var(--danger-light);
  color: var(--danger);
}

.confirm-icon-primary {
  border-color: #c7d2fe;
  background: var(--primary-light);
  color: var(--primary);
}

.confirm-content {
  margin-top: 14px;
}

.confirm-content span {
  color: var(--danger);
  font-size: 11px;
  font-weight: 850;
}

.confirm-content h2 {
  margin: 5px 0 0;
  color: var(--text);
  font-size: 18px;
  line-height: 1.25;
  font-weight: 850;
}

.confirm-content p {
  margin: 9px 0 0;
  color: var(--text-tertiary);
  font-size: 12px;
  line-height: 1.55;
}

.confirm-field {
  display: grid;
  gap: 6px;
  margin-top: 14px;
}

.confirm-field span {
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 850;
}

.confirm-field input {
  width: 100%;
  height: 36px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  outline: none;
  background: #f8fafc;
  color: var(--text);
  font-size: 13px;
}

.confirm-field input:focus {
  border-color: var(--primary);
  background: var(--surface);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

.repair-stats {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
  margin-top: 14px;
}

.repair-stats div {
  min-width: 0;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #f8fafc;
}

.repair-stats span {
  display: block;
  color: var(--text-tertiary);
  font-size: 10px;
  font-weight: 850;
}

.repair-stats strong {
  display: block;
  overflow: hidden;
  margin-top: 4px;
  color: var(--text);
  font-size: 13px;
  font-weight: 900;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.confirm-error {
  margin-top: 8px !important;
  color: var(--danger) !important;
  font-weight: 750;
}

.confirm-account {
  min-width: 0;
  display: grid;
  gap: 3px;
  margin-top: 12px;
  padding: 10px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: #f8fafc;
}

.confirm-account strong,
.confirm-account code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.confirm-account strong {
  color: var(--text);
  font-size: 13px;
  font-weight: 850;
}

.confirm-account code {
  color: var(--text-tertiary);
  font-family: 'SFMono-Regular', 'Cascadia Code', Consolas, monospace;
  font-size: 11px;
}

.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}

.confirm-actions button {
  height: 34px;
  padding: 0 13px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  font-weight: 850;
  cursor: pointer;
}

.confirm-actions button:disabled {
  opacity: 0.55;
  cursor: wait;
}

.confirm-cancel {
  border: 1px solid var(--border);
  background: var(--surface);
  color: var(--text-secondary);
}

.confirm-danger {
  border: 1px solid #dc2626;
  background: #dc2626;
  color: #fff;
}

.confirm-primary {
  border: 1px solid var(--primary);
  background: var(--primary);
  color: #fff;
}

.confirm-cancel:hover:not(:disabled) {
  background: #f8fafc;
}

.confirm-danger:hover:not(:disabled) {
  border-color: #b91c1c;
  background: #b91c1c;
}

.confirm-primary:hover:not(:disabled) {
  border-color: var(--primary-hover);
  background: var(--primary-hover);
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
.drawer-enter-active .usage-drawer,
.drawer-enter-active .log-drawer,
.drawer-leave-active .detail-drawer,
.drawer-leave-active .usage-drawer,
.drawer-leave-active .log-drawer {
  transition: transform 0.18s ease;
}

.drawer-enter-from,
.drawer-leave-to {
  opacity: 0;
}

.drawer-enter-from .detail-drawer,
.drawer-enter-from .usage-drawer,
.drawer-enter-from .log-drawer,
.drawer-leave-to .detail-drawer,
.drawer-leave-to .usage-drawer,
.drawer-leave-to .log-drawer {
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

  .detail-drawer,
  .usage-drawer {
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

/* Refined desktop console skin */
.app {
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.72) 0%, rgba(238, 243, 248, 0.88) 48%, #eaf0f6 100%);
}

.header {
  background: rgba(255, 255, 255, 0.9);
  border-bottom: 1px solid rgba(219, 228, 238, 0.9);
  box-shadow: 0 1px 0 rgba(255, 255, 255, 0.75), 0 10px 28px rgba(16, 24, 39, 0.04);
}

.header-bg {
  height: 2px;
  background: linear-gradient(90deg, var(--primary), var(--accent), #f59e0b);
}

.header-inner {
  min-height: 66px;
  padding: 10px 28px;
}

.logo {
  width: 42px;
  height: 42px;
  border: 1px solid rgba(199, 210, 254, 0.72);
  border-radius: 10px;
  box-shadow: 0 10px 24px rgba(79, 99, 232, 0.13);
}

.header-text h1 {
  font-size: 18px;
  letter-spacing: 0;
}

.header-count {
  color: var(--text-tertiary);
  font-size: 12px;
  font-weight: 750;
}

.header-right {
  gap: 10px;
}

.interval-wrap {
  height: 36px;
  padding: 0 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface-soft);
}

.interval-wrap select {
  height: 34px;
  padding: 0 22px 0 4px;
  border: 0;
  background-color: transparent;
  background-image: var(--select-caret);
  background-repeat: no-repeat;
  background-position: right 6px center;
  background-size: 12px 12px;
  font-weight: 750;
}

.interval-wrap select:focus {
  box-shadow: none;
}

.restart-toggle {
  height: 36px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface-soft);
  color: var(--text-secondary);
}

.restart-toggle input {
  width: 14px;
  height: 14px;
}

.btn-add {
  min-height: 38px;
  padding: 0 16px;
  border-radius: var(--radius-sm);
  background: var(--primary);
  box-shadow: 0 10px 22px rgba(79, 99, 232, 0.18);
}

.btn-add:hover:not(:disabled) {
  background: var(--primary-hover);
  box-shadow: 0 14px 30px rgba(79, 99, 232, 0.22);
}

.btn-oauth {
  background: #111827;
  box-shadow: 0 12px 24px rgba(16, 24, 39, 0.18);
}

.btn-oauth:hover:not(:disabled) {
  background: #0b1220;
  box-shadow: 0 16px 32px rgba(16, 24, 39, 0.22);
}

.content {
  padding: 18px 26px 26px;
}

.storage-panel,
.overview-panel,
.account-toolbar,
.account-table-wrap,
.compact-row,
.card {
  border-color: rgba(219, 228, 238, 0.96);
  box-shadow: var(--shadow-sm);
}

.storage-panel {
  padding: 12px;
  border-radius: var(--radius);
  background: rgba(255, 255, 255, 0.78);
  backdrop-filter: blur(14px);
}

@media (min-width: 900px) {
  .storage-panel {
    display: grid;
    grid-template-columns: minmax(260px, 340px) minmax(0, 1fr);
    align-items: start;
    gap: 12px;
  }

  .storage-bar {
    grid-column: 1;
    grid-row: 1;
    height: 100%;
    justify-content: space-between;
  }

  .storage-details {
    grid-column: 1 / -1;
    grid-row: 2;
  }

  .codex-proxy-panel {
    grid-column: 2;
    grid-row: 1;
  }

  .codex-feature-panel {
    grid-column: 1 / -1;
  }
}

.storage-bar {
  align-items: stretch;
  flex-direction: column;
  justify-content: flex-start;
  min-height: 0;
  padding: 10px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: linear-gradient(180deg, #ffffff 0%, var(--surface-soft) 100%);
}

.storage-summary {
  align-items: flex-start;
}

.storage-icon {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-sm);
  background: var(--accent-light);
  color: #0f766e;
}

.storage-title {
  font-size: 13px;
  font-weight: 850;
}

.storage-summary span {
  max-width: 240px;
  margin-top: 2px;
  line-height: 1.45;
  white-space: normal;
}

.storage-actions {
  justify-content: flex-start;
  flex-wrap: wrap;
  margin-top: 18px;
}

.btn-storage,
.btn-storage-primary,
.btn-storage-warning,
.codex-feature-head button,
.codex-feature-actions button,
.path-row button {
  min-height: 30px;
  border-radius: var(--radius-sm);
  font-weight: 850;
}

.btn-storage-primary {
  border-color: rgba(79, 99, 232, 0.2);
  background: #fff;
  color: var(--primary);
}

.tools-menu {
  min-width: 210px;
  border-color: rgba(219, 228, 238, 0.92);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-lg);
}

.tools-menu button {
  min-height: 32px;
  font-weight: 750;
}

.codex-proxy-panel,
.codex-feature-panel {
  margin-top: 0;
  padding: 12px;
  border-color: rgba(20, 184, 166, 0.22);
  border-radius: var(--radius-sm);
  background: linear-gradient(135deg, #ffffff 0%, #f6fffb 100%);
}

.codex-feature-panel {
  margin-top: 10px;
  border-color: var(--border);
  background: var(--surface-soft);
}

.codex-feature-head {
  min-height: 30px;
}

.codex-feature-head strong {
  font-size: 14px;
  font-weight: 900;
}

.codex-feature-head span {
  color: #0f766e;
  font-weight: 850;
}

.codex-feature-grid {
  gap: 7px;
}

.codex-proxy-grid {
  grid-template-columns: 0.7fr 1fr 1.15fr;
}

.codex-feature-item {
  min-height: 70px;
  padding: 10px;
  border-color: rgba(219, 228, 238, 0.82);
  background: rgba(255, 255, 255, 0.86);
}

.codex-feature-item span,
.codex-feature-item small {
  font-weight: 750;
}

.codex-feature-item strong {
  color: #0f9f75;
  font-size: 15px;
  font-weight: 900;
}

.codex-feature-item.warn strong {
  color: #b45309;
}

.codex-proxy-actions {
  display: grid;
  grid-template-columns: minmax(220px, 1fr) repeat(3, auto);
  align-items: center;
  gap: 8px;
}

.codex-proxy-actions select,
.toolbar-selects select {
  height: 34px;
  border-color: var(--border);
  background-color: #fff;
  background-image: var(--select-caret);
  background-repeat: no-repeat;
  background-position: right 10px center;
  background-size: 12px 12px;
}

.codex-proxy-actions button:nth-of-type(1) {
  border-color: rgba(20, 184, 166, 0.28);
  background: var(--accent-light);
  color: #0f766e;
}

.codex-feature-issues span {
  max-width: 100%;
  border-radius: var(--radius-sm);
}

.overview-panel {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 226px;
  gap: 12px;
  margin-bottom: 12px;
}

.stat-grid {
  gap: 10px;
}

.stat-item {
  position: relative;
  min-height: 94px;
  padding: 13px 14px;
  overflow: hidden;
  border-color: rgba(219, 228, 238, 0.96);
  border-radius: var(--radius-sm);
  background: rgba(255, 255, 255, 0.9);
}

.stat-item::before {
  content: "";
  position: absolute;
  inset: 0 auto 0 0;
  width: 3px;
  background: var(--primary);
  opacity: 0.72;
}

.stat-item:nth-child(2)::before {
  background: var(--accent);
}

.stat-item:nth-child(3)::before {
  background: #10b981;
}

.stat-warn::before {
  background: var(--danger);
}

.stat-label {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 900;
}

.stat-item strong {
  margin-top: 6px;
  font-size: 20px;
  font-weight: 900;
}

.stat-item small {
  font-weight: 700;
}

.overview-controls {
  gap: 8px;
}

.segmented {
  min-height: 42px;
  padding: 4px;
  border-color: rgba(219, 228, 238, 0.96);
  background: rgba(255, 255, 255, 0.62);
  box-shadow: var(--shadow-xs);
}

.segmented button {
  min-height: 32px;
  border-radius: 6px;
  font-weight: 900;
}

.segmented button.active {
  color: var(--primary);
  box-shadow: 0 1px 2px rgba(16, 24, 39, 0.05), 0 8px 16px rgba(79, 99, 232, 0.08);
}

.account-toolbar {
  position: sticky;
  top: 0;
  z-index: 25;
  margin-bottom: 12px;
  padding: 9px;
  border-radius: var(--radius-sm);
  background: rgba(255, 255, 255, 0.86);
  backdrop-filter: blur(14px);
}

.toolbar-main {
  gap: 9px;
}

.account-search {
  height: 38px;
  border-color: rgba(219, 228, 238, 0.96);
  background: var(--surface-soft);
}

.account-search input {
  height: 36px;
  font-weight: 700;
}

.toolbar-selects label span {
  color: var(--text-tertiary);
}

.btn-toolbar,
.btn-toolbar-icon {
  height: 36px;
  border-color: var(--border);
  background: #fff;
}

.btn-toolbar:hover:not(:disabled),
.btn-toolbar-icon:hover:not(:disabled),
.codex-feature-head button:hover:not(:disabled),
.codex-feature-actions button:hover:not(:disabled) {
  border-color: rgba(79, 99, 232, 0.28);
  background: var(--primary-light);
  color: var(--primary);
}

.btn-usage-toggle.active {
  border-color: rgba(20, 184, 166, 0.28);
  background: var(--accent-light);
  color: #0f766e;
}

.toolbar-result {
  padding: 0 4px;
  font-weight: 850;
}

.batch-failures {
  box-shadow: var(--shadow-xs);
}

.detail-backdrop,
.confirm-backdrop,
.oauth-backdrop,
.import-backdrop {
  background: rgba(16, 24, 39, 0.28);
  backdrop-filter: blur(10px);
}

.detail-drawer,
.usage-drawer,
.log-drawer,
.confirm-dialog,
.oauth-dialog,
.import-dialog {
  border-color: rgba(219, 228, 238, 0.92);
  box-shadow: var(--shadow-xl);
}

.detail-body,
.usage-drawer-body,
.log-body {
  background: var(--surface-soft);
}

.toast {
  top: 16px;
  border-radius: var(--radius-sm);
  font-weight: 750;
}

@media (max-width: 899px) {
  .storage-panel,
  .overview-panel {
    display: block;
  }

  .storage-bar {
    min-height: auto;
  }

  .codex-proxy-panel {
    margin-top: 10px;
  }

  .overview-controls {
    flex-direction: row;
    margin-top: 10px;
  }
}

@media (max-width: 760px) {
  .content {
    padding: 14px;
  }

  .header-inner {
    padding: 12px 16px;
  }

  .header-right {
    gap: 8px;
  }

  .codex-proxy-actions {
    grid-template-columns: 1fr;
  }

  .codex-proxy-actions select,
  .codex-proxy-actions button {
    width: 100%;
    max-width: 100%;
    min-width: 0;
    justify-self: stretch;
  }

  .codex-proxy-grid,
  .codex-feature-grid {
    grid-template-columns: 1fr;
  }

  .overview-controls {
    flex-direction: column;
  }

  .account-toolbar {
    top: 0;
  }
}

.codex-proxy-strip {
  position: relative;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
  padding: 9px 12px;
  border: 1px solid rgba(219, 228, 238, 0.96);
  border-radius: var(--radius-sm);
  background: rgba(255, 255, 255, 0.86);
  box-shadow: var(--shadow-sm);
  backdrop-filter: blur(14px);
}

.proxy-strip-main {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 10px;
}

.proxy-status-dot {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: #cbd5e1;
  box-shadow: 0 0 0 4px rgba(148, 163, 184, 0.14);
  flex-shrink: 0;
}

.proxy-status-dot.active {
  background: #10b981;
  box-shadow: 0 0 0 4px rgba(16, 185, 129, 0.14);
}

.proxy-status-dot.partial,
.proxy-status-dot.warn {
  background: #f59e0b;
  box-shadow: 0 0 0 4px rgba(245, 158, 11, 0.16);
}

.proxy-strip-copy {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.proxy-strip-copy strong {
  color: var(--text);
  font-size: 13px;
  font-weight: 900;
  white-space: nowrap;
}

.proxy-strip-copy span {
  padding: 3px 8px;
  border: 1px solid rgba(20, 184, 166, 0.2);
  border-radius: 999px;
  background: var(--accent-light);
  color: #0f766e;
  font-size: 11px;
  font-weight: 850;
  white-space: nowrap;
}

.proxy-strip-controls {
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  flex: 1;
}

.proxy-strip-controls select {
  min-width: 180px;
  max-width: 320px;
  height: 36px;
  padding: 0 30px 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  appearance: none;
  background-color: #fff;
  background-image: var(--select-caret);
  background-repeat: no-repeat;
  background-position: right 10px center;
  background-size: 12px 12px;
  color: var(--text);
  font-size: 12px;
  font-weight: 800;
}

.proxy-strip-controls select:disabled {
  opacity: 0.56;
}

.proxy-refresh:disabled,
.proxy-switch:disabled,
.btn-migration-warning:disabled {
  opacity: 0.55;
  cursor: wait;
}

.proxy-switch {
  height: 36px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px 0 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #fff;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 900;
  cursor: pointer;
  transition: all 0.15s var(--ease-out);
}

.proxy-switch:hover:not(:disabled) {
  border-color: rgba(20, 184, 166, 0.28);
  background: var(--accent-light);
  color: #0f766e;
}

.proxy-switch-track {
  position: relative;
  width: 34px;
  height: 18px;
  border-radius: 999px;
  background: #cbd5e1;
  transition: background 0.15s var(--ease-out);
  flex-shrink: 0;
}

.proxy-switch-thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 12px;
  height: 12px;
  border-radius: 999px;
  background: #fff;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.26);
  transition: transform 0.15s var(--ease-out);
}

.proxy-switch.active {
  border-color: rgba(20, 184, 166, 0.28);
  background: var(--accent-light);
  color: #0f766e;
}

.proxy-switch.active .proxy-switch-track {
  background: #14b8a6;
}

.proxy-switch.active .proxy-switch-thumb {
  transform: translateX(16px);
}

.proxy-error-strip,
.storage-details-panel {
  margin: -4px 0 12px;
  border: 1px solid rgba(245, 158, 11, 0.22);
  border-radius: var(--radius-sm);
  background: #fffbeb;
  color: #92400e;
  box-shadow: var(--shadow-xs);
}

.proxy-error-strip {
  padding: 8px 10px;
  font-size: 12px;
  font-weight: 750;
}

.storage-details-panel {
  padding: 10px;
  border-color: rgba(219, 228, 238, 0.96);
  background: rgba(255, 255, 255, 0.84);
}

.storage-details-panel .storage-details {
  display: grid;
  gap: 8px;
  margin-top: 0;
  padding-top: 0;
  border-top: 0;
}

.account-toolbar {
  overflow: visible;
  flex-wrap: wrap;
}

.toolbar-main {
  flex: 1 1 520px;
  min-width: min(100%, 520px);
}

.toolbar-actions {
  position: relative;
  margin-left: auto;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.toolbar-tools {
  z-index: 10;
  flex-shrink: 0;
}

.toolbar-tools .tools-menu {
  right: 0;
  left: auto;
  z-index: 1000;
  max-height: 300px;
  overflow-y: auto;
}

.btn-migration-warning {
  border-color: rgba(245, 158, 11, 0.28);
  background: #fffbeb;
  color: #b45309;
}

.btn-tools {
  border-color: rgba(79, 99, 232, 0.2);
  color: var(--primary);
}

@media (max-width: 760px) {
  .codex-proxy-strip {
    align-items: stretch;
    flex-direction: column;
  }

  .proxy-strip-controls {
    width: 100%;
  }

  .proxy-strip-controls select {
    max-width: none;
    flex: 1;
  }
}

@media (max-width: 760px) {
  .codex-proxy-strip {
    padding: 10px;
  }

  .proxy-strip-main {
    flex-wrap: wrap;
  }

  .proxy-strip-copy {
    flex-wrap: wrap;
  }

  .proxy-strip-controls {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 36px auto;
  }

  .proxy-strip-controls select,
  .proxy-refresh,
  .proxy-switch {
    width: 100%;
  }

  .toolbar-main {
    width: 100%;
    min-width: 0;
    flex: 0 0 auto;
  }

  .account-search {
    flex: 0 0 auto;
    min-width: 0;
  }

  .toolbar-actions {
    width: 100%;
    margin-left: 0;
    align-items: stretch;
    justify-content: flex-start;
    flex-wrap: nowrap;
  }

  .toolbar-tools,
  .toolbar-tools .btn-tools {
    width: 100%;
  }

  .toolbar-tools .tools-menu {
    right: 0;
    left: 0;
    min-width: 0;
    width: 100%;
    max-height: 260px;
  }
}

/* ── macOS polish pass ────────────────── */
.app {
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.62) 0%, rgba(246, 248, 251, 0.94) 120px),
    var(--bg);
  color: var(--text);
}

.header {
  border-bottom-color: var(--border);
  background: rgba(255, 255, 255, 0.92);
  box-shadow: 0 1px 0 rgba(255, 255, 255, 0.75) inset;
  backdrop-filter: blur(18px);
}

.header-bg {
  height: 1px;
  background: linear-gradient(90deg, #4f6bff 0%, #20b26b 58%, #e36a5d 100%);
  opacity: 0.84;
}

.header-inner {
  padding: 12px 24px;
}

.logo {
  width: 42px;
  height: 42px;
  border: 1px solid rgba(216, 224, 235, 0.9);
  border-radius: 14px;
  box-shadow: 0 1px 2px rgba(23, 32, 51, 0.06), 0 8px 18px rgba(79, 107, 255, 0.1);
}

.header-text h1 {
  color: var(--text);
  font-size: 18px;
  font-weight: 850;
  letter-spacing: 0;
}

.header-count {
  color: #6e7b91;
  font-size: 12px;
  font-weight: 650;
}

.header-right {
  gap: 10px;
}

.interval-wrap {
  height: 36px;
  padding: 0 9px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #fff;
}

.interval-wrap select {
  height: 34px;
  padding: 0 24px 0 4px;
  border: 0;
  background-color: transparent;
  color: var(--text);
  font-size: 13px;
  font-weight: 800;
  box-shadow: none;
}

.interval-wrap select:focus {
  box-shadow: none;
}

.restart-toggle {
  height: 36px;
  padding: 0 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #fff;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 750;
}

.btn-add {
  min-height: 36px;
  padding: 0 14px;
  border-radius: var(--radius-sm);
  background: var(--primary);
  font-size: 13px;
  font-weight: 800;
  box-shadow: 0 1px 2px rgba(23, 32, 51, 0.08), 0 10px 20px rgba(79, 107, 255, 0.16);
}

.btn-add:hover:not(:disabled) {
  background: var(--primary-hover);
  box-shadow: 0 1px 2px rgba(23, 32, 51, 0.08), 0 14px 26px rgba(79, 107, 255, 0.2);
}

.btn-oauth {
  background: #111827;
  box-shadow: 0 1px 2px rgba(23, 32, 51, 0.14), 0 10px 20px rgba(17, 24, 39, 0.18);
}

.content {
  padding: 18px 24px 24px;
}

.codex-proxy-strip,
.overview-panel,
.account-toolbar,
.storage-details-panel {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: rgba(255, 255, 255, 0.94);
  box-shadow: var(--shadow-sm);
  backdrop-filter: blur(16px);
}

.codex-proxy-strip {
  min-height: 54px;
  margin-bottom: 10px;
  padding: 8px 10px;
}

.proxy-status-dot {
  width: 9px;
  height: 9px;
  box-shadow: 0 0 0 4px rgba(149, 161, 179, 0.12);
}

.proxy-status-dot.active {
  background: var(--success);
  box-shadow: 0 0 0 4px rgba(32, 178, 107, 0.14);
}

.proxy-strip-copy strong {
  font-size: 13px;
  font-weight: 850;
}

.proxy-strip-copy span {
  padding: 3px 8px;
  border-color: rgba(32, 178, 107, 0.18);
  background: var(--accent-light);
  color: #15865a;
  font-weight: 800;
}

.proxy-url-chip {
  max-width: 220px;
  overflow: hidden;
  height: 26px;
  padding: 0 9px;
  border: 1px solid var(--border-light);
  border-radius: 999px;
  background: #f8fafc;
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 800;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
}

.proxy-url-chip:hover {
  border-color: rgba(79, 107, 255, 0.22);
  background: var(--primary-light);
  color: var(--primary);
}

.proxy-strip-controls select,
.toolbar-selects select,
.account-search,
.btn-toolbar,
.btn-toolbar-icon,
.proxy-refresh,
.proxy-switch {
  height: 36px;
  border-color: var(--border);
  border-radius: var(--radius-sm);
  background-color: #fff;
}

.proxy-strip-controls select,
.toolbar-selects select {
  color: var(--text);
  font-size: 12px;
  font-weight: 780;
}

.proxy-switch {
  padding-right: 11px;
  color: var(--text-secondary);
}

.proxy-switch.active {
  border-color: rgba(32, 178, 107, 0.22);
  background: #f8fffb;
  color: #15865a;
}

.proxy-switch-track {
  width: 34px;
  height: 18px;
  background: #c9d3df;
}

.proxy-switch.active .proxy-switch-track {
  background: var(--success);
}

.overview-panel {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 224px;
  gap: 0;
  margin-bottom: 10px;
  overflow: hidden;
}

.stat-grid {
  gap: 0;
}

.stat-item {
  min-height: 92px;
  padding: 13px 16px;
  border: 0;
  border-right: 1px solid var(--border-light);
  border-radius: 0;
  background: transparent;
  box-shadow: none;
}

.stat-item:last-child {
  border-right: 0;
}

.stat-item::before {
  inset: 10px auto 10px 0;
  width: 2px;
  border-radius: 999px;
  opacity: 0;
}

.stat-item:first-child::before,
.stat-warn::before {
  opacity: 1;
}

.stat-item:nth-child(2)::before,
.stat-item:nth-child(3)::before {
  opacity: 0;
}

.stat-label {
  color: #8b98aa;
  font-size: 11px;
  font-weight: 850;
}

.stat-item strong {
  margin-top: 7px;
  color: var(--text);
  font-size: 22px;
  line-height: 1.08;
  font-weight: 850;
}

.stat-item small {
  margin-top: 5px;
  color: #6e7b91;
  font-size: 11px;
  font-weight: 650;
}

.stat-warn strong {
  color: var(--danger);
}

.overview-controls {
  justify-content: center;
  gap: 8px;
  padding: 10px;
  border-left: 1px solid var(--border-light);
  background: #fbfdff;
}

.segmented {
  min-height: 42px;
  padding: 3px;
  border-color: var(--border-light);
  border-radius: var(--radius-sm);
  background: #f7f9fc;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.85);
}

.segmented button {
  min-height: 34px;
  border-radius: 7px;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 800;
}

.segmented button.active {
  background: #fff;
  color: var(--primary);
  box-shadow: 0 1px 2px rgba(23, 32, 51, 0.06), 0 8px 18px rgba(79, 107, 255, 0.08);
}

.account-toolbar {
  margin-bottom: 10px;
  padding: 9px;
}

.toolbar-main {
  gap: 8px;
}

.account-search {
  padding: 0 11px;
  background: #fff;
  color: #9aa6b7;
}

.account-search input {
  height: 34px;
  color: var(--text);
  font-size: 13px;
  font-weight: 650;
}

.toolbar-selects label {
  gap: 6px;
}

.toolbar-selects label span {
  color: var(--text-secondary);
  font-weight: 750;
}

.btn-toolbar,
.btn-toolbar-icon {
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 780;
  box-shadow: 0 1px 1px rgba(23, 32, 51, 0.025);
}

.btn-toolbar:hover:not(:disabled),
.btn-toolbar-icon:hover:not(:disabled) {
  border-color: rgba(79, 107, 255, 0.28);
  background: var(--primary-light);
  color: var(--primary);
}

.toolbar-result {
  color: #6e7b91;
  font-size: 12px;
  font-weight: 780;
}

.btn-usage-toggle.active {
  border-color: rgba(79, 107, 255, 0.2);
  background: var(--primary-light);
  color: var(--primary);
}

.btn-tools {
  border-color: rgba(79, 107, 255, 0.18);
  background: #f8faff;
  color: var(--primary);
}

.tools-menu {
  padding: 6px;
  border-color: var(--border);
  border-radius: 12px;
  background: #fff;
  box-shadow: 0 18px 42px rgba(23, 32, 51, 0.16), 0 1px 2px rgba(23, 32, 51, 0.08);
}

.tools-menu button {
  min-height: 32px;
  padding: 0 10px;
  border-radius: 8px;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 750;
}

.tools-menu button:hover {
  background: var(--surface-soft);
  color: var(--text);
}

@media (max-width: 980px) {
  .overview-panel {
    grid-template-columns: 1fr;
  }

  .overview-controls {
    border-top: 1px solid var(--border-light);
    border-left: 0;
    flex-direction: row;
  }
}

@media (max-width: 760px) {
  .header-inner {
    padding: 12px 16px;
  }

  .header-right {
    gap: 8px;
  }

  .interval-wrap,
  .restart-toggle,
  .btn-add {
    width: 100%;
    justify-content: center;
  }

  .content {
    padding: 14px;
  }

  .codex-proxy-strip {
    border-radius: var(--radius);
  }

  .stat-grid {
    grid-template-columns: 1fr;
  }

  .stat-item {
    min-height: 86px;
    border-right: 0;
    border-bottom: 1px solid var(--border-light);
  }

  .stat-item:last-child {
    border-bottom: 0;
  }

  .overview-controls {
    flex-direction: column;
  }
}

/* ── Refined dashboard controls ───────── */
.speed-switch.active {
  border-color: rgba(79, 107, 255, 0.26);
  background: var(--primary-light);
  color: var(--primary);
}

.speed-switch.active .proxy-switch-track {
  background: var(--primary);
}

.view-mode-actions {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #f7f9fc;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.86);
}

.view-mode-button {
  width: 32px;
  height: 30px;
  border-color: transparent;
  background: transparent;
}

.view-mode-button.active {
  border-color: rgba(79, 107, 255, 0.2);
  background: #fff;
  color: var(--primary);
  box-shadow: 0 1px 2px rgba(23, 32, 51, 0.06), 0 8px 18px rgba(79, 107, 255, 0.08);
}

.overview-panel {
  display: block;
  margin-bottom: 10px;
  overflow: visible;
  border: 0;
  border-radius: 0;
  background: transparent;
  box-shadow: none;
  backdrop-filter: none;
}

.stat-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}

.stat-item {
  position: relative;
  display: flex;
  align-items: center;
  min-width: 0;
  min-height: 78px;
  gap: 11px;
  overflow: hidden;
  padding: 11px 13px;
  border: 1px solid rgba(219, 228, 238, 0.86);
  border-radius: var(--radius-sm);
  background: #fff;
  box-shadow: 0 1px 2px rgba(23, 32, 51, 0.045), 0 16px 36px rgba(23, 32, 51, 0.055);
}

.stat-item::before {
  display: none;
}

.stat-item::after {
  content: "";
  position: absolute;
  right: -28px;
  bottom: -34px;
  width: 118px;
  height: 76px;
  border-radius: 999px;
  background: currentColor;
  opacity: 0.075;
}

.stat-current {
  color: var(--primary);
}

.stat-count {
  color: var(--accent);
}

.stat-quota {
  color: #f97316;
}

.stat-warn {
  color: #f43f5e;
}

.stat-icon {
  position: relative;
  z-index: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 42px;
  height: 42px;
  flex: 0 0 42px;
  border-radius: 12px;
  background: var(--primary);
  color: #fff;
  box-shadow: 0 12px 22px rgba(23, 32, 51, 0.14);
}

.stat-count .stat-icon {
  background: var(--accent);
}

.stat-quota .stat-icon {
  background: #f97316;
}

.stat-warn .stat-icon {
  background: #f43f5e;
}

.stat-content {
  position: relative;
  z-index: 1;
  min-width: 0;
  flex: 1;
  padding-right: 58px;
}

.stat-label {
  display: block;
  color: #46546a;
  font-size: 11px;
  font-weight: 900;
}

.stat-item strong {
  display: block;
  overflow: hidden;
  margin-top: 3px;
  color: currentColor;
  font-size: 21px;
  line-height: 1;
  font-weight: 950;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.stat-current strong {
  color: var(--text);
  font-size: 16px;
  line-height: 1.16;
}

.stat-item small {
  display: block;
  overflow: hidden;
  margin-top: 4px;
  color: #7d8aa0;
  font-size: 11px;
  font-weight: 750;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.stat-sparkline {
  position: absolute;
  right: 12px;
  bottom: 10px;
  width: 70px;
  height: 20px;
  color: currentColor;
  opacity: 0.68;
  z-index: 1;
}

.stat-sparkline polyline {
  fill: none;
  stroke: currentColor;
  stroke-width: 2.6;
  stroke-linecap: round;
  stroke-linejoin: round;
}

@media (max-width: 1180px) {
  .stat-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 760px) {
  .header-inner {
    align-items: stretch;
    flex-direction: column;
  }

  .header-right {
    width: 100%;
    min-width: 0;
  }

  .content {
    overflow-x: hidden;
  }

  .proxy-strip-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: stretch;
    justify-content: flex-start;
  }

  .proxy-strip-controls select {
    flex: 1 1 calc(100% - 44px);
    min-width: 0;
    max-width: none;
    width: auto;
  }

  .proxy-refresh {
    flex: 0 0 36px;
    width: 36px;
  }

  .proxy-switch {
    flex: 1 1 calc(50% - 4px);
    min-width: 0;
    justify-content: center;
  }

  .stat-grid {
    grid-template-columns: 1fr;
  }

  .stat-item {
    min-height: 86px;
    border-bottom: 1px solid rgba(219, 228, 238, 0.86);
  }

  .stat-content {
    padding-right: 58px;
  }

  .account-toolbar {
    align-items: stretch;
    flex-direction: column;
  }

  .toolbar-main,
  .toolbar-actions {
    width: 100%;
    min-width: 0;
    flex: 0 0 auto;
  }

  .toolbar-main {
    align-items: stretch;
    flex-direction: column;
  }

  .toolbar-selects {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 36px;
    width: 100%;
  }

  .toolbar-selects label,
  .toolbar-selects select {
    min-width: 0;
    width: 100%;
  }

  .toolbar-actions {
    flex-wrap: wrap;
    justify-content: flex-start;
  }

  .btn-usage-toggle {
    flex: 1 1 calc(50% - 4px);
  }

  .toolbar-actions > .btn-toolbar:not(.btn-usage-toggle):not(.btn-tools) {
    flex: 1 1 100%;
    justify-content: center;
  }

  .view-mode-actions {
    width: 100%;
    justify-content: stretch;
  }

  .view-mode-button {
    flex: 1;
    width: auto;
  }
}
</style>
