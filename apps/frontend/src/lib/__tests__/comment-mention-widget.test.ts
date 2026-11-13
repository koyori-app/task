// @vitest-environment happy-dom
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { describe, expect, it } from 'vitest';

import { mentionExtensions } from '../comment-mention-extensions';

const ALICE = '11111111-1111-4111-8111-111111111111';
const FORMER = '22222222-2222-4222-8222-222222222222';
const GONE = '33333333-3333-4333-8333-333333333333';
const MISSING = '44444444-4444-4444-8444-444444444444';

describe('入力欄のメンションの札（説明文の KFM の札と同じ形）', () => {
  it('koyori の固定 class で、`@名` を包んだ span 1 つだけを持つ（アバターは出さない）', () => {
    const host = document.createElement('div');
    document.body.append(host);
    const view = new EditorView({
      state: EditorState.create({
        doc: `<@${ALICE}> <@${FORMER}> <@${GONE}> <@${MISSING}>`,
        extensions: mentionExtensions({
          tooltipParent: document.body,
          candidates: () => [],
          names: {
            [ALICE]: {
              status: 'member',
              name: 'alice',
              avatar_url: 'https://cdn.example.com/a.png',
            },
            [FORMER]: { status: 'outside', name: 'former' },
            [GONE]: { status: 'gone', name: null },
          },
          disabled: () => false,
        }),
      }),
      parent: host,
    });

    const chips = [...host.querySelectorAll<HTMLElement>('.cm-mention')];
    expect(chips).toHaveLength(4);
    const expected = [
      ['member', '@alice'],
      ['outside', '@former'],
      ['gone', '@（退会した利用者）'],
      ['unknown', '@（不明な利用者）'],
    ] as const;
    chips.forEach((chip, i) => {
      const [state, label] = expected[i];
      // 入力欄は currentUserId を渡さないので、member でも self は付けない
      expect([...chip.classList], state).toEqual([
        'cm-mention',
        'koyori-mention',
        `koyori-mention-${state}`,
      ]);
      expect(chip.children).toHaveLength(1);
      expect(chip.childNodes).toHaveLength(1);
      expect(chip.children[0].tagName).toBe('SPAN');
      expect(chip.children[0].className).toBe('koyori-mention-label');
      expect(chip.children[0].textContent).toBe(label);
      expect(chip.querySelector('img, [data-initials]')).toBeNull();
      expect(chip.dataset.mentionStatus).toBe(state);
    });

    view.destroy();
    host.remove();
  });
});
