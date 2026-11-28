<script setup lang="ts">
import { IconRail, IconRailItem, IconRailSeparator, Skeleton } from '@koyori-app/ui-vue';
import { PhArrowClockwise, PhPlus } from '@phosphor-icons/vue';

import type { Tenant } from '@/stores/tenant';

/**
 * テナントのレール（表示だけ）。丸・角丸・バー・ツールチップ・右クリックと長押しの検出は ui の IconRailItem が持つ。
 * 切り替え・作成・メニューの中身は呼び出し側（AppTenantRail）が配線する。
 */
defineProps<{
  tenants: Tenant[];
  selectedTenantId: string | null;
  loading?: boolean;
  error?: string | null;
  /** 右クリック・長押しのメニューを開いているテナント。その項目のツールチップを止める */
  menuTenantId?: string | null;
}>();

const emit = defineEmits<{
  select: [tenant: Tenant];
  retry: [];
  create: [];
  menu: [tenant: Tenant, position: { x: number; y: number }];
}>();
</script>

<template>
  <IconRail label="テナント">
    <div v-if="loading" class="contents" aria-busy="true">
      <span class="sr-only">テナントを読み込み中…</span>
      <Skeleton v-for="n in 3" :key="n" width="40px" height="40px" radius="50%" />
    </div>
    <IconRailItem
      v-else-if="error"
      id="tenant-rail-retry"
      variant="action"
      :label="`${error}（再試行）`"
      :on-click="() => emit('retry')"
    >
      <template #icon><PhArrowClockwise :size="20" /></template>
    </IconRailItem>
    <template v-else>
      <span v-if="tenants.length === 0" class="sr-only">所属テナントがありません</span>
      <IconRailItem
        v-for="tenant in tenants"
        :id="`tenant-rail-${tenant.id}`"
        :key="tenant.id"
        :label="tenant.name"
        :src="tenant.icon_url || undefined"
        :emoji="tenant.icon_emoji ?? undefined"
        :selected="tenant.id === selectedTenantId"
        :menu-open="tenant.id === menuTenantId"
        :on-click="() => emit('select', tenant)"
        :on-menu="(position) => emit('menu', tenant, position)"
      />
    </template>
    <IconRailSeparator />
    <IconRailItem
      id="tenant-rail-create"
      variant="action"
      label="テナントを作成"
      :disabled="loading"
      :on-click="() => emit('create')"
    >
      <template #icon><PhPlus :size="20" /></template>
    </IconRailItem>
  </IconRail>
</template>
