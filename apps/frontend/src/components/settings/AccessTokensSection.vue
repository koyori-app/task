<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { PhCopy, PhKey, PhPlus } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import {
  Button,
  Checkbox,
  Dialog,
  Dropdown,
  Field,
  Input,
  Picker,
  Skeleton,
  type DropdownItem,
} from '@koyori-app/ui-vue';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import {
  personalTokensQueryOptions,
  useCreatePersonalTokenMutation,
  usePersonalTokensQuery,
  useRevokePersonalTokenMutation,
  useTenantsQuery,
} from '@/lib/api-vue-query';
import { codePointLength } from '@/lib/code-points';
import {
  EXPIRATION_PRESETS,
  SCOPE_CATALOG,
  expiresAtFromPreset,
  formatExpiry,
  formatLastUsed,
  maskedToken,
  type ExpirationKey,
  type PersonalToken,
  type TokenScope,
} from '@/lib/personal-tokens';

const TOKEN_NAME_MAX = 100;

const queryClient = useQueryClient();
const tokensQuery = usePersonalTokensQuery();
const tenantsQuery = useTenantsQuery();
const createToken = useCreatePersonalTokenMutation();
const revokeToken = useRevokePersonalTokenMutation();

/**
 * PAT を発行できるのはテナントオーナーと Admin（API 側は require_tenant_admin）。
 * 選択肢もそれと同じ条件で絞る——片方だけ広げると「選べるのに 403」になる。
 */
const issuableTenants = computed(() =>
  (tenantsQuery.data.value ?? []).filter(
    (t) => t.membership === 'Owner' || t.member_role === 'Admin',
  ),
);
const tenantItems = computed(() =>
  issuableTenants.value.map((tenant) => ({ value: tenant.id, label: tenant.name })),
);

/** 一覧・取り消しダイアログでトークンの束縛先テナントを表示するための対応表。 */
const tenantNamesById = computed(
  () => new Map((tenantsQuery.data.value ?? []).map((t) => [t.id, t.name])),
);

function tenantName(tenantId: string) {
  return tenantNamesById.value.get(tenantId) ?? '不明なテナント';
}

// --- 発行フォーム ---

const isFormOpen = ref(false);
const tokenName = ref('');
const nameError = ref<string | null>(null);
const selectedTenantId = ref<string | null>(null);
const expiration = ref<ExpirationKey>('90d');
const selectedScopes = ref<TokenScope[]>([]);
const scopesError = ref<string | null>(null);
const submitError = ref<string | null>(null);

/** 発行直後の平文トークン。この画面でしか見られない。 */
const createdToken = ref<string | null>(null);
const copied = ref(false);
const copyError = ref<string | null>(null);

const formTenantId = computed(() => selectedTenantId.value ?? issuableTenants.value[0]?.id ?? null);

function openForm() {
  isFormOpen.value = true;
  createdToken.value = null;
  copied.value = false;
  copyError.value = null;
}

function resetForm() {
  isFormOpen.value = false;
  tokenName.value = '';
  nameError.value = null;
  selectedTenantId.value = null;
  expiration.value = '90d';
  selectedScopes.value = [];
  scopesError.value = null;
  submitError.value = null;
}

function toggleScope(scope: TokenScope, checked: boolean) {
  scopesError.value = null;
  selectedScopes.value = checked
    ? [...selectedScopes.value, scope]
    : selectedScopes.value.filter((s) => s !== scope);
}

function validateName(): boolean {
  const name = tokenName.value.trim();
  if (name === '') {
    nameError.value = 'トークン名を入力してください。';
    return false;
  }
  if (codePointLength(name) > TOKEN_NAME_MAX) {
    nameError.value = `${TOKEN_NAME_MAX}文字以内で入力してください。`;
    return false;
  }
  nameError.value = null;
  return true;
}

async function onSubmit() {
  submitError.value = null;
  const nameOk = validateName();
  if (selectedScopes.value.length === 0) {
    scopesError.value = 'スコープを 1 つ以上選択してください。';
  }
  const tenantId = formTenantId.value;
  if (!nameOk || selectedScopes.value.length === 0 || !tenantId) return;

  try {
    const created = await createToken.mutateAsync({
      body: {
        name: tokenName.value.trim(),
        tenant_id: tenantId,
        // null = テナント内の全プロジェクトで有効。プロジェクト単位の絞り込み UI は未対応。
        project_ids: null,
        scopes: selectedScopes.value,
        expires_at: expiresAtFromPreset(expiration.value),
      },
    });
    await queryClient.invalidateQueries({ queryKey: personalTokensQueryOptions().queryKey });
    resetForm();
    createdToken.value = created.token;
  } catch (e) {
    const status = (e as { response?: { status?: number } }).response?.status;
    submitError.value =
      status === 403
        ? 'トークンを発行できるのは、選択したテナントのオーナーと Admin だけです。'
        : status === 400
          ? '入力内容を確認してください。'
          : 'トークンを発行できませんでした。時間をおいて再度お試しください。';
  }
}

