import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ref } from 'vue';

import CommentEditor from '@/components/tasks/CommentEditor.vue';

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
