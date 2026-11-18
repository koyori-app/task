import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ref } from 'vue';

import CommentEditor from '@/components/tasks/CommentEditor.vue';
import type { UploadedFile } from '@/lib/drive-upload';

/**
 * コメント入力欄（CodeMirror）の実ブラウザでの挙動。単体テスト（happy-dom）では CodeMirror が
 * 起動できず代役に替えているため、札・候補・IME の挙動はここで固定する。
 */
const ALICE = '11111111-1111-4111-8111-111111111111';
const BOB = '22222222-2222-4222-8222-222222222222';
const OUTSIDE = '33333333-3333-4333-8333-333333333333';

const candidates = [
  { id: ALICE, username: 'alice' },
  { id: BOB, username: 'bob' },
];

const meta = {
  title: 'Tasks/CommentEditor',
  component: CommentEditor,
  tags: ['autodocs'],
  // 実際の v-model は render の器が持つ。props の必須を満たすための既定
  args: { modelValue: '' },
} satisfies Meta<typeof CommentEditor>;

export default meta;
type Story = StoryObj<typeof meta>;

/** v-model の値を画面の下に出して、送る本文（印を含む）を読み取れるようにする器 */
function withValue(initial: string) {
  return (args: Record<string, unknown>) => ({
    components: { CommentEditor },
    setup() {
      const value = ref(initial);
      // meta の既定の modelValue は v-model を上書きしてしまうので外す
      const { modelValue: _ignored, ...rest } = args;
      return { args: rest, value };
    },
    template: `
      <div style="width: 420px">
        <CommentEditor v-model="value" v-bind="args" aria-label="コメントを入力" />
        <pre data-testid="out" style="font-size: 11px; white-space: pre-wrap">{{ value }}</pre>
      </div>`,
  });
}

function content(canvasElement: HTMLElement) {
  return canvasElement.querySelector<HTMLElement>('[data-comment-editor-input]')!;
}

function out(canvasElement: HTMLElement) {
  return within(canvasElement).getByTestId('out').textContent ?? '';
}

export const PickFromCandidates: Story = {
  args: { candidates },
  render: withValue(''),
  play: async ({ canvasElement }) => {
    const editor = await waitFor(async () => {
      const el = content(canvasElement);
      await expect(el).toBeTruthy();
      return el;
    });
    await userEvent.click(editor);
    await userEvent.keyboard('見て @al');

    // 候補が出る（前方一致で alice だけ）
    const popup = await waitFor(async () => {
      const el = document.querySelector('.cm-tooltip-autocomplete');
      await expect(el).toBeTruthy();
      return el as HTMLElement;
    });
    await expect(popup.textContent).toContain('@alice');
    await expect(popup.textContent).not.toContain('@bob');

    // 誤って確定せぬよう、候補は表示から少し経つまで確定を受けつけぬ（CodeMirror の既定）
    await new Promise((resolve) => setTimeout(resolve, 150));
    await userEvent.keyboard('{Enter}');

    // 送る本文は `<@id>`。入力欄の中には id の文字は無く、札だけが見える
    await waitFor(() => expect(out(canvasElement)).toBe(`見て <@${ALICE}> `));
    const chip = canvasElement.querySelector('.cm-mention');
    await expect(chip?.textContent).toBe('@alice');
    await expect(content(canvasElement).textContent).not.toContain(ALICE);
    await expect(content(canvasElement).textContent).not.toContain('<@');
  },
};

export const ChipIsOneUnit: Story = {
  args: { candidates },
  render: withValue(`<@${ALICE}> 確認 <@${BOB}>`),
  play: async ({ canvasElement }) => {
    const editor = await waitFor(async () => {
      const el = content(canvasElement);
      await expect(canvasElement.querySelectorAll('.cm-mention')).toHaveLength(2);
      return el;
    });
    // 既存の `<@id>` も、開いた時点で札で見える
    await expect(
      [...canvasElement.querySelectorAll('.cm-mention')].map((c) => c.textContent),
    ).toEqual(['@alice', '@bob']);
    await expect(editor.textContent).not.toContain(ALICE);

    // 末尾の札: caret は札の中に入らず、Backspace 一回で札が丸ごと消える（印の一部が残らない）
    await userEvent.click(editor);
    await userEvent.keyboard('{Control>}{End}{/Control}{Backspace}');
    await waitFor(() => expect(out(canvasElement)).toBe(`<@${ALICE}> 確認 `));

    // 先頭の札: Home から → で札を一度に越え、Delete 一回で札が丸ごと消える
    await userEvent.keyboard('{Control>}{Home}{/Control}{Delete}');
    await waitFor(() => expect(out(canvasElement)).toBe(' 確認 '));
    await expect(canvasElement.querySelectorAll('.cm-mention')).toHaveLength(0);
  },
};

