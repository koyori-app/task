// @vitest-environment happy-dom
// 貼り付けたファイルのアップロード（取るかの判定、上げている間の表示、結果の入れ方、失敗と再試行、中断）
import { history, moveLineDown, moveLineUp, transposeChars, undo } from '@codemirror/commands';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { UploadedFile } from '../drive-upload';
import {
  pasteUploadExtensions,
  resetPasteUploads,
  shouldUploadPaste,
  startPasteUpload,
  uploadMarkdown,
} from '../paste-upload-extension';

const ID_A = '0f8fad5b-d9cb-469f-a165-70867728950e';
const ID_B = '7c9e6679-7425-40de-944b-e07fc1f90ae7';
const url = (id: string) => `/v1/drive/files/${id}/content`;

type Call = {
  file: File;
  signal: AbortSignal;
  resolve: (value: UploadedFile) => void;
  reject: (reason: unknown) => void;
};

const views: EditorView[] = [];

afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
  document.body.innerHTML = '';
});

/** `upload` は手で解決する Promise を返す */
function setup(doc = '', options: { disabled?: boolean } = {}) {
  const calls: Call[] = [];
  const uploading: boolean[] = [];
  const extension = () =>
    pasteUploadExtensions({
      upload: (file, signal) =>
        new Promise<UploadedFile>((resolve, reject) => {
          calls.push({ file, signal, resolve, reject });
        }),
      disabled: () => options.disabled ?? false,
      onUploadingChange: (value) => uploading.push(value),
    });
  const host = document.createElement('div');
  document.body.append(host);
  const view = new EditorView({
    state: EditorState.create({
      doc,
      selection: { anchor: doc.length },
      extensions: extension(),
    }),
    parent: host,
  });
  views.push(view);
  return { view, host, calls, uploading, extension };
}

const file = (name: string, type = 'image/png') => new File(['x'], name, { type });
const uploaded = (id: string, name: string, mimeType = 'image/png'): UploadedFile => ({
  id,
  name,
  mimeType,
});

/** allSettled と、その後の dispatch を待つ */
async function settle() {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
}

const status = (host: HTMLElement) => host.querySelector('[role="status"]');
const alert = (host: HTMLElement) => host.querySelector('[role="alert"]');
const buttons = (host: HTMLElement) => [
  ...host.querySelectorAll<HTMLButtonElement>('.cm-paste-upload button'),
];

describe('shouldUploadPaste', () => {
  const data = (files: number, plain: string, html: string) => ({
    files: { length: files } as FileList,
    getData: (type: string) => (type === 'text/plain' ? plain : type === 'text/html' ? html : ''),
  });

  it.each([
    ['ファイルだけ', data(1, '', ''), true],
    ['ファイル + text/html だけ（web の画像のコピー）', data(1, '', '<img src="x">'), true],
    ['ファイル + text/plain だけ', data(1, 'a.png', ''), true],
    ['ファイル + plain + html（Excel・Word）', data(1, 'A\tB', '<table></table>'), false],
    ['ファイル無し', data(0, '', ''), false],
    ['ファイル無しの文字', data(0, 'a', ''), false],
  ])('%s', (_name, value, expected) => {
    expect(shouldUploadPaste(value)).toBe(expected);
  });

  it('clipboardData が無ければ取らない', () => {
    expect(shouldUploadPaste(null)).toBe(false);
  });
});

