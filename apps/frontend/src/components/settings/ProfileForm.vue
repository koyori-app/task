<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { useForm } from '@tanstack/vue-form';
import { computed, ref } from 'vue';
import UserAvatar from '@/components/UserAvatar.vue';
import { Button, Field, Input, Textarea } from '@koyori-app/ui-vue';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';
import { meQueryOptions, useUpdateProfileMutation } from '@/lib/api-vue-query';
import { codePointLength } from '@/lib/code-points';
import { useAuthStore } from '@/stores/auth';
import type { components } from '@/generated/api';

const props = defineProps<{ user: components['schemas']['UserResponse'] }>();

const USERNAME_MIN = 3;
const USERNAME_MAX = 255;
const BIO_MAX = 1000;
const AVATAR_URL_MAX = 2048;
/** backend の `AVATAR_MAX_BYTES` と同じ上限。超える画像は送る前に止める。 */
const AVATAR_MAX_BYTES = 2 * 1024 * 1024;
const AVATAR_TOO_LARGE = '画像は 2 MiB 以下にしてください。';
const apiBase = (import.meta.env.VITE_API_BASE ?? '/api').replace(/\/$/, '');

/** backend の `validate_avatar_url` と同じ判定。ずれると保存時まで気づけない。 */
function isHttpsUrl(value: string) {
  const lowered = value.toLowerCase();
  return lowered.startsWith('https://');
}

function validateUsername(raw: string) {
  const length = codePointLength(raw.trim());
  if (length < USERNAME_MIN) return `${USERNAME_MIN}文字以上で入力してください。`;
  if (length > USERNAME_MAX) return `${USERNAME_MAX}文字以内で入力してください。`;
  return undefined;
}

function validateBio(value: string) {
  return codePointLength(value) > BIO_MAX ? `${BIO_MAX}文字以内で入力してください。` : undefined;
}

function validateAvatarUrl(raw: string) {
  // 送信時と同じく trim した値で判定する。貼り付けで前後に空白が入っても、
  // 画面で弾いておいて送信では通る（またはその逆）というずれを作らない。
  const value = raw.trim();
  if (value === '') return undefined;
  if (codePointLength(value) > AVATAR_URL_MAX)
    return `${AVATAR_URL_MAX}文字以内で入力してください。`;
  if (!isHttpsUrl(value)) return 'https:// で始まる URL を入力してください。';
  return undefined;
}

const queryClient = useQueryClient();
const authStore = useAuthStore();
const updateProfile = useUpdateProfileMutation();
const submitError = ref<string | null>(null);
const saved = ref(false);
// アップロードで avatar_url が変わったら差し替える（未変更判定と検証の除外に使う）
const initialAvatarUrl = ref(props.user.avatar_url ?? '');
const avatarFileInput = ref<HTMLInputElement | null>(null);
const uploading = ref(false);

const form = useForm({
  defaultValues: {
    username: props.user.username,
    bio: props.user.bio ?? '',
    avatarUrl: initialAvatarUrl.value,
  },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    saved.value = false;
    const username = value.username.trim();
    const avatarUrl = value.avatarUrl.trim();
    try {
      await updateProfile.mutateAsync({
        body: {
          username,
          bio: value.bio,
          ...(value.avatarUrl === initialAvatarUrl.value
            ? {}
            : avatarUrl === ''
              ? { clear_avatar_url: true }
              : { avatar_url: avatarUrl }),
        },
      });
      await queryClient.invalidateQueries({ queryKey: meQueryOptions().queryKey });
      saved.value = true;
    } catch (e) {
      const err = e as { response?: { status?: number } };
      submitError.value =
        err.response?.status === 400
          ? '入力内容を確認してください。'
          : '保存できませんでした。時間をおいて再度お試しください。';
    }
  },
});

function resetSubmitFeedback() {
  saved.value = false;
  submitError.value = null;
}

/** 現在値（保存済みの値）なら、http の旧データや自サーバーの URL でもプレビューする。 */
function canPreviewAvatar(value: string) {
  return (
    value !== '' && (value === initialAvatarUrl.value || validateAvatarUrl(value) === undefined)
  );
}