export const CaretNeverEntersAChip: Story = {
  args: { candidates },
  render: withValue(`あ<@${ALICE}>い`),
  play: async ({ canvasElement }) => {
    const editor = await waitFor(async () => {
      await expect(canvasElement.querySelector('.cm-mention')).toBeTruthy();
      return content(canvasElement);
    });
    await userEvent.click(editor);
    await userEvent.keyboard('{Control>}{Home}{/Control}{ArrowRight}{ArrowRight}');
    // 「あ」の後ろ → 札の後ろ（札の途中には止まらぬ）。ここへ字を打つと印の外に入る
    await userEvent.keyboard('X');
    await waitFor(() => expect(out(canvasElement)).toBe(`あ<@${ALICE}>Xい`));
  },
};

export const OutsideAndGoneAreGrey: Story = {
  args: { candidates, mentions: { [OUTSIDE]: { status: 'outside', name: null } } },
  render: withValue(`<@${OUTSIDE}> <@${'44444444-4444-4444-8444-444444444444'}>`),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelectorAll('.cm-mention')).toHaveLength(2));
    const chips = [...canvasElement.querySelectorAll<HTMLElement>('.cm-mention')];
    await expect(chips[0].textContent).toBe('@（このプロジェクトの外の利用者）');
    await expect(chips[0].dataset.mentionStatus).toBe('outside');
    // 引けぬ id は不明な利用者の札。いずれも id は画面に出さない
    await expect(chips[1].textContent).toBe('@（不明な利用者）');
    await expect(content(canvasElement).textContent).not.toMatch(/[0-9a-f]{8}-[0-9a-f]{4}-/);
  },
};

export const ImeCompositionDoesNotAcceptACandidate: Story = {
  args: { candidates },
  render: withValue(''),
  play: async ({ canvasElement }) => {
    const editor = await waitFor(async () => {
      const el = content(canvasElement);
      await expect(el).toBeTruthy();
      return el;
    });
    await userEvent.click(editor);
    await userEvent.keyboard('@a');
    await waitFor(() => expect(document.querySelector('.cm-tooltip-autocomplete')).toBeTruthy());

    // 変換中の Enter は IME の確定であり、候補の確定ではない。実際の IME と同じく、
    // compositionstart のあとに変換中の字を DOM へ入れて（CodeMirror が変換中と認める）から鍵を送る
    editor.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
    const line = editor.querySelector('.cm-line')!;
    line.append(document.createTextNode('あ'));
    editor.dispatchEvent(new CompositionEvent('compositionupdate', { bubbles: true, data: 'あ' }));
    await new Promise((resolve) => setTimeout(resolve, 50));
    for (const key of ['Enter', 'Tab']) {
      editor.dispatchEvent(
        new KeyboardEvent('keydown', {
          key,
          code: key,
          keyCode: 229,
          isComposing: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    }
    // 候補は確定されず、本文に印も改行も入らない（変換中の字だけが本文に在る）
    await expect(out(canvasElement)).toBe('@aあ');
    await expect(canvasElement.querySelector('.cm-mention')).toBeNull();
    editor.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: 'あ' }));
  },
};

export const HandTypedNameStaysPlain: Story = {
  args: { candidates },
  render: withValue(''),
  play: async ({ canvasElement }) => {
    const editor = await waitFor(async () => {
      const el = content(canvasElement);
      await expect(el).toBeTruthy();
      return el;
    });
    await userEvent.click(editor);
    // 候補を選ばずに打った名前は素の文字のまま（メンションにならない）。Escape で候補を閉じる
    await userEvent.keyboard('@alice');
    await userEvent.keyboard('{Escape}');
    await userEvent.keyboard(' よろしく');
    await waitFor(() => expect(out(canvasElement)).toBe('@alice よろしく'));
    await expect(canvasElement.querySelector('.cm-mention')).toBeNull();
  },
};

