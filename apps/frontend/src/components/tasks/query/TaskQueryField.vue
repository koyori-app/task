<script setup lang="ts">
import { computed, ref, watch, useId, watchEffect } from 'vue';
import { onClickOutside } from '@vueuse/core';
import { Input, Tag, Button } from '@koyori-app/ui-vue';
import { Search } from '@lucide/vue';
import { tokenizeTaskQuery, serializeToken, serializeTaskQuery } from '@/lib/task-query/tokenize';
import {
  suggestionContext,
  keyCandidates,
  valueCandidates,
  applySuggestion,
  type SuggestData,
  type Candidate,
} from '@/lib/task-query/suggest';
import TaskQuerySuggestList from './TaskQuerySuggestList.vue';
import TaskQuerySyntaxCard from './TaskQuerySyntaxCard.vue';
const props = defineProps<{ modelValue: string; suggestData?: SuggestData }>();
const emit = defineEmits<{ 'update:modelValue': [value: string]; focus: [] }>();
const root = ref<HTMLElement>();
const input = ref<InstanceType<typeof Input>>();
const draft = ref('');
const committed = ref('');
const open = ref(false);
const syntax = ref(false);
const caret = ref(0);
const active = ref(-1);
const explicitSelection = ref(false);
const listId = useId();
const chips = computed(() =>
  tokenizeTaskQuery(committed.value).filter((t) => t.kind === 'qualifier'),
);
watch(
  () => props.modelValue,
  (value) => {
    const tokens = tokenizeTaskQuery(value);
    committed.value = serializeTaskQuery(tokens.filter((t) => t.kind === 'qualifier'));
    draft.value = serializeTaskQuery(tokens.filter((t) => t.kind !== 'qualifier'));
  },
  { immediate: true },
);
const context = computed(() => suggestionContext(draft.value, caret.value));
const candidates = computed(() => {
  const c = context.value;
  return c?.kind === 'keys'
    ? keyCandidates(c.prefix)
    : c?.kind === 'values'
      ? valueCandidates(c.key, c.prefix, props.suggestData ?? {})
      : [];
});
watch([context, candidates], () => {
  explicitSelection.value = false;
  const c = context.value;
  const normalize = (v: string) =>
    (c?.kind === 'values' && ['assignee', 'author'].includes(c.key)
      ? v.replace(/^@/, '')
      : v
    ).toLowerCase();
  active.value =
    c?.kind === 'values'
      ? candidates.value.findIndex(
          (v) => v.key !== 'no' && normalize(v.value) === normalize(c.prefix),
        )
      : -1;
});
const visible = computed(() => open.value && !syntax.value && candidates.value.length > 0);
function nativeInput() {
  return input.value?.$el?.querySelector('input') as HTMLInputElement | undefined;
}
watchEffect(() => {
  const el = nativeInput();
  if (!el) return;
  el.setAttribute('role', 'combobox');
  el.setAttribute('aria-expanded', String(visible.value));
  el.setAttribute('aria-controls', listId);
  el.setAttribute('aria-autocomplete', 'list');
  if (visible.value && active.value >= 0)
    el.setAttribute('aria-activedescendant', `${listId}-${active.value}`);
  else el.removeAttribute('aria-activedescendant');
});
function commit() {
  emit(
    'update:modelValue',
    serializeTaskQuery(tokenizeTaskQuery(`${committed.value} ${draft.value}`)),
  );
  open.value = false;
}
function select(candidate: Candidate) {
  if (!context.value) return;
  const result = applySuggestion(draft.value, context.value, candidate);
  draft.value = result.draft;
  caret.value = draft.value.length;
  if (result.commit) commit();
  else {
    open.value = true;
    nativeInput()?.focus();
  }
}
function keydown(event: KeyboardEvent) {
  if (event.target !== nativeInput()) return;
  if (event.isComposing || event.keyCode === 229) return;
  if (event.key === 'Escape') {
    if (open.value || syntax.value) event.preventDefault();
    open.value = false;
    syntax.value = false;
    return;
  }
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    if (!candidates.value.length) return;
    event.preventDefault();
    open.value = true;
    explicitSelection.value = true;
    active.value =
      active.value < 0
        ? event.key === 'ArrowDown'
          ? 0
          : candidates.value.length - 1
        : (active.value + (event.key === 'ArrowDown' ? 1 : -1) + candidates.value.length) %
          candidates.value.length;
  } else if (event.key === 'Enter') {
    event.preventDefault();
    const candidate = candidates.value[active.value];
    const c = context.value;
    const normalize = (v: string) =>
      (c?.kind === 'values' && ['assignee', 'author'].includes(c.key)
        ? v.replace(/^@/, '')
        : v
      ).toLowerCase();
    if (
      visible.value &&
      candidate &&
      (explicitSelection.value ||
        (candidate.key !== 'no' && normalize(candidate.value) === normalize(c?.prefix ?? '')))
    )
      select(candidate);
    else commit();
  } else if (
    event.key === 'Backspace' &&
    nativeInput()?.selectionStart === 0 &&
    nativeInput()?.selectionEnd === 0 &&
    chips.value.length
  ) {
    const tokens = tokenizeTaskQuery(committed.value);
    tokens.pop();
    committed.value = serializeTaskQuery(tokens);
    commit();
  }
}
function remove(index: number) {
  committed.value = serializeTaskQuery(chips.value.filter((_, i) => i !== index));
  commit();
}
function update(value: string | number) {
  draft.value = String(value);
  caret.value = nativeInput()?.selectionStart ?? draft.value.length;
  open.value = true;
  syntax.value = false;
}
onClickOutside(root, () => {
  open.value = false;
  syntax.value = false;
});
</script>
<template>
  <div
    ref="root"
    class="relative min-w-0 flex-1"
    @keydown="keydown"
    @keyup="caret = nativeInput()?.selectionStart ?? draft.length"
    @focusin="
      open = true;
      emit('focus');
    "
  >
    <Input
      ref="input"
      type="search"
      aria-label="タスクを検索"
      placeholder="タスクを検索..."
      :value="draft"
      :on-value-change="update"
      @click="caret = nativeInput()?.selectionStart ?? draft.length"
      ><template #prefix
        ><Search class="size-4 shrink-0" /><span class="flex max-w-[60%] gap-1 overflow-x-auto"
          ><Tag
            v-for="(token, index) in chips"
            :key="index"
            :label="serializeToken(token)"
            :on-remove="() => remove(index)"
            :remove-label="`${serializeToken(token)} を外す`" /></span></template
      ><template #suffix
        ><Button
          variant="ghost"
          ariaLabel="書き方の早見表"
          :on-click="
            () => {
              syntax = !syntax;
              open = false;
            }
          "
          >?</Button
        ></template
      ></Input
    >
    <div class="absolute inset-x-0 top-full z-50 mt-1">
      <TaskQuerySyntaxCard v-if="syntax" /><TaskQuerySuggestList
        v-else-if="visible"
        :id="listId"
        :candidates="candidates"
        :active="active"
        :prefix="context?.prefix ?? ''"
        :heading="context?.kind === 'values' ? `${context.key}: の候補` : '条件の一覧'"
        @select="select"
      />
    </div>
  </div>
</template>
