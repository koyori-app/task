import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import { nextTick } from 'vue';

import TaskActivityFeed from '@/components/tasks/TaskActivityFeed.vue';
import type { ActivityItem } from '@/lib/task-activity';

enableAutoUnmount(afterEach);

function activity(id: string, secondsAgo: number): ActivityItem {
  return {
    id,
    event_type: 'status_changed',
    payload: { to: 'In Progress' },
    created_at: new Date(new Date().getTime() - secondsAgo * 1000).toISOString(),
    user: { id: 'user-1', name: 'yupix' },
  } as ActivityItem;
}

describe('TaskActivityFeed', () => {
  it('残りがあるときだけ段階取得の導線を出す', async () => {
    const onLoadMore = vi.fn();
    const wrapper = mount(TaskActivityFeed, {
      props: { activities: [activity('act-1', 10)], hasMore: true, onLoadMore },
    });

    const button = wrapper.findAll('button').find((b) => b.text() === '以前の履歴を見る');
    expect(button).toBeDefined();
    await button!.trigger('click');
    expect(onLoadMore).toHaveBeenCalledTimes(1);

    await wrapper.setProps({ hasMore: false });
    expect(wrapper.findAll('button').some((b) => b.text() === '以前の履歴を見る')).toBe(false);
  });

  it('取得中は導線を押せない', () => {
    const wrapper = mount(TaskActivityFeed, {
      props: {
        activities: [activity('act-1', 10)],
        hasMore: true,
        fetchingMore: true,
        onLoadMore: () => {},
      },
    });

    const button = wrapper.findAll('button').find((b) => b.text() === '読み込み中…');
    expect(button).toBeDefined();
    expect(button!.attributes('disabled')).toBeDefined();
  });

  // テンプレートから new Date() を呼ぶと追跡されず、開いたままだと表示が止まる
  describe('相対時刻', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it('開いたままでも時間の経過に追従する', async () => {
      const wrapper = mount(TaskActivityFeed, {
        props: { activities: [activity('act-1', 5)] },
      });

      expect(wrapper.text()).toContain('たった今');

      // useNow の間隔（30 秒）を越えて進める
      vi.advanceTimersByTime(120_000);
      await nextTick();

      expect(wrapper.text()).not.toContain('たった今');
      expect(wrapper.text()).toContain('分前');
    });
  });

  describe('コミットのリンク', () => {
    const sha = 'a3f92c1e7b81d4000000000000000000000000aa';
    const payload = {
      host: 'github',
      sha,
      message: 'fix: トークン期限切れを修正 TASK-1',
      author_handle: 'yupix',
      author_name: 'Yupix',
      html_url: `https://github.com/acme/backend/commit/${sha}`,
    };

    function commitItem(
      overrides: Record<string, unknown> = {},
      user: ActivityItem['user'] = null,
    ): ActivityItem {
      return {
        ...activity('commit-1', 10),
        event_type: 'forge_commit_linked',
        user,
        payload: { ...payload, ...overrides },
      } as ActivityItem;
    }

    it('短縮 SHA をコミットへのリンクにし、作者とメッセージを出す', () => {
      const wrapper = mount(TaskActivityFeed, { props: { activities: [commitItem()] } });

      expect(wrapper.text()).toContain(
        'yupixがコミット a3f92c1 をリンクしました「fix: トークン期限切れを修正 TASK-1」',
      );
      const link = wrapper.get('a');
      expect(link.text()).toBe('a3f92c1');
      expect(link.attributes('href')).toBe(payload.html_url);
      expect(link.attributes('target')).toBe('_blank');
      expect(link.attributes('rel')).toBe('noopener noreferrer');
    });

    it('連携済みユーザーに結べていればそのユーザー名を出す', () => {
      const wrapper = mount(TaskActivityFeed, {
        props: {
          activities: [commitItem({}, { id: 'user-1', name: 'ゆぴ' } as ActivityItem['user'])],
        },
      });
      expect(wrapper.text()).toContain('ゆぴがコミット');
    });

    it('ホスト上のアカウント名が無ければ git の author 名を出す', () => {
      const wrapper = mount(TaskActivityFeed, {
        props: { activities: [commitItem({ author_handle: '' })] },
      });
      expect(wrapper.text()).toContain('Yupixがコミット');
    });

    it('http(s) 以外の URL はリンクにしない', () => {
      const wrapper = mount(TaskActivityFeed, {
        props: { activities: [commitItem({ html_url: 'javascript:alert(1)' })] },
      });
      expect(wrapper.find('a').exists()).toBe(false);
      expect(wrapper.text()).toContain('コミット a3f92c1 をリンクしました');
    });
  });
});