export const CopyDoesNotLeakIds: Story = {
  args: { candidates },
  render: withValue(`<@${ALICE}> 確認`),
  play: async ({ canvasElement }) => {
    const editor = await waitFor(async () => {
      await expect(canvasElement.querySelector('.cm-mention')).toBeTruthy();
      return content(canvasElement);
    });
    await userEvent.click(editor);
    await userEvent.keyboard('{Control>}a{/Control}');
    const data = new DataTransfer();
    editor.dispatchEvent(
      new ClipboardEvent('copy', { clipboardData: data, bubbles: true, cancelable: true }),
    );
    // クリップボードへは印ではなく札の字を渡す
    await expect(data.getData('text/plain')).toBe('@alice 確認');
    await expect(data.getData('text/plain')).not.toContain(ALICE);
  },
};

/** 二つの入力欄（同じ欄・別の欄への貼り付けを確かめる器）。それぞれの値を下に出す */
function twoEditors(first: string, second: string) {
  return (args: Record<string, unknown>) => ({
    components: { CommentEditor },
    setup() {
      const a = ref(first);
      const b = ref(second);
      const { modelValue: _ignored, ...rest } = args;
      return { args: rest, a, b };
    },
    template: `
      <div style="width: 420px">
        <CommentEditor v-model="a" v-bind="args" aria-label="一つめ" />
        <CommentEditor v-model="b" v-bind="args" aria-label="二つめ" />
        <pre data-testid="out-a" style="font-size: 11px; white-space: pre-wrap">{{ a }}</pre>
        <pre data-testid="out-b" style="font-size: 11px; white-space: pre-wrap">{{ b }}</pre>
      </div>`,
  });
}

function editors(canvasElement: HTMLElement) {
  return [...canvasElement.querySelectorAll<HTMLElement>('[data-comment-editor-input]')];
}

function outOf(canvasElement: HTMLElement, id: 'out-a' | 'out-b') {
  return within(canvasElement).getByTestId(id).textContent ?? '';
}

function fire(el: HTMLElement, type: 'copy' | 'cut' | 'paste', data: DataTransfer) {
  el.dispatchEvent(
    new ClipboardEvent(type, { clipboardData: data, bubbles: true, cancelable: true }),
  );
}

export const CopiedChipPastesAsChip: Story = {
  args: { candidates },
  render: twoEditors(`<@${ALICE}> 確認 <@${BOB}>`, '先頭 '),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(editors(canvasElement)).toHaveLength(2));
    await waitFor(() => expect(canvasElement.querySelectorAll('.cm-mention')).toHaveLength(2));
    const [first, second] = editors(canvasElement);

    await userEvent.click(first);
    await userEvent.keyboard('{Control>}a{/Control}');
    const data = new DataTransfer();
    fire(first, 'copy', data);
    // 外のアプリへ貼られる字は `@名`。印の文字も id も含まぬ。札の要素は text/html に載る
    await expect(data.getData('text/plain')).toBe('@alice 確認 @bob');
    await expect(data.getData('text/plain')).not.toContain('<@');
    await expect(data.getData('text/html')).toContain(`data-user-id="${ALICE}"`);

    // 同じ欄の末尾へ貼る（全選択の置き換え）→ 札のまま（本文が `<@id>`）
    fire(first, 'paste', data);
    await waitFor(() => expect(outOf(canvasElement, 'out-a')).toBe(`<@${ALICE}> 確認 <@${BOB}>`));

    // 別の欄の末尾へ貼る
    await userEvent.click(second);
    await userEvent.keyboard('{Control>}{End}{/Control}');
    fire(second, 'paste', data);
    await waitFor(() =>
      expect(outOf(canvasElement, 'out-b')).toBe(`先頭 <@${ALICE}> 確認 <@${BOB}>`),
    );
    await expect(canvasElement.querySelectorAll('.cm-mention')).toHaveLength(4);
  },
};

export const CutChipPastesAsChip: Story = {
  args: { candidates },
  render: twoEditors(`見て <@${ALICE}>`, ''),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelectorAll('.cm-mention')).toHaveLength(1));
    const [first, second] = editors(canvasElement);
    await userEvent.click(first);
    await userEvent.keyboard('{Control>}a{/Control}');
    const data = new DataTransfer();
    fire(first, 'cut', data);
    await expect(data.getData('text/plain')).toBe('見て @alice');
    await expect(data.getData('text/plain')).not.toContain('<@');
    await waitFor(() => expect(outOf(canvasElement, 'out-a')).toBe(''));

    await userEvent.click(second);
    fire(second, 'paste', data);
    await waitFor(() => expect(outOf(canvasElement, 'out-b')).toBe(`見て <@${ALICE}>`));
  },
};