async function uploadAvatar(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  // 同じファイルを選び直しても change が発火するよう空に戻す
  input.value = '';
  if (!file) return;
  resetSubmitFeedback();
  if (file.size > AVATAR_MAX_BYTES) {
    submitError.value = AVATAR_TOO_LARGE;
    return;
  }

  uploading.value = true;
  try {
    const body = new FormData();
    body.append('file', file);
    const res = await fetch(`${apiBase}/v1/auth/me/avatar`, {
      method: 'POST',
      body,
      credentials: 'include',
    });
    if (!res.ok) {
      submitError.value =
        res.status === 413
          ? AVATAR_TOO_LARGE
          : res.status === 400
            ? 'PNG・JPEG・GIF・WebP の画像を選んでください。'
            : 'アップロードできませんでした。時間をおいて再度お試しください。';
      return;
    }
    const updated = (await res.json()) as components['schemas']['UserResponse'];
    initialAvatarUrl.value = updated.avatar_url ?? '';
    form.setFieldValue('avatarUrl', initialAvatarUrl.value);
    queryClient.setQueryData(meQueryOptions().queryKey, updated);
    authStore.setUser(updated);
    saved.value = true;
  } catch {
    submitError.value = 'アップロードできませんでした。時間をおいて再度お試しください。';
  } finally {
    uploading.value = false;
  }
}
</script>

<template>
  <HydrationSafeForm
    v-slot="{ isHydrated }"
    @submit="() => form.handleSubmit()"
    @input="resetSubmitFeedback"
  >
    <div class="flex flex-col gap-6">
      <form.Field
        name="avatarUrl"
        :validators="{
          onBlur: ({ value }) =>
            value === initialAvatarUrl ? undefined : validateAvatarUrl(value),
          onSubmit: ({ value }) =>
            value === initialAvatarUrl ? undefined : validateAvatarUrl(value),
        }"
      >
        <template #default="{ field }">
          <Field
            :id="field.name"
            label="アバター"
            description="画像（PNG・JPEG・GIF・WebP、2 MiB まで）をアップロードするか、画像の URL を指定します。"
            :error="field.state.meta.errors.length ? field.state.meta.errors[0] : ''"
          >
            <div class="flex items-center gap-4">
              <UserAvatar
                :user="{
                  name: user.username,
                  avatar_url: canPreviewAvatar(field.state.value) ? field.state.value.trim() : null,
                }"
                size="2xl"
                shape="lg"
              />
              <div class="flex flex-1 flex-col gap-2">
                <Input
                  :name="field.name"
                  type="url"
                  input-mode="url"
                  placeholder="https://example.com/avatar.png"
                  :value="field.state.value"
                  :on-value-change="field.handleChange"
                  :on-blur="field.handleBlur"
                />
                <div class="flex gap-2">
                  <input
                    ref="avatarFileInput"
                    type="file"
                    accept="image/png,image/jpeg,image/gif,image/webp"
                    class="hidden"
                    data-testid="avatar-file-input"
                    @change="uploadAvatar"
                  />
                  <Button
                    type="button"
                    variant="tertiary"
                    :disabled="uploading"
                    :label="uploading ? 'アップロード中…' : '画像を選ぶ'"
                    @click="avatarFileInput?.click()"
                  />
                  <Button
                    type="button"
                    variant="tertiary"
                    :disabled="!field.state.value"
                    label="削除"
                    @click="
                      field.handleChange('');
                      resetSubmitFeedback();
                    "
                  />
                </div>
              </div>
            </div>
          </Field>
        </template>
      </form.Field>

      <form.Field
        name="username"
        :validators="{
          onBlur: ({ value }) => validateUsername(value),
          onSubmit: ({ value }) => validateUsername(value),
        }"
      >
        <template #default="{ field }">
          <Field
            :id="field.name"
            label="ユーザー名"
            :error="field.state.meta.errors.length ? field.state.meta.errors[0] : ''"
          >
            <Input
              :name="field.name"
              autocomplete="username"
              :value="field.state.value"
              :on-value-change="field.handleChange"
              :on-blur="field.handleBlur"
            />
          </Field>
        </template>
      </form.Field>

      <form.Field
        name="bio"
        :validators="{
          onBlur: ({ value }) => validateBio(value),
          onSubmit: ({ value }) => validateBio(value),
        }"
      >
        <template #default="{ field }">
          <Field
            :id="field.name"
            label="自己紹介"
            :error="field.state.meta.errors.length ? field.state.meta.errors[0] : ''"
          >
            <Textarea
              :name="field.name"
              :rows="4"
              :value="field.state.value"
              :on-value-change="field.handleChange"
              :on-blur="field.handleBlur"
            />
          </Field>
        </template>
      </form.Field>

      <p v-if="submitError" class="text-destructive text-sm">{{ submitError }}</p>
      <p v-else-if="saved" class="text-muted-foreground text-sm">保存しました。</p>

      <form.Subscribe>
        <template #default="{ canSubmit, isSubmitting }">
          <div>
            <Button
              type="submit"
              :disabled="!canSubmit || isSubmitting || !isHydrated"
              :label="isSubmitting ? '保存中…' : '変更を保存'"
            />
          </div>
        </template>
      </form.Subscribe>
    </div>
  </HydrationSafeForm>
</template>
