import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query';

import TaskAttachments from '../TaskAttachments.vue';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('TaskAttachments', () => {
  it('previews text safely and keeps downloads available', async () => {
    const contentUrl = '/api/v1/drive/files/file-1/content';
    vi.stubGlobal('fetch', async (request: Request | string) => {
      const url = typeof request === 'string' ? request : request.url;
      if (url.endsWith('/attachments')) {
        return Response.json({
          attachments: [
            {
              id: 'attachment-1',
              drive_file_id: 'file-1',
              name: 'memo.txt',
              mime_type: 'text/plain',
              size: 24,
              url: '/v1/drive/files/file-1/content',
              created_at: '2026-10-01T00:00:00Z',
            },
            {
              id: 'attachment-2',
              drive_file_id: 'file-2',
              name: 'archive.zip',
              mime_type: 'application/zip',
              size: 100,
              url: '/v1/drive/files/file-2/content',
              created_at: '2026-10-01T00:00:00Z',
            },
          ],
        });
      }
      if (url === contentUrl) return new Response('<script>alert(1)</script>');
      return Response.json({ message: 'not found' }, { status: 404 });
    });

    const wrapper = mount(TaskAttachments, {
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();
    const dialog = wrapper.get('dialog').element as HTMLDialogElement;
    dialog.showModal = vi.fn();
    const close = vi.fn();
    dialog.close = close;

    await wrapper.get('button[title="memo.txt をプレビュー"]').trigger('click');
    await flushPromises();

    expect(wrapper.get('dialog pre').text()).toBe('<script>alert(1)</script>');
    expect(wrapper.find('dialog script').exists()).toBe(false);
    expect(wrapper.get('dialog a').attributes('href')).toBe(contentUrl);
    expect(wrapper.get('a[title="archive.zip"]').attributes('href')).toContain('/file-2/content');
    await wrapper.get('button[aria-label="プレビューを閉じる"]').trigger('click');
    expect(close).toHaveBeenCalled();
  });

  it('shows an access error in the preview and releases an image URL on close', async () => {
    let deny = true;
    vi.stubGlobal('fetch', async (request: Request | string) => {
      const url = typeof request === 'string' ? request : request.url;
      if (url.endsWith('/attachments')) {
        return Response.json({
          attachments: [
            {
              id: 'attachment-1',
              drive_file_id: 'file-1',
              name: 'photo.png',
              mime_type: 'image/png',
              size: 5,
              url: '/v1/drive/files/file-1/content',
              created_at: '2026-10-01T00:00:00Z',
            },
          ],
        });
      }
      if (deny) return new Response(null, { status: 403 });
      return new Response(new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]));
    });
    const createObjectURL = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:preview');
    const revokeObjectURL = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    const wrapper = mount(TaskAttachments, {
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();
    const dialog = wrapper.get('dialog').element as HTMLDialogElement;
    dialog.showModal = vi.fn();
    dialog.close = vi.fn();

    await wrapper.get('button[title="photo.png をプレビュー"]').trigger('click');
    await flushPromises();
    expect(wrapper.get('dialog [role="alert"]').text()).toContain('権限がありません');

    deny = false;
    await wrapper.get('button[title="photo.png をプレビュー"]').trigger('click');
    await flushPromises();
    expect(wrapper.get('dialog img').attributes('src')).toBe('blob:preview');
    expect(createObjectURL).toHaveBeenCalledOnce();
    await wrapper.get('button[aria-label="プレビューを閉じる"]').trigger('click');
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:preview');
  });

  it('loads PDFs in a sandbox and rejects non-PDF content', async () => {
    let valid = true;
    vi.stubGlobal('fetch', async (request: Request | string) => {
      const url = typeof request === 'string' ? request : request.url;
      if (url.endsWith('/attachments')) {
        return Response.json({
          attachments: [
            {
              id: 'attachment-1',
              drive_file_id: 'file-1',
              name: 'spec.pdf',
              mime_type: 'application/pdf',
              size: 20,
              url: '/v1/drive/files/file-1/content',
              created_at: '2026-10-01T00:00:00Z',
            },
          ],
        });
      }
      return new Response(valid ? '%PDF-1.4\n' : '<script>alert(1)</script>');
    });
    const createObjectURL = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:preview');
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    const wrapper = mount(TaskAttachments, {
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();
    const dialog = wrapper.get('dialog').element as HTMLDialogElement;
    dialog.showModal = vi.fn();
    dialog.close = vi.fn();

    await wrapper.get('button[title="spec.pdf をプレビュー"]').trigger('click');
    await flushPromises();
    expect(wrapper.get('dialog iframe').attributes('sandbox')).toBe('');
    expect(createObjectURL).toHaveBeenCalledOnce();

    await wrapper.get('button[aria-label="プレビューを閉じる"]').trigger('click');
    valid = false;
    await wrapper.get('button[title="spec.pdf をプレビュー"]').trigger('click');
    await flushPromises();
    expect(wrapper.get('dialog [role="alert"]').text()).toContain(
      'PDF ファイルとして表示できません',
    );
    expect(wrapper.find('dialog iframe').exists()).toBe(false);
  });

  it('does not upload without the matching project folder', async () => {
    const upload = vi.fn();
    vi.stubGlobal('fetch', async (request: Request) => {
      if (request.method === 'GET' && request.url.endsWith('/attachments')) {
        return Response.json({ attachments: [] });
      }
      if (request.method === 'GET' && request.url.endsWith('/drive/folders')) {
        return Response.json([
          { id: 'other-folder', project_id: 'other-project', parent_id: null },
        ]);
      }
      upload();
      return Response.json({ message: 'unexpected' }, { status: 500 });
    });

    const wrapper = mount(TaskAttachments, {
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();
    const fileInput = wrapper.get('input[type="file"]');
    Object.defineProperty(fileInput.element, 'files', {
      configurable: true,
      value: [new File(['hello'], 'memo.txt')],
    });
    await fileInput.trigger('change');
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain('保存先が見つかりません');
    expect(upload).not.toHaveBeenCalled();
  });

  it('uploads into the project folder and retries linking without uploading twice', async () => {
    const requests: Request[] = [];
    let linkAttempts = 0;
    let linked = false;
    vi.stubGlobal('fetch', async (request: Request) => {
      requests.push(request.clone());
      const url = new URL(request.url);
      if (request.method === 'GET' && url.pathname.endsWith('/attachments')) {
        return Response.json({
          attachments: linked
            ? [
                {
                  id: 'attachment-1',
                  drive_file_id: 'file-1',
                  name: 'memo.txt',
                  mime_type: 'text/plain',
                  size: 5,
                  url: '/v1/drive/files/file-1/content',
                  created_at: '2026-10-01T00:00:00Z',
                },
              ]
            : [],
        });
      }
      if (request.method === 'GET' && url.pathname.endsWith('/drive/folders')) {
        return Response.json([
          { id: 'other-folder', project_id: 'other-project', parent_id: null },
          { id: 'project-folder', project_id: 'project-1', parent_id: null },
        ]);
      }
      if (request.method === 'POST' && url.pathname.endsWith('/drive/files')) {
        return Response.json({ id: 'file-1' }, { status: 201 });
      }
      if (request.method === 'POST' && url.pathname.endsWith('/attachments')) {
        linkAttempts++;
        if (linkAttempts === 1) return Response.json({ message: 'temporary' }, { status: 503 });
        linked = true;
        return Response.json({ id: 'attachment-1' }, { status: 201 });
      }
      return Response.json({ message: 'not found' }, { status: 404 });
    });

    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = mount(TaskAttachments, {
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    });
    await flushPromises();

    const fileInput = wrapper.get('input[type="file"]');
    Object.defineProperty(fileInput.element, 'files', {
      configurable: true,
      value: [new File(['hello'], 'memo.txt', { type: 'text/plain' })],
    });
    await fileInput.trigger('change');
    await flushPromises();

    const upload = requests.find(
      (request) => request.method === 'POST' && request.url.endsWith('/drive/files'),
    );
    expect(upload).toBeDefined();
    const form = await upload!.formData();
    expect([...form.keys()]).toEqual(['folder_id', 'file']);
    expect(form.get('folder_id')).toBe('project-folder');
    expect(wrapper.text()).toContain('紐付けを再試行');

    await wrapper
      .findAll('button')
      .find((button) => button.text() === '紐付けを再試行')!
      .trigger('click');
    await flushPromises();

    expect(
      requests.filter(
        (request) => request.method === 'POST' && request.url.endsWith('/drive/files'),
      ),
    ).toHaveLength(1);
    expect(linkAttempts).toBe(2);
    expect(wrapper.get('a').attributes('href')).toBe('/api/v1/drive/files/file-1/content');
  });

  it('keeps an unlinked upload for its own task across task switches', async () => {
    const linkPaths: string[] = [];
    vi.stubGlobal('fetch', async (request: Request) => {
      const url = new URL(request.url);
      if (request.method === 'GET' && url.pathname.endsWith('/attachments')) {
        return Response.json({ attachments: [] });
      }
      if (request.method === 'GET' && url.pathname.endsWith('/drive/folders')) {
        return Response.json([{ id: 'project-folder', project_id: 'project-1', parent_id: null }]);
      }
      if (request.method === 'POST' && url.pathname.endsWith('/drive/files')) {
        return Response.json({ id: 'file-1' }, { status: 201 });
      }
      if (request.method === 'POST' && url.pathname.endsWith('/attachments')) {
        linkPaths.push(url.pathname);
        return Response.json({ message: 'forbidden' }, { status: 403 });
      }
      return Response.json({ message: 'not found' }, { status: 404 });
    });

    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = mount(TaskAttachments, {
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    });
    await flushPromises();
    const fileInput = wrapper.get('input[type="file"]');
    Object.defineProperty(fileInput.element, 'files', {
      configurable: true,
      value: [new File(['hello'], 'memo.txt', { type: 'text/plain' })],
    });
    await fileInput.trigger('change');
    await flushPromises();
    expect(wrapper.text()).toContain('memo.txt はアップロード済みです');

    // 別のタスクでは出さず、添付も塞がない
    await wrapper.setProps({ taskId: 'TASK-2' });
    await flushPromises();
    expect(wrapper.text()).not.toContain('はアップロード済みです');
    expect(
      wrapper
        .findAll('button')
        .find((button) => button.text() === 'ファイルを添付')!
        .attributes('disabled'),
    ).toBeUndefined();

    // 元のタスクへ戻れば、元のタスクへの紐付けを再試行できる
    await wrapper.setProps({ taskId: 'TASK-1' });
    await flushPromises();
    expect(wrapper.text()).toContain('memo.txt はアップロード済みです');
    await wrapper
      .findAll('button')
      .find((button) => button.text() === '紐付けを再試行')!
      .trigger('click');
    await flushPromises();
    // 戻ってからの再試行も、props ではなく捕まえた ID で元のタスクへ紐付ける
    expect(linkPaths).toEqual([
      '/api/v1/tenants/tenant-1/projects/project-1/tasks/TASK-1/attachments',
      '/api/v1/tenants/tenant-1/projects/project-1/tasks/TASK-1/attachments',
    ]);
  });

  it('keeps upload progress and failures on the task that started them', async () => {
    let failUpload!: () => void;
    vi.stubGlobal('fetch', async (request: Request) => {
      const url = new URL(request.url);
      if (request.method === 'GET' && url.pathname.endsWith('/attachments')) {
        return Response.json({ attachments: [] });
      }
      if (request.method === 'GET' && url.pathname.endsWith('/drive/folders')) {
        return Response.json([{ id: 'project-folder', project_id: 'project-1', parent_id: null }]);
      }
      if (request.method === 'POST' && url.pathname.endsWith('/drive/files')) {
        await new Promise<void>((resolve) => {
          failUpload = resolve;
        });
        return Response.json({ message: 'down' }, { status: 500 });
      }
      return Response.json({ message: 'not found' }, { status: 404 });
    });

    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = mount(TaskAttachments, {
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    });
    await flushPromises();
    const fileInput = wrapper.get('input[type="file"]');
    Object.defineProperty(fileInput.element, 'files', {
      configurable: true,
      value: [new File(['hello'], 'memo.txt', { type: 'text/plain' })],
    });
    await fileInput.trigger('change');
    await flushPromises();
    expect(wrapper.text()).toContain('アップロード中…');

    // TASK-1 のアップロード中に TASK-2 へ切り替えると、TASK-2 では添付できる
    await wrapper.setProps({ taskId: 'TASK-2' });
    await flushPromises();
    const attachButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'ファイルを添付');
    expect(attachButton?.attributes('disabled')).toBeUndefined();

    // TASK-1 の失敗は TASK-2 の欄に出さず、TASK-1 へ戻ったときに出す
    failUpload();
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);

    // TASK-2 でも失敗させても、TASK-1 の失敗は押し出されない
    const otherInput = wrapper.get('input[type="file"]');
    Object.defineProperty(otherInput.element, 'files', {
      configurable: true,
      value: [new File([], 'empty.txt')],
    });
    await otherInput.trigger('change');
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain('空のファイルは添付できません');

    await wrapper.setProps({ taskId: 'TASK-1' });
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain('アップロードできませんでした');
  });

  describe('添付の解除', () => {
    const ME = 'user-me';
    function row(id: string, name: string, createdBy?: string) {
      return {
        id,
        drive_file_id: `file-${id}`,
        name,
        mime_type: 'text/plain',
        size: 5,
        url: `/v1/drive/files/file-${id}/content`,
        created_at: '2026-10-01T00:00:00Z',
        ...(createdBy ? { created_by: createdBy } : {}),
      };
    }
    function stubApi(
      attachments: ReturnType<typeof row>[],
      onDelete: () => Response = () => new Response(null, { status: 204 }),
    ) {
      const deletes: string[] = [];
      vi.stubGlobal('fetch', async (request: Request) => {
        if (request.method === 'GET' && request.url.endsWith('/attachments')) {
          return Response.json({ attachments });
        }
        if (request.method === 'DELETE') {
          deletes.push(new URL(request.url).pathname);
          return onDelete();
        }
        return Response.json({ message: 'not found' }, { status: 404 });
      });
      return deletes;
    }
    function mountAttachments(extra: { currentUserId?: string | null; isTenantOwner?: boolean }) {
      return mount(TaskAttachments, {
        props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1', ...extra },
        global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
      });
    }
    const detachSelector = (name: string) => `button[aria-label="${name} の添付を解除"]`;
    function confirmButton(wrapper: ReturnType<typeof mountAttachments>, label: string) {
      const button = wrapper.findAll('dialog button').find((b) => b.text() === label);
      if (!button) throw new Error(`button "${label}" not found`);
      return button;
    }

    it('他人が付けた添付には解除ボタンを出さず、自分が付けた添付には出す', async () => {
      stubApi([row('1', 'mine.txt', ME), row('2', 'theirs.txt', 'user-other')]);
      const wrapper = mountAttachments({ currentUserId: ME });
      await flushPromises();

      expect(wrapper.find(detachSelector('mine.txt')).exists()).toBe(true);
      expect(wrapper.find(detachSelector('theirs.txt')).exists()).toBe(false);
    });

    it('テナントオーナーには、他人が付けた添付にも出す', async () => {
      stubApi([row('2', 'theirs.txt', 'user-other')]);
      const wrapper = mountAttachments({ currentUserId: ME, isTenantOwner: true });
      await flushPromises();

      expect(wrapper.find(detachSelector('theirs.txt')).exists()).toBe(true);
    });

    it('作成者の分からない行（古いサーバー）や利用者が分からない間は、オーナー以外に出さない', async () => {
      stubApi([row('1', 'legacy.txt'), row('2', 'mine.txt', ME)]);
      const wrapper = mountAttachments({ currentUserId: ME });
      await flushPromises();
      expect(wrapper.find(detachSelector('legacy.txt')).exists()).toBe(false);

      await wrapper.setProps({ currentUserId: null });
      expect(wrapper.find(detachSelector('mine.txt')).exists()).toBe(false);

      await wrapper.setProps({ isTenantOwner: true });
      expect(wrapper.find(detachSelector('legacy.txt')).exists()).toBe(true);
    });

    it('ボタンを押しただけでは DELETE を送らず、確認の「解除する」で送る', async () => {
      const deletes = stubApi([row('1', 'mine.txt', ME)]);
      const wrapper = mountAttachments({ currentUserId: ME });
      await flushPromises();
      const dialogs = wrapper.findAll('dialog');
      for (const dialog of dialogs) {
        (dialog.element as HTMLDialogElement).showModal = vi.fn();
        (dialog.element as HTMLDialogElement).close = vi.fn();
      }

      await wrapper.get(detachSelector('mine.txt')).trigger('click');
      await flushPromises();
      expect(deletes).toEqual([]);
      expect(wrapper.text()).toContain('添付を解除しますか？');
      expect(wrapper.text()).toContain('ファイルは Drive に残ります');

      await confirmButton(wrapper, '解除する').trigger('click');
      await flushPromises();
      expect(deletes).toEqual([
        '/api/v1/tenants/tenant-1/projects/project-1/tasks/TASK-1/attachments/1',
      ]);
    });

    it('確認の「キャンセル」では DELETE を送らない', async () => {
      const deletes = stubApi([row('1', 'mine.txt', ME)]);
      const wrapper = mountAttachments({ currentUserId: ME });
      await flushPromises();

      await wrapper.get(detachSelector('mine.txt')).trigger('click');
      await flushPromises();
      await confirmButton(wrapper, 'キャンセル').trigger('click');
      await flushPromises();

      expect(deletes).toEqual([]);
      expect(wrapper.text()).toContain('mine.txt');
    });

    it('API が 403 を返したら、解除できる人を明記する', async () => {
      stubApi([row('1', 'memo.txt', ME)], () =>
        Response.json({ message: 'forbidden' }, { status: 403 }),
      );
      const wrapper = mountAttachments({ currentUserId: ME });
      await flushPromises();

      await wrapper.get(detachSelector('memo.txt')).trigger('click');
      await flushPromises();
      await confirmButton(wrapper, '解除する').trigger('click');
      await flushPromises();

      expect(wrapper.get('[role="alert"]').text()).toBe(
        '添付を解除できるのは、添付した人とテナントオーナーだけです。',
      );
      expect(wrapper.text()).toContain('memo.txt');
    });
  });

  it('keeps the list retryable when loading attachments fails', async () => {
    let listRequests = 0;
    vi.stubGlobal('fetch', async (request: Request) => {
      if (request.method === 'GET' && request.url.endsWith('/attachments')) {
        listRequests++;
        if (listRequests === 1) return Response.json({ message: 'down' }, { status: 500 });
        return Response.json({ attachments: [] });
      }
      return Response.json({ message: 'not found' }, { status: 404 });
    });

    const wrapper = mount(TaskAttachments, {
      props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1' },
      global: { plugins: [[VueQueryPlugin, { queryClient: new QueryClient() }]] },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('添付ファイルを読み込めませんでした');

    await wrapper
      .findAll('button')
      .find((button) => button.text() === '再試行')!
      .trigger('click');
    await flushPromises();

    expect(listRequests).toBe(2);
    expect(wrapper.text()).toContain('添付ファイルはありません');
  });
});