async function copyCreatedToken() {
  if (!createdToken.value) return;
  copyError.value = null;
  try {
    await navigator.clipboard.writeText(createdToken.value);
    copied.value = true;
  } catch {
    // 平文はこの画面でしか見られないため、失敗を握り潰すと取り逃す。手動コピーへ誘導する
    copyError.value = 'コピーできませんでした。表示中のトークンを選択してコピーしてください。';
  }
}

// --- 取り消し ---

const revokeTarget = ref<PersonalToken | null>(null);
const revokeError = ref<string | null>(null);

function openRevokeDialog(token: PersonalToken) {
  revokeTarget.value = token;
  revokeError.value = null;
}

const revokeDescription = computed(() =>
  revokeTarget.value
    ? `テナント「${tenantName(revokeTarget.value.tenant_id)}」の「${revokeTarget.value.name}」を取り消します。このトークンを使っている API・CLI は即座に認証できなくなります。この操作は元に戻せません。`
    : '',
);

function onRevokeClose() {
  // 取り消しリクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (!revokeToken.isPending.value) revokeTarget.value = null;
}

async function onRevokeConfirm() {
  if (!revokeTarget.value) return;
  revokeError.value = null;
  try {
    await revokeToken.mutateAsync({
      params: { path: { id: revokeTarget.value.id } },
    });
    await queryClient.invalidateQueries({ queryKey: personalTokensQueryOptions().queryKey });
    revokeTarget.value = null;
  } catch {
    revokeError.value = 'トークンを取り消せませんでした。時間をおいて再度お試しください。';
  }
}

