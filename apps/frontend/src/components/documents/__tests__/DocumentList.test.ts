import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

vi.mock('vike/client/router', () => ({ navigate: vi.fn() }));

import DocumentList from '../DocumentList.vue';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';

enableAutoUnmount(afterEach);

type Reply = { folders: unknown[]; files: { name: string }[] };

let reply: Reply;
let fileRequests: URL[];

beforeEach(() => {
  window.history.replaceState(null, '', '/acme/projects/ENG/docs');
  fileRequests = [];
  reply = {
    folders: [{ id: 'root-1', name: 'ENG', tenant_id: 't', created_by: 'u', created_at: '' }],
    files: [],
  };
  vi.stubGlobal('fetch', async (request: Request) => {
    const url = new URL(request.url);
    if (url.pathname.endsWith('/drive/folders')) return Response.json(reply.folders);
    if (url.pathname.endsWith('/drive/files')) {
      fileRequests.push(url);
      const q = url.searchParams.get('q') ?? '';
      const files = reply.files
        .filter((file) => file.name.includes(q))
        .map((file, index) => ({
          id: `file-${index}`,
          size: 1,
          mime_type: 'text/markdown',
          url: '',
          folder_id: 'root-1',
          created_at: '2026-10-01T00:00:00Z',
          updated_at: '2026-10-02T00:00:00Z',
          etag: '"x"',
          ...file,
        }));
      return Response.json({ files, total: files.length });
    }
    return Response.json({ message: 'not found' }, { status: 404 });
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function mountList(initialQuery = '') {
  return mount(DocumentList, {
    props: {
      tenantId: 'tenant-1' as TenantUuid,
      tenantSlug: 'acme',
      projectId: 'project-1' as ProjectUuid,
      projectKey: 'ENG',
      initialQuery,
    },
    global: {
      plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]],
      stubs: { CreateDocumentDialog: true },
    },
  });
}

/** デバウンス（300ms）を越えて、取得が落ち着くまで待つ */
async function settle() {
  await flushPromises();
  await new Promise((resolve) => setTimeout(resolve, 350));
  await flushPromises();
}

describe('DocumentList', () => {
  it('ルートフォルダ直下の .md を拡張子を落としたタイトルで並べ、詳細へリンクする', async () => {
    reply.files = [{ name: '設計メモ.md' }, { name: 'API.MD' }];
    const wrapper = mountList();
    await settle();

    const links = wrapper.findAll('li a');
    expect(links.map((link) => link.find('span').text())).toEqual(['設計メモ', 'API']);
    expect(links[0]!.attributes('href')).toBe('/acme/projects/ENG/docs/file-0');
    expect(links[0]!.find('time').attributes('datetime')).toBe('2026-10-02T00:00:00Z');

    // ルートの .md だけを名前順で、API の上限件数まで取る
    const params = fileRequests[0]!.searchParams;
    expect(params.get('folder_id')).toBe('root-1');
    expect(params.get('mime_type')).toBe('text/markdown');
    expect(params.get('sort')).toBe('name_asc');
    expect(params.get('limit')).toBe('200');
  });

  it('ルートフォルダが無ければ置き場が無い旨を出し、新規作成を押せない', async () => {
    reply.folders = [];
    const wrapper = mountList();
    await settle();

    expect(wrapper.text()).toContain('ドキュメント置き場がありません');
    expect(wrapper.get('button').attributes('disabled')).toBeDefined();
    expect(fileRequests).toHaveLength(0);
  });

  it('ドキュメントが 0 件なら空状態を出す', async () => {
    const wrapper = mountList();
    await settle();

    expect(wrapper.text()).toContain('ドキュメントはまだありません');
    expect(wrapper.find('ul').exists()).toBe(false);
  });

  it('検索語は一覧の q に渡り、URL の ?q= に写る（消すと外れる）', async () => {
    reply.files = [{ name: '設計メモ.md' }, { name: '議事録.md' }];
    const wrapper = mountList();
    await settle();

    await wrapper.get('input[name="q"]').setValue('議事');
    await settle();
    expect(fileRequests.at(-1)!.searchParams.get('q')).toBe('議事');
    expect(new URL(window.location.href).searchParams.get('q')).toBe('議事');
    expect(wrapper.findAll('li')).toHaveLength(1);

    await wrapper.get('input[name="q"]').setValue('');
    await settle();
    expect(window.location.search).toBe('');
    expect(fileRequests.at(-1)!.searchParams.has('q')).toBe(false);
  });

  it('URL の ?q= を初期値にする', async () => {
    reply.files = [{ name: '設計メモ.md' }, { name: '議事録.md' }];
    const wrapper = mountList('設計');
    await settle();

    expect((wrapper.get('input[name="q"]').element as HTMLInputElement).value).toBe('設計');
    expect(fileRequests.at(-1)!.searchParams.get('q')).toBe('設計');
    expect(wrapper.text()).not.toContain('議事録');
  });
});
