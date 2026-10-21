import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

/*
 * MarkdownEditor (CodeMirror) は happy-dom では起動できないため textarea の代役に差し替える。
 */
vi.mock('@/components/markdown/MarkdownEditor.vue', async () => ({
  default: (await import('@/components/markdown/__tests__/markdown-editor-stub')).default,
}));

import DocumentDetail from '../DocumentDetail.vue';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';

// window にリスナー（Cmd+S・離脱確認）を張るので、テストごとに必ず外す
enableAutoUnmount(afterEach);

const ETAG = '"sha256:old"';
const meta = {
  id: 'file-1',
  name: '仕様.md',
  size: 8,
  mime_type: 'text/markdown',
  url: '/v1/drive/files/file-1/content',
  folder_id: 'root-1',
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-02T00:00:00Z',
  etag: ETAG,
};

type Put = { ifMatch: string | null; body: unknown };

let putStatus: number;
let puts: Put[];
let patches: unknown[];
/** メタデータの応答のファイル名。他の人の改名を表す */
let metaName: string;
let queryClient: QueryClient;
let content: string;
/** 本文の応答に付く ETag。既定はメタデータと同じ */
let contentEtag: string;

beforeEach(() => {
  window.history.replaceState(null, '', '/acme/projects/ENG/docs/file-1');
  putStatus = 200;
  puts = [];
  patches = [];
  metaName = meta.name;
  queryClient = new QueryClient();
  content = '旧い本文';
  contentEtag = ETAG;
  vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
    if (typeof input === 'string') {
      // POST /internal/render-description
      const { description } = JSON.parse(init?.body as string) as { description: string };
      return Response.json({ html: `<p>${description}</p>` });
    }
    const url = new URL(input.url);
    const path = url.pathname;
    if (path.endsWith('/drive/folders')) return Response.json([{ id: 'root-1' }]);
    if (path.endsWith('/drive/files/file-1/tasks')) {
      return Response.json({
        tasks: [
          {
            task_id: 't1',
            seq_key: 'ENG-1',
            title: '手で結んだタスク',
            project: { id: 'p', name: 'ENG', key: 'ENG', is_personal: false },
            status: { id: 's', name: 'Todo', color: '#336699' },
            source: 'manual',
          },
          {
            task_id: 't2',
            seq_key: 'ENG-2',
            title: '本文から結んだタスク',
            project: { id: 'p', name: 'ENG', key: 'ENG', is_personal: false },
            status: { id: 's2', name: 'Done', color: '#00aa00' },
            source: 'body',
          },
        ],
      });
    }
    if (path === '/api/v1/drive/files/file-1/content') {
      return new Response(content, { headers: { ETag: contentEtag } });
    }
    if (path.endsWith('/drive/files/file-1/content') && input.method === 'PUT') {
      puts.push({ ifMatch: input.headers.get('If-Match'), body: await input.json() });
      if (putStatus !== 200) return Response.json({ message: 'x' }, { status: putStatus });
      return Response.json({ ...meta, etag: '"sha256:new"' });
    }
    if (path.endsWith('/drive/files/file-1') && input.method === 'PATCH') {
      patches.push(await input.json());
      return Response.json({ ...meta, name: metaName });
    }
    if (path.endsWith('/drive/files/file-1')) return Response.json({ ...meta, name: metaName });
    return Response.json({ message: 'not found' }, { status: 404 });
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = '';
});

function mountDetail(options: { editing?: boolean } = {}) {
  return mount(DocumentDetail, {
    props: {
      tenantId: 'tenant-1' as TenantUuid,
      tenantSlug: 'acme',
      projectId: 'project-1' as ProjectUuid,
      projectKey: 'ENG',
      fileId: 'file-1',
      initialEditing: options.editing ?? false,
      initialHtml: null,
      initialSource: null,
    },
    global: {
      plugins: [[VueQueryPlugin, { queryClient }]],
      stubs: { CreateTaskDialog: true },
    },
    attachTo: document.body,
  });
}

async function editBody(wrapper: ReturnType<typeof mountDetail>, text: string) {
  await wrapper.get('textarea').setValue(text);
}

