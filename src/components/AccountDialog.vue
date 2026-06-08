<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { Account } from '../types';

const props = defineProps<{
  account?: Account | null;
  saving?: boolean;
  initialJsonInfo?: string;
  loadingJson?: boolean;
}>();

const emit = defineEmits<{
  save: [data: { name: string; activationDate: string; jsonInfo: string }];
  close: [];
}>();

const name = ref('');
const activationDate = ref('');
const jsonInfo = ref('');
const jsonError = ref('');
const jsonValid = ref(false);
const isEdit = computed(() => !!props.account);

watch(
  () => props.account,
  (account) => {
    name.value = account?.name || '';
    activationDate.value = account?.activation_date || '';
    jsonError.value = '';
    jsonValid.value = false;
  },
  { immediate: true },
);

watch(
  () => props.initialJsonInfo,
  (value) => {
    jsonInfo.value = value || '';
    validateJson();
  },
  { immediate: true },
);

function validateJson(): boolean {
  jsonError.value = '';
  jsonValid.value = false;
  if (!jsonInfo.value.trim()) return true;
  try {
    const trimmed = jsonInfo.value.trim();
    if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) {
      jsonValid.value = true;
      return true;
    }

    const parsed = JSON.parse(trimmed);
    const hasAnyToken =
      typeof parsed?.tokens?.access_token === 'string' ||
      typeof parsed?.tokens?.refresh_token === 'string' ||
      typeof parsed?.tokens?.refreshToken === 'string' ||
      typeof parsed?.access_token === 'string' ||
      typeof parsed?.accessToken === 'string' ||
      typeof parsed?.refresh_token === 'string' ||
      typeof parsed?.refreshToken === 'string' ||
      typeof parsed?.token === 'string';

    if (!hasAnyToken) {
      jsonError.value = '未找到可导入的 token 或 auth.json 字段';
      return false;
    }

    jsonValid.value = true;
    return true;
  } catch {
    jsonError.value = 'JSON 格式错误';
    return false;
  }
}

function handleSave() {
  if (!name.value.trim()) return;
  if (props.loadingJson) return;
  if (!validateJson()) return;
  emit('save', {
    name: name.value.trim(),
    activationDate: activationDate.value,
    jsonInfo: jsonInfo.value.trim(),
  });
}

function handleBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) emit('close');
}

function formatJson() {
  if (!jsonInfo.value.trim()) return;
  try {
    jsonInfo.value = JSON.stringify(JSON.parse(jsonInfo.value), null, 2);
    validateJson();
  } catch { /* skip */ }
}
</script>

