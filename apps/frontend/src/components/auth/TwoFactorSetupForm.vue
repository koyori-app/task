<script setup lang="ts">
import { computed, ref } from 'vue';
import { Button, Field, Input } from '@koyori-app/ui-vue';
import HydrationSafeForm from '@/components/HydrationSafeForm.vue';

const props = defineProps<{
  otpauthUri: string;
  qrCodePng: string;
  pending: boolean;
  error: string | null;
  locked: boolean;
}>();
const emit = defineEmits<{ submit: [code: string] }>();

const value = ref('');

/** 手入力用のシークレット。otpauth URI の `secret` から取る。 */
const secret = computed(() => {
  try {
    return new URL(props.otpauthUri).searchParams.get('secret') ?? '';
  } catch {
    return '';
  }
});

function onSubmit() {
  const trimmed = value.value.trim();
  if (trimmed) emit('submit', trimmed);
}
</script>

<template>
  <HydrationSafeForm v-slot="{ isHydrated }" @submit="onSubmit">
    <div class="flex flex-col gap-6">
      <div class="flex flex-col items-center gap-2 text-center">
        <h1 class="text-2xl font-bold">2 段階認証の設定</h1>
        <p class="text-muted-foreground text-sm text-balance">
          このアカウントでは 2 段階認証が必須です。認証アプリで QR コードを読み取り、表示された 6
          桁のコードを入力してください。
        </p>
      </div>
      <div class="flex flex-col items-center gap-3">
        <img
          v-if="qrCodePng.startsWith('data:image/')"
          :src="qrCodePng"
          alt="認証アプリ用の QR コード"
          width="192"
          height="192"
          class="rounded-md border bg-white p-2"
        />
        <details class="w-full text-sm">
          <summary class="text-muted-foreground cursor-pointer text-center">
            QR コードを読み取れない場合
          </summary>
          <div class="mt-2 flex flex-col gap-2">
            <p class="text-muted-foreground">認証アプリに次のシークレットを手入力してください。</p>
            <code data-testid="totp-secret" class="rounded-md border bg-secondary p-2 break-all">{{
              secret
            }}</code>
            <p class="text-muted-foreground break-all">
              otpauth URL: <code data-testid="totp-uri">{{ otpauthUri }}</code>
            </p>
          </div>
        </details>
      </div>
      <Field id="two-factor-setup-code" label="認証アプリのコード" :error="error ?? ''">
        <Input
          name="code"
          placeholder="123456"
          autocomplete="one-time-code"
          :value="value"
          :on-value-change="(v: string) => (value = v)"
        />
      </Field>
      <Button
        type="submit"
        class="w-full"
        :disabled="pending || locked || !isHydrated"
        :label="pending ? '確認中…' : '設定を完了する'"
      />
    </div>
  </HydrationSafeForm>
</template>
