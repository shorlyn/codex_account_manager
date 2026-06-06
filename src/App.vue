<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useAccounts } from './composables/useAccounts';
import AccountList from './components/AccountList.vue';
import AccountDialog from './components/AccountDialog.vue';
import type { Account, MigrationStatus, StoragePaths } from './types';

const {
  accounts, loading, switchingId, currentAccountId, refreshInterval,
  restartCodexOnSwitch,
  loadAccounts, loadCurrentAccount, addAccount, updateAccount, deleteAccount,
  refreshQuota, switchAccount, loadRefreshInterval, setRefreshInterval,
  loadRestartCodexOnSwitch, setRestartCodexOnSwitch,
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
const showOauthDialog = ref(false);
const oauthLoginId = ref('');
const oauthUrl = ref('');
const oauthCallbackUrl = ref('');
const oauthError = ref('');
const oauthUrlCopied = ref(false);
const oauthTimedOut = ref(false);
const importInput = ref<HTMLInputElement | null>(null);
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
      openBrowser: true,
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

function closeOauthDialog() {
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
  closeOauthDialog();
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
    closeOauthDialog();
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
  try {
    await invoke('open_storage_folder');
    showMessage('已打开账号库目录');
  } catch (e) {
    showMessage(`打开账号库目录失败: ${e}`, 'error');
  }
}

async function openAuthFolder() {
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

async function exportBackup() {
  const password = prompt('请输入备份密码（至少 8 位）。导入时需要同一个密码。');
  if (!password) return;
  try {
    const backupText = await invoke<string>('export_encrypted_backup', { password });
    downloadTextFile(backupFileName(), backupText);
    showMessage('加密备份已导出');
  } catch (e) {
    showMessage(`导出备份失败: ${e}`, 'error');
  }
}

function openImportBackup() {
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
    const count = await invoke<number>('import_encrypted_backup', { backupText, password });
    await loadAccounts();
    await loadMigrationStatus();
    showMessage(`已导入 ${count} 个账号`);
  } catch (err) {
    showMessage(`导入备份失败: ${err}`, 'error');
  }
}

async function migrateOldAccounts() {
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

async function handleIntervalChange(e: Event) {
  await setRefreshInterval(Number((e.target as HTMLSelectElement).value));
}

async function handleRestartToggle(e: Event) {
  await setRestartCodexOnSwitch((e.target as HTMLInputElement).checked);
}
</script>

<template>
  <div class="app">
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
          <label class="restart-toggle" title="关闭后切换账号只写入 auth.json，不会结束或重启 Codex">
            <input type="checkbox" :checked="restartCodexOnSwitch" @change="handleRestartToggle" />
            <span>切换后重启 Codex</span>
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

          <div class="storage-actions">
            <button
              v-if="migrationStatus && migrationStatus.pending_plaintext_accounts > 0"
              class="btn-storage-warning"
              :disabled="migratingAccounts"
              @click="migrateOldAccounts"
            >
              {{ migratingAccounts ? '迁移中...' : `迁移旧账号 (${migrationStatus.pending_plaintext_accounts})` }}
            </button>
            <button class="btn-storage-primary" @click="exportBackup">
              导出加密备份
            </button>
            <button class="btn-storage" @click="openImportBackup">
              导入备份
            </button>
            <input
              ref="importInput"
              class="backup-input"
              type="file"
              accept="application/json,.json"
              @change="importBackup"
            />
            <button class="btn-storage-primary" @click="openStorageFolder">
              打开账号库目录
            </button>
            <button class="btn-storage" @click="openAuthFolder">
              打开当前账号目录
            </button>
            <button class="btn-storage" @click="showStorageDetails = !showStorageDetails">
              {{ showStorageDetails ? '收起' : '详情' }}
            </button>
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

      <AccountList
        :accounts="accounts"
        :loading="loading"
        :switching-id="switchingId"
        :current-account-id="currentAccountId"
        @run="handleRun"
        @edit="openEditDialog"
        @delete="handleDelete"
        @refresh="handleRefresh"
      />
    </main>

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

  .path-row {
    grid-template-columns: 1fr auto;
  }

  .path-label {
    grid-column: 1 / -1;
  }
}
</style>
