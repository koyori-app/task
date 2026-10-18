<script setup lang="ts">
import { computed } from 'vue';

import CommentMention from '@/components/tasks/CommentMention.vue';
import { splitBody, type MentionTable } from '@/lib/comment-mention';

/**
 * コメント本文の表示。`<@利用者ID>` の印だけを `@今の名` の札に描き、他の字は今までどおり素テキスト
 * （`whitespace-pre-wrap`。v-html は使わない）。既存のコメントにある `@username` の文字は印ではないので、
 * 札にせず素の文字のまま描く。
 */
const props = defineProps<{
  body: string;
  /** 印の id から引ける表示情報（API の `mentions`） */
  mentions?: MentionTable;
  currentUserId?: string | null;
  profileHref?: (userId: string) => string;
}>();

const segments = computed(() => splitBody(props.body, props.mentions ?? {}));
</script>

<template>
  <p class="whitespace-pre-wrap text-sm leading-relaxed">
    <template v-for="(segment, index) in segments" :key="index"
      ><CommentMention
        v-if="segment.kind === 'mention'"
        :user="segment.user"
        :user-id="segment.id"
        :current-user-id="currentUserId"
        :profile-href="profileHref"
      /><template v-else>{{ segment.text }}</template></template
    >
  </p>
</template>
