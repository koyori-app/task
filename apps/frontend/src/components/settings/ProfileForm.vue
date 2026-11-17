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
/** backend の `AVATAR_MAX_BYTES` と同じ上限。超える画像は送る前に止める。 */
const AVATAR_MAX_BYTES = 2 * 1024 * 1024;
const AVATAR_TOO_LARGE = '画像は 2 MiB 以下にしてください。';
const apiBase = (import.meta.env.VITE_API_BASE ?? '/api').replace(/\/$/, '');

function validateUsername(raw: string) {
  const length = codePointLength(raw.trim());
  if (length < USERNAME_MIN) return `${USERNAME_MIN}文字以上で入力してください。`;
  if (length > USERNAME_MAX) return `${USERNAME_MAX}文字以内で入力してください。`;
  return undefined;
}

function validateBio(value: string) {
  return codePointLength(value) > BIO_MAX ? `${BIO_MAX}文字以内で入力してください。` : undefined;
}

const queryClient = useQueryClient();
const authStore = useAuthStore();
const updateProfile = useUpdateProfileMutation();
const submitError = ref<string | null>(null);
const saved = ref(false);
// 保存済みのアバターの URL。アップロードの成功と削除の保存で差し替える
const avatarUrl = ref(props.user.avatar_url ?? '');
// 「削除」を押してまだ保存していない。保存で clear_avatar_url を送る
const avatarRemoved = ref(false);
// 保存済みの値はスキームを問わずそのまま表示する（http の旧データや開発環境の自サーバー）
const previewAvatarUrl = computed(() => (avatarRemoved.value ? null : avatarUrl.value || null));
const avatarFileInput = ref<HTMLInputElement | null>(null);
const uploading = ref(false);

const form = useForm({
  defaultValues: {
    username: props.user.username,
    bio: props.user.bio ?? '',
  },
  onSubmit: async ({ value }) => {
    submitError.value = null;
    saved.value = false;
    const username = value.username.trim();
    const removeAvatar = avatarRemoved.value;
    try {
      await updateProfile.mutateAsync({
        body: {
          username,
          bio: value.bio,
          ...(removeAvatar ? { clear_avatar_url: true } : {}),
        },
      });
      if (removeAvatar) {
        avatarUrl.value = '';
        avatarRemoved.value = false;
      }
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
    avatarUrl.value = updated.avatar_url ?? '';
    avatarRemoved.value = false;
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
      <Field
        id="avatar"
        label="アバター"
        description="画像（PNG・JPEG・GIF・WebP、2 MiB まで）をアップロードできます。"
      >
        <div
          class="flex items-center gap-4"
          role="group"
          aria-labelledby="avatar-label"
          aria-describedby="avatar-description"
        >
          <UserAvatar
            :user="{ name: user.username, avatar_url: previewAvatarUrl }"
            size="2xl"
            shape="lg"
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
              :disabled="!avatarUrl || avatarRemoved"
              label="削除"
              @click="
                avatarRemoved = true;
                resetSubmitFeedback();
              "
            />
          </div>
        </div>
      </Field>

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
