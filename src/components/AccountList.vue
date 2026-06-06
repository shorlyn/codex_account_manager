<script setup lang="ts">
import type { Account } from '../types';

const props = defineProps<{
  accounts: Account[];
  loading: boolean;
  switchingId: number | null;
  currentAccountId: string | null;
}>();

const emit = defineEmits<{
  run: [id: number];
  edit: [account: Account];
  delete: [id: number];
  refresh: [id: number];
}>();

function formatResetTime(timestamp: number): string {
  if (!timestamp) return '-';
  const d = new Date(timestamp * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function remaining(used: number): number {
  return Math.max(0, 100 - used);
}

function quotaColor(used: number): string {
  const rem = 100 - used;
  if (rem <= 20) return '#ef4444';
  if (rem <= 50) return '#f59e0b';
  return '#10b981';
}

function quotaGradient(used: number): string {
  const rem = 100 - used;
  if (rem <= 20) return 'linear-gradient(90deg, #ef4444, #f87171)';
  if (rem <= 50) return 'linear-gradient(90deg, #f59e0b, #fbbf24)';
  return 'linear-gradient(90deg, #10b981, #34d399)';
}

function getPlanClass(t: string): string {
  const v = t.toLowerCase();
  if (v === 'pro') return 'plan-pro';
  if (v === 'plus') return 'plan-plus';
  return 'plan-free';
}

function isCurrent(account: Account): boolean {
  if (!props.currentAccountId) return false;
  try {
    const parsed = JSON.parse(account.json_info);
    return parsed.tokens?.account_id === props.currentAccountId;
  } catch {
    return false;
  }
}
</script>

<template>
  <div class="account-list">
    <!-- Loading -->
    <div v-if="loading && accounts.length === 0" class="empty-state">
      <div class="spinner"></div>
      <p>加载中...</p>
    </div>

    <!-- Empty -->
    <div v-else-if="accounts.length === 0" class="empty-state">
      <div class="empty-illustration">
        <div class="empty-circle">
          <div class="empty-circle-inner">
            <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/>
              <circle cx="9" cy="7" r="4"/>
              <line x1="19" y1="8" x2="19" y2="14"/>
              <line x1="22" y1="11" x2="16" y2="11"/>
            </svg>
          </div>
        </div>
        <div class="empty-dots">
          <span></span><span></span><span></span>
        </div>
      </div>
      <h3>还没有账号</h3>
      <p>点击右上角「添加账号」开始管理</p>
    </div>

    <!-- Cards -->
    <div v-else class="card-grid">
      <div v-for="account in accounts" :key="account.id" :class="['card', { 'card-active': isCurrent(account) }]">
        <!-- Top accent -->
        <div class="card-accent"></div>

        <!-- Current indicator -->
        <div v-if="isCurrent(account)" class="active-indicator">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="20 6 9 17 4 12"/>
          </svg>
          当前使用
        </div>

        <!-- Header -->
        <div class="card-header">
          <div class="card-title-row">
            <h3>{{ account.name }}</h3>
            <span :class="['badge', getPlanClass(account.plan_type)]">
              {{ account.plan_type || '—' }}
            </span>
          </div>
          <span v-if="account.activation_date" class="card-date">{{ account.activation_date }}</span>
        </div>

        <!-- Body -->
        <div class="card-body">
          <!-- 5h -->
          <div class="quota-block">
            <div class="quota-head">
              <span class="quota-label">5 小时额度</span>
              <span class="quota-pct" :style="{ color: quotaColor(account.primary_used_percent) }">
                {{ remaining(account.primary_used_percent) }}%
              </span>
            </div>
            <div class="bar-track">
              <div
                class="bar-fill"
                :style="{
                  width: remaining(account.primary_used_percent) + '%',
                  background: quotaGradient(account.primary_used_percent)
                }"
              >
                <div class="bar-glow"></div>
              </div>
            </div>
            <span class="quota-reset">刷新 {{ formatResetTime(account.primary_reset_at) }}</span>
          </div>

          <!-- Weekly -->
          <div class="quota-block">
            <div class="quota-head">
              <span class="quota-label">周额度</span>
              <span class="quota-pct" :style="{ color: quotaColor(account.secondary_used_percent) }">
                {{ remaining(account.secondary_used_percent) }}%
              </span>
            </div>
            <div class="bar-track">
              <div
                class="bar-fill"
                :style="{
                  width: remaining(account.secondary_used_percent) + '%',
                  background: quotaGradient(account.secondary_used_percent)
                }"
              >
                <div class="bar-glow"></div>
              </div>
            </div>
            <span class="quota-reset">刷新 {{ formatResetTime(account.secondary_reset_at) }}</span>
          </div>
        </div>

        <!-- Footer -->
        <div class="card-footer">
          <button
            class="btn-run"
            :disabled="switchingId === account.id || !account.json_info"
            :title="!account.json_info ? 'JSON 为空' : '切换账号'"
            @click="emit('run', account.id)"
          >
            <svg v-if="switchingId === account.id" class="spin" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
            </svg>
            <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
              <polygon points="6 3 20 12 6 21 6 3"/>
            </svg>
            <span>{{ switchingId === account.id ? '切换中' : '运行' }}</span>
          </button>
          <div class="btn-group">
            <button class="btn-icon btn-refresh" title="刷新额度" @click="emit('refresh', account.id)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
              </svg>
            </button>
            <button class="btn-icon btn-edit" title="编辑" @click="emit('edit', account)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
              </svg>
            </button>
            <button class="btn-icon btn-delete" title="删除" @click="emit('delete', account.id)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.account-list { width: 100%; }

