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
let content: string;

beforeEach(() => {
  window.history.replaceState(null, '', '/acme/projects/ENG/docs/file-1');
  putStatus = 200;
  puts = [];
  content = '旧い本文';
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
    if (path === '/api/v1/drive/files/file-1/content') return new Response(content);
    if (path.endsWith('/drive/files/file-1/content') && input.method === 'PUT') {
      puts.push({ ifMatch: input.headers.get('If-Match'), body: await input.json() });
      if (putStatus !== 200) return Response.json({ message: 'x' }, { status: putStatus });
      return Response.json({ ...meta, etag: '"sha256:new"' });
    }
    if (path.endsWith('/drive/files/file-1')) return Response.json(meta);
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
      plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]],
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

  it('412 なら他の人の更新を知らせ、再読込で最新を取り直しても下書きは残す', async () => {
    putStatus = 412;
    const wrapper = mountDetail({ editing: true });
    await flushPromises();
    await editBody(wrapper, '自分の編集');

    await wrapper.get('form').trigger('submit');
    await flushPromises();

    const alert = wrapper.get('[role="alert"]');
    expect(alert.text()).toContain('他の人が更新しました。読み込み直してから編集してください');
    expect(wrapper.find('form').exists()).toBe(true);

    content = '他の人の本文';
    await alert.get('button').trigger('click');
    await flushPromises();

    expect(wrapper.text()).not.toContain('他の人が更新しました');
    expect((wrapper.get('textarea').element as HTMLTextAreaElement).value).toBe('自分の編集');
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
});
