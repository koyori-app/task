import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ref } from 'vue';

import MarkdownEditor from '@/components/markdown/MarkdownEditor.vue';
import type { UploadedFile } from '@/lib/drive-upload';

/**
 * markdown の入力欄（CodeMirror）への、ファイルの貼り付け（TASK-137）。単体テスト（happy-dom）では
 * 代役に替えているため、部品の配線（`uploadFile`・`uploadScope`・外からの本文の差し替え）はここで固定する。
 * 拡張そのものの挙動は paste-upload-extension の単体テストと CommentEditor の story が見る。
 */
const FILE_ID = '0f8fad5b-d9cb-469f-a165-70867728950e';
const FILE_URL = `/v1/drive/files/${FILE_ID}/content`;

const meta = {
  title: 'Markdown/MarkdownEditor',
  component: MarkdownEditor,
  // 実際の v-model は render の器が持つ。props の必須を満たすための既定
  args: { modelValue: '' },
} satisfies Meta<typeof MarkdownEditor>;

export default meta;
type Story = StoryObj<typeof meta>;

type UploadCall = {
  file: File;
  resolve: (value: UploadedFile) => void;
  reject: (reason: Error) => void;
};

/** 手で解決する `uploadFile` */
function manualUpload() {
  const calls: UploadCall[] = [];
  const uploadFile = (file: File) =>
    new Promise<UploadedFile>((resolve, reject) => {
      calls.push({ file, resolve, reject });
    });
  return { calls, uploadFile };
}

/** 宛先（`uploadScope`）と本文を外から替えられる器。分割表示でタスクだけ替える所と同じ形 */
function withControls(args: Record<string, unknown>) {
  return {
    components: { MarkdownEditor },
    setup() {
      const value = ref('');
      const scope = ref('task-a');
      const uploading = ref(false);
      const open = ref(true);
      const disabled = ref(false);
      const { modelValue: _ignored, ...rest } = args;
      return { args: rest, value, scope, uploading, open, disabled };
    },
    template: `
      <div style="width: 480px">
        <MarkdownEditor
          v-if="open"
          v-model="value"
          v-bind="args"
          :upload-scope="scope"
          :disabled="disabled"
          aria-label="説明"
          @update:uploading="uploading = $event"
        />
        <button type="button" data-testid="switch" @click="scope = 'task-b'">宛先を替える</button>
        <button type="button" data-testid="replace" @click="value = '別の本文'">本文を差し替える</button>
        <button type="button" data-testid="close" @click="open = false">入力欄を閉じる</button>
        <button type="button" data-testid="disable" @click="disabled = true">読み取り専用にする</button>
        <button type="button" data-testid="enable" @click="disabled = false">読み取り専用を解く</button>
        <pre data-testid="out" style="font-size: 11px; white-space: pre-wrap">{{ value }}</pre>
        <output data-testid="uploading">{{ uploading }}</output>
      </div>`,
  };
}

function out(canvasElement: HTMLElement, id = 'out') {
  return within(canvasElement).getByTestId(id).textContent ?? '';
}

function uploadNote(canvasElement: HTMLElement) {
  return canvasElement.querySelector<HTMLElement>('.cm-paste-upload');
}

async function pasteFile(canvasElement: HTMLElement, name: string, type: string) {
  const editor = await waitFor(async () => {
    const el = canvasElement.querySelector<HTMLElement>('[data-markdown-editor-input]');
    await expect(el).toBeTruthy();
    return el!;
  });
  await userEvent.click(editor);
  const data = new DataTransfer();
  data.items.add(new File(['x'], name, { type }));
  editor.dispatchEvent(
    new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
  );
  await waitFor(() => expect(uploadNote(canvasElement)).toBeTruthy());
}

const pasteVideo = manualUpload();

export const PasteVideo: Story = {
  args: { uploadFile: pasteVideo.uploadFile },
  render: withControls,
  play: async ({ canvasElement }) => {
    pasteVideo.calls.length = 0;
    await pasteFile(canvasElement, 'clip.mp4', 'video/mp4');
    // 上げている間は呼び出し側へ知らせる（保存を止めるため）。本文は変わらない
    await waitFor(() => expect(out(canvasElement, 'uploading')).toBe('true'));
    await expect(uploadNote(canvasElement)?.querySelector('[role="status"]')?.textContent).toBe(
      'アップロード中… clip.mp4',
    );
    await expect(out(canvasElement)).toBe('');

    // 動画も画像と同じ埋め込みの書式で入る（描画側が名前の拡張子でプレイヤーにする）
    pasteVideo.calls[0].resolve({ id: FILE_ID, name: 'clip.mp4', mimeType: 'video/mp4' });
    await waitFor(() => expect(out(canvasElement)).toBe(`![clip.mp4](${FILE_URL})\n`));
    await expect(out(canvasElement, 'uploading')).toBe('false');
    await expect(uploadNote(canvasElement)).toBeNull();
  },
};

const pasteScope = manualUpload();

export const PasteThenSwitchScope: Story = {
  args: { uploadFile: pasteScope.uploadFile },
  render: withControls,
  play: async ({ canvasElement }) => {
    pasteScope.calls.length = 0;
    await pasteFile(canvasElement, 'shot.png', 'image/png');
    await waitFor(() => expect(out(canvasElement, 'uploading')).toBe('true'));

    // 本文が空のまま宛先が替わると、前の宛先への貼り付けは捨て、保存の抑止も解く
    await userEvent.click(within(canvasElement).getByTestId('switch'));
    await waitFor(() => expect(uploadNote(canvasElement)).toBeNull());
    await expect(out(canvasElement, 'uploading')).toBe('false');

    pasteScope.calls[0].resolve({ id: FILE_ID, name: 'shot.png', mimeType: 'image/png' });
    await new Promise((resolve) => setTimeout(resolve, 100));
    await expect(out(canvasElement)).toBe('');
  },
};