/** スコープは見せるだけ。トークンの行ごとに出すので、配列は行の scopes から組む */
function scopeItems(scopes: string[]): DropdownItem[] {
  return scopes.map((scope) => ({ value: scope, label: scope, disabled: true }));
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="flex flex-col gap-1">
        <h2 class="text-lg font-semibold">パーソナルアクセストークン</h2>
        <p class="text-muted-foreground text-sm">
          API と CLI をあなたとして認証するトークンです。パスワードと同じように扱ってください。
        </p>
      </div>
      <Button
        v-if="!isFormOpen"
        type="button"
        :disabled="issuableTenants.length === 0"
        label="トークンを発行"
        @click="openForm"
      >
        <template #icon><PhPlus class="size-4" /></template>
      </Button>
    </div>

    <!-- 発行直後の平文トークン。再表示できないためコピーを促す。 -->
    <div
      v-if="createdToken"
      class="flex flex-col gap-2 rounded-lg border border-green-600/40 bg-green-600/5 p-4"
    >
      <p class="text-sm font-medium">新しいトークンを発行しました。</p>
      <p class="text-muted-foreground text-sm">
        このトークンは今しか表示されません。必ずコピーして安全な場所に保管してください。
      </p>
      <div class="flex items-center gap-2">
        <code
          class="bg-muted min-w-0 flex-1 truncate rounded-md px-3 py-2 font-mono text-sm"
          data-testid="created-token"
          >{{ createdToken }}</code
        >
        <Button
          type="button"
          variant="tertiary"
          :label="copied ? 'コピーしました' : 'コピー'"
          @click="copyCreatedToken"
        >
          <template #icon><PhCopy class="size-4" /></template>
        </Button>
      </div>
      <p v-if="copyError" role="alert" class="text-destructive text-sm">{{ copyError }}</p>
    </div>

    <!-- 発行フォーム -->
    <div v-if="isFormOpen" class="rounded-lg border p-4">
      <HydrationSafeForm v-slot="{ isHydrated }" @submit="onSubmit">
        <div class="flex flex-col gap-6">
          <h3 class="text-base font-semibold">新しいトークンを発行</h3>

          <Field
            id="token-name"
            label="トークン名"
            description="このトークンの用途がわかる名前を付けます。"
            :error="nameError ?? ''"
          >
            <Input
              placeholder="例: CI デプロイ"
              :value="tokenName"
              :on-value-change="
                (v: string) => {
                  tokenName = v;
                  nameError = null;
                }
              "
              :on-blur="validateName"
            />
          </Field>

          <!-- 候補が一つでも出す。出さぬと、どのテナントに縛られるかが発行後まで見えぬ。 -->
          <Field
            v-if="issuableTenants.length > 0"
            id="token-tenant"
            label="テナント"
            description="トークンはこのテナントの中でだけ使えます。"
          >
            <Picker
              label="テナント"
              :searchable="tenantItems.length > 8"
              :items="tenantItems"
              :selected-values="formTenantId ? [formTenantId] : []"
              :on-selection-change="(values: string[]) => (selectedTenantId = values[0] ?? null)"
            />
          </Field>

          <div class="flex flex-col gap-2">
            <p class="text-sm font-medium">有効期限</p>
            <div class="flex flex-wrap gap-2">
              <Button
                v-for="preset in EXPIRATION_PRESETS"
                :key="preset.key"
                type="button"
                :variant="expiration === preset.key ? 'primary' : 'tertiary'"
                :ariaPressed="expiration === preset.key ? 'true' : 'false'"
                :label="preset.label"
                @click="expiration = preset.key"
              />
            </div>
          </div>

          <fieldset class="min-w-0">
            <legend class="text-sm font-medium">スコープ</legend>
            <p id="token-scopes-error" role="alert" class="text-sm text-(--koyori-color-danger)">
              {{ scopesError ?? '' }}
            </p>
            <div class="mt-2 flex flex-col gap-3">
              <div v-for="entry in SCOPE_CATALOG" :key="entry.scope" class="flex flex-col">
                <Checkbox
                  :label="entry.scope"
                  :name="`scope-${entry.scope}`"
                  :checked="selectedScopes.includes(entry.scope)"
                  :ariaDescribedBy="`token-scope-${entry.scope}-description token-scopes-error`"
                  :on-checked-change="(checked: boolean) => toggleScope(entry.scope, checked)"
                />
                <p
                  :id="`token-scope-${entry.scope}-description`"
                  class="text-muted-foreground pl-[calc(var(--koyori-control-height-sm)+var(--koyori-space-xs))] text-sm"
                >
                  {{ entry.description }}
                </p>
              </div>
            </div>
          </fieldset>

          <p v-if="submitError" class="text-destructive text-sm">{{ submitError }}</p>

          <div class="flex gap-2">
            <Button
              type="submit"
              :disabled="!isHydrated || createToken.isPending.value"
              :label="createToken.isPending.value ? '発行中…' : 'トークンを発行'"
            />
            <Button type="button" variant="tertiary" label="キャンセル" @click="resetForm" />
          </div>
        </div>
      </HydrationSafeForm>
    </div>

    <!-- 一覧 -->
    <Skeleton v-if="tokensQuery.isPending.value" :lines="2" height="4rem" />
    <p v-else-if="tokensQuery.isError.value" class="text-destructive text-sm">
      トークンの一覧を取得できませんでした。再読み込みしてください。
    </p>
    <template v-else-if="tokensQuery.isSuccess.value">
      <div
        v-if="issuableTenants.length === 0 && !tenantsQuery.isPending.value"
        class="text-muted-foreground text-sm"
      >
        トークンを発行できるのは、テナントオーナーと Admin だけです。
      </div>

      <p
        v-if="tokensQuery.data.value!.length === 0 && !isFormOpen"
        class="text-muted-foreground text-sm"
      >
        トークンはまだありません。
      </p>

      <ul
        v-else-if="tokensQuery.data.value!.length > 0"
        class="divide-y rounded-lg border"
        data-testid="token-list"
      >
        <li
          v-for="token in tokensQuery.data.value"
          :key="token.id"
          class="flex items-center gap-4 p-4"
        >
          <div
            class="bg-muted text-muted-foreground hidden size-10 shrink-0 place-content-center rounded-full sm:grid"
          >
            <PhKey class="size-5" />
          </div>
          <div class="flex min-w-0 flex-1 flex-col gap-0.5">
            <p class="truncate text-sm font-medium">{{ token.name }}</p>
            <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 text-xs">
              <code class="font-mono">{{ maskedToken(token.token_last_four) }}</code>
              <span>·</span>
              <span>{{ tenantName(token.tenant_id) }}</span>
              <span>·</span>
              <span>{{ token.scopes.length }} スコープ</span>
              <span>·</span>
              <span>{{ formatExpiry(token.expires_at) }}</span>
            </p>
            <p class="text-muted-foreground text-xs">{{ formatLastUsed(token.last_used_at) }}</p>
          </div>
          <Dropdown variant="tertiary" label="スコープを表示" :items="scopeItems(token.scopes)" />
          <Button
            type="button"
            variant="tertiary"
            class="text-(--koyori-color-danger)"
            label="取り消し"
            @click="openRevokeDialog(token)"
          />
        </li>
      </ul>
    </template>

    <!-- 取り消し確認 -->
    <Dialog
      :open="revokeTarget !== null"
      title="トークンを取り消しますか？"
      :description="revokeDescription"
      :on-close="onRevokeClose"
    >
      <p v-if="revokeError" class="text-destructive text-sm">{{ revokeError }}</p>
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          :disabled="revokeToken.isPending.value"
          label="キャンセル"
          @click="revokeTarget = null"
        />
        <Button
          type="button"
          variant="danger"
          :disabled="revokeToken.isPending.value"
          :label="revokeToken.isPending.value ? '取り消し中…' : '取り消す'"
          @click="onRevokeConfirm"
        />
      </template>
    </Dialog>
  </div>
</template>