describe('uploadMarkdown', () => {
  it.each([
    ['画像', uploaded(ID_A, 'shot.png'), `![shot.png](${url(ID_A)})`],
    ['画像以外', uploaded(ID_A, 'a.pdf', 'application/pdf'), `[a.pdf](${url(ID_A)})`],
    [
      'Markdown の記号（* _ ` < &）を逃がす',
      uploaded(ID_A, '__init__*`<b>&amp;.py', 'text/x-python'),
      `[\\_\\_init\\_\\_\\*\\\`\\<b>\\&amp;.py](${url(ID_A)})`,
    ],
    [
      '名前の角括弧と \\ を逃がす',
      uploaded(ID_A, 'a[1]\\b.png'),
      `![a\\[1\\]\\\\b.png](${url(ID_A)})`,
    ],
    ['名前の改行は空白にする', uploaded(ID_A, 'a\nb.png'), `![a b.png](${url(ID_A)})`],
    [
      'MIME の引数（; より後ろ）は見ない',
      uploaded(ID_A, 'a.png', 'image/png; charset=binary'),
      `![a.png](${url(ID_A)})`,
    ],
    ['動画', uploaded(ID_A, 'a.mp4', 'video/mp4'), `![a.mp4](${url(ID_A)})`],
    ['音声', uploaded(ID_A, 'a.mp3', 'audio/mpeg'), `![a.mp3](${url(ID_A)})`],
    [
      '一覧に無い動画はリンク',
      uploaded(ID_A, 'a.mkv', 'video/x-matroska'),
      `[a.mkv](${url(ID_A)})`,
    ],
    [
      'MIME が画像でない HEIC も埋め込む',
      uploaded(ID_A, 'IMG_1.HEIC', 'application/octet-stream'),
      `![IMG_1.HEIC](${url(ID_A)})`,
    ],
    ['拡張子だけで種類が分かる音声', uploaded(ID_A, 'rec.caf', ''), `![rec.caf](${url(ID_A)})`],
  ])('%s', (_name, value, expected) => {
    expect(uploadMarkdown(value)).toBe(expected);
  });
});

