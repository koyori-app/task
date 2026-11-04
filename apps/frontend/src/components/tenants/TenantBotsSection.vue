<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { PhCopy } from '@phosphor-icons/vue';
import { Button, Checkbox, Dialog, Field, Input, Picker, Skeleton } from '@koyori-app/ui-vue';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { apiClient, fetchClient } from '@/lib/api-vue-query';
import {
  SCOPE_CATALOG,
  expiresAtFromPreset,
  formatExpiry,
  formatLastUsed,
  maskedToken,
  type TokenScope,
} from '@/lib/personal-tokens';
import type { components } from '@/generated/api';

type BotToken = components['schemas']['PersonalTokenResponse'];
type TenantRole = components['schemas']['TenantRole'];

const props = defineProps<{ tenantId: string }>();

const BOTS = '/v1/tenants/{tenant_id}/bots' as const;
const BOT = '/v1/tenants/{tenant_id}/bots/{bot_id}' as const;
const TOKENS = '/v1/tenants/{tenant_id}/bots/{bot_id}/tokens' as const;
const TOKEN = '/v1/tenants/{tenant_id}/bots/{bot_id}/tokens/{token_id}' as const;

/** BOT の PAT は期限を必ず持つ（API も無期限を拒む）。上限は 1 年。 */
const EXPIRY_PRESETS = [
  { key: '30d', label: '30 日' },
  { key: '90d', label: '90 日' },
  { key: '1y', label: '1 年' },
] as const;
type BotExpiry = (typeof EXPIRY_PRESETS)[number]['key'];

const ROLE_ITEMS: { value: TenantRole; label: string }[] = [
  { value: 'Member', label: 'Member' },
  { value: 'Viewer', label: 'Viewer' },
  { value: 'Admin', label: 'Admin' },
];

const queryClient = useQueryClient();
const botsQuery = apiClient.useQuery(
  'get',
  BOTS,
  { params: { path: { tenant_id: props.tenantId } } },
  { retry: false },
);

const selectedId = ref('');
const selectedBot = computed(() =>
  botsQuery.data.value?.find((bot) => bot.id === selectedId.value),
);
const tokensQuery = useQuery(
  computed(() => ({
    ...apiClient.queryOptions('get', TOKENS, {
      params: { path: { tenant_id: props.tenantId, bot_id: selectedId.value } },
    }),
    enabled: !!selectedBot.value,
    retry: false,
  })),
);
const projectsQuery = apiClient.useQuery(
  'get',
  '/v1/tenants/{tenant_id}/projects',
  { params: { path: { tenant_id: props.tenantId } } },
  { enabled: computed(() => !!selectedBot.value), retry: false },
);
const projectItems = computed(() =>
  (projectsQuery.data.value ?? []).map((project) => ({ value: project.id, label: project.name })),
);

const username = ref('');
const role = ref<TenantRole>('Member');
const tokenName = ref('');
const scopes = ref<TokenScope[]>(['read:task']);
const expiry = ref<BotExpiry>('30d');
const restrictProjects = ref(false);
const projectIds = ref<string[]>([]);
const busy = ref(false);
const error = ref('');

/** 発行直後の平文の PAT。この画面でしか見られず、閉じるか別の BOT へ移れば消す。 */
const createdToken = ref<string | null>(null);
const copied = ref(false);
const copyError = ref<string | null>(null);

watch(selectedId, () => {
  createdToken.value = null;
  tokenName.value = '';
  restrictProjects.value = false;
  projectIds.value = [];
});

async function refresh() {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ['get', BOTS] }),
    queryClient.invalidateQueries({ queryKey: ['get', TOKENS] }),
    queryClient.invalidateQueries({ queryKey: ['get', '/v1/tenants/{tenant_id}/members'] }),
  ]);
}

async function perform(operation: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    await operation();
    await refresh();
  } catch {
    error.value = '操作に失敗しました。権限と入力内容を確認してください。';
  } finally {
    busy.value = false;
  }
}