describe('DocumentDetail 閲覧', () => {
  it('タイトルと関連タスクを出し、手動で結んだものに印を付ける', async () => {
    const wrapper = mountDetail();
    await flushPromises();

    expect(wrapper.get('h1').text()).toBe('仕様');
    const items = wrapper.findAll('aside li');
    expect(items).toHaveLength(2);
    expect(items[0]!.text()).toContain('ENG-1');
    expect(items[0]!.text()).toContain('Todo');
    expect(items[0]!.find('[data-testid="manual-link"]').exists()).toBe(true);
    expect(items[1]!.find('[data-testid="manual-link"]').exists()).toBe(false);
    expect(items[0]!.get('a').attributes('href')).toBe('/acme/projects/ENG/tasks/ENG-1');
  });

  it('本文はサーバで描画した HTML を kfm-content の器に入れる', async () => {
    const wrapper = mountDetail();
    await flushPromises();

    expect(wrapper.get('.kfm-content').html()).toContain('<p>旧い本文</p>');
  });

  it('ルート直下でも .md 以外のファイルは見つからない扱い', async () => {
    const original = globalThis.fetch;
    vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
      if (
        typeof input !== 'string' &&
        new URL(input.url).pathname.endsWith('/drive/files/file-1')
      ) {
        return Response.json({ ...meta, name: 'notes.txt', mime_type: 'text/plain' });
      }
      return original(input, init);
    });
    const wrapper = mountDetail();
    await flushPromises();

    expect(wrapper.text()).toContain('ドキュメントが見つかりません');
    expect(wrapper.find('button').exists()).toBe(false);
  });

  it('ドキュメント置き場の直下に無いファイルは見つからない扱い', async () => {
    const original = globalThis.fetch;
    vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
      if (typeof input !== 'string' && new URL(input.url).pathname.endsWith('/drive/folders')) {
        return Response.json([{ id: 'other-root' }]);
      }
      return original(input, init);
    });
    const wrapper = mountDetail();
    await flushPromises();

    expect(wrapper.text()).toContain('ドキュメントが見つかりません');
  });

  it('ルートフォルダが無いプロジェクトでは Drive 直下の .md も見つからない扱い', async () => {
    const original = globalThis.fetch;
    vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
      if (typeof input !== 'string') {
        const path = new URL(input.url).pathname;
        if (path.endsWith('/drive/folders')) return Response.json([]);
        if (path.endsWith('/drive/files/file-1'))
          return Response.json({ ...meta, folder_id: null });
      }
      return original(input, init);
    });
    const wrapper = mountDetail({ editing: true });
    await flushPromises();

    expect(wrapper.text()).toContain('ドキュメントが見つかりません');
    expect(wrapper.find('form').exists()).toBe(false);
  });
});