describe('貼り付けたファイルのアップロード', () => {
  it('上げている間は文書を変えず、role=status の表示を出し、uploading を true にする', () => {
    const { view, host, calls, uploading } = setup('前');
    expect(startPasteUpload(view, [file('shot.png')])).toBe(true);
    expect(view.state.doc.toString()).toBe('前');
    expect(status(host)?.textContent).toBe('アップロード中… shot.png');
    expect(calls).toHaveLength(1);
    expect(calls[0].file.name).toBe('shot.png');
    expect(uploading).toEqual([true]);
  });

  it('成功した画像は貼り付けた位置に 1 行で入り、表示が消え、uploading が false に戻る', async () => {
    const { view, host, calls, uploading } = setup('前後');
    view.dispatch({ selection: { anchor: 1 } });
    startPasteUpload(view, [file('shot.png')]);
    calls[0].resolve(uploaded(ID_A, 'shot.png'));
    await settle();
    expect(view.state.doc.toString()).toBe(`前![shot.png](${url(ID_A)})\n後`);
    expect(status(host)).toBeNull();
    expect(uploading).toEqual([true, false]);
    // caret は入れた行の後ろへ進む
    expect(view.state.selection.main.head).toBe(view.state.doc.length - 1);
  });

  it('画像以外はリンクで入る', async () => {
    const { view, calls } = setup();
    startPasteUpload(view, [file('a.pdf', 'application/pdf')]);
    calls[0].resolve(uploaded(ID_A, 'a.pdf', 'application/pdf'));
    await settle();
    expect(view.state.doc.toString()).toBe(`[a.pdf](${url(ID_A)})\n`);
  });

  it('2 ファイルは、解決の順に依らずファイルの順で 2 行入る', async () => {
    const { view, host, calls } = setup();
    startPasteUpload(view, [file('a.png'), file('b.png')]);
    expect(status(host)?.textContent).toBe('アップロード中… a.png、b.png');
    calls[1].resolve(uploaded(ID_B, 'b.png'));
    await settle();
    expect(view.state.doc.toString()).toBe('');
    calls[0].resolve(uploaded(ID_A, 'a.png'));
    await settle();
    expect(view.state.doc.toString()).toBe(`![a.png](${url(ID_A)})\n![b.png](${url(ID_B)})\n`);
  });

  it('上げている間に前へ文字を打つと、ずれた先に入る', async () => {
    const { view, calls } = setup('本文');
    startPasteUpload(view, [file('a.png')]);
    view.dispatch({ changes: { from: 0, insert: '見出し\n' } });
    view.dispatch({ changes: { from: view.state.doc.length, insert: '続き' } });
    calls[0].resolve(uploaded(ID_A, 'a.png'));
    await settle();
    expect(view.state.doc.toString()).toBe(`見出し\n本文続き![a.png](${url(ID_A)})\n`);
  });

  it('表示を含む範囲を消すと、その場で中断して uploading を false に戻す（上げ終わるのを待たない）', async () => {
    const { view, host, calls, uploading } = setup('あいう');
    view.dispatch({ selection: { anchor: 1 } });
    startPasteUpload(view, [file('a.png')]);
    view.dispatch({ changes: { from: 0, to: 3 } });
    expect(status(host)).toBeNull();
    // 回線が止まっていても、消した時点で送信・保存の抑止が解ける
    expect(calls[0].signal.aborted).toBe(true);
    expect(uploading).toEqual([true, false]);
    calls[0].resolve(uploaded(ID_A, 'a.png'));
    await settle();
    expect(view.state.doc.toString()).toBe('');
    expect(uploading).toEqual([true, false]);
  });

  it('表示をまたがない編集（前後への入力・表示の隣までの削除）では中断しない', async () => {
    const { view, host, calls, uploading } = setup('あいう');
    view.dispatch({ selection: { anchor: 1 } });
    startPasteUpload(view, [file('a.png')]);
    view.dispatch({ changes: { from: 0, insert: '前' } });
    view.dispatch({ changes: { from: 0, to: 2 } });
    view.dispatch({ changes: { from: 0, to: view.state.doc.length } });
    expect(calls[0].signal.aborted).toBe(false);
    expect(status(host)).not.toBeNull();
    expect(uploading).toEqual([true]);
    calls[0].resolve(uploaded(ID_A, 'a.png'));
    await settle();
    expect(view.state.doc.toString()).toBe(`![a.png](${url(ID_A)})\n`);
  });

  it('2 つの貼り付けのうち、表示が消えた方だけを中断する', async () => {
    const { view, calls, uploading } = setup('あいうえお');
    view.dispatch({ selection: { anchor: 1 } });
    startPasteUpload(view, [file('a.png')]);
    view.dispatch({ selection: { anchor: 4 } });
    startPasteUpload(view, [file('b.png')]);
    view.dispatch({ changes: { from: 0, to: 2 } });
    expect(calls.map((call) => call.signal.aborted)).toEqual([true, false]);
    // もう片方が上げている間は、抑止を続ける
    expect(uploading).toEqual([true]);
    calls[1].resolve(uploaded(ID_B, 'b.png'));
    await settle();
    expect(view.state.doc.toString()).toBe(`うえ![b.png](${url(ID_B)})\nお`);
    expect(uploading).toEqual([true, false]);
  });

  it('失敗の表示を含む範囲を消すと、再試行用のファイルを捨てる（次の貼り付けは通常どおり）', async () => {
    const { view, host, calls, uploading } = setup('あいう');
    view.dispatch({ selection: { anchor: 1 } });
    startPasteUpload(view, [file('a.png')]);
    calls[0].reject(new Error('x'));
    await settle();
    view.dispatch({ changes: { from: 0, to: 3 } });
    expect(host.querySelector('.cm-paste-upload')).toBeNull();
    expect(uploading).toEqual([true, false]);
    startPasteUpload(view, [file('b.png')]);
    calls[1].resolve(uploaded(ID_B, 'b.png'));
    await settle();
    expect(view.state.doc.toString()).toBe(`![b.png](${url(ID_B)})\n`);
    expect(uploading).toEqual([true, false, true, false]);
  });

  it('undo で表示を含む範囲が消えても中断する', async () => {
    const calls: Call[] = [];
    const uploading: boolean[] = [];
    const host = document.createElement('div');
    document.body.append(host);
    const view = new EditorView({
      state: EditorState.create({
        doc: '',
        extensions: [
          history(),
          pasteUploadExtensions({
            upload: (f, signal) =>
              new Promise<UploadedFile>((resolve, reject) => {
                calls.push({ file: f, signal, resolve, reject });
              }),
            disabled: () => false,
            onUploadingChange: (value) => uploading.push(value),
          }),
        ],
      }),
      parent: host,
    });
    views.push(view);
    view.dispatch({ changes: { from: 0, insert: 'あいう' }, userEvent: 'input.type' });
    view.dispatch({ selection: { anchor: 1 } });
    startPasteUpload(view, [file('a.png')]);
    undo(view);
    expect(view.state.doc.toString()).toBe('');
    expect(calls[0].signal.aborted).toBe(true);
    expect(uploading).toEqual([true, false]);
  });

  describe('字を消さない入れ替え（隣の行を消して入れ直す操作）', () => {
    const line = `![a.png](${url(ID_A)})\n`;

    it('下の行を上へ動かしても中断せず、表示は元の行の同じ位置に付いていく', async () => {
      const { view, host, calls, uploading } = setup('スクショです\n二行目');
      view.dispatch({ selection: { anchor: 6 } });
      startPasteUpload(view, [file('a.png')]);
      // 2 行目に caret を置いて Alt+↑。CodeMirror は 1 行目を消して 2 行目の下へ入れ直す
      view.dispatch({ selection: { anchor: view.state.doc.length } });
      expect(moveLineUp(view)).toBe(true);
      expect(view.state.doc.toString()).toBe('二行目\nスクショです');
      expect(calls[0].signal.aborted).toBe(false);
      expect(status(host)).not.toBeNull();
      expect(uploading).toEqual([true]);
      calls[0].resolve(uploaded(ID_A, 'a.png'));
      await settle();
      expect(view.state.doc.toString()).toBe(`二行目\nスクショです${line}`);
      expect(uploading).toEqual([true, false]);
    });

    it('上の行を下へ動かしても中断せず、表示は元の行の同じ位置に付いていく', async () => {
      const { view, host, calls } = setup('一行目\nスクショです');
      view.dispatch({ selection: { anchor: 6 } });
      startPasteUpload(view, [file('a.png')]);
      // 1 行目に caret を置いて Alt+↓。CodeMirror は 2 行目を消して 1 行目の上へ入れ直す
      view.dispatch({ selection: { anchor: 0 } });
      expect(moveLineDown(view)).toBe(true);
      expect(view.state.doc.toString()).toBe('スクショです\n一行目');
      expect(calls[0].signal.aborted).toBe(false);
      expect(status(host)).not.toBeNull();
      calls[0].resolve(uploaded(ID_A, 'a.png'));
      await settle();
      expect(view.state.doc.toString()).toBe(`スク${line}ショです\n一行目`);
    });

    it('表示のある行そのものを動かしても中断しない', async () => {
      const { view, calls } = setup('一行目\nスクショです');
      view.dispatch({ selection: { anchor: 6 } });
      startPasteUpload(view, [file('a.png')]);
      expect(moveLineUp(view)).toBe(true);
      expect(calls[0].signal.aborted).toBe(false);
      calls[0].resolve(uploaded(ID_A, 'a.png'));
      await settle();
      expect(view.state.doc.toString()).toBe(`スク${line}ショです\n一行目`);
    });

    it('caret の前後の字の入れ替え（Ctrl+T）でも中断しない', async () => {
      const { view, host, calls } = setup('あいう');
      view.dispatch({ selection: { anchor: 1 } });
      startPasteUpload(view, [file('a.png')]);
      expect(transposeChars(view)).toBe(true);
      expect(view.state.doc.toString()).toBe('いあう');
      expect(calls[0].signal.aborted).toBe(false);
      expect(status(host)).not.toBeNull();
      calls[0].resolve(uploaded(ID_A, 'a.png'));
      await settle();
      expect(view.state.doc.toString()).toBe(`いあ${line}う`);
    });
  });

  it('失敗すると文書を変えずに role=alert へ文言を出し、再試行で入る', async () => {
    const { view, host, calls, uploading } = setup();
    startPasteUpload(view, [file('big.png')]);
    calls[0].reject(new Error('ファイルがサイズ上限または保存容量を超えています'));
    await settle();
    expect(view.state.doc.toString()).toBe('');
    expect(status(host)).toBeNull();
    expect(alert(host)?.textContent).toBe(
      'big.png を貼り付けできませんでした: ファイルがサイズ上限または保存容量を超えています',
    );
    // 失敗の表示は「上げている最中」に数えない
    expect(uploading).toEqual([true, false]);

    const [retry] = buttons(host);
    expect(retry.textContent).toBe('再試行');
    retry.click();
    expect(calls).toHaveLength(2);
    expect(calls[1].file.name).toBe('big.png');
    expect(alert(host)).toBeNull();
    expect(status(host)?.textContent).toBe('アップロード中… big.png');
    expect(uploading).toEqual([true, false, true]);

    calls[1].resolve(uploaded(ID_A, 'big.png'));
    await settle();
    expect(view.state.doc.toString()).toBe(`![big.png](${url(ID_A)})\n`);
    expect(host.querySelector('.cm-paste-upload')).toBeNull();
    expect(uploading).toEqual([true, false, true, false]);
  });

  it('上げている間の取り消しは signal を中断して表示を消し、uploading を false に戻す', async () => {
    const { view, host, calls, uploading } = setup('本文');
    startPasteUpload(view, [file('big.mp4', 'video/mp4')]);
    const [cancel] = buttons(host);
    expect(cancel.textContent).toBe('取り消し');
    expect(buttons(host)).toHaveLength(1);
    cancel.click();
    expect(calls[0].signal.aborted).toBe(true);
    expect(host.querySelector('.cm-paste-upload')).toBeNull();
    expect(uploading).toEqual([true, false]);
    // 後から上げ終わっても本文には入らない
    calls[0].resolve(uploaded(ID_A, 'big.mp4', 'video/mp4'));
    await settle();
    expect(view.state.doc.toString()).toBe('本文');
  });

  it('失敗の取り消しは表示を消し、upload を呼ばない', async () => {
    const { view, host, calls } = setup();
    startPasteUpload(view, [file('a.png')]);
    calls[0].reject(new Error('ファイルをアップロードできませんでした'));
    await settle();
    const [, cancel] = buttons(host);
    expect(cancel.textContent).toBe('取り消し');
    cancel.click();
    expect(host.querySelector('.cm-paste-upload')).toBeNull();
    expect(calls).toHaveLength(1);
    expect(view.state.doc.toString()).toBe('');
  });

  it('2 ファイルの片方だけ失敗したら、成功した 1 行だけ入り、失敗した名前だけが残る', async () => {
    const { view, host, calls } = setup();
    startPasteUpload(view, [file('ok.png'), file('ng.png')]);
    calls[0].resolve(uploaded(ID_A, 'ok.png'));
    calls[1].reject(new Error('ファイルをアップロードできませんでした'));
    await settle();
    expect(view.state.doc.toString()).toBe(`![ok.png](${url(ID_A)})\n`);
    expect(alert(host)?.textContent).toBe(
      'ng.png を貼り付けできませんでした: ファイルをアップロードできませんでした',
    );

    // 再試行は失敗した分だけを上げ、成功した行の後ろに入る
    buttons(host)[0].click();
    expect(calls).toHaveLength(3);
    expect(calls[2].file.name).toBe('ng.png');
    calls[2].resolve(uploaded(ID_B, 'ng.png'));
    await settle();
    expect(view.state.doc.toString()).toBe(`![ok.png](${url(ID_A)})\n![ng.png](${url(ID_B)})\n`);
  });

  it('resetPasteUploads は signal を中断して表示を消し、後から解決しても文書を変えない', async () => {
    const { view, host, calls, uploading } = setup();
    startPasteUpload(view, [file('a.png')]);
    resetPasteUploads(view);
    expect(calls[0].signal.aborted).toBe(true);
    expect(status(host)).toBeNull();
    expect(uploading).toEqual([true, false]);
    calls[0].resolve(uploaded(ID_A, 'a.png'));
    await settle();
    expect(view.state.doc.toString()).toBe('');
  });

  it('resetPasteUploads は失敗の表示も消す', async () => {
    const { view, host, calls } = setup();
    startPasteUpload(view, [file('a.png')]);
    calls[0].reject(new Error('x'));
    await settle();
    expect(alert(host)).not.toBeNull();
    resetPasteUploads(view);
    expect(host.querySelector('.cm-paste-upload')).toBeNull();
  });

  it('view.destroy() は signal を中断し、uploading を false に戻す', async () => {
    const { view, calls, uploading } = setup();
    startPasteUpload(view, [file('a.png')]);
    view.destroy();
    expect(calls[0].signal.aborted).toBe(true);
    expect(uploading).toEqual([true, false]);
    // 破棄した view へ dispatch しない（すれば例外になる）
    calls[0].resolve(uploaded(ID_A, 'a.png'));
    await settle();
  });

  it('setState（disabled の切り替えで部品が呼ぶ）は旧い貼り付けを中断し、uploading を false に戻す', async () => {
    const { view, host, calls, uploading, extension } = setup('本文');
    startPasteUpload(view, [file('a.png')]);
    view.setState(EditorState.create({ doc: '本文', extensions: extension() }));
    expect(calls[0].signal.aborted).toBe(true);
    expect(status(host)).toBeNull();
    expect(uploading).toEqual([true, false]);
    calls[0].resolve(uploaded(ID_A, 'a.png'));
    await settle();
    expect(view.state.doc.toString()).toBe('本文');
  });

  it('disabled の間は取らない', () => {
    const { view, host, calls, uploading } = setup('', { disabled: true });
    expect(startPasteUpload(view, [file('a.png')])).toBe(false);
    expect(calls).toHaveLength(0);
    expect(status(host)).toBeNull();
    expect(uploading).toEqual([]);
  });

  it('拡張の無い入力欄では何もしない', () => {
    const host = document.createElement('div');
    document.body.append(host);
    const view = new EditorView({ state: EditorState.create({ doc: '' }), parent: host });
    views.push(view);
    expect(startPasteUpload(view, [file('a.png')])).toBe(false);
    expect(() => resetPasteUploads(view)).not.toThrow();
  });

  it('upload が同期で投げても失敗として出す', async () => {
    const onUploadingChange = vi.fn();
    const host = document.createElement('div');
    document.body.append(host);
    const view = new EditorView({
      state: EditorState.create({
        doc: '',
        extensions: pasteUploadExtensions({
          upload: () => {
            throw new Error('保存先を読み込めませんでした');
          },
          disabled: () => false,
          onUploadingChange,
        }),
      }),
      parent: host,
    });
    views.push(view);
    startPasteUpload(view, [file('a.png')]);
    await settle();
    expect(alert(host)?.textContent).toBe(
      'a.png を貼り付けできませんでした: 保存先を読み込めませんでした',
    );
    expect(onUploadingChange.mock.calls).toEqual([[true], [false]]);
  });
});

