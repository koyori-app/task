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
  resolveMentionAutocompleteTooltipParent,
  setMentionNames,
} from '@/lib/comment-mention-extensions';
import type { UploadedFile } from '@/lib/drive-upload';
import { pasteUploadExtensions, resetPasteUploads } from '@/lib/paste-upload-extension';

/**
 * コメント用の入力欄（CodeMirror 6）。
 *
 * 本文は `<@利用者ID>` の印を含む素テキストで、印は**入力欄の中でも文字としては見せない**:
 * 印を `@今の名` の札（widget）に置き換えて描き、札は一塊として扱う（caret は札の中に入らず、
 * Backspace / Delete は札を丸ごと消す）。送る本文は文書そのものなので、送る時の変換は無い。
 * コピー・切り取りは、`text/plain` に `@今の名`、`text/html` に札の要素（id と版を持つ）を載せ、貼り付けでは
 * その印が在る時だけ `<@id>` に戻す（形式: docs/dev/features/tasks/4.collaboration-clipboard.md）。
 *
 * `@` を打つと候補（`candidates`。担当者候補と同じ集合）が出る。選ぶと、打った `@検索語` が印に替わる。
 * 手で打った `@name`（候補から選ばなかった物）は素の文字のままで、メンションにはならない。
 *
 * IME の変換中は候補を確定しない（Enter は変換の確定に使われるため、変換中は補完へ渡さない）。
 *
 * Mod-Enter（Mac は Ctrl-Enter も）で `submit` を emit する（変換中と disabled の間は送らない）。
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
    /**
     * 貼り付けたファイルを上げる関数。渡した時だけ、ファイルの貼り付けを受けて本文に行を入れる
     * （`useProjectFileUpload` で作る）。省略すれば貼り付けは文字だけを扱う
     */
    uploadFile?: (file: File, signal: AbortSignal) => Promise<UploadedFile>;
    /**
     * 貼り付けの宛先を表す値（タスクとスレッドなど）。変わったら進行中のアップロードを捨てる。
     * 部品を使い回して宛先だけ替える所で、前の宛先の結果が今の本文に入らないようにする
     */
    uploadScope?: string;
  }>(),
  {
    placeholder: '',
    disabled: false,
    ariaLabel: undefined,
    minHeightClass: 'min-h-16',
    bare: false,
    candidates: () => [],
    mentions: () => ({}),
    uploadFile: undefined,
    uploadScope: undefined,
  },
);

const emit = defineEmits<{
  'update:modelValue': [value: string];
  /** 貼り付けたファイルを上げている最中か。呼び出し側は true の間、送信・保存を止める */
  'update:uploading': [value: boolean];
  /** Mod-Enter。呼び出し側は送信・保存する */
  submit: [];
}>();

const host = useTemplateRef<HTMLDivElement>('host');
let view: EditorView | null = null;
let lastEmitted: string | null = null;
let mentionTooltipParent: HTMLElement | null = null;

/** 印の id を札の字へ引く表。候補（選んだばかりの者）を先に、API の表で上書きする。 */
const nameTable = computed<MentionTable>(() => nameTableOf(props.candidates, props.mentions));

/** Mod-Enter（Mac は Ctrl-Enter も）。変換中と disabled の間は送らず、空行も入れない */
function submitFromKey(target: EditorView): boolean {
  if (target.composing || props.disabled) return true;
  emit('submit');
  return true;
}

