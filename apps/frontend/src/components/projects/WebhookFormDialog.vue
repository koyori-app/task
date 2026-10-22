<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { PhCopy } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import { Button, Dialog, Field, Input } from '@koyori-app/ui-vue';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  WEBHOOK_EVENTS,
  WEBHOOK_FORMATS,
  webhookErrorMessage,
  type WebhookFormat,
} from '@/components/projects/webhook-events';
import { apiClient } from '@/lib/api-vue-query';
import { codePointLength } from '@/lib/code-points';
import type { components } from '@/generated/api';

type WebhookResponse = components['schemas']['WebhookResponse'];
type UpdateWebhookRequest = components['schemas']['UpdateWebhookRequest'];

const WEBHOOKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/webhooks' as const;
const WEBHOOK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/webhooks/{id}' as const;

const SECRET_MIN_LENGTH = 16;
/** 一覧 API が送信先 URL の代わりに返す値 */
const REDACTED_URL = '[redacted]';

/**
 * Webhook の作成・編集ダイアログ。`webhook` が null なら作成、あれば編集。
 * フォーム初期値を props から取るため、親は開くたびに `v-if` でマウントし直すこと。
 * 作成に成功したら同じダイアログで secret を一度だけ表示する（API は二度と返さない）。
 */
const props = defineProps<{
  tenantId: string;
  projectId: string;
  webhook: WebhookResponse | null;
}>();

const emit = defineEmits<{ close: [] }>();

const queryClient = useQueryClient();
const submitError = ref<string | null>(null);
/** 作成直後の平文 secret。この画面でしか見られない */
const createdSecret = ref<string | null>(null);
const copied = ref(false);
const copyError = ref<string | null>(null);
/** 伏せられた URL は欄に出さず、secret と同じく空欄を「変更しない」とする */
const urlHidden = props.webhook?.url === REDACTED_URL;

function isUrl(value: string) {
  try {
    new URL(value.trim());
    return true;
  } catch {
    return false;
  }
}

// secret の長さは backend（chars().count()）と同じくコードポイントで数える。
// 編集時の空欄は「変更しない」
const schema = type({
  url: type('string').narrow((url) => (urlHidden && url.trim() === '') || isUrl(url)),
  secret: type('string').narrow(
    (secret) =>
      (props.webhook !== null && secret === '') || codePointLength(secret) >= SECRET_MIN_LENGTH,
  ),
  format: type.enumerated(...WEBHOOK_FORMATS.map((f) => f.value)),
  events: type('string[]').narrow((events) => events.length >= 1),
});

const createMutation = apiClient.useMutation('post', WEBHOOKS_PATH);
const updateMutation = apiClient.useMutation('put', WEBHOOK_PATH);

const isPending = computed(() => createMutation.isPending.value || updateMutation.isPending.value);

const form = useForm({
  defaultValues: {
    url: urlHidden ? '' : (props.webhook?.url ?? ''),
    secret: '',
    format: (props.webhook?.format ?? 'json') as WebhookFormat,
    events: [...(props.webhook?.events ?? [])],
  },
  validators: { onSubmit: schema },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    const url = value.url.trim();
    try {
      if (props.webhook) {
        const original = props.webhook;
        // 変えたフィールドだけ送る（PUT は指定したものだけ更新する）
        const body: UpdateWebhookRequest = {};
        if (urlHidden ? url !== '' : url !== original.url) body.url = url;
        if (value.secret !== '') body.secret = value.secret;
        if (value.format !== original.format) body.format = value.format;
        const sameEvents =
          value.events.length === original.events.length &&
          value.events.every((e) => original.events.includes(e));
        if (!sameEvents) body.events = value.events;
        await updateMutation.mutateAsync({
          params: {
            path: { tenant_id: props.tenantId, project_id: props.projectId, id: original.id },
          },
          body,
        });
        await queryClient.invalidateQueries({ queryKey: ['get', WEBHOOKS_PATH] });
        emit('close');
      } else {
        const created = await createMutation.mutateAsync({
          params: { path: { tenant_id: props.tenantId, project_id: props.projectId } },
          body: { url, secret: value.secret, events: value.events, format: value.format },
        });
        createdSecret.value = created.secret;
        await queryClient.invalidateQueries({ queryKey: ['get', WEBHOOKS_PATH] });
      }
    } catch (e) {
      submitError.value = webhookErrorMessage(e, 'Webhook を保存できませんでした');
    }
  },
});

function toggleEvent(events: string[], event: string, checked: boolean) {
  return checked ? [...events, event] : events.filter((e) => e !== event);
}

async function copySecret() {
  if (!createdSecret.value) return;
  copyError.value = null;
  try {
    await navigator.clipboard.writeText(createdSecret.value);
    copied.value = true;
  } catch {
    // 平文はこの画面でしか見られないため、失敗を握り潰すと取り逃す。手動コピーへ誘導する
    copyError.value = 'コピーできませんでした。表示中のシークレットを選択してコピーしてください。';
  }
}

