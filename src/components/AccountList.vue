<script setup lang="ts">
import type { Account } from '../types';

const props = defineProps<{
  accounts: Account[];
  loading: boolean;
  switchingId: number | null;
  currentAccountRecordId: number | null;
  viewMode: 'cards' | 'compact' | 'table';
  emptyTitle?: string;
  emptyDescription?: string;
}>();

const emit = defineEmits<{
  run: [id: number];
  edit: [account: Account];
  delete: [id: number];
  refresh: [id: number];
  profile: [id: number];
  detail: [account: Account];
}>();

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

function remaining(used: number): number {
  return Math.max(0, Math.min(100, 100 - used));
}

function quotaTone(used: number): 'high' | 'medium' | 'low' {
  const rem = 100 - used;
  if (rem <= 20) return 'low';
  if (rem <= 50) return 'medium';
  return 'high';
}

function quotaText(used: number): string {
  return `${remaining(used)}%`;
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

function getPlanClass(t: string): string {
  const v = t.toLowerCase();
  if (v === 'pro') return 'plan-pro';
  if (v === 'plus') return 'plan-plus';
  return 'plan-free';
}

function isCurrent(account: Account): boolean {
  return props.currentAccountRecordId === account.id;
}

function shortAccountId(value: string | null): string {
  if (!value) return '未识别账号 ID';
  if (value.length <= 16) return value;
  return `${value.slice(0, 8)}...${value.slice(-6)}`;
}

function extractHttpStatus(error: string): number | null {
  const explicitMatch = error.match(/\b(?:http\s*status|status(?:\s*code)?|code|http)\D*(\d{3})\b/i);
  if (explicitMatch) return Number(explicitMatch[1]);
  const standaloneMatch = error.match(/\b([45]\d{2})\b/);
  return standaloneMatch ? Number(standaloneMatch[1]) : null;
}

function errorReasonLabel(account: Account): string | null {
  if (!account.has_json_info) return '无凭据';
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
    return '401 授权无效';
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
    return '402 额度/付款';
  }

  if (status === 403 || error.includes('forbidden') || error.includes('permission') || error.includes('权限')) {
    return '403 权限拒绝';
  }

  if (status === 429 || error.includes('rate limit') || error.includes('too many requests') || error.includes('频率')) {
    return '429 频率限制';
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
    return '网络/超时';
  }

  if (error.includes('json') || error.includes('parse') || error.includes('解析')) {
    return '数据解析';
  }

  return status ? `${status} 其他错误` : '其他错误';
}

function isQuotaDepleted(account: Account): boolean {
  if (!account.has_json_info || account.last_quota_error) return false;
  const primaryEmpty = account.primary_window_present && remaining(account.primary_used_percent) <= 0;
  const secondaryEmpty = account.secondary_window_present && remaining(account.secondary_used_percent) <= 0;
  return primaryEmpty || secondaryEmpty;
}

function isSoftQuotaError(account: Account): boolean {
  const label = errorReasonLabel(account);
  return label === '网络/超时' || label === '数据解析' || label === '其他错误' || Boolean(label?.match(/^[45]\d{2} 其他错误$/));
}

function statusLabel(account: Account): string {
  if (!account.has_json_info) return '无凭据';
  if (account.last_quota_error) return errorReasonLabel(account) ?? '异常';
  if (isQuotaDepleted(account)) return '额度耗尽';
  if (isCurrent(account)) return '当前';
  return '可用';
}