const pasteReplaced = manualUpload();

export const PasteThenReplaceBody: Story = {
  args: { uploadFile: pasteReplaced.uploadFile },
  render: withControls,
  play: async ({ canvasElement }) => {
    pasteReplaced.calls.length = 0;
    await pasteFile(canvasElement, 'shot.png', 'image/png');

    // 外から本文が差し替わったら、前の本文への貼り付けは捨てる
    await userEvent.click(within(canvasElement).getByTestId('replace'));
    await waitFor(() => expect(out(canvasElement)).toBe('別の本文'));
    await waitFor(() => expect(uploadNote(canvasElement)).toBeNull());
    await expect(out(canvasElement, 'uploading')).toBe('false');

    pasteReplaced.calls[0].resolve({ id: FILE_ID, name: 'shot.png', mimeType: 'image/png' });
    await new Promise((resolve) => setTimeout(resolve, 100));
    await expect(out(canvasElement)).toBe('別の本文');
  },
};

const pasteClosed = manualUpload();

export const PasteThenCloseEditor: Story = {
  args: { uploadFile: pasteClosed.uploadFile },
  render: withControls,
  play: async ({ canvasElement }) => {
    pasteClosed.calls.length = 0;
    await pasteFile(canvasElement, 'shot.png', 'image/png');
    await waitFor(() => expect(out(canvasElement, 'uploading')).toBe('true'));

    // 入力欄を閉じる（編集の確定・取り消し）と、保存の抑止を解く。解かないと次に開いた時に保存できない
    await userEvent.click(within(canvasElement).getByTestId('close'));
    await waitFor(() =>
      expect(canvasElement.querySelector('[data-markdown-editor-input]')).toBeNull(),
    );
    await expect(out(canvasElement, 'uploading')).toBe('false');

    pasteClosed.calls[0].resolve({ id: FILE_ID, name: 'shot.png', mimeType: 'image/png' });
    await new Promise((resolve) => setTimeout(resolve, 100));
    await expect(out(canvasElement)).toBe('');
  },
};

const pasteDisabled = manualUpload();

export const PasteThenDisable: Story = {
  args: { uploadFile: pasteDisabled.uploadFile },
  render: withControls,
  play: async ({ canvasElement }) => {
    pasteDisabled.calls.length = 0;
    await pasteFile(canvasElement, 'shot.png', 'image/png');
    await waitFor(() => expect(out(canvasElement, 'uploading')).toBe('true'));

    // 読み取り専用への切り替え（保存中）は state を作り直す。前の貼り付けは捨て、抑止を解く
    await userEvent.click(within(canvasElement).getByTestId('disable'));
    await waitFor(() => expect(uploadNote(canvasElement)).toBeNull());
    await expect(out(canvasElement, 'uploading')).toBe('false');

    pasteDisabled.calls[0].resolve({ id: FILE_ID, name: 'shot.png', mimeType: 'image/png' });
    await new Promise((resolve) => setTimeout(resolve, 100));
    await expect(out(canvasElement)).toBe('');
  },
};

const pasteAfterSave = manualUpload();

export const PasteFailsAfterSaveFailed: Story = {
  args: { uploadFile: pasteAfterSave.uploadFile },
  render: withControls,
  play: async ({ canvasElement }) => {
    pasteAfterSave.calls.length = 0;
    await waitFor(() =>
      expect(canvasElement.querySelector('[data-markdown-editor-input]')).toBeTruthy(),
    );
    // 保存に失敗すると、読み取り専用が一度入って戻る（state を 2 回作り直す）
    await userEvent.click(within(canvasElement).getByTestId('disable'));
    await userEvent.click(within(canvasElement).getByTestId('enable'));
    await waitFor(() =>
      expect(
        canvasElement
          .querySelector('[data-markdown-editor-input]')
          ?.getAttribute('contenteditable'),
      ).toBe('true'),
    );

    // その後も貼り付けを受け、失敗の「再試行」「取り消し」が効く
    await pasteFile(canvasElement, 'a.png', 'image/png');
    pasteAfterSave.calls[0].reject(new Error('ファイルをアップロードできませんでした'));
    const retry = await waitFor(() =>
      within(uploadNote(canvasElement)!).getByRole('button', { name: '再試行' }),
    );
    await userEvent.click(retry);
    await waitFor(() => expect(pasteAfterSave.calls).toHaveLength(2));
    pasteAfterSave.calls[1].reject(new Error('ファイルをアップロードできませんでした'));
    const cancel = await waitFor(() =>
      within(uploadNote(canvasElement)!).getByRole('button', { name: '取り消し' }),
    );
    await userEvent.click(cancel);
    await waitFor(() => expect(uploadNote(canvasElement)).toBeNull());

    // 取り消した後の貼り付けも入る
    await pasteFile(canvasElement, 'b.png', 'image/png');
    pasteAfterSave.calls[2].resolve({ id: FILE_ID, name: 'b.png', mimeType: 'image/png' });
    await waitFor(() => expect(out(canvasElement)).toBe(`![b.png](${FILE_URL})\n`));
  },
};
