<script setup lang="ts">
import { useForm } from '@tanstack/vue-form';
import { type } from 'arktype';
import { useQueryClient } from '@tanstack/vue-query';
import { PhCopy } from '@phosphor-icons/vue';
import { computed, ref } from 'vue';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Field, FieldDescription, FieldError, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
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
  url: type('string').narrow(isUrl),
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
    url: props.webhook?.url ?? '',
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
        if (url !== original.url) body.url = url;
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
        await queryClient.invalidateQueries({ queryKey: ['get', WEBHOOKS_PATH] });
        createdSecret.value = created.secret;
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

function onOpenChange(open: boolean) {
  // 保存リクエスト進行中はダイアログを閉じない（結果の見逃し防止）
  if (!open && isPending.value) return;
  if (!open) emit('close');
}
</script>

<template>
  <Dialog :open="true" @update:open="onOpenChange">
    <DialogContent class="max-w-lg">
      <template v-if="createdSecret">
        <DialogHeader>
          <DialogTitle>Webhook を作成しました</DialogTitle>
          <DialogDescription>
            署名用のシークレットは今しか表示されません。必ずコピーして受信側に設定してください。
          </DialogDescription>
        </DialogHeader>
        <div class="flex items-center gap-2">
          <code
            class="min-w-0 flex-1 truncate rounded-md bg-muted px-3 py-2 font-mono text-sm"
            data-testid="created-secret"
            >{{ createdSecret }}</code
          >
          <Button type="button" variant="outline" size="sm" @click="copySecret">
            <PhCopy class="size-4" />
            {{ copied ? 'コピーしました' : 'コピー' }}
          </Button>
        </div>
        <p v-if="copyError" role="alert" class="text-sm text-destructive">{{ copyError }}</p>
        <DialogFooter>
          <Button type="button" @click="emit('close')">閉じる</Button>
        </DialogFooter>
      </template>

      <template v-else>
        <DialogHeader>
          <DialogTitle>{{ webhook ? 'Webhook を編集' : 'Webhook を追加' }}</DialogTitle>
          <DialogDescription
            >イベントを送る URL と形式、購読するイベントを設定します。</DialogDescription
          >
        </DialogHeader>

        <form class="flex flex-col gap-4" @submit.prevent="form.handleSubmit">
          <form.Field name="url">
            <template #default="{ field }">
              <Field>
                <FieldLabel for="webhook-url">URL</FieldLabel>
                <Input
                  id="webhook-url"
                  type="url"
                  class="font-mono"
                  placeholder="https://example.com/webhook"
                  :model-value="field.state.value"
                  @blur="field.handleBlur"
                  @update:model-value="(v) => field.handleChange(String(v))"
                />
                <FieldError v-if="field.state.meta.errors.length"
                  >URL の形式で入力してください</FieldError
                >
              </Field>
            </template>
          </form.Field>

          <form.Field name="secret">
            <template #default="{ field }">
              <Field>
                <FieldLabel for="webhook-secret">シークレット</FieldLabel>
                <Input
                  id="webhook-secret"
                  type="password"
                  autocomplete="new-password"
                  :placeholder="webhook ? '空欄なら変更しません' : ''"
                  :model-value="field.state.value"
                  @blur="field.handleBlur"
                  @update:model-value="(v) => field.handleChange(String(v))"
                />
                <FieldDescription>16 文字以上。Discord 形式では署名に使いません</FieldDescription>
                <FieldError v-if="field.state.meta.errors.length"
                  >シークレットは 16 文字以上で入力してください</FieldError
                >
              </Field>
            </template>
          </form.Field>

          <form.Field name="format">
            <template #default="{ field }">
              <Field>
                <FieldLabel for="webhook-format">形式</FieldLabel>
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
                      {{ format.label }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            </template>
          </form.Field>

          <form.Field name="events">
            <template #default="{ field }">
              <Field>
                <FieldLabel>イベント</FieldLabel>
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
                          field.handleChange(
                            toggleEvent(field.state.value, event.value, v === true),
                          )
                      "
                    />
                    {{ event.label }}
                  </label>
                </div>
                <FieldError v-if="field.state.meta.errors.length"
                  >イベントを 1 つ以上選んでください</FieldError
                >
              </Field>
            </template>
          </form.Field>

          <p v-if="submitError" role="alert" class="text-sm text-destructive">{{ submitError }}</p>

          <DialogFooter>
            <Button type="button" variant="outline" :disabled="isPending" @click="emit('close')">
              キャンセル
            </Button>
            <form.Subscribe>
              <template #default="{ canSubmit, isSubmitting }">
                <Button type="submit" :disabled="!canSubmit || isSubmitting || isPending">
                  {{
                    isSubmitting || isPending
                      ? '保存中…'
                      : webhook
                        ? '変更を保存'
                        : 'Webhook を追加'
                  }}
                </Button>
              </template>
            </form.Subscribe>
          </DialogFooter>
        </form>
      </template>
    </DialogContent>
  </Dialog>
</template>
