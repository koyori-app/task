<script setup lang="ts">
import { computed } from 'vue';
import { getActivePinia } from 'pinia';
import { PhUserPlus } from '@phosphor-icons/vue';

import UserAvatar from '@/components/UserAvatar.vue';
import { Button } from '@/components/ui/button';
import UserSelector from '@/components/users/UserSelector.vue';
import { useAuthStore } from '@/stores/auth';

export type PickableMember = { id: string; username: string; avatar_url?: string | null };

/**
 * 担当者の選択。一覧の行・まとめの一覧・詳細の 3 箇所で使う。
 *
 * 中身は `UserSelector`（利用者を選ぶ所は全てこれを通す）。担当は複数付けられ、
 * 一人ずつ保存するので `toggle` で付け外しを返す（契約は作り直す前と同じ）。
 */
const props = withDefaults(
  defineProps<{
    members: PickableMember[];
    /** 選択済みのユーザー（表示にも使うので id だけでなく実体で受ける） */
    selected: PickableMember[];
    disabled?: boolean;
    /**
     * 候補の取得状態。取得中・失敗を「候補が 0 人」と混ぜると、
     * 権限や通信で候補が取れていないのに「メンバーがいません」と出てしまう。
     */
    membersState?: { loading?: boolean; error?: boolean; onRetry?: () => void };
    /** 重ねて出すアバターの最大数。超過分は +N */
    maxDisplay?: number;
    /** 自分の id。候補で自分を選んだ者の次に置く。省けばログイン中の利用者 */
    currentUserId?: string | null;
  }>(),
  { maxDisplay: 3, currentUserId: undefined },
);

const emit = defineEmits<{
  toggle: [userId: string, checked: boolean];
}>();

// 試験などで pinia の無い所でも組めるよう、在る時だけ引く。
const authStore = getActivePinia() ? useAuthStore() : null;
const me = computed(() => props.currentUserId ?? authStore?.user?.id ?? null);

const selectedIds = computed(() => props.selected.map((user) => user.id));
const shown = computed(() => props.selected.slice(0, props.maxDisplay));
const hiddenCount = computed(() => Math.max(0, props.selected.length - props.maxDisplay));
const label = computed(() =>
  props.selected.length
    ? `担当者: ${props.selected.map((user) => user.username).join('、')}`
    : '担当者を割り当てる',
);
</script>

<template>
  <UserSelector
    :users="members"
    :model-value="selectedIds"
    :disabled="disabled"
    :state="membersState"
    :current-user-id="me"
    search-label="メンバーを検索"
    empty-text="担当者に指定できる利用者がいません"
    no-match-text="一致するメンバーがいません"
    @toggle="(userId, checked) => emit('toggle', userId, checked)"
  >
    <template #trigger>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        class="h-7 gap-0 px-1"
        :disabled="disabled"
        :aria-label="label"
      >
        <template v-if="selected.length">
          <UserAvatar
            v-for="user in shown"
            :key="user.id"
            :user="user"
            size="md"
            decorative
            class="-ml-1.5 border border-background first:ml-0"
          />
          <span v-if="hiddenCount" class="ml-1 text-xs text-muted-foreground">
            +{{ hiddenCount }}
          </span>
        </template>
        <PhUserPlus v-else class="size-4 text-muted-foreground" />
      </Button>
    </template>
  </UserSelector>
</template>
