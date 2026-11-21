<script setup lang="ts">
import { computed, ref } from 'vue';
import { Button, Checkbox, Field, Picker } from '@koyori-app/ui-vue';
import type { AuthorizationPreview } from '@/lib/oauth-authorize';
import { SCOPE_CATALOG, type TokenScope } from '@/lib/personal-tokens';

const props = defineProps<{
  /** 同意の API が確かめた、画面に出す中身。 */
  preview: AuthorizationPreview;
  /** 接続を使えるテナントの候補（自分が所属するテナント）。 */
  tenants: { id: string; name: string }[];
  /** サインイン中の利用者の名前。 */
  username: string;
  /** 承認か拒否を送っている間。 */
  pending?: boolean;
  error?: string | null;
}>();

const emit = defineEmits<{
  approve: [choice: { tenantIds: string[]; scopes: TokenScope[] }];
  deny: [];
}>();

const SCOPE_DESCRIPTIONS = new Map(SCOPE_CATALOG.map((entry) => [entry.scope, entry.description]));

/**
 * 戻り先が利用者の端末（loopback）のクライアントは、端末で動く別のプロセスが同じ `client_id` を
 * 名乗れる。頼んでいない同意画面に気づけるよう、選択を済ませた状態では出さない。
 */
const isLoopback = computed(() => props.preview.redirect_is_loopback);

const tenantItems = computed(() =>
  props.tenants.map((tenant) => ({ value: tenant.id, label: tenant.name })),
);

/** null = 未操作。 */
const chosenTenantIds = ref<string[] | null>(null);
const chosenScopes = ref<TokenScope[] | null>(null);

/** 候補が 1 つなら選択済みで始める。loopback のクライアントでは、1 つでも未選択から始める。 */
const tenantIds = computed(
  () =>
    chosenTenantIds.value ??
    (!isLoopback.value && props.tenants.length === 1 ? [props.tenants[0]!.id] : []),
);

/** 求められた scope をチェック済みで出す。loopback のクライアントでは、書き込みだけ外して出す。 */
const scopes = computed(
  () =>
    chosenScopes.value ??
    props.preview.scopes.filter((scope) => !(isLoopback.value && scope.startsWith('write:'))),
);

function toggleScope(scope: TokenScope, checked: boolean) {
  const next = checked ? [...scopes.value, scope] : scopes.value.filter((s) => s !== scope);
  // 送る順は、求められた順に揃える
  chosenScopes.value = props.preview.scopes.filter((s) => next.includes(s));
}

const canApprove = computed(
  () => !props.pending && tenantIds.value.length > 0 && scopes.value.length > 0,
);

function approve() {
  if (!canApprove.value) return;
  emit('approve', { tenantIds: tenantIds.value, scopes: scopes.value });
}
</script>

<template>
  <div class="flex flex-col gap-5 text-left">
    <div class="flex flex-col gap-1">
      <!-- 名前はクライアントの自己申告なので、client_id の host を主に出す -->
      <h1 class="text-xl font-bold break-words">
        <span data-testid="client-host">{{ preview.client_host }}</span>
        のアプリが接続を求めています
      </h1>
      <p class="text-muted-foreground text-sm break-words">
        アプリが名乗る名前: <span data-testid="client-name">{{ preview.client_name }}</span>
      </p>
    </div>

    <dl class="flex flex-col gap-2 text-sm">
      <div class="flex flex-col">
        <dt class="text-muted-foreground">許可のあとの戻り先</dt>
        <dd class="font-mono break-all" data-testid="redirect-host">{{ preview.redirect_host }}</dd>
      </div>
      <div class="flex flex-col">
        <dt class="text-muted-foreground">接続するアカウント</dt>
        <dd>{{ username }}</dd>
      </div>
    </dl>

    <p v-if="tenants.length === 0" role="alert" class="text-sm" data-testid="no-tenants">
      接続できるテナントがありません。テナントに所属すると接続できます。
    </p>

    <template v-else>
      <Field
        id="oauth-consent-tenant"
        label="テナント"
        description="アプリは、選んだテナントの中でだけ動けます。複数選べます。"
      >
        <Picker
          label="テナント"
          selection-mode="multiple"
          :searchable="tenantItems.length > 8"
          :items="tenantItems"
          :selected-values="tenantIds"
          :on-selection-change="(values: string[]) => (chosenTenantIds = values)"
        />
      </Field>

      <fieldset class="min-w-0">
        <legend class="text-sm font-medium">アプリに許可すること</legend>
        <div class="mt-2 flex flex-col gap-3">
          <div v-for="scope in preview.scopes" :key="scope" class="flex flex-col">
            <Checkbox
              :label="scope"
              :name="`oauth-scope-${scope}`"
              :checked="scopes.includes(scope)"
              :ariaDescribedBy="`oauth-scope-${scope}-description`"
              :on-checked-change="(checked: boolean) => toggleScope(scope, checked)"
            />
            <p
              :id="`oauth-scope-${scope}-description`"
              class="text-muted-foreground pl-[calc(var(--koyori-control-height-sm)+var(--koyori-space-xs))] text-sm"
            >
              {{ SCOPE_DESCRIPTIONS.get(scope) }}
            </p>
          </div>
        </div>
      </fieldset>

      <p
        v-if="isLoopback"
        role="note"
        class="rounded-lg border border-amber-600/50 bg-amber-600/10 p-3 text-sm"
        data-testid="loopback-warning"
      >
        この端末で動くアプリが接続を求めています。自分で Claude Code や Codex
        から接続を始めた直後でなければ、拒否してください。
      </p>
    </template>

    <p v-if="error" role="alert" class="text-destructive text-sm">{{ error }}</p>

    <div class="flex flex-col gap-2">
      <Button
        v-if="tenants.length > 0"
        type="button"
        :disabled="!canApprove"
        :label="pending ? '送信中…' : '許可する'"
        @click="approve"
      />
      <Button
        type="button"
        variant="tertiary"
        :disabled="pending"
        label="拒否する"
        @click="emit('deny')"
      />
    </div>
    <p class="text-muted-foreground text-xs">
      許可した接続は、アカウント設定の「連携中のアプリ」から解除できます。
    </p>
  </div>
</template>
