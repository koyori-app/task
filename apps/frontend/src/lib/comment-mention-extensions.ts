/**
 * メンション（`<@利用者ID>` の印）を CodeMirror 6 で扱う拡張。コメントの入力欄（CommentEditor）と
 * 説明文の入力欄（MarkdownEditor）が**同じ拡張**を使う。
 *
 * - 印は入力欄の中でも文字としては見せず、`@今の名` の札（widget）に置き換えて一塊で扱う
 *   （caret は札の中に入らず、Backspace / Delete は札を丸ごと消す）
 * - `@` を打つと候補が出て、選ぶと打った `@検索語` が印に替わる。手で打った `@name` は素の文字
 * - IME の変換中は候補を確定しない（変換中の Enter は IME の確定に使われる）
 * - コピー・切り取りは `text/plain` に `@今の名`、`text/html` に札の要素（id と版）を載せ、
 *   貼り付けではその印がある時だけ `<@id>` に戻す（形式: docs/features/tasks/4.collaboration-clipboard.md）
 */
import {
  acceptCompletion,
  autocompletion,
  closeCompletion,
  completionStatus,
  moveCompletionSelection,
  startCompletion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import {
  Compartment,
  EditorState,
  Facet,
  Prec,
  StateField,
  type Extension,
} from '@codemirror/state';
import { Decoration, EditorView, WidgetType, keymap, type DecorationSet } from '@codemirror/view';

import {
  filterCandidates,
  findMarkers,
  fromClipboardHtml,
  lookup,
  markerOf,
  mentionChip,
  mentionTrigger,
  toClipboardHtml,
  toPlainText,
  type MentionCandidate,
  type MentionTable,
  type MentionUser,
} from '@/lib/comment-mention';
import { avatarInitials } from '@/lib/initials';
// 札の見た目は説明文の KFM の札と同じ一枚（span.kfm-mention）を使う
import '@/lib/remark-kfm-mention/style.css';

export interface MentionEditorOptions {
  /** `@` で出す候補（呼ぶたびに最新を返す） */
  candidates: () => readonly MentionCandidate[];
  /** 札の字を引く表の初期値。以後は `setMentionNames` で替える */
  names: MentionTable;
  /** 読み取り専用か（切り取り・貼り付けを止める） */
  disabled: () => boolean;
  /**
   * 位置が markdown のコード（コードブロック・インラインコード）の中か。
   * 中の `<@id>` は札にしない（表示側の KFM と同じ扱い）。コメントの入力欄は不要。
   */
  inCode?: (state: EditorState, pos: number) => boolean;
}

/** 印の id を札の字へ引く表。候補（選んだばかりの者）を先に、API の表で上書きする。 */
export function nameTableOf(
  candidates: readonly MentionCandidate[],
  mentions: MentionTable,
): MentionTable {
  const table: MentionTable = {};
  for (const candidate of candidates) {
    table[candidate.id.toLowerCase()] = {
      status: 'member',
      name: candidate.username,
      avatar_url: candidate.avatar_url ?? null,
    };
  }
  return { ...table, ...mentions };
}

/** 現在の引き表を state に載せる（widget が読む）。 */
const namesFacet = Facet.define<MentionTable, MentionTable>({
  combine: (values) => values[values.length - 1] ?? {},
});

const namesCompartment = new Compartment();

/** 引き表（API の表・候補）が替わった時に、札の字を引き直す。 */
export function setMentionNames(view: EditorView, table: MentionTable): void {
  view.dispatch({ effects: namesCompartment.reconfigure(namesFacet.of(table)) });
}

/** 札の字の引き表を読む（コピーで `@名` を作る時にも使う）。 */
export function mentionNamesOf(state: EditorState): MentionTable {
  return state.facet(namesFacet);
}

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
      other.user?.name === this.user?.name &&
      other.user?.avatar_url === this.user?.avatar_url
    );
  }

  /** 説明文の KFM の札と同じ形（丸い地・頭のアバター・`@名`）。clipboard の形式は doc の文から作るので変わらぬ */
  override toDOM(): HTMLElement {
    const chip = mentionChip(this.user, this.id);
    const el = document.createElement('span');
    el.className = `cm-mention kfm-mention kfm-mention-${chip.state}`;
    el.dataset.mention = '';
    el.dataset.mentionStatus = chip.state;
    if (chip.avatar) {
      const avatar = document.createElement('span');
      avatar.className = 'kfm-mention-avatar';
      avatar.dataset.initials = avatarInitials(chip.avatar.name);
      avatar.setAttribute('aria-hidden', 'true');
      if (chip.avatar.src) {
        const img = document.createElement('img');
        img.src = chip.avatar.src;
        img.alt = '';
        avatar.append(img);
      }
      el.append(avatar);
    }
    el.append(chip.label);
    return el;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

/** 補完の確定キー。IME の変換中は触らない（`false` で既定の動きへ渡す）。 */
function composingGuard(run: (v: EditorView) => boolean) {
  return (v: EditorView) => (v.composing ? false : run(v));
}

export const mentionTheme = EditorView.theme({
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

export function mentionExtensions(options: MentionEditorOptions): Extension[] {
  function buildDecorations(state: EditorState): DecorationSet {
    const names = state.facet(namesFacet);
    const text = state.doc.toString();
    const ranges = findMarkers(text)
      .filter((marker) => !options.inCode?.(state, marker.from))
      .map((marker) =>
        Decoration.replace({
          widget: new MentionWidget(marker.id, lookup(names, marker.id)),
        }).range(marker.from, marker.to),
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

  /** `@検索語` の補完。 */
  function mentionSource(context: CompletionContext): CompletionResult | null {
    const line = context.state.doc.lineAt(context.pos);
    const before = line.text.slice(0, context.pos - line.from);
    const trigger = mentionTrigger(before);
    if (!trigger) return null;
    if (options.inCode?.(context.state, context.pos)) return null;
    const found = filterCandidates(options.candidates(), trigger.query).map((candidate) => ({
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
    if (found.length === 0) return null;
    return { from: line.from + trigger.from, to: context.pos, options: found, filter: false };
  }

  function copyWithMentions(event: ClipboardEvent, v: EditorView, cut: boolean): boolean {
    const names = v.state.facet(namesFacet);
    const ranges = v.state.selection.ranges.filter((range) => !range.empty);
    if (ranges.length === 0 || !event.clipboardData) return false;
    const text = ranges.map((range) => v.state.sliceDoc(range.from, range.to)).join('\n');
    const resolve = (id: string) => lookup(names, id);
    // text/plain は `@名`（外のアプリへはこの字が貼られる）。text/html には札の要素に id と版を持たせ、
    // task の入力欄へ貼れば印に戻る
    event.clipboardData.setData('text/plain', toPlainText(text, resolve));
    event.clipboardData.setData('text/html', toClipboardHtml(text, resolve));
    event.preventDefault();
    if (cut && !options.disabled()) {
      v.dispatch(v.state.replaceSelection(''), { userEvent: 'delete.cut', scrollIntoView: true });
    }
    return true;
  }

  /**
   * 貼り付け: task の札の印（既知の版・正しい id）を持つ `text/html` なら、`<@id>` の文として入れる。
   * 印が無い・版が知らぬ・id が不正なら `false` を返し、CodeMirror の既定（`text/plain`）に任せる。
   */
  function pasteWithMentions(event: ClipboardEvent, v: EditorView): boolean {
    if (options.disabled() || !event.clipboardData) return false;
    const html = event.clipboardData.getData('text/html');
    if (!html) return false;
    const text = fromClipboardHtml(html);
    if (text === null) return false;
    event.preventDefault();
    v.dispatch(v.state.replaceSelection(text), { userEvent: 'input.paste', scrollIntoView: true });
    return true;
  }

  return [
    mentionField,
    namesCompartment.of(namesFacet.of(options.names)),
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
        // 候補が開いている時だけ閉じる。入力直後の「問い合わせ中」で Escape を奪うと、説明欄の
        // 「編集の取り消し」（呼び出し側が Escape を受ける）が一度効かなくなる
        {
          key: 'Escape',
          run: (v) => (completionStatus(v.state) === 'active' ? closeCompletion(v) : false),
        },
      ]),
    ),
    mentionTheme,
    EditorView.domEventHandlers({
      copy: (event, v) => copyWithMentions(event, v, false),
      cut: (event, v) => copyWithMentions(event, v, true),
      paste: (event, v) => pasteWithMentions(event, v),
    }),
  ];
}

/** `@` を打って候補を開く（入力欄の @ ボタン用）。前が字なら空白を一つ挟む。 */
export function openMentionPicker(view: EditorView): void {
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
