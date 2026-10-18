import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import CommentBody from '../CommentBody.vue';
import type { MentionTable } from '@/lib/comment-mention';

enableAutoUnmount(afterEach);

const ME = '11111111-1111-4111-8111-111111111111';
const OTHER = '22222222-2222-4222-8222-222222222222';
const OUT = '33333333-3333-4333-8333-333333333333';
const GONE = '44444444-4444-4444-8444-444444444444';

const mentions: MentionTable = {
  [ME]: { status: 'member', name: '田中' },
  [OTHER]: { status: 'member', name: 'bob' },
  [OUT]: { status: 'outside', name: null },
  [GONE]: { status: 'gone', name: null },
};

describe('CommentBody', () => {
  it('<@id> を @今の名 の札に描き、id の文字は画面に出さない', () => {
    const wrapper = mount(CommentBody, {
      props: { body: `<@${OTHER}> さん、確認お願いします`, mentions, currentUserId: ME },
    });
    const chip = wrapper.get('[data-mention]');
    expect(chip.text()).toBe('@bob');
    expect(chip.attributes('data-mention-status')).toBe('member');
    expect(wrapper.text()).toBe('@bob さん、確認お願いします');
    expect(wrapper.html()).not.toContain(OTHER);
  });

  it('己へのメンションは強調し、退会・対象外は灰色の定型の句にする', () => {
    const wrapper = mount(CommentBody, {
      props: { body: `<@${ME}> <@${OTHER}> <@${OUT}> <@${GONE}>`, mentions, currentUserId: ME },
    });
    const chips = wrapper.findAll('[data-mention]');
    expect(chips.map((c) => c.text())).toEqual([
      '@田中',
      '@bob',
      '@（このプロジェクトの外の利用者）',
      '@（退会した利用者）',
    ]);
    expect(chips[0].attributes('data-mention-self')).toBeDefined();
    expect(chips[1].attributes('data-mention-self')).toBeUndefined();
    expect(chips[2].attributes('data-mention-status')).toBe('outside');
    expect(chips[3].attributes('data-mention-status')).toBe('gone');
    expect(wrapper.html()).not.toMatch(/[0-9a-f]{8}-[0-9a-f]{4}-/);
  });

  it('表に無い id は不明な利用者の札にする', () => {
    const wrapper = mount(CommentBody, { props: { body: `<@${OUT}>`, mentions: {} } });
    expect(wrapper.get('[data-mention]').text()).toBe('@（不明な利用者）');
  });

  it('既存の素の @username は札にせず今のまま描く。HTML はエスケープされたまま', () => {
    const wrapper = mount(CommentBody, {
      props: { body: '@alice へ\n<script>alert(1)</script>', mentions },
    });
    expect(wrapper.find('[data-mention]').exists()).toBe(false);
    expect(wrapper.text()).toContain('@alice へ');
    expect(wrapper.text()).toContain('<script>alert(1)</script>');
    expect(wrapper.find('script').exists()).toBe(false);
  });

  it('profileHref を渡すと member の札だけをリンクにする', () => {
    const wrapper = mount(CommentBody, {
      props: {
        body: `<@${OTHER}> <@${OUT}>`,
        mentions,
        profileHref: (id: string) => `/acme/users/${id}`,
      },
    });
    const chips = wrapper.findAll('[data-mention]');
    expect(chips[0].element.tagName).toBe('A');
    expect(chips[0].attributes('href')).toBe(`/acme/users/${OTHER}`);
    expect(chips[1].element.tagName).toBe('SPAN');
  });
});
