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
    // 入口（説明の見出しの 📎）は消費側にある。別のタスクでは選択を開ける
    const picker = vi.spyOn(fileInput.element as HTMLInputElement, 'click');
    (wrapper.vm as unknown as { openPicker: () => void }).openPicker();
    expect(picker).toHaveBeenCalledOnce();

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
    // 進行中かどうかは、入口（📎）を置く消費側へ uploading で渡す
    const exposed = wrapper.vm as unknown as { uploading: boolean; openPicker: () => void };
    expect(exposed.uploading).toBe(true);
    const picker = vi.spyOn(fileInput.element as HTMLInputElement, 'click');
    exposed.openPicker();
    expect(picker).not.toHaveBeenCalled();

    // TASK-1 のアップロード中に TASK-2 へ切り替えると、TASK-2 では添付できる
    await wrapper.setProps({ taskId: 'TASK-2' });
    await flushPromises();
    expect(exposed.uploading).toBe(false);
    exposed.openPicker();
    expect(picker).toHaveBeenCalledOnce();

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
      onDelete: () => Response | Promise<Response> = () => new Response(null, { status: 204 }),
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

    it('解除の応答を待つ間に別のタスクで開いた確認は、応答が届いても閉じない', async () => {
      let finishDelete!: (response: Response) => void;
      stubApi(
        [row('1', 'first.txt', ME), row('2', 'second.txt', ME)],
        () => new Promise<Response>((resolve) => (finishDelete = resolve)),
      );
      const wrapper = mountAttachments({ currentUserId: ME });
      await flushPromises();

      await wrapper.get(detachSelector('first.txt')).trigger('click');
      await flushPromises();
      await confirmButton(wrapper, '解除する').trigger('click');
      await flushPromises();

      await wrapper.setProps({ taskId: 'TASK-2' });
      await flushPromises();
      await wrapper.get(detachSelector('second.txt')).trigger('click');
      await flushPromises();
      expect(wrapper.text()).toContain('「second.txt」をこのタスクから外します');
      // 応答を待っているのは別の添付なので、この確認は「解除中…」にせず、キャンセルで閉じられる
      expect(confirmButton(wrapper, '解除する').attributes('disabled')).toBeDefined();
      expect(confirmButton(wrapper, 'キャンセル').attributes('disabled')).toBeUndefined();

      finishDelete(new Response(null, { status: 204 }));
      await flushPromises();
      expect(wrapper.text()).toContain('「second.txt」をこのタスクから外します');
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
    expect(wrapper.text()).not.toContain('添付ファイルを読み込めませんでした');
    expect(wrapper.find('[data-task-attachment-tiles]').exists()).toBe(false);
  });

  describe('タイルとサムネイル', () => {
    const attachment = (id: string, name: string, mime: string, extra: object = {}) => ({
      id,
      drive_file_id: `file-${id}`,
      name,
      mime_type: mime,
      size: 5,
      url: `/v1/drive/files/file-${id}/content`,
      created_at: '2026-10-01T00:00:00Z',
      created_by: 'user-1',
      ...extra,
    });
    const png = () => new Response(new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]));

    /** タスクごとの添付の一覧と、中身の応答を差し替えられる fetch */
    function stubFetch(
      lists: Record<string, object[]>,
      content: (url: string, request: Request) => Response | Promise<Response> = () => png(),
    ) {
      const contentRequests: Request[] = [];
      vi.stubGlobal('fetch', async (input: Request | string, init?: RequestInit) => {
        const request =
          typeof input === 'string' ? new Request(`http://localhost${input}`, init) : input;
        const match = /\/tasks\/([^/]+)\/attachments$/.exec(request.url);
        if (match) return Response.json({ attachments: lists[match[1]] ?? [] });
        contentRequests.push(request);
        return content(request.url, request);
      });
      return contentRequests;
    }

    function mountAttachments(
      props: Record<string, unknown> = {},
      queryClient = new QueryClient(),
    ) {
      return mount(TaskAttachments, {
        props: { tenantId: 'tenant-1', projectId: 'project-1', taskId: 'TASK-1', ...props },
        global: { plugins: [[VueQueryPlugin, { queryClient }]] },
      });
    }

    function spyUrls() {
      let n = 0;
      return {
        create: vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:thumb-${++n}`),
        revoke: vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {}),
      };
    }

    it('0 件と読み込み中は何も描かない（見出しもボタンも無い）', async () => {
      stubFetch({ 'TASK-1': [] });
      const wrapper = mountAttachments();

      // 閉じているダイアログ（プレビューと解除の確認）は DOM にあるが描かれない
      const visible = () => {
        const root = wrapper.get('[data-task-attachments]').element.cloneNode(true) as HTMLElement;
        for (const dialog of root.querySelectorAll('dialog')) dialog.remove();
        return root.textContent!.trim();
      };
      expect(visible()).toBe('');
      await flushPromises();
      expect(visible()).toBe('');
      expect(wrapper.find('[data-task-attachment-tiles]').exists()).toBe(false);
      // ファイルを選ぶ欄は、消費側の 📎 から開くために残す
      expect(wrapper.find('input[type="file"]').exists()).toBe(true);
    });

    it('画像のタイルにサムネイルを出し、それ以外は種類のアイコンと拡張子を出す', async () => {
      const requests = stubFetch({
        'TASK-1': [
          attachment('1', 'photo.png', 'image/png'),
          attachment('2', 'archive.ZIP', 'application/zip'),
          attachment('3', 'memo.txt', 'text/plain'),
        ],
      });
      const urls = spyUrls();
      const wrapper = mountAttachments({
        members: [{ id: 'user-1', username: 'yupix', avatar_url: null }],
      });
      await flushPromises();

      const tiles = wrapper.findAll('[data-task-attachment-tile]');
      expect(tiles).toHaveLength(3);
      // 固定の大きさで折り返して並べる
      expect(wrapper.get('[data-task-attachment-tiles]').classes()).toEqual(
        expect.arrayContaining(['flex', 'flex-wrap', 'gap-3']),
      );
      expect(tiles[0].classes()).toEqual(expect.arrayContaining(['h-[152px]', 'w-[164px]']));

      expect(tiles[0].get('[data-task-attachment-thumbnail]').attributes('src')).toBe(
        'blob:thumb-1',
      );
      expect(tiles[1].find('[data-task-attachment-thumbnail]').exists()).toBe(false);
      expect(tiles[1].get('[data-task-attachment-open]').text()).toBe('zip');
      expect(tiles[2].get('[data-task-attachment-open]').text()).toBe('txt');
      // 中身を取るのは画像だけ。添付 id ごとに 1 本
      expect(requests.map((request) => new URL(request.url).pathname)).toEqual([
        '/api/v1/drive/files/file-1/content',
      ]);
      expect(urls.create).toHaveBeenCalledOnce();

      // ファイル名と投稿者。サイズは出さない
      expect(tiles[0].text()).toContain('photo.png');
      expect(tiles[0].findComponent({ name: 'UserAvatar' }).exists()).toBe(true);
      expect(wrapper.text()).not.toContain('KB');
      // プレビューできない種類はダウンロードのリンクになる
      expect(tiles[1].get('[data-task-attachment-open]').element.tagName).toBe('A');
      expect(tiles[0].get('[data-task-attachment-open]').element.tagName).toBe('BUTTON');
      // ダウンロードは今の文言で残る
      expect(tiles[0].find('a[aria-label="photo.png をダウンロード"]').exists()).toBe(true);
    });

    it('投稿者がメンバーに見つからなければアバターを出さない', async () => {
      stubFetch({
        'TASK-1': [attachment('1', 'a.zip', 'application/zip', { created_by: 'gone' })],
      });
      const wrapper = mountAttachments({
        members: [{ id: 'user-1', username: 'yupix', avatar_url: null }],
      });
      await flushPromises();
      expect(wrapper.findComponent({ name: 'UserAvatar' }).exists()).toBe(false);
    });

    it('サムネイルの取得に失敗したら種類のアイコンに倒し、取り直さない', async () => {
      const queryClient = new QueryClient();
      const requests = stubFetch(
        { 'TASK-1': [attachment('1', 'photo.png', 'image/png')] },
        () => new Response(null, { status: 500 }),
      );
      const urls = spyUrls();
      const wrapper = mountAttachments({}, queryClient);
      await flushPromises();

      const tile = wrapper.get('[data-task-attachment-tile]');
      expect(tile.find('[data-task-attachment-thumbnail]').exists()).toBe(false);
      expect(tile.get('[data-task-attachment-open]').text()).toBe('png');
      expect(urls.create).not.toHaveBeenCalled();

      // 一覧を取り直しても、同じ添付は取り直さない
      await queryClient.invalidateQueries();
      await flushPromises();
      expect(requests).toHaveLength(1);
    });

    it('一覧を取り直しても、取得済みの添付のサムネイルは取り直さない', async () => {
      const queryClient = new QueryClient();
      const lists = { 'TASK-1': [attachment('1', 'photo.png', 'image/png')] };
      const requests = stubFetch(lists);
      const urls = spyUrls();
      const wrapper = mountAttachments({}, queryClient);
      await flushPromises();

      // 新しい画像が増えた一覧。増えた分だけ取る
      lists['TASK-1'] = [...lists['TASK-1'], attachment('2', 'next.png', 'image/png')];
      await queryClient.invalidateQueries();
      await flushPromises();

      expect(requests).toHaveLength(2);
      expect(urls.create).toHaveBeenCalledTimes(2);
      expect(urls.revoke).not.toHaveBeenCalled();
      expect(
        wrapper.findAll('[data-task-attachment-thumbnail]').map((img) => img.attributes('src')),
      ).toEqual(['blob:thumb-1', 'blob:thumb-2']);
    });

    it('一覧から消えた添付の object URL を revoke する', async () => {
      const queryClient = new QueryClient();
      const lists = {
        'TASK-1': [attachment('1', 'a.png', 'image/png'), attachment('2', 'b.png', 'image/png')],
      };
      stubFetch(lists);
      const urls = spyUrls();
      const wrapper = mountAttachments({}, queryClient);
      await flushPromises();
      expect(urls.create).toHaveBeenCalledTimes(2);

      lists['TASK-1'] = [lists['TASK-1'][1]];
      await queryClient.invalidateQueries();
      await flushPromises();

      expect(urls.revoke.mock.calls).toEqual([['blob:thumb-1']]);
      expect(
        wrapper.findAll('[data-task-attachment-thumbnail]').map((img) => img.attributes('src')),
      ).toEqual(['blob:thumb-2']);
    });

    it('タスクを切り替えたら全部 revoke し、古いタスク宛ての応答は捨てる', async () => {
      // TASK-2 の中身だけ、応答を手で返す
      let release!: (response: Response) => void;
      const pending = new Promise<Response>((resolve) => (release = resolve));
      const requests = stubFetch(
        {
          'TASK-1': [attachment('1', 'a.png', 'image/png')],
          'TASK-2': [attachment('2', 'slow.png', 'image/png')],
          'TASK-3': [],
        },
        (url) => (url.includes('file-2') ? pending : png()),
      );
      const urls = spyUrls();
      const wrapper = mountAttachments();
      await flushPromises();
      expect(urls.create).toHaveBeenCalledOnce();

      await wrapper.setProps({ taskId: 'TASK-2' });
      await flushPromises();
      expect(urls.revoke.mock.calls).toEqual([['blob:thumb-1']]);
      expect(wrapper.find('[data-task-attachment-thumbnail]').exists()).toBe(false);

      // TASK-2 の取得が終わる前に TASK-3 へ移る。取得は止まり、届いた応答からは URL を作らない
      const slow = requests.find((request) => request.url.includes('file-2'))!;
      await wrapper.setProps({ taskId: 'TASK-3' });
      await flushPromises();
      expect(slow.signal.aborted).toBe(true);
      release(png());
      await flushPromises();

      expect(urls.create).toHaveBeenCalledOnce();
      expect(wrapper.find('[data-task-attachment-thumbnail]').exists()).toBe(false);
    });

    it('unmount で object URL を revoke し、取得を止める', async () => {
      let release!: (response: Response) => void;
      const pending = new Promise<Response>((resolve) => (release = resolve));
      const requests = stubFetch(
        {
          'TASK-1': [
            attachment('1', 'a.png', 'image/png'),
            attachment('2', 'slow.png', 'image/png'),
          ],
        },
        (url) => (url.includes('file-2') ? pending : png()),
      );
      const urls = spyUrls();
      const wrapper = mountAttachments();
      await flushPromises();
      expect(urls.create).toHaveBeenCalledOnce();

      wrapper.unmount();
      expect(urls.revoke.mock.calls).toEqual([['blob:thumb-1']]);
      expect(requests.find((request) => request.url.includes('file-2'))!.signal.aborted).toBe(true);
      release(png());
      await flushPromises();
      expect(urls.create).toHaveBeenCalledOnce();
    });
  });
});
