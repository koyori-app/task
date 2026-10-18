<script setup lang="ts">
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap, placeholder as placeholderExt } from '@codemirror/view';
import { computed, onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue';

import type { MentionCandidate, MentionTable } from '@/lib/comment-mention';
import {
  mentionExtensions,
  nameTableOf,
  openMentionPicker as openPicker,
  setMentionNames,
} from '@/lib/comment-mention-extensions';

/**
 * コメント用の入力欄（CodeMirror 6）。
 *
 * 本文は `<@利用者ID>` の印を含む素テキストで、印は**入力欄の中でも文字としては見せない**:
 * 印を `@今の名` の札（widget）に置き換えて描き、札は一塊として扱う（caret は札の中に入らず、
 * Backspace / Delete は札を丸ごと消す）。送る本文は文書そのものなので、送る時の変換は無い。
 * コピー・切り取りは、`text/plain` に `@今の名`、`text/html` に札の要素（id と版を持つ）を載せ、貼り付けでは
 * その印が在る時だけ `<@id>` に戻す（形式: docs/features/tasks/4.collaboration-clipboard.md）。
 *
 * `@` を打つと候補（`candidates`。担当者候補と同じ集合）が出る。選ぶと、打った `@検索語` が印に替わる。
 * 手で打った `@name`（候補から選ばなかった物）は素の文字のままで、メンションにはならない。
 *
 * IME の変換中は候補を確定しない（Enter は変換の確定に使われるため、変換中は補完へ渡さない）。
 *
 * SSR しない: CodeMirror は DOM を要求するため、マウント後にクライアントで組み立てる
 * （MarkdownEditor と同じ。器の div は常に出るのでレイアウトは前後で変わらない）。
 */
const props = withDefaults(
  defineProps<{
    /** 編集中の本文（印を含む） */
    modelValue: string;
    placeholder?: string;
    /** 読み取り専用にする（送信中など） */
    disabled?: boolean;
    ariaLabel?: string;
    /** 器の最小高さ（Tailwind クラス） */
    minHeightClass?: string;
    /** 枠・影を持たぬ（親が枠で囲う時） */
    bare?: boolean;
    /** `@` で出す候補 */
    candidates?: readonly MentionCandidate[];
    /** 本文に出る印の id から引ける表示情報（API の `mentions`） */
    mentions?: MentionTable;
  }>(),
  {
    placeholder: '',
    disabled: false,
    ariaLabel: undefined,
    minHeightClass: 'min-h-16',
    bare: false,
    candidates: () => [],
    mentions: () => ({}),
  },
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();

const host = useTemplateRef<HTMLDivElement>('host');
let view: EditorView | null = null;
let lastEmitted: string | null = null;

/** 印の id を札の字へ引く表。候補（選んだばかりの者）を先に、API の表で上書きする。 */
const nameTable = computed<MentionTable>(() => nameTableOf(props.candidates, props.mentions));

function extensions(): Extension[] {
  return [
    history(),
    ...mentionExtensions({
      candidates: () => props.candidates,
      names: nameTable.value,
      disabled: () => props.disabled,
    }),
    keymap.of([...defaultKeymap, ...historyKeymap]),
    EditorView.lineWrapping,
    theme,
    EditorState.readOnly.of(props.disabled),
    EditorView.editable.of(!props.disabled),
    ...(props.placeholder ? [placeholderExt(props.placeholder)] : []),
    EditorView.contentAttributes.of({
      ...(props.ariaLabel ? { 'aria-label': props.ariaLabel } : {}),
      'data-comment-editor-input': '',
    }),
    EditorView.updateListener.of((update) => {
      if (!update.docChanged) return;
      const value = update.state.doc.toString();
      lastEmitted = value;
      emit('update:modelValue', value);
    }),
  ];
}

/** 器 (Tailwind) に枠を任せ、CodeMirror 側は寸法・余白・札の見た目だけ持つ */
const theme = EditorView.theme({
  '&': { height: '100%', backgroundColor: 'transparent', color: 'inherit', fontSize: 'inherit' },
  '&.cm-focused': { outline: 'none' },
  '.cm-content': { padding: '0.5rem 0.75rem', caretColor: 'var(--foreground)' },
  '.cm-line': { padding: '0' },
  '.cm-scroller': { lineHeight: '1.6', overflow: 'auto', fontFamily: 'inherit' },
  '.cm-placeholder': { color: 'var(--muted-foreground)' },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--foreground)' },
});

onMounted(() => {
  if (!host.value) return;
  view = new EditorView({
    state: EditorState.create({ doc: props.modelValue, extensions: extensions() }),
    parent: host.value,
  });
});

onBeforeUnmount(() => {
  view?.destroy();
  view = null;
});

watch(
  () => props.modelValue,
  (next) => {
    if (!view) return;
    // 自分が emit した値が戻ってきただけなら触らない（caret が末尾へ飛ぶのを防ぐ）
    if (next === lastEmitted) return;
    if (next === view.state.doc.toString()) return;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
  },
);

// 引き表（API の表・候補）が替わったら札の字を引き直す
watch(nameTable, (table) => {
  if (view) setMentionNames(view, table);
});

// disabled / placeholder の変更は state ごと作り直す（MarkdownEditor と同じ理由）
watch(
  () => [props.disabled, props.placeholder],
  () => {
    if (!view) return;
    view.setState(EditorState.create({ doc: props.modelValue, extensions: extensions() }));
  },
);

function focus() {
  view?.focus();
}

/** `@` を打って候補を開く（入力欄の @ ボタン用）。 */
function openMentionPicker() {
  if (!view || props.disabled) return;
  openPicker(view);
}

defineExpose({ focus, openMentionPicker });
</script>

<template>
  <div
    ref="host"
    data-comment-editor
    class="grid w-full overflow-hidden bg-transparent text-sm"
    :class="[
      bare
        ? ''
        : 'border-input focus-within:border-ring focus-within:ring-ring/50 dark:bg-input/30 rounded-md border shadow-xs transition-[color,box-shadow] focus-within:ring-[3px]',
      minHeightClass,
      disabled ? 'cursor-not-allowed opacity-50' : '',
    ]"
  />
</template>