function statusClass(account: Account): string {
  if (!account.has_json_info) return 'status-empty';
  if (account.last_quota_error) return isSoftQuotaError(account) ? 'status-warning' : 'status-error';
  if (isQuotaDepleted(account)) return 'status-error';
  if (isCurrent(account)) return 'status-current';
  return 'status-ok';
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
        <div class="empty-mark">
          <svg width="34" height="34" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <rect x="3" y="4" width="18" height="16" rx="3"/>
            <path d="M7 9h10"/>
            <path d="M7 13h6"/>
            <path d="M16 17l2 2 4-4"/>
          </svg>
        </div>
      </div>
      <h3>{{ emptyTitle || '还没有账号' }}</h3>
      <p>{{ emptyDescription || '使用 OAuth 登录或导入 auth.json 后，账号会出现在这里' }}</p>
    </div>

    <!-- Cards -->
    <div v-else-if="viewMode === 'cards'" class="card-grid">
      <div v-for="account in accounts" :key="account.id" :class="['card', { 'card-active': isCurrent(account) }]">
        <div class="card-accent"></div>

        <div class="card-header">
          <div class="card-title-row">
            <div class="account-title">
              <h3 :title="account.name">{{ account.name }}</h3>
              <span class="account-id" :title="account.account_id || ''">{{ shortAccountId(account.account_id) }}</span>
            </div>
            <div class="card-tags">
              <span v-if="isCurrent(account)" class="status-tag current-tag">
                <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="20 6 9 17 4 12"/>
                </svg>
                当前
              </span>
              <span :class="['badge', getPlanClass(account.plan_type)]">
                {{ account.plan_type || 'unknown' }}
              </span>
            </div>
          </div>
          <div class="card-meta">
            <span v-if="account.activation_date">开通 {{ account.activation_date }}</span>
            <span>记录 #{{ account.id }}</span>
          </div>
        </div>

        <div class="card-body">
          <div v-if="account.primary_window_present" :class="['quota-block', `quota-${quotaTone(account.primary_used_percent)}`]">
            <div class="quota-head">
              <span class="quota-label">{{ quotaWindowLabel(account.primary_window_minutes, 'primary') }}</span>
              <span class="quota-pct">{{ quotaText(account.primary_used_percent) }}</span>
            </div>
            <div class="bar-track">
              <div
                class="bar-fill"
                :style="{ width: remaining(account.primary_used_percent) + '%' }"
              ></div>
            </div>
            <span class="quota-reset">刷新 {{ formatResetTime(account.primary_reset_at) }}</span>
          </div>

          <div v-if="account.secondary_window_present" :class="['quota-block', `quota-${quotaTone(account.secondary_used_percent)}`]">
            <div class="quota-head">
              <span class="quota-label">{{ quotaWindowLabel(account.secondary_window_minutes, 'secondary') }}</span>
              <span class="quota-pct">{{ quotaText(account.secondary_used_percent) }}</span>
            </div>
            <div class="bar-track">
              <div
                class="bar-fill"
                :style="{ width: remaining(account.secondary_used_percent) + '%' }"
              ></div>
            </div>
            <span class="quota-reset">刷新 {{ formatResetTime(account.secondary_reset_at) }}</span>
          </div>

          <div :class="['quota-status', { 'quota-status-error': account.last_quota_error }]">
            <span class="quota-status-line">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10"/>
                <polyline points="12 6 12 12 16 14"/>
              </svg>
              上次检查 {{ formatCheckedTime(account.last_quota_checked_at) }}
            </span>
            <span v-if="account.last_quota_error" class="quota-error" :title="account.last_quota_error">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round">
                <circle cx="12" cy="12" r="10"/>
                <line x1="12" y1="8" x2="12" y2="12"/>
                <line x1="12" y1="16" x2="12.01" y2="16"/>
              </svg>
              <span class="quota-error-reason">{{ errorReasonLabel(account) }}</span>
              <span>{{ account.last_quota_error }}</span>
            </span>
          </div>
        </div>

        <div class="card-footer">
          <button
            class="btn-run"
            :disabled="switchingId === account.id || !account.has_json_info"
            :title="!account.has_json_info ? 'JSON 为空' : '切换账号'"
            @click="emit('run', account.id)"
          >
            <svg v-if="switchingId === account.id" class="spin" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
              <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
            </svg>
            <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
              <polygon points="6 3 20 12 6 21 6 3"/>
            </svg>
            <span>{{ switchingId === account.id ? '切换中' : (isCurrent(account) ? '已在运行' : '运行') }}</span>
          </button>
          <div class="btn-group">
            <button class="btn-icon btn-refresh" title="刷新额度" @click="emit('refresh', account.id)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
              </svg>
            </button>
            <button class="btn-icon btn-profile" title="刷新资料" @click="emit('profile', account.id)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="8" r="4"/>
                <path d="M4 21a8 8 0 0 1 16 0"/>
              </svg>
            </button>
            <button class="btn-icon btn-detail" title="详情" @click="emit('detail', account)">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10"/>
                <path d="M12 16v-4"/>
                <path d="M12 8h.01"/>
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

    <!-- Compact -->
    <div v-else-if="viewMode === 'compact'" class="compact-list">
      <div
        v-for="account in accounts"
        :key="account.id"
        :class="['compact-row', { current: isCurrent(account), disabled: !account.has_json_info }]"
      >
        <div class="compact-main">
          <div class="compact-title">
            <span :class="['table-status-dot', statusClass(account)]"></span>
            <strong :title="account.name">{{ account.name }}</strong>
            <span v-if="isCurrent(account)" class="compact-current">当前</span>
            <span :class="['badge', getPlanClass(account.plan_type)]">{{ account.plan_type || 'unknown' }}</span>
          </div>
          <span class="compact-sub" :title="account.account_id || ''">
            {{ shortAccountId(account.account_id) }} · #{{ account.id }} · {{ formatCheckedTime(account.last_quota_checked_at) }}
          </span>
        </div>

        <div class="compact-quotas">
          <div class="compact-quota">
            <span>{{ quotaWindowLabel(account.primary_window_minutes, 'primary', true) }}</span>
            <strong :class="`quota-text-${quotaTone(account.primary_used_percent)}`">{{ quotaText(account.primary_used_percent) }}</strong>
          </div>
          <div v-if="account.secondary_window_present" class="compact-quota">
            <span>{{ quotaWindowLabel(account.secondary_window_minutes, 'secondary', true) }}</span>
            <strong :class="`quota-text-${quotaTone(account.secondary_used_percent)}`">{{ quotaText(account.secondary_used_percent) }}</strong>
          </div>
        </div>

        <div class="compact-actions">
          <button
            class="compact-run"
            :disabled="switchingId === account.id || !account.has_json_info"
            :title="!account.has_json_info ? 'JSON 为空' : '切换账号'"
            @click="emit('run', account.id)"
          >
            {{ switchingId === account.id ? '切换中' : (isCurrent(account) ? '运行中' : '运行') }}
          </button>
          <button class="btn-icon btn-refresh" title="刷新额度" @click="emit('refresh', account.id)">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
          </button>
          <button class="btn-icon btn-detail" title="详情" @click="emit('detail', account)">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"/>
              <path d="M12 16v-4"/>
              <path d="M12 8h.01"/>
            </svg>
          </button>
          <button class="btn-icon btn-edit" title="编辑" @click="emit('edit', account)">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
              <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- Table -->
    <div v-else class="account-table-wrap">
      <table class="account-table">
        <thead>
          <tr>
            <th>账号</th>
            <th>状态</th>
            <th>额度一</th>
            <th>额度二</th>
            <th>上次检查</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="account in accounts"
            :key="account.id"
            :class="{ current: isCurrent(account), disabled: !account.has_json_info }"
          >
            <td>
              <div class="table-account">
                <strong :title="account.name">{{ account.name }}</strong>
                <span :title="account.account_id || ''">{{ shortAccountId(account.account_id) }} · #{{ account.id }}</span>
              </div>
            </td>
            <td>
              <div class="table-status">
                <span :class="['table-status-dot', statusClass(account)]"></span>
                <span>{{ statusLabel(account) }}</span>
                <span :class="['badge', getPlanClass(account.plan_type)]">{{ account.plan_type || 'unknown' }}</span>
              </div>
            </td>
            <td>
              <div v-if="account.primary_window_present" class="table-quota">
                <small>{{ quotaWindowLabel(account.primary_window_minutes, 'primary') }}</small>
                <div class="table-quota-line">
                  <strong :class="`quota-text-${quotaTone(account.primary_used_percent)}`">{{ quotaText(account.primary_used_percent) }}</strong>
                  <small>{{ formatResetTime(account.primary_reset_at) }}</small>
                </div>
                <div class="bar-track table-track">
                  <div :class="['bar-fill', `fill-${quotaTone(account.primary_used_percent)}`]" :style="{ width: remaining(account.primary_used_percent) + '%' }"></div>
                </div>
              </div>
              <span v-else class="table-quota-empty">-</span>
            </td>
            <td>
              <div v-if="account.secondary_window_present" class="table-quota">
                <small>{{ quotaWindowLabel(account.secondary_window_minutes, 'secondary') }}</small>
                <div class="table-quota-line">
                  <strong :class="`quota-text-${quotaTone(account.secondary_used_percent)}`">{{ quotaText(account.secondary_used_percent) }}</strong>
                  <small>{{ formatResetTime(account.secondary_reset_at) }}</small>
                </div>
                <div class="bar-track table-track">
                  <div :class="['bar-fill', `fill-${quotaTone(account.secondary_used_percent)}`]" :style="{ width: remaining(account.secondary_used_percent) + '%' }"></div>
                </div>
              </div>
              <span v-else class="table-quota-empty">-</span>
            </td>
            <td>
              <div class="table-checked" :title="account.last_quota_error || ''">
                <span>{{ formatCheckedTime(account.last_quota_checked_at) }}</span>
                <small v-if="account.last_quota_error">
                  {{ errorReasonLabel(account) }} · {{ account.last_quota_error }}
                </small>
              </div>
            </td>
            <td>
              <div class="table-actions">
                <button
                  class="btn-icon btn-run-icon"
                  :disabled="switchingId === account.id || !account.has_json_info"
                  :title="!account.has_json_info ? 'JSON 为空' : '切换账号'"
                  @click="emit('run', account.id)"
                >
                  <svg v-if="switchingId === account.id" class="spin" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                    <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
                  </svg>
                  <svg v-else width="13" height="13" viewBox="0 0 24 24" fill="currentColor">
                    <polygon points="6 3 20 12 6 21 6 3"/>
                  </svg>
                </button>
                <button class="btn-icon btn-refresh" title="刷新额度" @click="emit('refresh', account.id)">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
                  </svg>
                </button>
                <button class="btn-icon btn-profile" title="刷新资料" @click="emit('profile', account.id)">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="12" cy="8" r="4"/>
                    <path d="M4 21a8 8 0 0 1 16 0"/>
                  </svg>
                </button>
                <button class="btn-icon btn-detail" title="详情" @click="emit('detail', account)">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="12" cy="12" r="10"/>
                    <path d="M12 16v-4"/>
                    <path d="M12 8h.01"/>
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
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.account-list { width: 100%; }

/* ── Grid ─────────────────────────────── */
.card-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
}