function extensions(): Extension[] {
  if (!mentionTooltipParent) {
    throw new Error('CommentEditor: mentionTooltipParent is unset');
  }
  return [
    history(),
    // メンションの paste より先に見るため、前に置く
    ...(props.uploadFile
      ? [
          pasteUploadExtensions({
            upload: (file, signal) => props.uploadFile!(file, signal),
            disabled: () => props.disabled,
            onUploadingChange: (value) => emit('update:uploading', value),
          }),
        ]
      : []),
    ...mentionExtensions({
      tooltipParent: mentionTooltipParent,
      candidates: () => props.candidates,
      names: nameTable.value,
      disabled: () => props.disabled,
    }),
    // 既定の Mod-Enter（空行の挿入）より先に置く（先に登録した束が優先される）
    keymap.of([
      { key: 'Mod-Enter', run: submitFromKey },
      { mac: 'Ctrl-Enter', run: submitFromKey },
      ...defaultKeymap,
      ...historyKeymap,
    ]),
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
  // `&` はテーマ class を持つ要素すべてに当たり、候補の箱の器（tooltipParent の直下に CodeMirror が作る
  // 空の div）にも付く。高さは編集器本体（.cm-editor）だけに与え、dialog の中で器が dialog と同じ
  // 高さの空白になって dialog 自体がスクロールしないようにする（TASK-505）
  '&.cm-editor': { height: '100%' },
  '&': { backgroundColor: 'transparent', color: 'inherit', fontSize: 'inherit' },
  '&.cm-focused': { outline: 'none' },
  '.cm-content': { padding: '0.5rem 0.75rem', caretColor: 'var(--foreground)' },
  '.cm-line': { padding: '0' },
  '.cm-scroller': { lineHeight: '1.6', overflow: 'auto', fontFamily: 'inherit' },
  '.cm-placeholder': { color: 'var(--muted-foreground)' },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--foreground)' },
  // 候補の箱は編集器の外（body か dialog）に置くので、器の字の大きさ（text-sm）を引き継がない。
  // 編集器の中に置いていた時と同じ大きさを、ここで与える
  '.cm-tooltip.cm-tooltip-autocomplete': { fontSize: '0.875rem', lineHeight: '1.25rem' },
});

onMounted(() => {
  if (!host.value) return;
  mentionTooltipParent = resolveMentionAutocompleteTooltipParent(host.value);
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
    // 外から本文が差し替わったら、前の本文への貼り付けは捨てる
    resetPasteUploads(view);
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
  },
);

// 宛先が替わったら進行中の貼り付けを捨てる。空の下書き同士の切り替えでは本文が '' のままで、
// 上の watch に当たらない
watch(
  () => props.uploadScope,
  () => {
    if (view) resetPasteUploads(view);
  },
);

// 引き表（API の表・候補）が替わったら札の字を引き直す
watch(nameTable, (table) => {
  if (view) setMentionNames(view, table);
});

// disabled / placeholder の変更は state ごと作り直す（MarkdownEditor と同じ理由）。
// uploadFile の有無も同じ（テナントとプロジェクトが mount の後で分かる所で、拡張を足し直す）。
// 関数が入れ替わっただけ（上げる先のプロジェクトが替わった）では作り直さないよう、有無だけを見る
/**
 * disabled になった時に入力欄がフォーカスを持っていたか。戻った時に返す。
 * キーで送ると、disabled で編集不可になった時点でフォーカスが body へ落ち、続きの打鍵が入らない
 */
let refocusOnEnable = false;

watch([() => props.disabled, () => props.placeholder, () => !!props.uploadFile], () => {
  if (!view) return;
  if (props.disabled && view.hasFocus) refocusOnEnable = true;
  view.setState(EditorState.create({ doc: props.modelValue, extensions: extensions() }));
  if (props.disabled || !refocusOnEnable) return;
  refocusOnEnable = false;
  // 送信中に利用者が別の所へフォーカスを移していたら奪わない
  const active = document.activeElement;
  if (!active || active === document.body || active instanceof HTMLDialogElement) view.focus();
});

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
        : 'border-input focus-within:border-ring focus-within:ring-ring/50 rounded-md border shadow-xs transition-[color,box-shadow] focus-within:ring-[3px]',
      minHeightClass,
      disabled ? 'cursor-not-allowed opacity-50' : '',
    ]"
  />
</template>
