/**
 * 貼り付けたファイルを Drive へ上げ、本文に Markdown の行として入れる CodeMirror 6 の拡張 (TASK-137)。
 * コメントの入力欄 (CommentEditor) と説明文の入力欄 (MarkdownEditor) が同じ拡張を使う。
 *
 * 上げている間の表示は**文書に入れず**、widget の decoration で出す。文書に入れると v-model に載り、
 * 途中の保存や下書きに「アップロード中…」の字が残る。
 *
 * 進行中のアップロードの持ち主は ViewPlugin の実体で、寿命は次のとおり (呼び出し側と揃えてある):
 *
 * | 起きること | 進行中のアップロード | 結果 |
 * |---|---|---|
 * | 入力欄の破棄 | `destroy()` で中断 | 捨てる |
 * | `setState` (disabled / placeholder の変更) | 旧 plugin の `destroy()` で中断 | 捨てる |
 * | 外からの本文の差し替え・宛先の切り替え | 部品が `resetPasteUploads` を呼ぶ | 捨てる |
 * | 利用者が widget を含む範囲を消す (undo で消える場合も同じ) | plugin の `update` で中断 | 捨てる |
 * | 行の入れ替え (Alt+↑/↓)・字の入れ替え (Ctrl+T) | 続く | widget を置き直し、その位置へ入れる |
 *
 * 結果を捨てても、上げ終えたファイルは Drive に残る。送信・保存は、呼び出し側が
 * `onUploadingChange` を見て上げている間は止める。
 */
