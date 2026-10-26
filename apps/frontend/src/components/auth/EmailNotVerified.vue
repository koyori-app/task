<script setup lang="ts">
import { ref } from 'vue';
import { Button } from '@koyori-app/ui-vue';
import { useResendVerificationEmailMutation } from '@/lib/api-vue-query';

const props = defineProps<{ email: string; backHref?: string; resetHref?: string }>();
const emit = defineEmits<{ logout: [] }>();

const resendMutation = useResendVerificationEmailMutation();
const sent = ref(false);

async function resend() {
  await resendMutation.mutateAsync({ body: { email: props.email } });
  sent.value = true;
}
</script>

<template>
  <div class="flex min-h-svh items-center justify-center">
    <div class="flex max-w-md flex-col items-center gap-6 px-4 text-center">
      <div class="flex flex-col gap-2">
        <h1 class="text-2xl font-bold">メールアドレスを確認してください</h1>
        <p class="text-muted-foreground text-sm">
          <span class="font-medium text-foreground">{{ props.email }}</span>
          に確認メールを送信しました。メール内のリンクをクリックして認証を完了してください。
        </p>
      </div>
      <div class="flex flex-col items-center gap-3">
        <p v-if="sent" class="text-sm text-green-600 dark:text-green-400">
          確認メールを再送しました。
        </p>
        <Button
          :disabled="resendMutation.isPending.value || sent"
          variant="tertiary"
          :label="resendMutation.isPending.value ? '送信中…' : '確認メールを再送する'"
          @click="resend"
        />
        <a v-if="props.backHref" :href="props.backHref">
          <Button variant="ghost" label="サインインページへ戻る" />
        </a>
        <Button v-else variant="ghost" label="サインアウト" @click="emit('logout')" />
        <a v-if="props.resetHref" :href="props.resetHref">
          <Button variant="ghost" label="パスワードを再設定する" />
        </a>
      </div>
    </div>
  </div>
</template>