async function createBot() {
  const name = username.value.trim();
  if (!name) return;
  await perform(async () => {
    const result = await fetchClient.POST(BOTS, {
      params: { path: { tenant_id: props.tenantId } },
      body: { username: name, role: role.value },
    });
    if (!result.data || result.error) throw new Error('create bot');
    selectedId.value = result.data.id;
    username.value = '';
  });
}

async function setSuspended(id: string, isSuspended: boolean) {
  await perform(async () => {
    const result = await fetchClient.PATCH(BOT, {
      params: { path: { tenant_id: props.tenantId, bot_id: id } },
      body: { is_suspended: isSuspended },
    });
    if (!result.response.ok) throw new Error('suspend bot');
  });
}

function toggleScope(scope: TokenScope, checked: boolean) {
  scopes.value = checked
    ? [...scopes.value, scope]
    : scopes.value.filter((entry) => entry !== scope);
}

const canIssue = computed(
  () =>
    !!selectedBot.value &&
    !selectedBot.value.is_suspended &&
    tokenName.value.trim() !== '' &&
    scopes.value.length > 0 &&
    (!restrictProjects.value || projectIds.value.length > 0),
);

async function issueToken() {
  if (!canIssue.value) return;
  await perform(async () => {
    const result = await fetchClient.POST(TOKENS, {
      params: { path: { tenant_id: props.tenantId, bot_id: selectedId.value } },
      body: {
        name: tokenName.value.trim(),
        scopes: scopes.value,
        project_ids: restrictProjects.value ? projectIds.value : null,
        expires_at: expiresAtFromPreset(expiry.value)!,
      },
    });
    if (!result.data || result.error) throw new Error('issue token');
    createdToken.value = result.data.token;
    copied.value = false;
    copyError.value = null;
    tokenName.value = '';
  });
}

async function copyCreatedToken() {
  if (!createdToken.value) return;
  copyError.value = null;
  try {
    await navigator.clipboard.writeText(createdToken.value);
    copied.value = true;
  } catch {
    copyError.value = 'コピーできませんでした。表示中のトークンを選択してコピーしてください。';
  }
}

// --- 取り消し ---

const revokeTarget = ref<BotToken | null>(null);
const revokeDescription = computed(() =>
  revokeTarget.value && selectedBot.value
    ? `BOT「${selectedBot.value.username}」の「${revokeTarget.value.name}」を取り消します。このトークンを使っている連携は即座に認証できなくなります。この操作は元に戻せません。`
    : '',
);

async function onRevokeConfirm() {
  const target = revokeTarget.value;
  if (!target) return;
  await perform(async () => {
    const result = await fetchClient.DELETE(TOKEN, {
      params: {
        path: { tenant_id: props.tenantId, bot_id: selectedId.value, token_id: target.id },
      },
    });
    if (!result.response.ok) throw new Error('revoke token');
  });
  revokeTarget.value = null;
}
</script>

