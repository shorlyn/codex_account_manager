<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useAccounts } from './composables/useAccounts';
import AccountList from './components/AccountList.vue';
import AccountDialog from './components/AccountDialog.vue';
import type { Account } from './types';

const {
  accounts, loading, switchingId, currentAccountId, refreshInterval,
  loadAccounts, loadCurrentAccount, addAccount, updateAccount, deleteAccount,
  refreshQuota, switchAccount, startAutoRefresh,
} = useAccounts();

const showDialog = ref(false);
const editingAccount = ref<Account | null>(null);
const message = ref('');
const messageType = ref<'success' | 'error'>('success');
let messageTimer: ReturnType<typeof setTimeout> | null = null;

const intervalOptions = [
  { label: '关闭', value: 0 },
  { label: '5 分钟', value: 5 },
  { label: '10 分钟', value: 10 },
  { label: '15 分钟', value: 15 },
  { label: '30 分钟', value: 30 },
];

onMounted(async () => {
  await loadAccounts();
  await loadCurrentAccount();
  startAutoRefresh(10);
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

async function handleSave(data: { name: string; activationDate: string; jsonInfo: string }) {
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
  } catch (e) { showMessage(`保存失败: ${e}`, 'error'); }
}

async function handleRun(id: number) {
  try { await switchAccount(id); showMessage('账号已切换，Codex 已重启'); }
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

function handleIntervalChange(e: Event) {
  startAutoRefresh(Number((e.target as HTMLSelectElement).value));
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
        @save="handleSave"
        @close="closeDialog"
      />
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

/* ── Content ──────────────────────────── */
.content {
  flex: 1;
  padding: 24px 28px;
  overflow-y: auto;
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
</style>
