<script setup lang="ts">
import { computed, h } from 'vue';

import UserAvatar from '@/components/UserAvatar.vue';
import {
  activityLine,
  timelineDateTime,
  timelineTime,
  type ActivityLookups,
  type ActivityValue,
  type HistoryEntry,
} from '@/lib/activity-timeline';

/**
 * タイムラインの履歴 1 件。同じ人の連続した操作を束ねて出す。
 *
 * コメント（14px）より小さい 12px にして、読む物と記録を見分けられるようにする。
 * 利用者が選ぶ色は読めるとは限らないので、文字には使わず点とアイコンに載せる。
 */
const props = defineProps<{
  entry: HistoryEntry;
  lookups?: ActivityLookups;
}>();

const lines = computed(() =>
  props.entry.items.map((item) => ({ id: item.id, line: activityLine(item, props.lookups) })),
);

function ValueLabel({ value }: { value: ActivityValue }) {
  const mark = value.icon
    ? h(value.icon, {
        class: 'size-3.5 shrink-0',
        style: { color: value.color ?? undefined },
        'aria-hidden': 'true',
      })
    : value.color
      ? h('span', {
          class: 'size-2 shrink-0 rounded-full',
          style: { backgroundColor: value.color },
          'aria-hidden': 'true',
        })
      : null;
  return h('span', { class: 'inline-flex min-w-0 items-center gap-1' }, [
    mark,
    h('span', { class: 'min-w-0 break-words' }, value.text),
  ]);
}
ValueLabel.props = ['value'];
</script>

<template>
  <div class="flex items-start gap-2 px-2 py-1.5 text-xs" data-activity-history>
    <span class="flex h-5 w-6 shrink-0 items-center justify-center">
      <UserAvatar :user="{ name: entry.actor }" size="sm" decorative />
    </span>
    <div class="min-w-0 flex-1">
      <p class="flex h-5 items-center gap-1.5">
        <span class="truncate font-medium text-(--koyori-color-text-secondary)">{{
          entry.actor
        }}</span>
        <time
          class="shrink-0 text-muted-foreground"
          :datetime="entry.at"
          :title="timelineDateTime(entry.at)"
          >{{ timelineTime(entry.at) }}</time
        >
      </p>
      <ul class="flex flex-col gap-0.5 text-(--koyori-color-text-secondary)">
        <li
          v-for="{ id, line } in lines"
          :key="id"
          class="flex min-h-4.5 min-w-0 items-start"
          data-activity-line
        >
          <template v-if="line.kind === 'text'">
            <span class="min-w-0 break-words">{{ line.text }}</span>
          </template>
          <template v-else>
            <span class="w-16.5 shrink-0 text-muted-foreground">{{ line.label }}</span>
            <span class="flex min-w-0 flex-1 flex-wrap items-center gap-x-1">
              <template v-if="line.kind === 'change'">
                <template v-if="line.from">
                  <ValueLabel :value="line.from" />
                  <span class="text-muted-foreground" aria-hidden="true">→</span>
                  <span class="sr-only">から</span>
                </template>
                <ValueLabel :value="line.to" />
              </template>
              <!-- 長い値が折り返しても、増減の印を値から離さない -->
              <span v-else-if="line.kind === 'membership'" class="flex min-w-0 items-start gap-1">
                <span aria-hidden="true">{{ line.added ? '＋' : '−' }}</span>
                <span class="sr-only">{{ line.added ? '追加' : '削除' }}</span>
                <ValueLabel :value="line.value" />
              </span>
              <template v-else>
                <a
                  v-if="line.url"
                  :href="line.url"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-link underline underline-offset-2"
                  :class="line.mono && 'font-mono'"
                  >{{ line.text }}</a
                >
                <span v-else :class="line.mono && 'font-mono'">{{ line.text }}</span>
                <span v-if="line.verb">{{ line.verb }}</span>
                <span
                  v-if="line.note"
                  class="line-clamp-2 min-w-0 break-words"
                  :title="line.note"
                  >{{ line.note }}</span
                >
              </template>
            </span>
          </template>
        </li>
      </ul>
    </div>
  </div>
</template>
