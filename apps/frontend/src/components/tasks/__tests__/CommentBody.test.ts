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
    // 生 HTML は要素にならず、字として見える
    expect(wrapper.find('script').exists()).toBe(false);
    expect(wrapper.html()).not.toContain('<script');
    expect(wrapper.text()).toContain('<script>alert(1)</script>');
    expect(wrapper.get('a').attributes('href')).toBeUndefined();
    expect(wrapper.html()).not.toContain('javascript:');
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

  it('描画の上限（65536 文字）を超える本文は描かず、素の文字で出す', async () => {
    const body = `# 長い\n${'あ'.repeat(65_536)}`;
    const wrapper = mount(CommentBody, { props: { body, commentId: 'comment-5' } });
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(wrapper.find('.kfm-content').exists()).toBe(false);
    expect(wrapper.find('h1').exists()).toBe(false);
    expect(wrapper.get('[data-comment-body]').text().startsWith('# 長い')).toBe(true);
  });

  it('札は頭にアバターを置く（画像があれば img、無ければ頭文字の丸。字には混ぜぬ）', async () => {
    const wrapper = mount(CommentBody, {
      props: {
        body: `<@${ME}> <@${OTHER}>`,
        commentId: 'comment-6',
        mentions: {
          [ME]: { status: 'member', name: '田中', avatar_url: 'https://cdn.example.com/me.png' },
          [OTHER]: { status: 'member', name: 'bob', avatar_url: null },
        },
        currentUserId: ME,
      },
    });
    await waitForKfm(wrapper);
    const chips = wrapper.findAll('.kfm-mention');
    expect(chips[0].classes()).toEqual(
      expect.arrayContaining(['kfm-mention', 'kfm-mention-member', 'kfm-mention-self']),
    );
    const own = chips[0].get('.kfm-mention-avatar');
    expect(own.attributes('aria-hidden')).toBe('true');
    expect(own.get('img').attributes()).toMatchObject({
      src: 'https://cdn.example.com/me.png',
      alt: '',
    });
    const bob = chips[1].get('.kfm-mention-avatar');
    expect(bob.attributes('data-initials')).toBe('BO');
    expect(bob.find('img').exists()).toBe(false);
    // 札の字は名だけ（頭文字は CSS で描くので text に出ない）
    expect(chips.map((c) => c.text())).toEqual(['@田中', '@bob']);
  }, 30_000);

  it('外れた者の札は灰色で頭にアバター、名の引けぬ者・退会した者の札はアバター無し', async () => {
    const wrapper = mount(CommentBody, {
      props: {
        body: `<@${OTHER}> <@${OUT}> <@${GONE}>`,
        commentId: 'comment-7',
        mentions: {
          [OTHER]: { status: 'outside', name: 'bob', avatar_url: 'https://cdn.example.com/b.png' },
          [OUT]: { status: 'outside', name: null },
          [GONE]: { status: 'gone', name: null },
        },
      },
    });
    await waitForKfm(wrapper);
    const chips = wrapper.findAll('.kfm-mention');
    expect(chips[0].classes()).toContain('kfm-mention-outside');
    expect(chips[0].find('.kfm-mention-avatar img').exists()).toBe(true);
    expect(chips[1].find('.kfm-mention-avatar').exists()).toBe(false);
    expect(chips[2].find('.kfm-mention-avatar').exists()).toBe(false);
    expect(chips[2].classes()).toContain('kfm-mention-gone');
  }, 30_000);

  it('危ない URL のアバターは img にせぬ', async () => {
    const wrapper = mount(CommentBody, {
      props: {
        body: `<@${OTHER}>`,
        commentId: 'comment-8',
        mentions: { [OTHER]: { status: 'member', name: 'bob', avatar_url: 'javascript:alert(1)' } },
      },
    });
    await waitForKfm(wrapper);
    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.get('.kfm-mention-avatar').attributes('data-initials')).toBe('BO');
  }, 30_000);
  it('mentionTenant を渡すと、いま入れる者の札だけをプロフィールへの link にする', async () => {
    const wrapper = mount(CommentBody, {
      props: {
        body: `<@${ME}> <@${OTHER}> <@${OUT}>`,
        commentId: 'comment-9',
        mentions: {
          [ME]: { status: 'member', name: '田中' },
          [OTHER]: { status: 'member', name: 'bob' },
          [OUT]: { status: 'outside', name: 'project-out' },
        },
        currentUserId: ME,
        mentionTenant: 'acme',
      },
    });
    await waitForKfm(wrapper);
    const links = wrapper.findAll('a.kfm-mention');
    expect(links.map((a) => a.attributes('href'))).toEqual([
      `/acme/users/${ME}`,
      `/acme/users/${OTHER}`,
    ]);
    expect(links[0].classes()).toContain('kfm-mention-self');
    // 外れた者の札は link にせぬ
    expect(wrapper.get('span.kfm-mention-outside').text()).toBe('@project-out');
  }, 30_000);

  it('mentionTenant を渡さぬコメントの札は link にならない', async () => {
    const wrapper = mount(CommentBody, {
      props: { body: `<@${OTHER}>`, commentId: 'comment-10', mentions },
    });
    await waitForKfm(wrapper);
    expect(wrapper.find('a').exists()).toBe(false);
    expect(wrapper.get('span.kfm-mention').text()).toBe('@bob');
  }, 30_000);
});
