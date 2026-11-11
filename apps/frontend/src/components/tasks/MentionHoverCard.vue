<script setup lang="ts">
import { HoverCard } from '@koyori-app/ui-vue';
import { computed, ref } from 'vue';

import MentionProfileCard from '@/components/tasks/MentionProfileCard.vue';
import type { TenantUuid } from '@/lib/api-ids';
import { useUserProfileQuery } from '@/lib/api-vue-query';
import type { MentionTable } from '@/lib/comment-mention';
import {
  MENTION_CARD_SELECTOR,
  mentionCardTarget,
  profileMetaLine,
  type MentionCardTarget,
} from '@/lib/mention-card';
import { userProfileHref } from '@/lib/task-display';

/**
 * メンションの札のプロフィールカード。本文の容れ物（v-html の要素）1 つにつき 1 つ置く。
 * 札を 1 つずつ登録せず、容れ物の中の札（`MENTION_CARD_SELECTOR`）を HoverCard の委譲で受ける。
 * 入力欄（CodeMirror）の札には、容れ物の外なので当たらない。
 *
 * 名前・画像・本人かどうかは容れ物が持つ `mentions` の表から読み込みを待たずに出し、
 * 所属・自己紹介・ゲストの印はプロフィール API が返ってから足す。失敗（客分の閲覧者の 403・通信失敗・404）は
 * 読み込み中と同じ見た目のまま（エラーの文言は出さぬ）。
 */
const props = defineProps<{
  container: HTMLElement | null;
  mentions?: MentionTable;
  currentUserId?: string | null;
  /** プロフィール API のテナント（UUID）。無ければ取りに行かず、名前と画像だけ */
  tenantId?: string | null;
  /** 「プロフィールを見る」の道のテナント（表示用 id）。無ければリンクを出さぬ */
  mentionTenant?: string;
}>();

const target = ref<MentionCardTarget | null>(null);

// 取得する人は、表示する対象とは別に、閉じても最後の人のまま保つ。空に戻すと、取得の途中で閉じたとき
// クエリのキーが替わって取得が中断され（AbortSignal）、キャッシュに入らず開き直しで取り直しになる
const profileUserId = ref('');

function onOpenChange(open: boolean, trigger: HTMLElement) {
  // 閉じたら中身を空にし、次の札で前の人が一瞬見えるのを防ぐ
  target.value = open
    ? mentionCardTarget(trigger, props.mentions ?? {}, props.currentUserId)
    : null;
  if (target.value && target.value.kind !== 'gone') profileUserId.value = target.value.userId;
}

// 同じ人は、キャッシュのある間（最後に閉じてから gcTime の既定 5 分）開き直しても取りに行かぬ。失敗した人も同じ
const profileQuery = useUserProfileQuery(
  () => (props.tenantId || null) as TenantUuid | null,
  profileUserId,
  { staleTime: Infinity, cacheFailures: true },
);

const label = computed(() => {
  const current = target.value;
  if (!current) return 'メンションのプロフィール';
  return current.kind === 'gone' ? '退会した利用者' : `${current.name} のプロフィール`;
});

const cardProps = computed(() => {
  const current = target.value;
  if (!current) return null;
  if (current.kind === 'gone') return { kind: 'gone' as const };
  const profile = profileQuery.data.value;
  // 外れた者は 404 になり得るので、プロフィールが取れるまでリンクを出さぬ
  const linkable = current.kind === 'member' || !!profile;
  return {
    kind: current.kind,
    name: current.name,
    avatarUrl: current.avatarUrl,
    self: current.self,
    guest: profile?.membership === 'Guest',
    meta: profile ? profileMetaLine(profile) : null,
    bio: profile?.bio?.trim() || null,
    href:
      props.mentionTenant && linkable ? userProfileHref(props.mentionTenant, current.userId) : null,
  };
});
</script>

<template>
  <HoverCard
    :container="container"
    :selector="MENTION_CARD_SELECTOR"
    :label="label"
    :on-open-change="onOpenChange"
  >
    <template #content>
      <MentionProfileCard v-if="cardProps" v-bind="cardProps" />
    </template>
  </HoverCard>
</template>
