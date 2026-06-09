import type { Account, CodexUsageRollup } from '../types';

export function quotaRemaining(used: number): number {
  return Math.max(0, Math.min(100, 100 - used));
}

export function quotaWindowLabel(minutes: number | null, fallback: 'primary' | 'secondary', compact = false): string {
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

export function quotaSummaryLabel(accounts: Account[], field: 'primary' | 'secondary'): string {
  const minutes = Array.from(new Set(
    accounts
      .map(account => field === 'primary' ? account.primary_window_minutes : account.secondary_window_minutes)
      .filter((value): value is number => typeof value === 'number' && value > 0),
  ));
  if (minutes.length === 1) return quotaWindowLabel(minutes[0], field, true);
  return field === 'primary' ? '额度一' : '额度二';
}

export function formatResetTime(timestamp: number): string {
  if (!timestamp) return '-';
  const d = new Date(timestamp * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function formatCheckedTime(value: string): string {
  if (!value) return '尚未检查';
  return value.replace('T', ' ');
}

export function formatTokenAmount(value: number | null | undefined): string {
  const normalized = typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
  if (normalized >= 100_000_000) return `${(normalized / 100_000_000).toFixed(2).replace(/\.00$/, '')}亿`;
  if (normalized >= 10_000) return `${(normalized / 10_000).toFixed(1).replace(/\.0$/, '')}万`;
  return Math.round(normalized).toLocaleString('zh-CN');
}

export function formatExactNumber(value: number | null | undefined): string {
  const normalized = typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
  return Math.round(normalized).toLocaleString('zh-CN');
}

export function formatUsd(value: number | null | undefined): string {
  const normalized = typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: normalized >= 100 ? 2 : 4,
  }).format(normalized);
}

export function formatCredits(value: number | null | undefined): string {
  const normalized = typeof value === 'number' && Number.isFinite(value) ? Math.max(0, value) : 0;
  return normalized.toLocaleString('zh-CN', {
    maximumFractionDigits: normalized >= 100 ? 1 : 3,
  });
}

export function usageSuccessRate(usage: CodexUsageRollup | null | undefined): string {
  if (!usage || usage.request_count <= 0) return '-';
  return `${((usage.success_count / usage.request_count) * 100).toFixed(1)}%`;
}
