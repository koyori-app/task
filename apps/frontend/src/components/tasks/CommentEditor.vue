<script setup lang="ts">
import {
  acceptCompletion,
  autocompletion,
  closeCompletion,
  moveCompletionSelection,
  startCompletion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import {
  Compartment,
  EditorState,
  Facet,
  Prec,
  StateField,
  type Extension,
} from '@codemirror/state';
import {
  Decoration,
  EditorView,
  WidgetType,
  keymap,
  placeholder as placeholderExt,
  type DecorationSet,
} from '@codemirror/view';
import { computed, onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue';

import {
  filterCandidates,
  findMarkers,
  lookup,
  markerOf,
  mentionLabel,
  mentionTrigger,
  toPlainText,
  type MentionCandidate,
  type MentionTable,
  type MentionUser,
} from '@/lib/comment-mention';

/**
 * コメント用の入力欄（CodeMirror 6）。
 *
 * 本文は `<@利用者ID>` の印を含む素テキストで、印は**入力欄の中でも文字としては見せない**:
 * 印を `@今の名` の札（widget）に置き換えて描き、札は一塊として扱う（caret は札の中に入らず、
 * Backspace / Delete は札を丸ごと消す）。送る本文は文書そのものなので、送る時の変換は無い。
 * コピー・切り取りも、印ではなく `@今の名` の字を渡す（id を貼り付け先へ漏らさぬ）。
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
const nameTable = computed<MentionTable>(() => {
  const table: MentionTable = {};
  for (const candidate of props.candidates) {
    table[candidate.id.toLowerCase()] = {
      status: 'member',
      name: candidate.username,
      avatar_url: candidate.avatar_url ?? null,
    };
  }
  return { ...table, ...props.mentions };
});

/** 現在の引き表を state に載せる（widget が読む）。 */
const namesFacet = Facet.define<MentionTable, MentionTable>({
  combine: (values) => values[values.length - 1] ?? {},
});

class MentionWidget extends WidgetType {
  constructor(
    readonly id: string,
    readonly user: MentionUser | undefined,
  ) {
    super();
  }

  override eq(other: MentionWidget): boolean {
    return (
      other.id === this.id &&
      other.user?.status === this.user?.status &&
      other.user?.name === this.user?.name
    );
  }

  override toDOM(): HTMLElement {
    const el = document.createElement('span');
    el.className = 'cm-mention';
    el.dataset.mention = '';
    el.dataset.mentionStatus = this.user?.status ?? 'unknown';
    el.textContent = mentionLabel(this.user);
    return el;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

function buildDecorations(state: EditorState): DecorationSet {
  const names = state.facet(namesFacet);
  const text = state.doc.toString();
  const ranges = findMarkers(text).map((marker) =>
    Decoration.replace({ widget: new MentionWidget(marker.id, lookup(names, marker.id)) }).range(
      marker.from,
      marker.to,
    ),
  );
  return Decoration.set(ranges, true);
}

/** 札の置き換え。docChanged か引き表が替わった時だけ作り直す。 */
const mentionField = StateField.define<DecorationSet>({
  create: (state) => buildDecorations(state),
  update(value, tr) {
    if (tr.docChanged || tr.startState.facet(namesFacet) !== tr.state.facet(namesFacet)) {
      return buildDecorations(tr.state);
    }
    return value;
  },
  provide: (field) => [
    EditorView.decorations.from(field),
    // 札は一塊: caret が中へ入らず、Backspace / Delete が丸ごと消す
    EditorView.atomicRanges.of((v) => v.state.field(field)),
  ],
});

const namesCompartment = new Compartment();

/** `@検索語` の補完。候補は `props.candidates`。 */
function mentionSource(context: CompletionContext): CompletionResult | null {
  const line = context.state.doc.lineAt(context.pos);
  const before = line.text.slice(0, context.pos - line.from);
  const trigger = mentionTrigger(before);
  if (!trigger) return null;
  const options = filterCandidates(props.candidates, trigger.query).map((candidate) => ({
    label: `@${candidate.username}`,
    type: 'mention',
    apply: (v: EditorView, _completion: unknown, from: number, to: number) => {
      const marker = `${markerOf(candidate.id)} `;
      v.dispatch({
        changes: { from, to, insert: marker },
        selection: { anchor: from + marker.length },
        userEvent: 'input.complete',
      });
    },
  }));
  if (options.length === 0) return null;
  return { from: line.from + trigger.from, to: context.pos, options, filter: false };
}

/**
 * 補完の確定キー。IME の変換中は触らない（`false` で既定の動きへ渡す）:
 * 変換中の Enter は IME の確定であり、補完の確定ではない。
 */
function composingGuard(run: (v: EditorView) => boolean) {
  return (v: EditorView) => (v.composing ? false : run(v));
}

function extensions(): Extension[] {
  return [
    history(),
    mentionField,
    namesCompartment.of(namesFacet.of(nameTable.value)),
    autocompletion({
      override: [mentionSource],
      activateOnTyping: true,
      defaultKeymap: false,
      icons: false,
      closeOnBlur: true,
    }),
    Prec.highest(
      keymap.of([
        { key: 'ArrowDown', run: moveCompletionSelection(true) },
        { key: 'ArrowUp', run: moveCompletionSelection(false) },
        { key: 'Enter', run: composingGuard(acceptCompletion) },
        { key: 'Tab', run: composingGuard(acceptCompletion) },
        { key: 'Escape', run: closeCompletion },
      ]),
    ),
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
    EditorView.domEventHandlers({
      // 印をクリップボードへ出さぬ（id を貼り付け先へ漏らさず、札の字 `@名` を渡す）
      copy: (event, v) => copyAsPlain(event, v, false),
      cut: (event, v) => copyAsPlain(event, v, true),
    }),
    EditorView.updateListener.of((update) => {
      if (!update.docChanged) return;
      const value = update.state.doc.toString();
      lastEmitted = value;
      emit('update:modelValue', value);
    }),
  ];
}

function copyAsPlain(event: ClipboardEvent, v: EditorView, cut: boolean): boolean {
  const names = v.state.facet(namesFacet);
  const ranges = v.state.selection.ranges.filter((range) => !range.empty);
  if (ranges.length === 0 || !event.clipboardData) return false;
  const text = ranges.map((range) => v.state.sliceDoc(range.from, range.to)).join('\n');
  event.clipboardData.setData(
    'text/plain',
    toPlainText(text, (id) => lookup(names, id)),
  );
  event.preventDefault();
  if (cut && !props.disabled) {
    v.dispatch(v.state.replaceSelection(''), { userEvent: 'delete.cut', scrollIntoView: true });
  }
  return true;
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
  '.cm-mention': {
    padding: '0 0.25rem',
    borderRadius: '0.25rem',
    fontWeight: '500',
    color: 'var(--primary)',
    backgroundColor: 'color-mix(in oklab, var(--primary) 12%, transparent)',
  },
  '.cm-mention[data-mention-status="outside"], .cm-mention[data-mention-status="gone"], .cm-mention[data-mention-status="unknown"]':
    {
      color: 'var(--muted-foreground)',
      backgroundColor: 'var(--muted)',
    },
  '.cm-tooltip.cm-tooltip-autocomplete': {
    border: '1px solid var(--border)',
    borderRadius: '0.375rem',
    backgroundColor: 'var(--popover)',
    color: 'var(--popover-foreground)',
    boxShadow: '0 4px 12px rgb(0 0 0 / 0.12)',
  },
  '.cm-tooltip-autocomplete > ul > li': { padding: '0.25rem 0.5rem' },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: 'var(--accent)',
    color: 'var(--accent-foreground)',
  },
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
  view?.dispatch({ effects: namesCompartment.reconfigure(namesFacet.of(table)) });
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

/** `@` を打って候補を開く（入力欄の @ ボタン用）。前が字なら空白を一つ挟む。 */
function openMentionPicker() {
  if (!view || props.disabled) return;
  const { from, to } = view.state.selection.main;
  const prev = from > 0 ? view.state.sliceDoc(from - 1, from) : '';
  const insert = prev !== '' && !/\s/.test(prev) ? ' @' : '@';
  view.dispatch({
    changes: { from, to, insert },
    selection: { anchor: from + insert.length },
    userEvent: 'input.type',
  });
  view.focus();
  startCompletion(view);
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