function onClose() {
  // 保存リクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (isPending.value) return;
  emit('close');
}
</script>

<template>
  <div class="contents [--koyori-dialog-width:32rem]">
    <Dialog
      :open="true"
      :title="
        createdSecret ? 'Webhook を作成しました' : webhook ? 'Webhook を編集' : 'Webhook を追加'
      "
      :description="
        createdSecret
          ? '署名用のシークレットは今しか表示されません。必ずコピーして受信側に設定してください。'
          : 'イベントを送る URL と形式、購読するイベントを設定します。'
      "
      :on-close="onClose"
    >
      <div v-if="createdSecret" class="grid gap-4">
        <div class="flex items-center gap-2">
          <code
            class="min-w-0 flex-1 truncate rounded-md bg-muted px-3 py-2 font-mono text-sm"
            data-testid="created-secret"
            >{{ createdSecret }}</code
          >
          <Button
            type="button"
            variant="tertiary"
            :label="copied ? 'コピーしました' : 'コピー'"
            @click="copySecret"
          >
            <template #icon><PhCopy class="size-4" /></template>
          </Button>
        </div>
        <p v-if="copyError" role="alert" class="text-sm text-destructive">{{ copyError }}</p>
      </div>

      <form
        v-else
        id="webhook-form"
        class="flex flex-col gap-4"
        @submit.prevent="form.handleSubmit"
      >
        <form.Field name="url">
          <template #default="{ field }">
            <Field
              id="webhook-url"
              label="URL"
              :description="
                urlHidden
                  ? '送信先の URL は認証情報を含みうるため表示しません。変えるときだけ入力してください'
                  : undefined
              "
              :error="field.state.meta.errors.length ? 'URL の形式で入力してください' : ''"
            >
              <Input
                type="url"
                class="[&_input]:font-mono"
                :placeholder="urlHidden ? '空欄なら変更しません' : 'https://example.com/webhook'"
                :value="field.state.value"
                :on-value-change="field.handleChange"
                :on-blur="field.handleBlur"
              />
            </Field>
          </template>
        </form.Field>

        <form.Field name="secret">
          <template #default="{ field }">
            <Field
              id="webhook-secret"
              label="シークレット"
              description="16 文字以上。Discord 形式では署名に使いません"
              :error="
                field.state.meta.errors.length ? 'シークレットは 16 文字以上で入力してください' : ''
              "
            >
              <Input
                type="password"
                autocomplete="new-password"
                :placeholder="webhook ? '空欄なら変更しません' : ''"
                :value="field.state.value"
                :on-value-change="field.handleChange"
                :on-blur="field.handleBlur"
              />
            </Field>
          </template>
        </form.Field>

        <form.Field name="format">
          <template #default="{ field }">
            <div class="flex flex-col gap-2">
              <Label for="webhook-format">形式</Label>
              <Select
                :model-value="field.state.value"
                @update:model-value="(v) => field.handleChange(v as WebhookFormat)"
              >
                <SelectTrigger id="webhook-format" class="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem
                    v-for="format in WEBHOOK_FORMATS"
                    :key="format.value"
                    :value="format.value"
                  >
                    <span class="inline-flex items-center gap-2">
                      <component :is="format.icon" class="size-4" aria-hidden="true" />
                      {{ format.label }}
                    </span>
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
          </template>
        </form.Field>

        <form.Field name="events">
          <template #default="{ field }">
            <div class="flex flex-col gap-2">
              <Label>イベント</Label>
              <div class="flex flex-col gap-2">
                <label
                  v-for="event in WEBHOOK_EVENTS"
                  :key="event.value"
                  class="flex cursor-pointer items-center gap-2 text-sm"
                >
                  <Checkbox
                    :model-value="field.state.value.includes(event.value)"
                    :aria-label="event.label"
                    @update:model-value="
                      (v) =>
                        field.handleChange(toggleEvent(field.state.value, event.value, v === true))
                    "
                  />
                  {{ event.label }}
                </label>
              </div>
              <p
                v-if="field.state.meta.errors.length"
                role="alert"
                class="text-sm text-(--koyori-color-danger)"
              >
                イベントを 1 つ以上選んでください
              </p>
            </div>
          </template>
        </form.Field>

        <p v-if="submitError" role="alert" class="text-sm text-destructive">{{ submitError }}</p>
      </form>

      <template #actions>
        <Button v-if="createdSecret" type="button" label="閉じる" @click="emit('close')" />
        <template v-else>
          <Button
            type="button"
            variant="tertiary"
            :disabled="isPending"
            label="キャンセル"
            @click="emit('close')"
          />
          <form.Subscribe>
            <template #default="{ canSubmit, isSubmitting }">
              <Button
                type="submit"
                form="webhook-form"
                :disabled="!canSubmit || isSubmitting || isPending"
                :label="
                  isSubmitting || isPending ? '保存中…' : webhook ? '変更を保存' : 'Webhook を追加'
                "
              />
            </template>
          </form.Subscribe>
        </template>
      </template>
    </Dialog>
  </div>
</template>
