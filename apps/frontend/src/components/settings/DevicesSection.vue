<script setup lang="ts">
import { useQueryClient } from '@tanstack/vue-query';
import { PhDesktop } from '@phosphor-icons/vue';
import { ref } from 'vue';
import { Button, Dialog } from '@koyori-app/ui-vue';
import { Skeleton } from '@/components/ui/skeleton';
import { devicesQueryOptions, useDevicesQuery, useRevokeDeviceMutation } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';
import { formatExpiry, formatLastUsed, maskedToken } from '@/lib/personal-tokens';

type Device = components['schemas']['DeviceToken'];

const queryClient = useQueryClient();
const devicesQuery = useDevicesQuery();
const revokeDevice = useRevokeDeviceMutation();

const revokeTarget = ref<Device | null>(null);
const revokeError = ref<string | null>(null);

function onRevokeClose() {
  if (!revokeDevice.isPending.value) revokeTarget.value = null;
}

async function onRevokeConfirm() {
  if (!revokeTarget.value) return;
  revokeError.value = null;
  try {
    await revokeDevice.mutateAsync({ params: { path: { id: revokeTarget.value.id } } });
    await queryClient.invalidateQueries({ queryKey: devicesQueryOptions().queryKey });
    revokeTarget.value = null;
  } catch {
    revokeError.value = '端末を失効できませんでした。時間をおいて再度お試しください。';
  }
}
</script>

<template>
  <div class="flex flex-col gap-3">
    <div class="flex flex-col gap-1">
      <h3 class="text-base font-semibold">Koyori Desktop の端末</h3>
      <p class="text-muted-foreground text-sm">
        承認した Koyori Desktop の一覧です。失効した端末はすぐにサインアウトされます。
      </p>
    </div>

    <Skeleton v-if="devicesQuery.isPending.value" class="h-16 w-full" />
    <p v-else-if="devicesQuery.isError.value" class="text-destructive text-sm">
      端末の一覧を取得できませんでした。再読み込みしてください。
    </p>
    <p v-else-if="devicesQuery.data.value!.length === 0" class="text-muted-foreground text-sm">
      承認した端末はありません。
    </p>
    <ul v-else class="divide-y rounded-lg border" data-testid="device-list">
      <li
        v-for="device in devicesQuery.data.value"
        :key="device.id"
        class="flex items-center gap-4 p-4"
      >
        <div
          class="bg-muted text-muted-foreground hidden size-10 shrink-0 place-content-center rounded-full sm:grid"
        >
          <PhDesktop class="size-5" />
        </div>
        <div class="flex min-w-0 flex-1 flex-col gap-0.5">
          <p class="truncate text-sm font-medium">{{ device.name }}</p>
          <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 text-xs">
            <code class="font-mono">{{ maskedToken(device.token_last_four) }}</code>
            <span>·</span>
            <span>{{ formatExpiry(device.expires_at) }}</span>
          </p>
          <p class="text-muted-foreground text-xs">{{ formatLastUsed(device.last_used_at) }}</p>
        </div>
        <Button
          type="button"
          variant="tertiary"
          class="text-(--koyori-color-danger)"
          label="失効"
          @click="
            revokeTarget = device;
            revokeError = null;
          "
        />
      </li>
    </ul>

    <Dialog
      :open="revokeTarget !== null"
      title="端末を失効しますか？"
      :description="
        revokeTarget
          ? `「${revokeTarget.name}」を失効します。この端末の Koyori Desktop はすぐにサインアウトされ、使うにはもう一度承認が必要です。`
          : ''
      "
      :on-close="onRevokeClose"
    >
      <p v-if="revokeError" class="text-destructive text-sm">{{ revokeError }}</p>
      <template #actions>
        <Button
          type="button"
          variant="tertiary"
          :disabled="revokeDevice.isPending.value"
          label="キャンセル"
          @click="revokeTarget = null"
        />
        <Button
          type="button"
          variant="danger"
          :disabled="revokeDevice.isPending.value"
          :label="revokeDevice.isPending.value ? '失効中…' : '失効する'"
          @click="onRevokeConfirm"
        />
      </template>
    </Dialog>
  </div>
</template>