export const PlainOrForeignHtmlPastesAsText: Story = {
  args: { candidates },
  render: withValue(''),
  play: async ({ canvasElement }) => {
    const editor = await waitFor(async () => {
      const el = content(canvasElement);
      await expect(el).toBeTruthy();
      return el;
    });
    await userEvent.click(editor);

    // task の印が無い素の文字は今どおり素の文字（`@alice` は札にならぬ）
    const plain = new DataTransfer();
    plain.setData('text/plain', '@alice');
    fire(editor, 'paste', plain);
    await waitFor(() => expect(out(canvasElement)).toBe('@alice'));
    await expect(canvasElement.querySelector('.cm-mention')).toBeNull();

    // 外のアプリの HTML に紛れた id（印の属性が無い／版が知らぬ値／id が壊れておる）は札にしない
    for (const html of [
      `<span data-user-id="${ALICE}">@x</span>`,
      `<span data-task-mention="2" data-user-id="${ALICE}">@x</span>`,
      `<span data-task-mention="1" data-user-id="not-a-uuid">@x</span>`,
    ]) {
      const foreign = new DataTransfer();
      foreign.setData('text/plain', '[外]');
      foreign.setData('text/html', html);
      fire(editor, 'paste', foreign);
    }
    await waitFor(() => expect(out(canvasElement)).toBe('@alice[外][外][外]'));
    await expect(out(canvasElement)).not.toContain(`<@${ALICE}>`);
  },
};

// ---- 貼り付けたファイルのアップロード（TASK-137） ----

const FILE_ID = '0f8fad5b-d9cb-469f-a165-70867728950e';
const FILE_URL = `/v1/drive/files/${FILE_ID}/content`;

type UploadCall = {
  file: File;
  resolve: (value: UploadedFile) => void;
  reject: (reason: Error) => void;
};

/** 手で解決する `uploadFile`。上げている間・失敗・再試行の各状態を play から進める */
function manualUpload() {
  const calls: UploadCall[] = [];
  const uploadFile = (file: File) =>
    new Promise<UploadedFile>((resolve, reject) => {
      calls.push({ file, resolve, reject });
    });
  return { calls, uploadFile };
}

function png(name = 'shot.png') {
  return new File(['x'], name, { type: 'image/png' });
}

/** ファイルだけを載せたクリップボード（スクリーンショットの貼り付けと同じ形） */
function fileClipboard(...files: File[]) {
  const data = new DataTransfer();
  for (const file of files) data.items.add(file);
  return data;
}

function uploadNote(canvasElement: HTMLElement) {
  return canvasElement.querySelector<HTMLElement>('.cm-paste-upload');
}

/** 空の本文に出す案内（placeholder）が見えているか */
function placeholderShown(canvasElement: HTMLElement) {
  const el = canvasElement.querySelector('.cm-placeholder');
  return el !== null && getComputedStyle(el).display !== 'none';
}

async function ready(canvasElement: HTMLElement) {
  const editor = await waitFor(async () => {
    const el = content(canvasElement);
    await expect(el).toBeTruthy();
    return el;
  });
  await userEvent.click(editor);
  return editor;
}

const pasteImage = manualUpload();

export const PasteImage: Story = {
  args: { candidates, uploadFile: pasteImage.uploadFile },
  render: withValue('見て '),
  play: async ({ canvasElement }) => {
    pasteImage.calls.length = 0;
    const editor = await ready(canvasElement);
    await userEvent.keyboard('{Control>}{End}{/Control}');
    fire(editor, 'paste', fileClipboard(png()));

    // 上げている間は入力欄の中に表示だけが出て、本文（送る文字列）は変わらない
    await waitFor(() =>
      expect(uploadNote(canvasElement)?.textContent).toBe('アップロード中… shot.png'),
    );
    await expect(uploadNote(canvasElement)?.getAttribute('role')).toBe('status');
    await expect(out(canvasElement)).toBe('見て ');
    await expect(pasteImage.calls).toHaveLength(1);

    pasteImage.calls[0].resolve({ id: FILE_ID, name: 'shot.png', mimeType: 'image/png' });
    await waitFor(() => expect(out(canvasElement)).toBe(`見て ![shot.png](${FILE_URL})\n`));
    await expect(uploadNote(canvasElement)).toBeNull();
  },
};

const pasteFailed = manualUpload();