/* ── Table ────────────────────────────── */
.account-table-wrap {
  width: 100%;
  overflow-x: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  box-shadow: var(--shadow-sm);
}

.account-table {
  width: 100%;
  min-width: 1280px;
  border-collapse: separate;
  border-spacing: 0;
  table-layout: fixed;
}

.account-table th {
  padding: 11px 14px;
  background: #f8fafc;
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 800;
  text-align: left;
  letter-spacing: 0;
  border-bottom: 1px solid var(--border-light);
}

.account-table td {
  padding: 12px 14px;
  border-bottom: 1px solid var(--border-light);
  color: var(--text);
  font-size: 12px;
  vertical-align: middle;
}

.account-table tr:last-child td {
  border-bottom: none;
}

.account-table tr.current {
  background: #ecfdf5;
}

.account-table tr.disabled {
  opacity: 0.62;
}

.account-table tr:hover {
  background: #f8fafc;
}

.account-table th:nth-child(1),
.account-table td:nth-child(1) {
  width: 18%;
}

.account-table th:nth-child(2),
.account-table td:nth-child(2) {
  width: 13%;
}

.account-table th:nth-child(3),
.account-table td:nth-child(3),
.account-table th:nth-child(4),
.account-table td:nth-child(4) {
  width: 13%;
}