describe('DocumentDetail 編集', () => {
  it('保存は読み込んだ時点の ETag を If-Match に付けて本文を送り、閲覧へ戻る', async () => {
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, '新しい本文');

    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(puts).toEqual([{ ifMatch: ETAG, body: { content: '新しい本文' } }]);
    expect(wrapper.find('form').exists()).toBe(false);
    expect(window.location.search).toBe('');
  });

  it('If-Match は本文の応答の ETag で、メタデータの ETag が新しくても使わない', async () => {
    // メタデータと本文は別々に取る。その間に他の人が保存するとメタデータの ETag だけが新しくなる
    contentEtag = '"sha256:body"';
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, '新しい本文');

    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(puts).toEqual([{ ifMatch: '"sha256:body"', body: { content: '新しい本文' } }]);
  });

  async function submitConflict(wrapper: ReturnType<typeof mountDetail>) {
    putStatus = 412;
    await flushPromises();
    await editBody(wrapper, '自分の編集');
    await wrapper.get('form').trigger('submit');
    await flushPromises();

    const alert = wrapper.get('[role="alert"]');
    expect(alert.text()).toContain('他の人が更新しました');
    expect(wrapper.find('form').exists()).toBe(true);
    // 他の人の保存で本文と ETag が進んでいる
    content = '他の人の本文';
    contentEtag = '"sha256:theirs"';
    return alert;
  }

  it('412 のあと「最新を読み込む」は下書きを捨てて最新の本文から編集し直す', async () => {
    const wrapper = mountDetail({ editing: true });
    const alert = await submitConflict(wrapper);

    await alert.get('button').trigger('click');
    await flushPromises();

    expect(wrapper.text()).not.toContain('他の人が更新しました');
    expect((wrapper.get('textarea').element as HTMLTextAreaElement).value).toBe('他の人の本文');
    expect(puts).toHaveLength(1);
  });

  it('412 のあと「自分の下書きで上書き」だけが新しい ETag で下書きを送る', async () => {
    const wrapper = mountDetail({ editing: true });
    const alert = await submitConflict(wrapper);

    putStatus = 200;
    await alert.findAll('button')[1]!.trigger('click');
    await flushPromises();

    expect(puts).toEqual([
      { ifMatch: ETAG, body: { content: '自分の編集' } },
      { ifMatch: '"sha256:theirs"', body: { content: '自分の編集' } },
    ]);
    expect(wrapper.find('form').exists()).toBe(false);
  });

  it('本文だけを直して上書きしても、他の人が変えたタイトルを下書きの名前へ戻さない', async () => {
    const wrapper = mountDetail({ editing: true });
    const alert = await submitConflict(wrapper);
    // 412 の間に他の人がタイトルも変えた
    metaName = '改名後.md';

    putStatus = 200;
    await alert.findAll('button')[1]!.trigger('click');
    await flushPromises();

    expect(puts).toHaveLength(2);
    expect(patches).toEqual([]);
  });

  it('メタデータを取り直して名前が変わっても、タイトルに触れていなければ未保存の変更にしない', async () => {
    const confirm = vi.fn(() => false);
    vi.stubGlobal('confirm', confirm);
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    metaName = '改名後.md';
    await queryClient.refetchQueries();
    await flushPromises();

    const cancel = wrapper.findAll('button').find((button) => button.text() === 'キャンセル')!;
    await cancel.trigger('click');

    expect(confirm).not.toHaveBeenCalled();
    expect(wrapper.find('form').exists()).toBe(false);
    // 取り直した名前が出ている（取り直しが効いている）
    expect(wrapper.get('h1').text()).toBe('改名後');
  });

  it('タイトルを変えたときだけ改名を送る', async () => {
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await wrapper.get('input').setValue('新しい仕様');

    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(puts).toEqual([]);
    expect(patches).toEqual([{ name: '新しい仕様.md' }]);
  });

  it.each([
    ['Cmd+S', { metaKey: true }],
    ['Ctrl+S', { ctrlKey: true }],
  ])('%s で保存する', async (_label, modifier) => {
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, 'ショートカットで保存');

    const event = new KeyboardEvent('keydown', { key: 's', cancelable: true, ...modifier });
    window.dispatchEvent(event);
    await flushPromises();

    expect(event.defaultPrevented).toBe(true);
    expect(puts).toHaveLength(1);
    expect(puts[0]!.body).toEqual({ content: 'ショートカットで保存' });
  });

  it('修飾キーなしの s では保存しない', async () => {
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, '変更');

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's' }));
    await flushPromises();

    expect(puts).toHaveLength(0);
  });

  it('未保存でキャンセルするときは確認し、断れば編集を続ける', async () => {
    const confirm = vi.fn(() => false);
    vi.stubGlobal('confirm', confirm);
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, '未保存');

    const cancel = wrapper.findAll('button').find((button) => button.text() === 'キャンセル')!;
    await cancel.trigger('click');

    expect(confirm).toHaveBeenCalled();
    expect(wrapper.find('form').exists()).toBe(true);
  });

  it('保存の応答を待つ間はタイトルと本文を編集できない', async () => {
    // 待つ間の追記は送った本文に入らず、成功で下書きごと閉じて失われる
    let release!: () => void;
    const original = globalThis.fetch;
    vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
      if (typeof input !== 'string' && input.method === 'PUT') {
        await new Promise<void>((resolve) => (release = resolve));
      }
      return original(input, init);
    });
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, '送る本文');

    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(wrapper.get('input').attributes('disabled')).toBeDefined();
    expect(wrapper.get('textarea').attributes('disabled')).toBeDefined();

    release();
    await flushPromises();
    expect(puts).toEqual([{ ifMatch: ETAG, body: { content: '送る本文' } }]);
  });
});

