<script setup lang="ts">
import { PhCheck, PhWarningCircle, PhX } from '@phosphor-icons/vue';
import { computed } from 'vue';
import { Button } from '@koyori-app/ui-vue';

import { commitHref, type ForgeTarget } from '@/lib/forge-host';
import { mergeGateView, type ReviewSummary } from '@/lib/review-findings';

const props = defineProps<{
  summary: ReviewSummary;
  /** コミットへのリンクを組む材料（PR の行） */
  target: ForgeTarget;
}>();

const emit = defineEmits<{ 'show-unresolved': []; compose: [] }>();

const view = computed(() => mergeGateView(props.summary));

const TONE_CLASS = {
  success: 'text-(--koyori-color-success)',
  danger: 'text-(--koyori-color-danger)',
  warning: 'text-(--koyori-color-warning)',
} as const;

/** 条件の文を、コミットの置き場所（`{sha}`）の前後に分ける。 */
const conditionParts = computed(() => {
  const condition = view.value.condition;
  if (!condition) return null;
  const [before, after = ''] = condition.sha ? condition.text.split('{sha}') : [condition.text];
  const full = props.summary.cached_pr_head_sha ?? condition.sha;
  return {
    before,
    after,
    sha: condition.sha,
    full,
    href: condition.sha && full ? commitHref(props.target, full) : null,
  };
});

const ACTION_LABELS = { 'show-unresolved': '未解決を見る', compose: '指摘を起票' } as const;

function runAction(action: 'show-unresolved' | 'compose') {
  if (action === 'compose') emit('compose');
  else emit('show-unresolved');
}
</script>

<template>
  <section
    class="bg-card flex flex-col gap-3 rounded-lg border p-4"
    aria-label="マージ判定"
    data-testid="merge-gate"
    :data-gate="view.kind"
  >
    <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
      <component
        :is="view.kind === 'ready' ? PhCheck : PhWarningCircle"
        class="size-4 shrink-0"
        :class="TONE_CLASS[view.tone]"
        aria-hidden="true"
      />
      <h2 class="text-sm font-semibold">{{ view.title }}</h2>
      <span v-if="view.checkedAt" class="text-muted-foreground text-xs">{{ view.checkedAt }}</span>
    </div>

    <div
      v-if="view.condition && conditionParts"
      class="flex flex-wrap items-center gap-2 text-sm"
      data-testid="merge-gate-condition"
    >
      <PhX class="size-4 shrink-0 text-(--koyori-color-danger)" aria-hidden="true" />
      <p class="min-w-0 flex-1">
        {{ conditionParts.before
        }}<template v-if="conditionParts.sha"
          ><a
            v-if="conditionParts.href"
            class="text-link font-mono underline underline-offset-2"
            :href="conditionParts.href"
            :title="conditionParts.full ?? undefined"
            target="_blank"
            rel="noopener noreferrer"
            >{{ conditionParts.sha }}</a
          ><span v-else class="font-mono" :title="conditionParts.full ?? undefined">{{
            conditionParts.sha
          }}</span></template
        >{{ conditionParts.after }}
      </p>
      <Button
        v-if="view.condition.action"
        type="button"
        variant="ghost"
        :label="ACTION_LABELS[view.condition.action]"
        @click="runAction(view.condition.action)"
      />
    </div>

    <!-- 代行の条件（作成者の不在）はオーナー自身が作れるので、マージ可否を読むこの場所に痕跡を出す -->
    <p
      v-if="summary.owner_override_rejections > 0"
      class="text-xs text-(--koyori-color-text-secondary)"
      data-testid="merge-gate-override"
    >
      オーナー代行での棄却 {{ summary.owner_override_rejections }} 件
    </p>
  </section>
</template>
