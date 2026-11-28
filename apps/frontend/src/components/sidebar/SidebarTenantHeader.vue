<script setup lang="ts">
import { Dropdown, EllipsisIcon } from '@koyori-app/ui-vue';

import { useTenantMenu } from '@/composables/useTenantMenu';
import type { Tenant } from '@/stores/tenant';

const props = defineProps<{
  /** 今のテナント。決まらないあいだは null で、メニューを出さない */
  tenant: Tenant | null;
  /** 見出しに出す名前。読み込み中などの事情は呼び出し側が tenantStatusLabel で作る */
  label: string;
  /** サイドバーを畳んだ状態。名前は読み上げだけにして「…」を残す */
  rail?: boolean;
}>();

const { items, onSelect } = useTenantMenu(() => props.tenant);
</script>

<template>
  <div class="flex items-center gap-1 border-b pb-2" :class="rail ? 'justify-center' : ''">
    <!-- 切り替えは左のレールで行うので、見出しは押せない -->
    <h2 class="min-w-0 flex-1 truncate font-bold text-base" :class="rail ? 'sr-only' : ''">
      {{ label }}
    </h2>
    <Dropdown
      v-if="tenant"
      variant="ghost"
      ariaLabel="テナントのメニュー"
      :items="items"
      :on-select="onSelect"
      top-layer
    >
      <template #icon><EllipsisIcon /></template>
    </Dropdown>
  </div>
</template>