describe('DocumentDetail 通信の失敗', () => {
  /** 指定したメソッドとパスの要求だけ、オフライン時の fetch と同じく reject させる */
  function failRequest(method: string, pathSuffix: string) {
    const original = globalThis.fetch;
    vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
      if (
        typeof input !== 'string' &&
        input.method === method &&
        new URL(input.url).pathname.endsWith(pathSuffix)
      ) {
        throw new TypeError('Failed to fetch');
      }
      return original(input, init);
    });
  }

  it('本文の保存で通信が切れたら、下書きを残してエラーを出す', async () => {
    failRequest('PUT', '/drive/files/file-1/content');
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, '送れなかった本文');

    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toBe('保存できませんでした');
    const textarea = wrapper.get('textarea');
    expect((textarea.element as HTMLTextAreaElement).value).toBe('送れなかった本文');
    expect(textarea.attributes('disabled')).toBeUndefined();
  });

  it('改名で通信が切れたら、本文は保存済みと分かるエラーを出して編集を続ける', async () => {
    failRequest('PATCH', '/drive/files/file-1');
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, '新しい本文');
    await wrapper.get('input').setValue('新しい仕様');

    await wrapper.get('form').trigger('submit');
    await flushPromises();

    expect(puts).toHaveLength(1);
    expect(wrapper.get('[role="alert"]').text()).toBe(
      '本文は保存しましたが、タイトルを変更できませんでした',
    );
    expect((wrapper.get('input').element as HTMLInputElement).value).toBe('新しい仕様');
  });

  it('タスク作成後の結び付けで通信が切れたら、タスクは作成済みと分かるエラーを出す', async () => {
    failRequest('POST', '/tasks/task-9/attachments');
    const wrapper = mountDetail();
    await flushPromises();

    wrapper.findComponent({ name: 'CreateTaskDialog' }).vm.$emit('created', { id: 'task-9' });
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toBe(
      'タスクは作成しましたが、このドキュメントに結び付けられませんでした',
    );
  });
});

describe('DocumentDetail の T ショートカット', () => {
  const pressT = () =>
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 't', cancelable: true }));
  const createOpen = (wrapper: ReturnType<typeof mountDetail>) =>
    wrapper.getComponent({ name: 'CreateTaskDialog' }).props('open');

  it('閲覧中は T で作成ダイアログを開く', async () => {
    const wrapper = mountDetail();
    await flushPromises();
    pressT();
    await flushPromises();
    expect(createOpen(wrapper)).toBe(true);
  });

  it('編集中は T で開かない', async () => {
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    expect(wrapper.find('form').exists()).toBe(true);
    pressT();
    await flushPromises();
    expect(createOpen(wrapper)).toBe(false);
  });

  it('読み込み中は T で開かず、読み込み後も勝手に開かない', async () => {
    const original = globalThis.fetch;
    let finish!: () => void;
    const gate = new Promise<void>((resolve) => {
      finish = resolve;
    });
    vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
      if (typeof input !== 'string' && new URL(input.url).pathname.endsWith('/drive/folders')) {
        await gate;
      }
      return original(input, init);
    });
    const wrapper = mountDetail();
    await flushPromises();
    expect(wrapper.find('[aria-label="読み込み中"]').exists()).toBe(true);
    pressT();
    finish();
    await flushPromises();
    expect(wrapper.get('h1').text()).toBe('仕様');
    expect(createOpen(wrapper)).toBe(false);
  });

  it('取得に失敗したら T で開かない', async () => {
    const original = globalThis.fetch;
    vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
      if (
        typeof input !== 'string' &&
        new URL(input.url).pathname.endsWith('/drive/files/file-1')
      ) {
        return Response.json({ message: 'x' }, { status: 500 });
      }
      return original(input, init);
    });
    const wrapper = mountDetail();
    await flushPromises();
    expect(wrapper.text()).toContain('ドキュメントが見つかりません');
    pressT();
    await flushPromises();
    expect(createOpen(wrapper)).toBe(false);
  });

  it('対象外のファイルでは T で開かない', async () => {
    const original = globalThis.fetch;
    vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
      if (typeof input !== 'string' && new URL(input.url).pathname.endsWith('/drive/folders')) {
        return Response.json([{ id: 'other-root' }]);
      }
      return original(input, init);
    });
    const wrapper = mountDetail();
    await flushPromises();
    expect(wrapper.text()).toContain('ドキュメントが見つかりません');
    pressT();
    await flushPromises();
    expect(createOpen(wrapper)).toBe(false);
  });
});
