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
import { useTenantStore } from '@/stores/tenant';

/**
 * メンションの札のプロフィールカード。本文の容れ物（v-html の要素）1 つにつき 1 つ置く。
 * 札を 1 つずつ登録せず、容れ物の中の札（`MENTION_CARD_SELECTOR`）を HoverCard の委譲で受ける。
 * 入力欄（CodeMirror）の札には、容れ物の外なので当たらない。
 *
 * 名前・画像・本人かどうかは容れ物が持つ `mentions` の表から読み込みを待たずに出し、
 * 所属・自己紹介・ゲストの印はプロフィール API が返ってから足す。失敗（通信失敗・5xx・404）は
 * 読み込み中と同じ見た目のまま（エラーの文言は出さぬ）。
 *
 * 取得の決まり:
 * - 成功した人は、キャッシュの寿命（最後に購読が外れてから既定の 5 分）の間、開き直しても窓のフォーカスが
 *   戻っても取り直さぬ（`staleTime: Infinity`）。404 は null として同じくキャッシュされる
 * - 取得に AbortSignal を渡さぬので、取得の途中でカードを閉じても、別の人へ移っても取得は最後まで終わり、
 *   キャッシュに入る。同じ人の取得は 1 回で済む
 * - 失敗（403・5xx・通信の失敗）した人は、キャッシュに残らず、次に開いたときに取り直す
 * - 閲覧者がそのテナントのゲストなら取りに行かぬ（プロフィール API はゲストの閲覧者に 403 を返す）。
 *   名前とアバターだけのカードを出す
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

function onOpenChange(open: boolean, trigger: HTMLElement) {
  // 閉じたら中身を空にし、次の札で前の人が一瞬見えるのを防ぐ
  target.value = open
    ? mentionCardTarget(trigger, props.mentions ?? {}, props.currentUserId)
    : null;
}

// テナント一覧（ログイン後に読み込み済み）の membership から、閲覧者がこのテナントのゲストかを引く。
// 一覧に無い（読み込み前・別テナント）ときはゲストと見なさず取りに行く
const tenantStore = useTenantStore();
const viewerIsGuest = computed(
  () =>
    !!props.tenantId &&
    tenantStore.tenants.find((tenant) => tenant.id === props.tenantId)?.membership === 'Guest',
);

// 取る人は開いているカードの人。閉じる・退会した者では空になり、新たには取りに行かぬ（取得の途中のものは最後まで終わる。購読が外れ、キャッシュは gc まで残る）
const profileQuery = useUserProfileQuery(
  () => (props.tenantId && !viewerIsGuest.value ? props.tenantId : null) as TenantUuid | null,
  () => (target.value && target.value.kind !== 'gone' ? target.value.userId : ''),
  { staleTime: Infinity, abortable: false },
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