<template>
  <div class="backdrop" @click="handleBackdrop">
    <div class="dialog">
      <div class="dialog-header">
        <div class="traffic-lights" aria-hidden="true">
          <span class="traffic-red"></span>
          <span class="traffic-yellow"></span>
          <span class="traffic-green"></span>
        </div>
        <h2>{{ isEdit ? '编辑账号' : '添加账号' }}</h2>
        <button class="close-btn" @click="emit('close')">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>

      <div class="dialog-body">
        <section class="dialog-section basic-section">
          <div class="section-head">
            <span>基本信息</span>
          </div>
          <div class="field">
            <label>账号名称 <span class="req">*</span></label>
            <input v-model="name" type="text" placeholder="例如: My Account" @keyup.enter="handleSave" />
          </div>
          <div class="field">
            <label>开通时间</label>
            <input v-model="activationDate" type="date" />
          </div>
          <div v-if="account" class="readonly-grid">
            <div>
              <span>记录 ID</span>
              <strong>#{{ account.id }}</strong>
            </div>
            <div>
              <span>账号类型</span>
              <strong>{{ account.plan_type || 'unknown' }}</strong>
            </div>
            <div class="readonly-wide">
              <span>账号标识</span>
              <strong :title="account.account_id || ''">{{ account.account_id || '未识别' }}</strong>
            </div>
          </div>
        </section>

        <section class="dialog-section auth-section">
          <div class="section-head">
            <span>auth.json</span>
            <button class="fmt-btn" @click="formatJson" :disabled="props.loadingJson || !jsonInfo.trim()">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>
              </svg>
              格式化
            </button>
          </div>
          <textarea
            v-model="jsonInfo"
            rows="10"
            :disabled="props.loadingJson"
            :placeholder="props.loadingJson ? '正在读取已保存的 auth.json...' : (isEdit ? '留空则不修改；可粘贴 auth.json、access_token 或 refresh_token...' : '粘贴 auth.json、access_token 或 refresh_token...')"
            @input="validateJson()"
          ></textarea>
          <div class="field-msg">
            <span v-if="jsonError" class="msg-err">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                <circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/>
              </svg>
              {{ jsonError }}
            </span>
            <span v-else-if="jsonValid" class="msg-ok">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
              </svg>
              格式正确
            </span>
          </div>
        </section>
      </div>

      <div class="dialog-footer">
        <button class="btn-cancel" @click="emit('close')">取消</button>
        <button class="btn-save" :disabled="!name.trim() || props.saving || props.loadingJson" @click="handleSave">
          <svg v-if="props.saving" class="spin" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
            <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
          </svg>
          {{ props.saving ? '保存中...' : (props.loadingJson ? '读取中...' : (isEdit ? '保存' : '添加')) }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 42, 0.45);
  backdrop-filter: blur(6px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.dialog {
  background: var(--surface);
  border-radius: var(--radius-xl);
  width: 520px;
  max-width: 92vw;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  box-shadow: var(--shadow-xl), 0 0 0 1px rgba(0, 0, 0, 0.04);
  animation: dialog-enter 0.3s var(--ease-out);
}

@keyframes dialog-enter {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(10px);
  }
}

/* ── Header ───────────────────────────── */
.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 20px 24px;
  border-bottom: 1px solid var(--border-light);
}

.dialog-title {
  display: flex;
  align-items: center;
  gap: 12px;
}

