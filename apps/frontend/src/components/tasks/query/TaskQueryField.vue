<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { onClickOutside } from '@vueuse/core';
import {
  Combobox,
  Button,
  type ComboboxOption,
  type ComboboxSection,
  type ComboboxTag,
} from '@koyori-app/ui-vue';
import { Search } from '@lucide/vue';
import {
  tokenizeTaskQuery,
  serializeToken,
  serializeTaskQuery,
  dropEmptyValues,
} from '@/lib/task-query/tokenize';
import {
  suggestionContext,
  keyCandidates,
  valueCandidates,
  normalizeQueryValue,
  applySuggestion,
  type SuggestData,
  type Candidate,
} from '@/lib/task-query/suggest';
import TaskQuerySyntaxCard from './TaskQuerySyntaxCard.vue';
const props = defineProps<{ modelValue: string; suggestData?: SuggestData }>();
const emit = defineEmits<{ 'update:modelValue': [value: string]; focus: [] }>();
const root = ref<HTMLElement>();
const draft = ref('');
const committed = ref('');
const syntax = ref(false);
const caret = ref(0);
/**
 * 確定のあとは候補を出さない。確定で下書きが空になると、キーの候補が結果の上にかぶさって
 * Esc か欄の外を押すまで消えないため。次の入力・↑↓・欄のクリック・欄への再フォーカスで戻す。
 */
const closed = ref(false);
const chips = computed(() =>
  tokenizeTaskQuery(committed.value).filter((t) => t.kind === 'qualifier'),
);
const tags = computed<ComboboxTag[]>(() =>
  chips.value.map((token) => {
    const text = serializeToken(token);
    const prefix = `${token.negated ? '-' : ''}${token.key}:`;
    return { prefix, label: text.slice(prefix.length), removeLabel: `${text} を外す` };
  }),
);
watch(
  () => props.modelValue,
  (value) => {
    const tokens = tokenizeTaskQuery(value);
    committed.value = serializeTaskQuery(tokens.filter((t) => t.kind === 'qualifier'));
    draft.value = serializeTaskQuery(tokens.filter((t) => t.kind !== 'qualifier'));
    caret.value = draft.value.length;
    syncCaret();
  },
  { immediate: true },
);
const context = computed(() => suggestionContext(draft.value, caret.value));
const candidates = computed(() => {
  const c = context.value;
  return c?.kind === 'keys'
    ? keyCandidates(c.prefix)
    : c?.kind === 'values'
      ? valueCandidates(c.key, c.prefix, props.suggestData ?? {}, c.chosen)
      : [];
});
/**
 * 候補の識別子。Combobox は選択中の候補をこの値で覚えるので、候補を作り直しても（同じ key と value なら）
 * 選択が外れない。「ラベルなし」のような別キーの候補（no:label）と、同じ値の通常の候補を区別する。
 */
function optionValue(candidate: Candidate) {
  const c = context.value;
  return c?.kind === 'values' ? `${candidate.key ?? c.key}:${candidate.value}` : candidate.value;
}
/** 入力が候補の値と完全に一致する候補の位置。「なし」の行は数えない。 */
const exactIndex = computed(() => {
  const c = context.value;
  if (c?.kind !== 'values') return -1;
  return candidates.value.findIndex(
    (v) =>
      v.key !== 'no' &&
      normalizeQueryValue(c.key, v.value) === normalizeQueryValue(c.key, c.prefix),
  );
});
function highlight(label: string): [number, number][] | undefined {
  const prefix = context.value?.prefix ?? '';
  const index = prefix ? label.toLowerCase().indexOf(prefix.toLowerCase()) : -1;
  return index < 0 ? undefined : [[index, index + prefix.length]];
}
const options = computed<ComboboxSection[]>(() => {
  const c = context.value;
  if (closed.value || syntax.value || !c || !candidates.value.length) return [];
  return [
    {
      heading: c.kind === 'values' ? `${c.key}: の候補` : '条件の一覧',
      options: candidates.value.map((candidate): ComboboxOption => ({
        value: optionValue(candidate),
        label: candidate.label,
        description: candidate.hint,
        meta: candidate.count !== undefined ? `${candidate.count} 件` : undefined,
        highlight: highlight(candidate.label),
      })),
    },
  ];
});
function commit() {
  closed.value = true;
  emit(
    'update:modelValue',
    serializeTaskQuery(dropEmptyValues(tokenizeTaskQuery(`${committed.value} ${draft.value}`))),
  );
}
/** 値を入れる。確定しない候補では一覧を開いたままにし、続けて残りの値を選ばせる。 */
function select(value: string) {
  const candidate = candidates.value.find((c) => optionValue(c) === value);
  if (!candidate || !context.value) return;
  const result = applySuggestion(draft.value, context.value, candidate);
  draft.value = result.draft;
  caret.value = result.caret;
  // 確定する候補では位置を渡さない。親が式を返すと下書きは語だけになり、位置がずれるため
  syncCaret(result.commit ? undefined : result.caret);
  if (result.commit) commit();
}
function remove(index: number) {
  committed.value = serializeTaskQuery(chips.value.filter((_, i) => i !== index));
  commit();
}
/**
 * 値を差し替えると入力欄のキャレットは末尾へ動くが、Combobox が覚えている位置は古いまま。
 * 古いままだと、のちに矢印でその位置へ戻しても動いたと見なされず、候補が古い文脈で出る。
 * keyup を送って Combobox にいまの位置を数え直させる。
 * 後ろに語が残る下書きでは、値を差し替えるとブラウザがキャレットを末尾へ動かすので `at` へ戻す。
 */
function syncCaret(at?: number) {
  nextTick(() => {
    const input = root.value?.querySelector('input');
    if (at !== undefined) input?.setSelectionRange(at, at);
    input?.dispatchEvent(new Event('keyup'));
  });
}
function reopen(event: Event) {
  if (event.target instanceof HTMLInputElement) closed.value = false;
}
function update(value: string, at: number) {
  closed.value = false;
  draft.value = value;
  caret.value = at;
  syntax.value = false;
}
function keydown(event: KeyboardEvent) {
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') reopen(event);
  // 早見表は欄の外（「?」のボタンなど）にフォーカスがあっても Esc で閉じる
  if (event.key !== 'Escape' || !syntax.value) return;
  if (event.isComposing || event.keyCode === 229) return;
  event.preventDefault();
  syntax.value = false;
}
onClickOutside(root, () => {
  syntax.value = false;
});
</script>
<template>
  <div
    ref="root"
    class="relative min-w-0 flex-1"
    @keydown="keydown"
    @click="reopen"
    @focusin="
      reopen($event);
      emit('focus');
    "
  >
    <Combobox
      aria-label="タスクを検索"
      placeholder="タスクを検索..."
      :value="draft"
      :tags="tags"
      :options="options"
      :auto-highlight="exactIndex === 0"
      :on-value-change="update"
      :on-caret-change="(at: number) => (caret = at)"
      :on-select="select"
      :on-commit="commit"
      :on-tag-remove="remove"
    >
      <template #prefix><Search class="size-4 shrink-0" /></template>
      <template #suffix
        ><Button variant="ghost" ariaLabel="書き方の早見表" :on-click="() => (syntax = !syntax)"
          >?</Button
        ></template
      >
      <template #footer
        >↑↓ で選ぶ　Enter で入れる（続けて選べます）　選ばずに Enter で検索　Esc で閉じる<br />「-」を前に付けると除外（-label:bug）</template
      >
    </Combobox>
    <div v-if="syntax" class="absolute inset-x-0 top-full z-50 mt-1"><TaskQuerySyntaxCard /></div>
  </div>
</template>