.account-table th:nth-child(5),
.account-table td:nth-child(5) {
  width: 17%;
}

.account-table th:nth-child(6),
.account-table td:nth-child(6) {
  position: sticky;
  right: 0;
  z-index: 2;
  width: 236px;
  background: var(--surface);
  box-shadow: -8px 0 12px rgba(15, 23, 42, 0.06);
}

.account-table th:nth-child(6) {
  z-index: 3;
  background: #f8fafc;
}

.account-table tr.current td:nth-child(6) {
  background: #ecfdf5;
}

.account-table tr:hover td:nth-child(6) {
  background: #f8fafc;
}

.table-account,
.table-checked {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.table-account strong,
.table-checked span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.table-account span,
.table-checked small,
.table-quota small {
  overflow: hidden;
  color: var(--text-tertiary);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.table-status {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
  white-space: nowrap;
}

.table-status-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  flex-shrink: 0;
}

.status-ok {
  background: #10b981;
}

.status-current {
  background: var(--primary);
}

.status-error,
.status-empty {
  background: #ef4444;
}

.status-warning {
  background: #f59e0b;
}

.table-quota {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.table-quota-empty {
  color: var(--text-tertiary);
  font-size: 12px;
}

.table-quota-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.table-quota-line strong {
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.table-track {
  height: 5px;
}

.bar-fill.fill-high {
  background: #10b981;
}

.bar-fill.fill-medium {
  background: #f59e0b;
}

.bar-fill.fill-low {
  background: #ef4444;
}

.quota-text-high {
  color: #059669;
}

.quota-text-medium {
  color: #d97706;
}

.quota-text-low {
  color: #dc2626;
}

.table-actions {
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
  width: 100%;
}

.table-actions .btn-icon {
  width: 30px;
  height: 30px;
}

.btn-run-icon {
  color: var(--primary);
}

.btn-run-icon:hover:not(:disabled) {
  background: var(--primary-light);
  border-color: #c7d2fe;
}

/* ── Compact ──────────────────────────── */
.compact-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.compact-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 14px;
  min-height: 66px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  box-shadow: var(--shadow-xs);
}

.compact-row:hover {
  border-color: #cbd5e1;
  box-shadow: var(--shadow-sm);
}

.compact-row.current {
  border-color: rgba(16, 185, 129, 0.55);
  background: #ecfdf5;
}

.compact-row.disabled {
  opacity: 0.62;
}

.compact-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.compact-title {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 7px;
}

.compact-title strong {
  overflow: hidden;
  color: var(--text);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.compact-current {
  padding: 2px 6px;
  border: 1px solid #a7f3d0;
  border-radius: 999px;
  background: var(--success-light);
  color: #047857;
  font-size: 10px;
  font-weight: 800;
  white-space: nowrap;
}

.compact-sub {
  overflow: hidden;
  color: var(--text-tertiary);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.compact-quotas,
.compact-actions {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.compact-quota {
  min-width: 54px;
  padding: 6px 8px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: #f8fafc;
}

.compact-quota span {
  display: block;
  color: var(--text-tertiary);
  font-size: 10px;
  font-weight: 800;
}

.compact-quota strong {
  display: block;
  margin-top: 2px;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
}

.compact-run {
  height: 32px;
  min-width: 66px;
  padding: 0 12px;
  border: 0;
  border-radius: var(--radius-sm);
  background: var(--primary);
  color: #fff;
  font-size: 12px;
  font-weight: 800;
  cursor: pointer;
}

.compact-run:hover:not(:disabled) {
  background: var(--primary-hover);
}

.compact-run:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* ── Card ─────────────────────────────── */
.card {
  position: relative;
  background: var(--surface);
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  overflow: hidden;
  transition: border-color 0.2s var(--ease-out), box-shadow 0.2s var(--ease-out), transform 0.2s var(--ease-out);
  box-shadow: var(--shadow-sm);
  min-height: 302px;
  display: flex;
  flex-direction: column;
}

.card:hover {
  border-color: #cbd5e1;
  box-shadow: var(--shadow-md);
  transform: translateY(-2px);
}

/* Current active card */
.card-active {
  border-color: rgba(16, 185, 129, 0.62);
  box-shadow: var(--shadow-md), 0 0 0 2px rgba(16, 185, 129, 0.1);
}

.card-active:hover {
  border-color: rgba(16, 185, 129, 0.78);
  box-shadow: var(--shadow-md), 0 0 0 2px rgba(16, 185, 129, 0.14);
}

.card-active .card-accent {
  background: linear-gradient(90deg, #10b981, #34d399);
  opacity: 1;
}

.card-accent {
  height: 3px;
  background: var(--border-light);
  transition: background 0.2s var(--ease-out);
}

.card:hover .card-accent {
  background: linear-gradient(90deg, var(--primary), var(--accent));
}

/* ── Card header ──────────────────────── */
.card-header {
  padding: 16px 18px 0;
}

.card-title-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
}

.account-title {
  min-width: 0;
  flex: 1;
}

.account-title h3 {
  font-size: 15px;
  font-weight: 700;
  color: var(--text);
  margin: 0;
  letter-spacing: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.account-id,
.card-meta {
  color: var(--text-tertiary);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.account-id {
  display: block;
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card-tags {
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 6px;
  flex-shrink: 0;
  max-width: 45%;
}

.card-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 9px;
}

.status-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  line-height: 1;
  white-space: nowrap;
}

.current-tag {
  background: var(--success-light);
  color: #047857;
  border: 1px solid #a7f3d0;
}

/* ── Badge ────────────────────────────── */
.badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 3px 9px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0;
  line-height: 1;
  flex-shrink: 0;
}

.plan-free {
  background: #f1f5f9;
  color: #64748b;
  border: 1px solid #e2e8f0;
}

.plan-plus {
  background: #eff6ff;
  color: #3b82f6;
  border: 1px solid #bfdbfe;
}

.plan-pro {
  background: #faf5ff;
  color: #9333ea;
  border: 1px solid #e9d5ff;
}

/* ── Card body ────────────────────────── */
.card-body {
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1;
}

.quota-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 11px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: #fafbfc;
}

.quota-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}

.quota-label {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-secondary);
  letter-spacing: 0;
}

.quota-pct {
  font-size: 13px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  line-height: 1;
  letter-spacing: 0;
}

/* ── Progress bar ─────────────────────── */
.bar-track {
  height: 6px;
  background: #e5e7eb;
  border-radius: 4px;
  overflow: hidden;
}

.bar-fill {
  position: relative;
  height: 100%;
  border-radius: 4px;
  background: #10b981;
  transition: width 0.3s var(--ease-out);
  min-width: 2px;
}

.quota-medium .bar-fill {
  background: #f59e0b;
}

.quota-low .bar-fill {
  background: #ef4444;
}

.quota-high .quota-pct {
  color: #059669;
}

.quota-medium .quota-pct {
  color: #d97706;
}

.quota-low .quota-pct {
  color: #dc2626;
}

.quota-reset {
  font-size: 11px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
}

.quota-status {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-light);
  background: var(--surface);
  color: var(--text-tertiary);
  font-size: 11px;
  line-height: 1.35;
}

.quota-status-error {
  border-color: #fecaca;
  background: var(--danger-light);
  color: var(--danger);
}

.quota-status-line,
.quota-error {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
}

.quota-error span,
.quota-error {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
}

.quota-error-reason {
  flex-shrink: 0;
  color: #b91c1c;
}

/* ── Card footer ──────────────────────── */
.card-footer {
  padding: 12px 18px;
  border-top: 1px solid var(--border-light);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  background: #fafafa;
}

.btn-group {
  display: flex;
  gap: 5px;
  flex-shrink: 0;
}

/* ── Buttons ──────────────────────────── */
.btn-run {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-width: 92px;
  justify-content: center;
  padding: 8px 14px;
  background: var(--primary);
  color: #fff;
  border: none;
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
  transition: all 0.2s var(--ease-out);
  box-shadow: none;
}

.btn-run:hover:not(:disabled) {
  background: var(--primary-hover);
}

.btn-run:active:not(:disabled) {
  transform: translateY(0);
}

.btn-run:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.btn-icon {
  width: 32px;
  height: 32px;
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

.btn-profile { color: #0f766e; }
.btn-profile:hover:not(:disabled) {
  background: #ccfbf1;
  border-color: #99f6e4;
  color: #0d9488;
}

.btn-detail { color: #475569; }
.btn-detail:hover:not(:disabled) {
  background: #f1f5f9;
  border-color: #cbd5e1;
  color: #0f172a;
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
  padding: 96px 20px;
  text-align: center;
}

.empty-mark {
  width: 72px;
  height: 72px;
  border-radius: var(--radius-sm);
  border: 1px solid #c7d2fe;
  background: var(--primary-light);
  color: var(--primary);
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 20px;
}

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

@media (max-width: 980px) {
  .card-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 640px) {
  .card-grid {
    grid-template-columns: 1fr;
  }

  .compact-row {
    grid-template-columns: 1fr;
    align-items: stretch;
  }

  .compact-quotas,
  .compact-actions {
    justify-content: space-between;
  }

  .compact-quota,
  .compact-run {
    flex: 1;
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
    justify-content: space-between;
  }

  .btn-icon {
    flex: 1;
  }
}
</style>