<template>
  <section
    class="mb-7 flex flex-col gap-4 rounded-[10px] border p-4"
    aria-labelledby="tenant-bots-title"
  >
    <div class="flex flex-col gap-1">
      <h2 id="tenant-bots-title" class="text-sm font-semibold">BOT アカウント</h2>
      <p class="text-muted-foreground text-sm">
        このテナントだけに属する自動化用のアカウントです。画面には入れず、PAT で API と CLI
        を使います。
      </p>
    </div>
    <p v-if="error" role="alert" class="text-destructive text-sm">{{ error }}</p>

    <HydrationSafeForm v-slot="{ isHydrated }" @submit="createBot">
      <div class="flex flex-wrap items-end gap-3">
        <Field id="bot-name" label="BOT 名" class="min-w-48 flex-1">
          <Input
            placeholder="例: release-pipeline"
            :value="username"
            :on-value-change="(v: string) => (username = v)"
          />
        </Field>
        <Field id="bot-role" label="ロール">
          <Picker
            label="ロール"
            :items="ROLE_ITEMS"
            :selected-values="[role]"
            :on-selection-change="(values: string[]) => (role = (values[0] as TenantRole) ?? role)"
          />
        </Field>
        <Button
          type="submit"
          :disabled="!isHydrated || busy || !username.trim()"
          label="BOT を作成"
        />
      </div>
    </HydrationSafeForm>

    <Skeleton v-if="botsQuery.isPending.value" :lines="2" height="3rem" />
    <p v-else-if="botsQuery.isError.value" role="alert" class="text-destructive text-sm">
      BOT の一覧を取得できませんでした。
      <Button type="button" variant="tertiary" label="再試行" @click="botsQuery.refetch()" />
    </p>
    <p v-else-if="!botsQuery.data.value?.length" class="text-muted-foreground text-sm">
      BOT はまだありません。
    </p>
    <ul v-else class="divide-y rounded-lg border" data-testid="bot-list">
      <li
        v-for="bot in botsQuery.data.value"
        :key="bot.id"
        class="flex flex-wrap items-center gap-3 px-3.5 py-2.5"
      >
        <span
          class="bg-foreground text-background rounded-sm px-1 text-[10px] leading-4 font-semibold"
          >BOT</span
        >
        <div class="flex min-w-0 flex-1 flex-col">
          <span class="truncate text-sm font-medium">{{ bot.username }}</span>
          <span class="text-muted-foreground text-xs"
            >{{ bot.role }}、{{ bot.is_suspended ? '停止中' : '有効' }}</span
          >
        </div>
        <Button
          type="button"
          variant="tertiary"
          :disabled="busy"
          :aria-pressed="selectedId === bot.id ? 'true' : 'false'"
          label="PAT を管理"
          @click="selectedId = bot.id"
        />
        <Button
          type="button"
          variant="tertiary"
          :disabled="busy"
          :label="bot.is_suspended ? '再開' : '停止'"
          @click="setSuspended(bot.id, !bot.is_suspended)"
        />
      </li>
    </ul>

    <section
      v-if="selectedBot"
      class="flex flex-col gap-4 border-t pt-4"
      aria-labelledby="bot-tokens-title"
    >
      <div class="flex flex-col gap-1">
        <h3 id="bot-tokens-title" class="text-sm font-semibold">
          {{ selectedBot.username }} の PAT
        </h3>
        <p class="text-muted-foreground text-sm">
          停止すると発行済みの PAT をすべて取り消します。再開した後は新しい PAT を発行してください。
        </p>
      </div>

      <!-- 発行直後の平文の PAT。再表示できないためコピーを促す。 -->
      <div
        v-if="createdToken"
        class="flex flex-col gap-2 rounded-lg border border-green-600/40 bg-green-600/5 p-4"
      >
        <p class="text-sm font-medium">BOT の PAT を発行しました。</p>
        <p class="text-muted-foreground text-sm">
          このトークンは今しか表示されません。必ずコピーして安全な場所に保管してください。
        </p>
        <div class="flex items-center gap-2">
          <code
            class="bg-muted min-w-0 flex-1 truncate rounded-md px-3 py-2 font-mono text-sm"
            data-testid="created-bot-token"
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
          <Button type="button" variant="tertiary" label="閉じる" @click="createdToken = null" />
        </div>
        <p v-if="copyError" role="alert" class="text-destructive text-sm">{{ copyError }}</p>
      </div>

      <HydrationSafeForm v-slot="{ isHydrated }" @submit="issueToken">
        <div class="flex flex-col gap-5 rounded-lg border p-4">
          <Field id="bot-token-name" label="トークン名" description="用途がわかる名前を付けます。">
            <Input
              placeholder="例: nightly-sync"
              :value="tokenName"
              :on-value-change="(v: string) => (tokenName = v)"
            />
          </Field>

          <div class="flex flex-col gap-2">
            <p class="text-sm font-medium">有効期限</p>
            <div class="flex flex-wrap gap-2">
              <Button
                v-for="preset in EXPIRY_PRESETS"
                :key="preset.key"
                type="button"
                :variant="expiry === preset.key ? 'primary' : 'tertiary'"
                :ariaPressed="expiry === preset.key ? 'true' : 'false'"
                :label="preset.label"
                @click="expiry = preset.key"
              />
            </div>
          </div>

          <fieldset class="min-w-0">
            <legend class="text-sm font-medium">スコープ</legend>
            <div class="mt-2 flex flex-col gap-3">
              <div v-for="entry in SCOPE_CATALOG" :key="entry.scope" class="flex flex-col">
                <Checkbox
                  :label="entry.scope"
                  :name="`bot-scope-${entry.scope}`"
                  :checked="scopes.includes(entry.scope)"
                  :ariaDescribedBy="`bot-scope-${entry.scope}-description`"
                  :on-checked-change="(checked: boolean) => toggleScope(entry.scope, checked)"
                />
                <p
                  :id="`bot-scope-${entry.scope}-description`"
                  class="text-muted-foreground pl-[calc(var(--koyori-control-height-sm)+var(--koyori-space-xs))] text-sm"
                >
                  {{ entry.description }}
                </p>
              </div>
            </div>
          </fieldset>

          <div class="flex flex-col gap-2">
            <Checkbox
              label="プロジェクトを限定する"
              name="bot-restrict-projects"
              :checked="restrictProjects"
              :on-checked-change="(checked: boolean) => (restrictProjects = checked)"
            />
            <Field v-if="restrictProjects" id="bot-token-projects" label="使えるプロジェクト">
              <Picker
                label="使えるプロジェクト"
                selection-mode="multiple"
                :searchable="projectItems.length > 8"
                :items="projectItems"
                :selected-values="projectIds"
                :on-selection-change="(values: string[]) => (projectIds = values)"
              />
            </Field>
          </div>

          <div>
            <Button
              type="submit"
              :disabled="!isHydrated || busy || !canIssue"
              :label="busy ? '発行中…' : 'PAT を発行'"
            />
          </div>
        </div>
      </HydrationSafeForm>

      <p v-if="tokensQuery.isError.value" role="alert" class="text-destructive text-sm">
        PAT の一覧を取得できませんでした。
      </p>
      <p v-else-if="tokensQuery.data.value?.length === 0" class="text-muted-foreground text-sm">
        PAT はまだありません。
      </p>
      <ul
        v-else-if="tokensQuery.data.value"
        class="divide-y rounded-lg border"
        data-testid="bot-token-list"
      >
        <li
          v-for="token in tokensQuery.data.value"
          :key="token.id"
          class="flex flex-wrap items-center gap-3 px-3.5 py-2.5"
        >
          <div class="flex min-w-0 flex-1 flex-col gap-0.5">
            <p class="truncate text-sm font-medium">{{ token.name }}</p>
            <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 text-xs">
              <code class="font-mono">{{ maskedToken(token.token_last_four) }}</code>
              <span>{{ token.scopes?.length ?? 0 }} スコープ</span>
              <span>{{ formatExpiry(token.expires_at) }}</span>
              <span>{{ formatLastUsed(token.last_used_at) }}</span>
            </p>
          </div>
          <Button
            type="button"
            variant="tertiary"
            class="text-(--koyori-color-danger)"
            :disabled="busy"
            label="取り消し"
            @click="revokeTarget = token"
          />
        </li>
      </ul>
    </section>

    <Dialog
      :open="revokeTarget !== null"
      title="BOT の PAT を取り消しますか？"
      :description="revokeDescription"
      :on-close="() => !busy && (revokeTarget = null)"
    >
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          :disabled="busy"
          label="キャンセル"
          @click="revokeTarget = null"
        />
        <Button
          type="button"
          variant="danger"
          :disabled="busy"
          :label="busy ? '取り消し中…' : '取り消す'"
          @click="onRevokeConfirm"
        />
      </template>
    </Dialog>
  </section>
</template>
