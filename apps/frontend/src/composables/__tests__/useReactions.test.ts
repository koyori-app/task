import { flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, ref } from 'vue';

import type { components, paths } from '@/generated/api';

import { GET_TASK_PATH } from '../useTaskDetail';
import { LIST_COMMENTS_PATH } from '../useTaskComments';
import { useReactions } from '../useReactions';

const { control, requests, fetchMock } = vi.hoisted(() => {
  const control: {
    reject?: { status: number; body: unknown };
    hold?: Promise<void>;
    usersReject?: boolean;
  } = {};
  const requests: { method: string; url: string }[] = [];
  const fetchMock = async (req: Request) => {
    requests.push({ method: req.method, url: req.url });
    const json = (body: unknown, status = 200) =>
      new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
      });
    if (req.url.endsWith('/users')) {
      if (control.usersReject) return json({ message: 'boom' }, 500);
      return json({
        emoji: '👍',
        count: 1,
        users: [{ id: 'u-1', name: '田中', avatar_url: null }],
      });
    }
    if (req.method === 'GET') return json({ message: 'unused' }, 404);
    await control.hold;
    if (control.reject) return json(control.reject.body, control.reject.status);
    return json({ emoji: '👍', count: 1, reacted: true, users: [] });
  };
  return { control, requests, fetchMock };
});

vi.mock('@/lib/api-vue-query', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api-vue-query')>();
  const { default: createFetchClient } = await import('openapi-fetch');
  const { createClient } = await import('@koyori-app/openapi-vue-query');
  const testFetchClient = createFetchClient<paths>({
    baseUrl: 'http://test.local/api',
    fetch: (req: Request) => fetchMock(req),
  });
  return {
    ...actual,
    fetchClient: testFetchClient,
    apiClient: createClient<paths>(testFetchClient),
  };
});

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));

type Summary = components['schemas']['ReactionSummary'];

const ME = { id: 'me', name: '自分', avatar_url: null };
const OTHER = { id: 'other', name: '他の人', avatar_url: null };
const PATH = { tenant_id: 't', project_id: 'p', id: 'ENG-1' };
const TASK_KEY = ['get', GET_TASK_PATH, { params: { path: PATH } }];
const COMMENTS_KEY = ['get', LIST_COMMENTS_PATH, { params: { path: PATH } }];

const othersThumbs: Summary = { emoji: '👍', count: 1, reacted: false, users: [OTHER] };
const myThumbs: Summary = { emoji: '👍', count: 1, reacted: true, users: [ME] };

function commentList(threadReactions: Summary[], replyReactions: Summary[] = []) {
  return {
    comments: [
      {
        id: 'c-1',
        reactions: threadReactions,
        replies: [{ id: 'r-1', reactions: replyReactions }],
      },
    ],
    mentions: {},
  };
}