/* ── Grid ─────────────────────────────── */
.card-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 18px;
}

/* ── Card ─────────────────────────────── */
.card {
  position: relative;
  background: var(--surface);
  border-radius: var(--radius-lg);
  border: 1px solid var(--border);
  overflow: hidden;
  transition: all 0.3s var(--ease-out);
  box-shadow: var(--shadow-sm);
}

.card:hover {
  border-color: #c7d2fe;
  box-shadow: var(--shadow-md), 0 0 0 1px rgba(99, 102, 241, 0.06);
  transform: translateY(-2px);
}

/* Current active card */
.card-active {
  border-color: #a7f3d0;
  box-shadow: var(--shadow-md), 0 0 0 1px rgba(16, 185, 129, 0.15), 0 0 20px rgba(16, 185, 129, 0.06);
}

.card-active:hover {
  border-color: #6ee7b7;
  box-shadow: var(--shadow-md), 0 0 0 1px rgba(16, 185, 129, 0.2), 0 0 24px rgba(16, 185, 129, 0.1);
}

.card-active .card-accent {
  background: linear-gradient(90deg, #10b981, #34d399);
  opacity: 1;
}

.active-indicator {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 6px 14px;
  background: linear-gradient(90deg, #ecfdf5, #d1fae5);
  color: #059669;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.02em;
}

.card-accent {
  height: 3px;
  background: linear-gradient(90deg, var(--primary), var(--accent));
  opacity: 0;
  transition: opacity 0.3s var(--ease-out);
}

.card:hover .card-accent {
  opacity: 1;
}

/* ── Card header ──────────────────────── */
.card-header {
  padding: 18px 20px 0;
}

.card-title-row {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.card-title-row h3 {
  font-size: 15px;
  font-weight: 600;
  color: var(--text);
  margin: 0;
  letter-spacing: -0.01em;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  flex: 1;
}

.card-date {
  display: block;
  font-size: 12px;
  color: var(--text-tertiary);
  margin-top: 3px;
}

/* ── Badge ────────────────────────────── */
.badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 10px;
  border-radius: 20px;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  flex-shrink: 0;
}

.plan-free {
  background: #f1f5f9;
  color: #64748b;
}

.plan-plus {
  background: #eff6ff;
  color: #3b82f6;
}

.plan-pro {
  background: #faf5ff;
  color: #9333ea;
}

/* ── Card body ────────────────────────── */
.card-body {
  padding: 16px 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.quota-block {
  display: flex;
  flex-direction: column;
  gap: 7px;
}

.quota-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}

.quota-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.quota-pct {
  font-size: 22px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  line-height: 1;
  letter-spacing: -0.02em;
}

/* ── Progress bar ─────────────────────── */
.bar-track {
  height: 7px;
  background: var(--border-light);
  border-radius: 4px;
  overflow: hidden;
}

.bar-fill {
  position: relative;
  height: 100%;
  border-radius: 4px;
  transition: width 0.6s var(--ease-out);
  min-width: 2px;
}

.bar-glow {
  position: absolute;
  inset: 0;
  border-radius: 4px;
  background: linear-gradient(180deg, rgba(255,255,255,0.35) 0%, transparent 100%);
}

.quota-reset {
  font-size: 11px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
}

/* ── Card footer ──────────────────────── */
.card-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--border-light);
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: linear-gradient(180deg, #fafbfc 0%, #f8fafc 100%);
}

.btn-group {
  display: flex;
  gap: 6px;
}

/* ── Buttons ──────────────────────────── */
.btn-run {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 8px 18px;
  background: linear-gradient(135deg, var(--primary), var(--primary-hover));
  color: #fff;
  border: none;
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s var(--ease-out);
  box-shadow: 0 2px 6px rgba(99, 102, 241, 0.2);
}

.btn-run:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.3);
}