describe('paste イベント', () => {
  function paste(view: EditorView, data: { files: File[]; plain?: string; html?: string }) {
    const event = new Event('paste', { bubbles: true, cancelable: true });
    Object.defineProperty(event, 'clipboardData', {
      value: {
        files: data.files,
        getData: (type: string) =>
          type === 'text/plain'
            ? (data.plain ?? '')
            : type === 'text/html'
              ? (data.html ?? '')
              : '',
      },
    });
    view.contentDOM.dispatchEvent(event);
    return event;
  }

  it('ファイルだけの貼り付けを取り、既定の動きを止める', () => {
    const { view, calls } = setup();
    const event = paste(view, { files: [file('a.png')] });
    expect(event.defaultPrevented).toBe(true);
    expect(calls).toHaveLength(1);
  });

  it('文字と HTML を伴う貼り付けは取らない（既存の経路へ流す）', () => {
    const { view, calls } = setup();
    paste(view, { files: [file('a.png')], plain: 'A', html: '<b>A</b>' });
    expect(calls).toHaveLength(0);
  });

  it('disabled の間は取らない', () => {
    const { view, host, calls } = setup('', { disabled: true });
    paste(view, { files: [file('a.png')] });
    expect(calls).toHaveLength(0);
    expect(status(host)).toBeNull();
  });
});