describe('useReactions', () => {
  let queryClient: QueryClient;
  let reactions: ReturnType<typeof useReactions>;

  function mountHost() {
    const Host = defineComponent({
      setup() {
        reactions = useReactions({
          tenantId: 't',
          projectId: 'p',
          taskId: 'ENG-1',
          currentUser: ME,
        });
        return () => null;
      },
    });
    mount(Host, { global: { plugins: [[VueQueryPlugin, { queryClient }]] } });
  }

  function taskReactions() {
    return queryClient.getQueryData<{ reactions: Summary[] }>(TASK_KEY)?.reactions;
  }

  function comments() {
    return queryClient.getQueryData<ReturnType<typeof commentList>>(COMMENTS_KEY)?.comments[0];
  }

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    control.reject = undefined;
    control.hold = undefined;
    control.usersReject = undefined;
    requests.length = 0;
    queryClient.setQueryData(TASK_KEY, { id: 'task', reactions: [] });
    queryClient.setQueryData(COMMENTS_KEY, commentList([]));
    mountHost();
  });

  it('絵文字を 1 回だけエンコードしてタスク本体へ PUT する', async () => {
    await reactions.toggle(null, '👍');
    expect(requests[0]).toEqual({
      method: 'PUT',
      url: 'http://test.local/api/v1/tenants/t/projects/p/tasks/ENG-1/reactions/%F0%9F%91%8D',
    });
  });

  it('コメントへは comments/{cid} の下へ送る', async () => {
    await reactions.toggle('c-1', '👍');
    expect(requests[0]?.url).toBe(
      'http://test.local/api/v1/tenants/t/projects/p/tasks/ENG-1/comments/c-1/reactions/%F0%9F%91%8D',
    );
  });

  it('自分が押していれば DELETE、他の人だけが押していれば PUT する', async () => {
    queryClient.setQueryData(TASK_KEY, { id: 'task', reactions: [myThumbs] });
    await reactions.toggle(null, '👍');
    expect(requests[0]?.method).toBe('DELETE');

    requests.length = 0;
    queryClient.setQueryData(TASK_KEY, { id: 'task', reactions: [othersThumbs] });
    await reactions.toggle(null, '👍');
    expect(requests[0]?.method).toBe('PUT');
  });

  it('応答の前に表示を変え、その間は同じ対象の操作を受けない', async () => {
    let release = () => {};
    control.hold = new Promise((resolve) => (release = resolve));
    const pending = reactions.toggle(null, '👍');
    await flushPromises();

    expect(taskReactions()).toEqual([{ emoji: '👍', count: 1, reacted: true, users: [ME] }]);
    expect(reactions.isBusy(null)).toBe(true);
    expect(reactions.isBusy('c-1')).toBe(false);
    await reactions.toggle(null, '🎉');
    expect(requests.filter((request) => request.method !== 'GET')).toHaveLength(1);

    release();
    await pending;
    expect(reactions.isBusy(null)).toBe(false);
  });

  it('返信の札も書き換え、他のコメントの札には触らない', async () => {
    let release = () => {};
    control.hold = new Promise((resolve) => (release = resolve));
    queryClient.setQueryData(COMMENTS_KEY, commentList([othersThumbs], []));
    const pending = reactions.toggle('r-1', '🎉');
    await flushPromises();

    expect(comments()?.replies[0]?.reactions.map((item) => item.emoji)).toEqual(['🎉']);
    expect(comments()?.reactions).toEqual([othersThumbs]);
    release();
    await pending;
  });

  it('失敗したら表示を戻し、上限超過は決まった文言を出す', async () => {
    control.reject = { status: 400, body: { message: 'reaction-limit-exceeded' } };
    queryClient.setQueryData(TASK_KEY, { id: 'task', reactions: [othersThumbs] });
    await reactions.toggle(null, '👍');

    expect(taskReactions()).toEqual([othersThumbs]);
    expect(reactions.errorFor(null)).toBe('これ以上種類を増やせません');
    expect(reactions.errorFor('c-1')).toBeNull();
  });

  it('bad-request は内部の語を見せない。それ以外の拒否は理由を添える', async () => {
    control.reject = { status: 400, body: { message: 'bad-request' } };
    await reactions.toggle('c-1', '👍');
    expect(reactions.errorFor('c-1')).toBe('リアクションを送れませんでした');

    control.reject = { status: 403, body: { message: 'forbidden' } };
    await reactions.toggle('c-1', '👍');
    expect(reactions.errorFor('c-1')).toBe('リアクションを送れませんでした（forbidden）');
  });

  it('次の操作で前の失敗の文言を消す', async () => {
    control.reject = { status: 400, body: { message: 'bad-request' } };
    await reactions.toggle(null, '👍');
    control.reject = undefined;
    await reactions.toggle(null, '👍');
    expect(reactions.errorFor(null)).toBeNull();
  });

  it('成功したらコメント一覧とタスク詳細を invalidate する', async () => {
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    await reactions.toggle('c-1', '👍');
    const keys = invalidate.mock.calls.map(
      ([filters]) => (filters as { queryKey?: unknown }).queryKey,
    );
    expect(keys).toContainEqual(COMMENTS_KEY);
    expect(keys).toContainEqual(TASK_KEY);
  });

  it('失敗したら invalidate しない', async () => {
    control.reject = { status: 500, body: { message: 'boom' } };
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
    await reactions.toggle('c-1', '👍');
    expect(invalidate).not.toHaveBeenCalled();
  });

  it('押した全員を対象ごとの GET で取る', async () => {
    await expect(reactions.fetchUsers(null, '👍')).resolves.toHaveLength(1);
    await reactions.fetchUsers('c-1', '👍');
    expect(requests.map((request) => request.url)).toEqual([
      'http://test.local/api/v1/tenants/t/projects/p/tasks/ENG-1/reactions/%F0%9F%91%8D/users',
      'http://test.local/api/v1/tenants/t/projects/p/tasks/ENG-1/comments/c-1/reactions/%F0%9F%91%8D/users',
    ]);
  });

  describe('同じペインで別のタスクへ切り替えた時', () => {
    const taskId = ref('ENG-1');

    function mountSwitchingHost() {
      taskId.value = 'ENG-1';
      const Host = defineComponent({
        setup() {
          reactions = useReactions({
            tenantId: 't',
            projectId: 'p',
            taskId,
            currentUser: ME,
          });
          return () => null;
        },
      });
      mount(Host, { global: { plugins: [[VueQueryPlugin, { queryClient }]] } });
    }

    function holdResponses() {
      let release = () => {};
      control.hold = new Promise((resolve) => (release = resolve));
      return release;
    }

    it('前のタスクの送信中でも次のタスクは押せる', async () => {
      mountSwitchingHost();
      const release = holdResponses();
      const first = reactions.toggle(null, '👍');
      await flushPromises();
      expect(reactions.isBusy(null)).toBe(true);

      taskId.value = 'ENG-2';
      expect(reactions.isBusy(null)).toBe(false);
      const second = reactions.toggle(null, '👍');
      await flushPromises();
      expect(requests.map((request) => request.url)).toEqual([
        'http://test.local/api/v1/tenants/t/projects/p/tasks/ENG-1/reactions/%F0%9F%91%8D',
        'http://test.local/api/v1/tenants/t/projects/p/tasks/ENG-2/reactions/%F0%9F%91%8D',
      ]);

      // 前のタスクへ戻れば、まだ送信中と分かる
      taskId.value = 'ENG-1';
      expect(reactions.isBusy(null)).toBe(true);
      release();
      await Promise.all([first, second]);
      expect(reactions.isBusy(null)).toBe(false);
    });

    it('前のタスクの失敗を次のタスクに出さず、戻れば前のタスクに出す', async () => {
      mountSwitchingHost();
      control.reject = { status: 400, body: { message: 'reaction-limit-exceeded' } };
      const release = holdResponses();
      const pending = reactions.toggle(null, '👍');
      await flushPromises();

      taskId.value = 'ENG-2';
      release();
      await pending;
      expect(reactions.errorFor(null)).toBeNull();

      taskId.value = 'ENG-1';
      expect(reactions.errorFor(null)).toBe('これ以上種類を増やせません');
    });

    it('コメントの状態もタスクごとに分ける', async () => {
      mountSwitchingHost();
      const release = holdResponses();
      control.reject = { status: 400, body: { message: 'bad-request' } };
      const pending = reactions.toggle('c-1', '👍');
      await flushPromises();
      expect(reactions.isBusy('c-1')).toBe(true);

      taskId.value = 'ENG-2';
      expect(reactions.isBusy('c-1')).toBe(false);
      release();
      await pending;
      expect(reactions.errorFor('c-1')).toBeNull();

      taskId.value = 'ENG-1';
      expect(reactions.errorFor('c-1')).toBe('リアクションを送れませんでした');
    });
  });

  it('全員の取得の失敗は呼び手へ伝える（一覧が文言を出す）', async () => {
    control.usersReject = true;
    await expect(reactions.fetchUsers(null, '👍')).rejects.toBeDefined();
  });
});
