import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import CommentBody from '../CommentBody.vue';
import type { MentionTable } from '@/lib/comment-mention';

enableAutoUnmount(afterEach);

beforeAll(async () => {
  await import('@/lib/markup-renderer');
}, 30_000);

async function waitForKfm(wrapper: ReturnType<typeof mount>) {
  await vi.waitFor(() => expect(wrapper.find('.kfm-content').exists()).toBe(true), {
    timeout: 10_000,
  });
}

const ME = '11111111-1111-4111-8111-111111111111';
const OTHER = '22222222-2222-4222-8222-222222222222';
const OUT = '33333333-3333-4333-8333-333333333333';
const GONE = '44444444-4444-4444-8444-444444444444';

const mentions: MentionTable = {
  [ME]: { status: 'member', name: '田中' },
  [OTHER]: { status: 'member', name: 'bob' },
  [OUT]: { status: 'outside', name: 'project-out' },
  [GONE]: { status: 'gone', name: '退会太郎' },
};

describe('CommentBody', () => {
  it('見出し・箇条書き・code・link を comment profile の KFM で描く', async () => {
    const wrapper = mount(CommentBody, {
      props: {
        body: '# 見出し\n- 箇条書き\n- `const ok = true`\n\n[安全なリンク](https://example.com)',
        commentId: 'comment-1',
      },
    });
    await waitForKfm(wrapper);

    expect(wrapper.get('h1').text()).toBe('見出し');
    expect(wrapper.findAll('li')).toHaveLength(2);
    expect(wrapper.get('code').text()).toBe('const ok = true');
    expect(wrapper.get('a').attributes('href')).toBe('https://example.com');
  }, 30_000);

  it('メンション・己への強調・名前つきの対象外と退会を同じ KFM の札にする', async () => {
    const wrapper = mount(CommentBody, {
      props: {
        body: `<@${ME}> <@${OTHER}> <@${OUT}> <@${GONE}>`,
        commentId: 'comment-2',
        mentions,
        currentUserId: ME,
      },
    });
    await waitForKfm(wrapper);

    const chips = wrapper.findAll('.kfm-mention');
    expect(chips.map((c) => c.text())).toEqual(['@田中', '@bob', '@project-out', '@退会太郎']);
    expect(chips[0].classes()).toContain('kfm-mention-self');
    expect(chips[1].classes()).not.toContain('kfm-mention-self');
    expect(chips[2].classes()).toContain('kfm-mention-outside');
    expect(chips[3].classes()).toContain('kfm-mention-gone');
    expect(wrapper.html()).not.toMatch(/[0-9a-f]{8}-[0-9a-f]{4}-/);
  }, 30_000);

  it('生 HTML と危険な URL を同じ sanitizer で落とし、素の @username は札にしない', async () => {
    const wrapper = mount(CommentBody, {
      props: {
        body: '@alice へ\n<script>alert(1)</script>\n[危険](javascript:alert(2))',
        commentId: 'comment-3',
        mentions,
      },
    });
    await waitForKfm(wrapper);

    expect(wrapper.find('.kfm-mention').exists()).toBe(false);
    expect(wrapper.text()).toContain('@alice へ');
    expect(wrapper.find('script').exists()).toBe(false);
    expect(wrapper.get('a').attributes('href')).toBeUndefined();
    expect(wrapper.html()).not.toContain('javascript:');
    expect(wrapper.html()).not.toContain('alert(1)');
  }, 30_000);

  it('編集後に body prop が替わると同じ境界で描き直す', async () => {
    const wrapper = mount(CommentBody, {
      props: { body: '編集前', commentId: 'comment-4', mentions },
    });
    await waitForKfm(wrapper);
    expect(wrapper.text()).toBe('編集前');

    await wrapper.setProps({ body: '## 編集後\n- 再描画' });
    await vi.waitFor(() => expect(wrapper.find('h2').text()).toBe('編集後'), {
      timeout: 10_000,
    });
    expect(wrapper.get('li').text()).toBe('再描画');
  }, 30_000);
});