.dialog-icon {
  width: 36px;
  height: 36px;
  border-radius: 10px;
  background: linear-gradient(135deg, var(--primary-light), #e0e7ff);
  color: var(--primary);
  display: flex;
  align-items: center;
  justify-content: center;
}

.dialog-header h2 {
  font-size: 17px;
  font-weight: 600;
  color: var(--text);
  margin: 0;
}

.close-btn {
  width: 32px;
  height: 32px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--text-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s;
}

.close-btn:hover {
  background: var(--border-light);
  color: var(--text);
}

/* ── Body ─────────────────────────────── */
.dialog-body {
  padding: 24px;
  overflow-y: auto;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.field label {
  display: block;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  margin-bottom: 6px;
}

.req { color: var(--danger); }

.field-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}

.field-top label { margin-bottom: 0; }

.fmt-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: none;
  border: none;
  color: var(--primary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  padding: 3px 8px;
  border-radius: 6px;
  transition: all 0.15s;
}

.fmt-btn:hover:not(:disabled) { background: var(--primary-light); }
.fmt-btn:disabled { color: var(--text-tertiary); cursor: not-allowed; }

input[type="text"],
input[type="date"] {
  width: 100%;
  padding: 10px 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  font-size: 14px;
  color: var(--text);
  background: var(--surface);
  transition: all 0.2s var(--ease-out);
}

input:hover { border-color: #cbd5e1; }

input:focus {
  outline: none;
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

textarea {
  width: 100%;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
  color: var(--text);
  background: var(--surface);
  resize: vertical;
  line-height: 1.6;
  transition: all 0.2s var(--ease-out);
}

textarea:hover { border-color: #cbd5e1; }

textarea:disabled {
  opacity: 0.65;
  cursor: wait;
}

textarea:focus {
  outline: none;
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

textarea::placeholder {
  color: var(--text-tertiary);
  font-family: inherit;
}

.field-msg {
  min-height: 20px;
  margin-top: 4px;
}

.msg-err,
.msg-ok {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  font-weight: 500;
}

.msg-err { color: var(--danger); }
.msg-ok { color: var(--success); }

/* ── Footer ───────────────────────────── */
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 16px 24px;
  border-top: 1px solid var(--border-light);
  background: linear-gradient(180deg, transparent 0%, rgba(248, 250, 252, 0.5) 100%);
}

.btn-cancel,
.btn-save {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 9px 22px;
  border: none;
  border-radius: var(--radius-sm);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s var(--ease-out);
}

.btn-cancel {
  background: var(--border-light);
  color: var(--text-secondary);
}

.btn-cancel:hover { background: var(--border); }

.btn-save {
  background: linear-gradient(135deg, var(--primary), var(--primary-hover));
  color: #fff;
  box-shadow: 0 2px 6px rgba(99, 102, 241, 0.2);
}

.btn-save:hover:not(:disabled) {
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.3);
  transform: translateY(-1px);
}

.btn-save:active:not(:disabled) {
  transform: translateY(0);
}

.btn-save:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* Refined editor dialog */
.backdrop {
  background: rgba(16, 24, 39, 0.32);
  backdrop-filter: blur(10px);
}

.dialog {
  width: min(720px, 94vw);
  border: 1px solid rgba(219, 228, 238, 0.92);
  border-radius: 12px;
  box-shadow: var(--shadow-xl);
}

.dialog-header {
  padding: 16px 18px;
  background: linear-gradient(180deg, #ffffff 0%, #f8fbfd 100%);
}

.dialog-icon {
  width: 34px;
  height: 34px;
  border: 1px solid rgba(79, 99, 232, 0.18);
  border-radius: var(--radius-sm);
  background: var(--primary-light);
}

.dialog-header h2 {
  font-size: 16px;
  font-weight: 850;
}

.dialog-body {
  padding: 18px;
  gap: 14px;
  background: #fff;
}

.field label {
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 850;
}

input[type="text"],
input[type="date"],
textarea {
  border-color: var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface-soft);
}

input[type="text"],
input[type="date"] {
  height: 38px;
  padding: 0 12px;
  font-weight: 700;
}

textarea {
  min-height: 300px;
  padding: 12px;
  background: #101827;
  color: #e5edf7;
  font-size: 12px;
  line-height: 1.58;
  caret-color: #a5b4fc;
}

textarea::placeholder {
  color: #7f8da3;
}

textarea:hover {
  border-color: #b8c6d8;
}

textarea:focus {
  border-color: var(--primary);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

.fmt-btn {
  min-height: 28px;
  border: 1px solid rgba(79, 99, 232, 0.18);
  background: #fff;
  font-weight: 850;
}

.dialog-footer {
  padding: 14px 18px;
  background: var(--surface-soft);
}

.btn-cancel,
.btn-save {
  min-height: 36px;
  padding: 0 16px;
  font-weight: 850;
}

.btn-save {
  background: var(--primary);
  box-shadow: 0 10px 22px rgba(79, 99, 232, 0.16);
}

.btn-save:hover:not(:disabled) {
  background: var(--primary-hover);
  box-shadow: 0 14px 30px rgba(79, 99, 232, 0.2);
}

/* ── macOS polish pass ────────────────── */
.backdrop {
  background: rgba(23, 32, 51, 0.26);
  backdrop-filter: blur(12px);
}

.dialog {
  width: min(900px, 94vw);
  border: 1px solid var(--border);
  border-radius: 14px;
  background: #fff;
  box-shadow: 0 22px 64px rgba(23, 32, 51, 0.18), 0 1px 2px rgba(23, 32, 51, 0.08);
}

.dialog-header {
  position: relative;
  min-height: 44px;
  justify-content: center;
  padding: 0 44px;
  border-bottom-color: var(--border);
  background: linear-gradient(180deg, #ffffff 0%, #f7f9fc 100%);
}

.dialog-header h2 {
  color: var(--text);
  font-size: 13px;
  font-weight: 850;
}

.traffic-lights {
  position: absolute;
  left: 16px;
  top: 50%;
  display: inline-flex;
  gap: 7px;
  transform: translateY(-50%);
}

.traffic-lights span {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  box-shadow: inset 0 0 0 1px rgba(23, 32, 51, 0.08);
}

.traffic-red {
  background: #ff5f57;
}

.traffic-yellow {
  background: #febc2e;
}

.traffic-green {
  background: #28c840;
}

.close-btn {
  position: absolute;
  top: 6px;
  right: 10px;
  color: var(--text-tertiary);
}

.dialog-body {
  display: grid;
  grid-template-columns: minmax(240px, 0.9fr) minmax(360px, 1.4fr);
  gap: 0;
  padding: 0;
  background: #fff;
}

.dialog-section {
  min-width: 0;
  padding: 18px;
}

.basic-section {
  border-right: 1px solid var(--border-light);
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 28px;
  margin-bottom: 12px;
}

.section-head span {
  color: var(--text);
  font-size: 12px;
  font-weight: 850;
}

.field {
  margin-bottom: 14px;
}

.field label {
  margin-bottom: 6px;
  color: #6e7b91;
  font-size: 12px;
  font-weight: 760;
}

input[type="text"],
input[type="date"] {
  height: 36px;
  padding: 0 11px;
  border-color: var(--border);
  border-radius: var(--radius-sm);
  background: #fff;
  color: var(--text);
  font-size: 13px;
  font-weight: 650;
}

input:hover {
  border-color: #c7d1df;
}

input:focus,
textarea:focus {
  border-color: rgba(79, 107, 255, 0.66);
  box-shadow: 0 0 0 3px var(--primary-glow);
}

.readonly-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid var(--border-light);
}

.readonly-grid div {
  min-width: 0;
  padding: 9px 10px;
  border: 1px solid var(--border-light);
  border-radius: var(--radius-sm);
  background: var(--surface-soft);
}

.readonly-wide {
  grid-column: 1 / -1;
}

.readonly-grid span,
.readonly-grid strong {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.readonly-grid span {
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 750;
}

.readonly-grid strong {
  margin-top: 4px;
  color: var(--text);
  font-size: 12px;
  font-weight: 800;
}

.auth-section {
  display: flex;
  min-height: 430px;
  flex-direction: column;
}

textarea {
  flex: 1;
  min-height: 340px;
  padding: 12px 13px;
  border-color: #1f2a3b;
  border-radius: 10px;
  background: #111827;
  color: #e7edf5;
  font-family: 'SFMono-Regular', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  line-height: 1.58;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.035);
}

textarea:hover {
  border-color: #2d3a4f;
}

textarea::placeholder {
  color: #8290a5;
}

.fmt-btn {
  min-height: 28px;
  padding: 0 9px;
  border: 1px solid var(--border);
  border-radius: 7px;
  background: #fff;
  color: var(--primary);
  font-size: 12px;
  font-weight: 800;
}

.fmt-btn:hover:not(:disabled) {
  border-color: rgba(79, 107, 255, 0.26);
  background: var(--primary-light);
}

.field-msg {
  min-height: 20px;
  margin-top: 8px;
}

.msg-err,
.msg-ok {
  font-size: 12px;
  font-weight: 750;
}

.msg-ok {
  color: var(--success);
}

.msg-err {
  color: var(--danger);
}

.dialog-footer {
  min-height: 58px;
  padding: 11px 16px;
  border-top-color: var(--border);
  background: #fbfdff;
}

.btn-cancel,
.btn-save {
  min-height: 34px;
  padding: 0 18px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 800;
}

.btn-cancel {
  background: #fff;
  color: var(--text-secondary);
}

.btn-save {
  border-color: transparent;
  background: var(--primary);
  box-shadow: 0 10px 20px rgba(79, 107, 255, 0.16);
}

.btn-save:hover:not(:disabled) {
  background: var(--primary-hover);
  box-shadow: 0 14px 26px rgba(79, 107, 255, 0.2);
}

@media (max-width: 760px) {
  .dialog {
    width: min(94vw, 540px);
  }

  .dialog-body {
    grid-template-columns: 1fr;
  }

  .basic-section {
    border-right: 0;
    border-bottom: 1px solid var(--border-light);
  }

  .auth-section {
    min-height: 360px;
  }

  textarea {
    min-height: 260px;
  }
}
</style>