.btn-run:active:not(:disabled) {
  transform: translateY(0);
}

.btn-run:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.btn-icon {
  width: 34px;
  height: 34px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all 0.2s var(--ease-out);
}

.btn-icon:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.btn-refresh { color: var(--primary); }
.btn-refresh:hover:not(:disabled) {
  background: var(--primary-light);
  border-color: #c7d2fe;
  color: var(--primary-hover);
}

.btn-edit { color: var(--warning); }
.btn-edit:hover:not(:disabled) {
  background: var(--warning-light);
  border-color: #fde68a;
  color: #d97706;
}

.btn-delete { color: var(--danger); }
.btn-delete:hover:not(:disabled) {
  background: var(--danger-light);
  border-color: #fecaca;
  color: #dc2626;
}

/* ── Empty state ──────────────────────── */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 100px 20px;
  text-align: center;
}

.empty-illustration {
  margin-bottom: 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

.empty-circle {
  width: 88px;
  height: 88px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--primary-light), #e0e7ff);
  display: flex;
  align-items: center;
  justify-content: center;
  animation: float 3s ease-in-out infinite;
}

.empty-circle-inner {
  width: 60px;
  height: 60px;
  border-radius: 50%;
  background: var(--surface);
  color: var(--primary);
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: var(--shadow);
}

.empty-dots {
  display: flex;
  gap: 6px;
}

.empty-dots span {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--border);
}

.empty-dots span:nth-child(2) { animation: dot-pulse 1.5s ease-in-out 0.2s infinite; }
.empty-dots span:nth-child(3) { animation: dot-pulse 1.5s ease-in-out 0.4s infinite; }

.empty-state h3 {
  font-size: 18px;
  font-weight: 600;
  color: var(--text);
  margin: 0 0 6px;
}

.empty-state p {
  font-size: 14px;
  color: var(--text-tertiary);
  margin: 0;
}

/* ── Spinner ──────────────────────────── */
.spinner {
  width: 36px;
  height: 36px;
  border: 3px solid var(--border);
  border-top-color: var(--primary);
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

@keyframes float {
  0%, 100% { transform: translateY(0); }
  50% { transform: translateY(-6px); }
}

@keyframes dot-pulse {
  0%, 100% { opacity: 0.3; transform: scale(1); }
  50% { opacity: 1; transform: scale(1.2); }
}

@media (max-width: 980px) {
  .card-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 640px) {
  .card-grid {
    grid-template-columns: 1fr;
  }

  .card-footer {
    align-items: stretch;
    flex-direction: column;
    gap: 10px;
  }

  .btn-run {
    justify-content: center;
  }

  .btn-group {
    justify-content: flex-end;
  }
}
</style>