export const PasteUploadFailed: Story = {
  args: { candidates, uploadFile: pasteFailed.uploadFile, placeholder: 'コメントを入力…' },
  render: withValue(''),
  play: async ({ canvasElement }) => {
    pasteFailed.calls.length = 0;
    const editor = await ready(canvasElement);
    await expect(placeholderShown(canvasElement)).toBe(true);
    fire(editor, 'paste', fileClipboard(png('big.png')));
    await waitFor(() => expect(pasteFailed.calls).toHaveLength(1));
    // 本文は空のままだが、案内（placeholder）は貼り付けの表示と並べない
    await expect(uploadNote(canvasElement)).toBeTruthy();
    await expect(placeholderShown(canvasElement)).toBe(false);
    pasteFailed.calls[0].reject(new Error('ファイルがサイズ上限または保存容量を超えています'));

    // 失敗は入力欄の中に理由つきで出て、本文は変わらない
    const alert = await waitFor(async () => {
      const el = canvasElement.querySelector('.cm-paste-upload [role="alert"]');
      await expect(el).toBeTruthy();
      return el as HTMLElement;
    });
    await expect(alert.textContent).toBe(
      'big.png を貼り付けできませんでした: ファイルがサイズ上限または保存容量を超えています',
    );
    await expect(out(canvasElement)).toBe('');
    await expect(placeholderShown(canvasElement)).toBe(false);

    // 再試行は同じファイルをもう一度上げ、成功すれば行が入る
    await userEvent.click(
      within(uploadNote(canvasElement)!).getByRole('button', { name: '再試行' }),
    );
    await waitFor(() => expect(pasteFailed.calls).toHaveLength(2));
    await expect(pasteFailed.calls[1].file.name).toBe('big.png');
    await expect(uploadNote(canvasElement)?.textContent).toBe('アップロード中… big.png');
    pasteFailed.calls[1].resolve({ id: FILE_ID, name: 'big.png', mimeType: 'image/png' });
    await waitFor(() => expect(out(canvasElement)).toBe(`![big.png](${FILE_URL})\n`));
    await expect(uploadNote(canvasElement)).toBeNull();
  },
};

const pasteText = manualUpload();

export const PasteTextWithImage: Story = {
  args: { candidates, uploadFile: pasteText.uploadFile },
  render: withValue(''),
  play: async ({ canvasElement }) => {
    pasteText.calls.length = 0;
    const editor = await ready(canvasElement);
    // 表計算・文書のアプリは、文字と HTML と画像を同時に載せる。文字として貼り、上げない
    const data = fileClipboard(png('cells.png'));
    data.setData('text/plain', 'A\tB');
    data.setData('text/html', '<table><tr><td>A</td><td>B</td></tr></table>');
    fire(editor, 'paste', data);
    await waitFor(() => expect(out(canvasElement)).toBe('A\tB'));
    await expect(pasteText.calls).toHaveLength(0);
    await expect(uploadNote(canvasElement)).toBeNull();
  },
};

const pasteScope = manualUpload();

export const PasteThenSwitchScope: Story = {
  args: { candidates, uploadFile: pasteScope.uploadFile },
  render: (args: Record<string, unknown>) => ({
    components: { CommentEditor },
    setup() {
      const value = ref('');
      const scope = ref('task-a/');
      const { modelValue: _ignored, ...rest } = args;
      return { args: rest, value, scope };
    },
    // 入力欄を使い回して宛先（タスク・スレッド）だけ替える所と同じ形。本文は空のまま替わる
    template: `
      <div style="width: 420px">
        <CommentEditor v-model="value" v-bind="args" :upload-scope="scope" aria-label="コメントを入力" />
        <button type="button" data-testid="switch" @click="scope = 'task-b/'">宛先を替える</button>
        <pre data-testid="out" style="font-size: 11px; white-space: pre-wrap">{{ value }}</pre>
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    pasteScope.calls.length = 0;
    const editor = await ready(canvasElement);
    fire(editor, 'paste', fileClipboard(png()));
    await waitFor(() => expect(uploadNote(canvasElement)).toBeTruthy());

    // 宛先が替わると、前の宛先への貼り付けは捨てる
    await userEvent.click(within(canvasElement).getByTestId('switch'));
    await waitFor(() => expect(uploadNote(canvasElement)).toBeNull());

    // 後から上げ終わっても、今の宛先の本文には入らない
    pasteScope.calls[0].resolve({ id: FILE_ID, name: 'shot.png', mimeType: 'image/png' });
    await new Promise((resolve) => setTimeout(resolve, 100));
    await expect(out(canvasElement)).toBe('');
    await expect(uploadNote(canvasElement)).toBeNull();
  },
};