import {
  Facet,
  Prec,
  StateEffect,
  StateField,
  type Extension,
  type Transaction,
} from '@codemirror/state';
import {
  Decoration,
  EditorView,
  ViewPlugin,
  WidgetType,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view';

import type { UploadedFile } from '@/lib/drive-upload';
import { driveMediaKind } from '@/lib/rehype-kfm-drive-url';

export type PasteUploadOptions = {
  upload: (file: File, signal: AbortSignal) => Promise<UploadedFile>;
  /** 読み取り専用か (貼り付けでファイルを取らない) */
  disabled: () => boolean;
  /** 上げている最中の貼り付けが 1 つ以上あるかが変わった時だけ呼ぶ (失敗の表示は数えない) */
  onUploadingChange: (uploading: boolean) => void;
};

const optionsFacet = Facet.define<PasteUploadOptions, PasteUploadOptions | null>({
  combine: (values) => values[0] ?? null,
});

type BatchMark = { id: number; names: readonly string[] };

const addEffect = StateEffect.define<BatchMark & { pos: number }>({
  map: (value, changes) => ({ ...value, pos: changes.mapPos(value.pos, 1) }),
});
const failEffect = StateEffect.define<BatchMark & { message: string }>();
const removeEffect = StateEffect.define<{ id: number }>();

const NAME_SEPARATOR = '、';

class UploadWidget extends WidgetType {
  constructor(
    readonly id: number,
    readonly names: readonly string[],
    /** 失敗の文言。上げている間は null */
    readonly error: string | null,
  ) {
    super();
  }

  override eq(other: UploadWidget): boolean {
    return (
      other.id === this.id &&
      other.error === this.error &&
      other.names.join('\n') === this.names.join('\n')
    );
  }

  override toDOM(view: EditorView): HTMLElement {
    const root = document.createElement('span');
    const names = this.names.join(NAME_SEPARATOR);
    const message = document.createElement('span');
    const cancel = this.button('取り消し', () => view.plugin(pasteUploadPlugin)?.cancel(this.id));
    if (this.error === null) {
      root.className = 'cm-paste-upload';
      message.setAttribute('role', 'status');
      message.textContent = `アップロード中… ${names}`;
      // 上げている間は送信・保存が止まる。回線が止まった時に利用者が自分で抜けられるよう、
      // ここからも取り消せるようにする (表示は本文の端にあると範囲の削除では消えない)
      root.append(message, cancel);
      return root;
    }
    root.className = 'cm-paste-upload cm-paste-upload-error';
    message.setAttribute('role', 'alert');
    message.textContent = `${names} を貼り付けできませんでした: ${this.error}`;
    root.append(
      message,
      this.button('再試行', () => view.plugin(pasteUploadPlugin)?.retry(this.id)),
      cancel,
    );
    return root;
  }

  private button(label: string, run: () => void): HTMLButtonElement {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = label;
    // 押しても入力欄からフォーカスを奪わない (blur で確定する呼び出し側がある)
    button.addEventListener('mousedown', (event) => event.preventDefault());
    button.addEventListener('click', (event) => {
      event.preventDefault();
      run();
    });
    return button;
  }

  // widget の中の操作 (ボタン) は CodeMirror に渡さない
  override ignoreEvent(): boolean {
    return true;
  }
}

function mark(id: number, names: readonly string[], error: string | null) {
  return Decoration.widget({ widget: new UploadWidget(id, names, error), side: 1, id });
}

function positionOf(set: DecorationSet, id: number): number | null {
  for (const iter = set.iter(); iter.value !== null; iter.next()) {
    if (iter.value.spec.id === id) return iter.from;
  }
  return null;
}

/**
 * 字を消さない入れ替えか。行の入れ替え (Alt+↑/↓) は動かす行でなく**隣の行**を消して反対側へ入れ直し、
 * 字の入れ替え (Ctrl+T) は caret の前後の字を置き換える。どちらも文書の上では「範囲の削除」なので、
 * その中の widget は位置の写しで落ちるが、利用者は何も消していない。
 * ドラッグでの移動 (`move.drop`) は選んだ範囲を動かす操作なので含めない (範囲の削除と同じ扱い)。
 */
function isReorder(tr: Transaction): boolean {
  return tr.isUserEvent('move.line') || tr.isUserEvent('move.character');
}

/**
 * 入れ替えで落ちた widget の新しい位置。行の入れ替えでは、消された行は同じ字のまま別の所へ入り直すので、
 * その行の中の同じ位置へ付いていく。見つからなければ、消された範囲があった所の直後に置く。
 */
function reorderedPosition(tr: Transaction, pos: number): number {
  const line = tr.startState.doc.lineAt(pos);
  const lineBreak = tr.startState.lineBreak;
  let found: number | null = null;
  tr.changes.iterChanges((_fromA, _toA, fromB, _toB, inserted) => {
    if (found !== null) return;
    const text = inserted.toString();
    if (text === lineBreak + line.text) found = fromB + lineBreak.length + (pos - line.from);
    else if (text === line.text + lineBreak) found = fromB + (pos - line.from);
  });
  return found ?? tr.changes.mapPos(pos, 1);
}

/**
 * 1 回の貼り付けにつき widget 1 つ。位置は文書の変更に追従し、widget を含む範囲が消えたら無くなる
 * (plugin がその貼り付けを中断する)。字を消さない入れ替えでは無くさず、置き直す。
 */
const uploadField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    let next = value.map(tr.changes);
    if (tr.docChanged && next.size < value.size && isReorder(tr)) {
      const kept = new Set<unknown>();
      for (const iter = next.iter(); iter.value !== null; iter.next()) kept.add(iter.value.spec.id);
      const restored = [];
      for (const iter = value.iter(); iter.value !== null; iter.next()) {
        if (kept.has(iter.value.spec.id)) continue;
        restored.push(iter.value.range(reorderedPosition(tr, iter.from)));
      }
      next = next.update({ add: restored, sort: true });
    }
    for (const effect of tr.effects) {
      if (effect.is(removeEffect)) {
        const { id } = effect.value;
        next = next.update({ filter: (_from, _to, deco) => deco.spec.id !== id });
      } else if (effect.is(addEffect)) {
        const { id, names, pos } = effect.value;
        next = next.update({ add: [mark(id, names, null).range(pos)] });
      } else if (effect.is(failEffect)) {
        const { id, names, message } = effect.value;
        const pos = positionOf(next, id);
        if (pos === null) continue;
        next = next.update({
          filter: (_from, _to, deco) => deco.spec.id !== id,
          add: [mark(id, names, message).range(pos)],
        });
      }
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

type Batch = {
  /** これから上げる (上げている・失敗して残した) ファイル */
  files: File[];
  /** 上げている間だけ持つ。失敗して再試行を待つ間は null */
  controller: AbortController | null;
};

/*
 * 進行中のアップロードの持ち主。フィールドに `#private` を使わない: 呼び出し側が view を Vue の
 * reactive に入れると、破棄 (`destroy`) が proxy 越しに呼ばれ、proxy 越しの `#private` は TypeError に
 * なる (部品は view を proxy にしない形で持つが、ここが壊れると保存の抑止が解けなくなるので備える)。
 */
class PasteUploadPlugin {
  readonly batches = new Map<number, Batch>();
  readonly options: PasteUploadOptions | null;
  nextId = 1;
  uploading = false;

  constructor(readonly view: EditorView) {
    // setState での破棄の時点で view.state が新旧どちらを指すかに依らないよう、作る時に取っておく
    this.options = view.state.facet(optionsFacet);
  }

  start(files: readonly File[]): boolean {
    if (this.options === null || this.options.disabled() || files.length === 0) return false;
    const id = this.nextId++;
    this.batches.set(id, { files: [...files], controller: new AbortController() });
    this.view.dispatch({
      effects: addEffect.of({
        id,
        names: files.map((file) => file.name),
        pos: this.view.state.selection.main.head,
      }),
    });
    this.notify();
    void this.run(id);
    return true;
  }

  retry(id: number): void {
    const batch = this.batches.get(id);
    const pos = positionOf(this.view.state.field(uploadField), id);
    if (!batch || batch.controller !== null || pos === null) return;
    batch.controller = new AbortController();
    this.view.dispatch({
      effects: [
        removeEffect.of({ id }),
        addEffect.of({ id, names: batch.files.map((file) => file.name), pos }),
      ],
    });
    this.notify();
    void this.run(id);
  }

  cancel(id: number): void {
    this.batches.get(id)?.controller?.abort();
    this.batches.delete(id);
    this.view.dispatch({ effects: removeEffect.of({ id }) });
    this.notify();
  }

  /** 全部を中断し、表示を消す */
  reset(): void {
    this.abortAll();
    const effects: StateEffect<{ id: number }>[] = [];
    for (const iter = this.view.state.field(uploadField).iter(); iter.value !== null; iter.next()) {
      effects.push(removeEffect.of({ id: iter.value.spec.id as number }));
    }
    if (effects.length > 0) this.view.dispatch({ effects });
    this.notify();
  }

  /**
   * 表示を含む範囲が消えたら、その貼り付けを取り消す。利用者が消した = 要らないと見なす。
   * 表示が無いと「取り消し」も無く、回線が止まっていると送信・保存の抑止から抜けられなくなる。
   * 失敗して再試行を待っていたものも、表示が無ければ再試行できないので捨てる。
   */
  update(update: ViewUpdate): void {
    if (!update.docChanged || this.batches.size === 0) return;
    const marks = update.state.field(uploadField);
    for (const [id, batch] of [...this.batches]) {
      if (positionOf(marks, id) !== null) continue;
      batch.controller?.abort();
      this.batches.delete(id);
    }
    this.notify();
  }

  destroy(): void {
    this.abortAll();
    this.notify();
  }

  private abortAll(): void {
    for (const batch of this.batches.values()) batch.controller?.abort();
    this.batches.clear();
  }

  private notify(): void {
    let uploading = false;
    for (const batch of this.batches.values()) {
      if (batch.controller !== null) uploading = true;
    }
    if (uploading === this.uploading) return;
    this.uploading = uploading;
    this.options?.onUploadingChange(uploading);
  }

  private async run(id: number): Promise<void> {
    const batch = this.batches.get(id);
    const options = this.options;
    const controller = batch?.controller;
    if (!batch || !controller || options === null) return;
    const files = batch.files;
    const results = await Promise.allSettled(
      files.map(async (file) => options.upload(file, controller.signal)),
    );
    // 中断 (破棄・setState・reset・取り消し・表示を含む範囲の削除) の後は、この view にも文書にも触らない。
    // batch を外す経路はどれも先に中断するので、中断の有無だけを見れば足りる
    if (controller.signal.aborted) return;

    const pos = positionOf(this.view.state.field(uploadField), id);
    if (pos === null) {
      // 表示が無い。範囲の削除は `update` が先に中断するので通常ここへは来ないが、
      // 表示が無いまま本文へ入れないよう、結果は捨てる
      this.batches.delete(id);
      this.notify();
      return;
    }

    const lines: string[] = [];
    const failed: File[] = [];
    let message = '';
    results.forEach((result, index) => {
      if (result.status === 'fulfilled') {
        lines.push(uploadMarkdown(result.value));
        return;
      }
      if (failed.length === 0) {
        message =
          result.reason instanceof Error && result.reason.message
            ? result.reason.message
            : 'ファイルをアップロードできませんでした';
      }
      failed.push(files[index]);
    });

    const insert = lines.length > 0 ? `${lines.join('\n')}\n` : '';
    const selection = this.view.state.selection.main;
    const spec = {
      ...(insert ? { changes: { from: pos, insert } } : {}),
      // caret が貼り付けた位置にあれば、入れた行の後ろへ進める
      ...(insert && selection.empty && selection.head === pos
        ? { selection: { anchor: pos + insert.length } }
        : {}),
      ...(insert ? { userEvent: 'input.paste' } : {}),
    };
    if (failed.length === 0) {
      this.batches.delete(id);
      this.view.dispatch({ ...spec, effects: removeEffect.of({ id }) });
    } else {
      batch.files = failed;
      batch.controller = null;
      this.view.dispatch({
        ...spec,
        effects: failEffect.of({ id, names: failed.map((file) => file.name), message }),
      });
    }
    this.notify();
  }
}

const pasteUploadPlugin = ViewPlugin.fromClass(PasteUploadPlugin);

const pasteUploadTheme = EditorView.baseTheme({
  '.cm-paste-upload': { color: 'var(--muted-foreground)' },
  '.cm-paste-upload-error': { color: 'var(--destructive)' },
  '.cm-paste-upload button': {
    marginLeft: '0.5em',
    color: 'var(--link)',
    textDecoration: 'underline',
    cursor: 'pointer',
  },
  // 表示は文書に入らないので、本文が空の間は CodeMirror の placeholder も同じ行に出る。
  // 「コメントを入力…アップロード中…」と並ばないよう、表示がある行では案内を隠す
  '.cm-line:has(.cm-paste-upload) .cm-placeholder': { display: 'none' },
});

/**
 * 貼り付けをファイルとして扱うか。ファイルがあり、`text/plain` か `text/html` の片方でも空の時だけ取る。
 * Excel・Word のように文字と HTML と画像を同時に載せる貼り付けは文字として扱う (既存の経路に任せる)。
 * web の画像のコピーは `text/html` (img タグ) と画像だけなので、ファイルとして取る。
 */
export function shouldUploadPaste(data: Pick<DataTransfer, 'files' | 'getData'> | null): boolean {
  if (data === null || data.files.length === 0) return false;
  return data.getData('text/plain') === '' || data.getData('text/html') === '';
}

/**
 * 本文に入れる 1 行。画像・動画・音声は埋め込み `![名前](URL)`、それ以外はリンク `[名前](URL)`。
 * URL は API の相対パスで、web は描画時に `/api` を足す (rehype-kfm-drive-url)。
 * 描画側は名前の拡張子で動画・音声を見分けるので、名前はそのまま残す。
 */
export function uploadMarkdown(file: UploadedFile): string {
  // 名前は Markdown として解釈させない (`__init__.py` が太字の `init.py` にならないようにする)。
  // 語の中の `_` は強調にならないので逃がさない (`IMG_0001.HEIC` を本文でそのまま読めるようにする)
  const name = file.name
    .replace(/[\r\n]+/g, ' ')
    .replace(/[\\[\]*`<&~]/g, '\\$&')
    .replace(/(?<![\p{L}\p{N}])_|_(?![\p{L}\p{N}])/gu, '\\_');
  const mime = file.mimeType.split(';')[0].trim().toLowerCase();
  const embed = mime.startsWith('image/') || driveMediaKind(file.name) !== undefined;
  return `${embed ? '!' : ''}[${name}](/v1/drive/files/${file.id}/content)`;
}

/** `files` を上げ始める。拡張が無い・読み取り専用・ファイルが無いなら何もせず false */
export function startPasteUpload(view: EditorView, files: readonly File[]): boolean {
  return view.plugin(pasteUploadPlugin)?.start(files) ?? false;
}

/** 進行中のアップロードを全部中断し、表示を消す。拡張が無ければ何もしない */
export function resetPasteUploads(view: EditorView): void {
  view.plugin(pasteUploadPlugin)?.reset();
}

export function pasteUploadExtensions(options: PasteUploadOptions): Extension {
  return [
    optionsFacet.of(options),
    uploadField,
    pasteUploadPlugin,
    pasteUploadTheme,
    // メンションの paste (text/html の印を見る) より先に見る
    Prec.high(
      EditorView.domEventHandlers({
        paste(event, view) {
          const data = event.clipboardData;
          if (!shouldUploadPaste(data)) return false;
          if (!startPasteUpload(view, Array.from(data!.files))) return false;
          event.preventDefault();
          return true;
        },
      }),
    ),
  ];
}
