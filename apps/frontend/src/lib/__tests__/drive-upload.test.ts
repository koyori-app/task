// 貼り付けで使うプロジェクトの Drive へのアップロード（手順、文言、中断）
import { afterEach, describe, expect, it, vi } from 'vitest';

import { uploadProjectFile } from '../drive-upload';

const target = { tenantId: 'tenant-1', projectId: 'project-1' };
const saved = {
  id: '0f8fad5b-d9cb-469f-a165-70867728950e',
  name: 'shot.png',
  mime_type: 'image/png',
  size: 3,
  url: '/v1/drive/files/0f8fad5b-d9cb-469f-a165-70867728950e/content',
  folder_id: 'root-1',
  created_at: '2026-11-18T00:00:00Z',
  updated_at: '2026-11-18T00:00:00Z',
  etag: '"sha256:x"',
};

type Seen = { method: string; url: URL; form?: FormData };

/** `folders` と `upload` の応答を差し替えられる fetch の代役。見た要求を返す */
function stubFetch(
  options: {
    folders?: () => Response;
    upload?: () => Response | Promise<Response>;
  } = {},
) {
  const seen: Seen[] = [];
  vi.stubGlobal('fetch', async (request: Request) => {
    const url = new URL(request.url);
    if (request.method === 'GET' && url.pathname.endsWith('/drive/folders')) {
      seen.push({ method: 'GET', url });
      return options.folders?.() ?? Response.json([{ id: 'root-1' }]);
    }
    if (request.method === 'POST' && url.pathname.endsWith('/drive/files')) {
      seen.push({ method: 'POST', url, form: await request.formData() });
      return (await options.upload?.()) ?? Response.json(saved, { status: 201 });
    }
    return Response.json({ message: 'not found' }, { status: 404 });
  });
  return seen;
}

const png = () => new File(['abc'], 'shot.png', { type: 'image/png' });

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('uploadProjectFile', () => {
  it('プロジェクトのルートフォルダを引き、そこへ file を上げて id・名前・MIME を返す', async () => {
    const seen = stubFetch();
    await expect(uploadProjectFile(target, png())).resolves.toEqual({
      id: saved.id,
      name: 'shot.png',
      mimeType: 'image/png',
    });

    expect(seen.map((request) => request.method)).toEqual(['GET', 'POST']);
    expect(seen[0].url.pathname).toBe('/api/v1/tenants/tenant-1/drive/folders');
    expect(Object.fromEntries(seen[0].url.searchParams)).toEqual({
      project_id: 'project-1',
      root_only: 'true',
    });
    expect(seen[1].url.pathname).toBe('/api/v1/tenants/tenant-1/drive/files');
    const form = seen[1].form!;
    expect([...form.keys()].sort()).toEqual(['file', 'folder_id']);
    expect(form.get('folder_id')).toBe('root-1');
    const sent = form.get('file') as File;
    expect(sent.name).toBe('shot.png');
    expect(await sent.text()).toBe('abc');
  });

  it('0 バイトは API を呼ばずに断る', async () => {
    const seen = stubFetch();
    await expect(uploadProjectFile(target, new File([], 'empty.txt'))).rejects.toThrow(
      '空のファイルは貼り付けできません',
    );
    expect(seen).toHaveLength(0);
  });

  it('ルートフォルダが 0 件なら上げない', async () => {
    const seen = stubFetch({ folders: () => Response.json([]) });
    await expect(uploadProjectFile(target, png())).rejects.toThrow(
      'プロジェクトの保存先が見つかりません',
    );
    expect(seen.map((request) => request.method)).toEqual(['GET']);
  });

  it('フォルダの取得に失敗したら上げない', async () => {
    const seen = stubFetch({
      folders: () => Response.json({ message: 'x' }, { status: 500 }),
    });
    await expect(uploadProjectFile(target, png())).rejects.toThrow('保存先を読み込めませんでした');
    expect(seen.map((request) => request.method)).toEqual(['GET']);
  });

  it('413 はサイズ・容量の文言にする', async () => {
    stubFetch({ upload: () => Response.json({ message: 'x' }, { status: 413 }) });
    await expect(uploadProjectFile(target, png())).rejects.toThrow(
      'ファイルがサイズ上限または保存容量を超えています',
    );
  });

  it.each([400, 403, 500])('%i は汎用の文言にする', async (status) => {
    stubFetch({ upload: () => Response.json({ message: 'x' }, { status }) });
    await expect(uploadProjectFile(target, png())).rejects.toThrow(
      'ファイルをアップロードできませんでした',
    );
  });

  it('フォルダの取得で通信が切れたら、ブラウザの文言ではなく保存先の文言にする', async () => {
    stubFetch({
      folders: () => {
        throw new TypeError('Failed to fetch');
      },
    });
    await expect(uploadProjectFile(target, png())).rejects.toThrow('保存先を読み込めませんでした');
  });

  it('アップロードで通信が切れたら、ブラウザの文言ではなく汎用の文言にする', async () => {
    stubFetch({
      upload: () => {
        throw new TypeError('Failed to fetch');
      },
    });
    await expect(uploadProjectFile(target, png())).rejects.toThrow(
      'ファイルをアップロードできませんでした',
    );
  });

  it('通信の途中で中断されたら、文言ではなく AbortError を投げる', async () => {
    const controller = new AbortController();
    stubFetch({
      upload: () => {
        controller.abort();
        throw new DOMException('The operation was aborted.', 'AbortError');
      },
    });
    await expect(uploadProjectFile(target, png(), controller.signal)).rejects.toMatchObject({
      name: 'AbortError',
    });
  });

  it('呼ぶ前に中断済みなら API を呼ばず AbortError を投げる', async () => {
    const seen = stubFetch();
    const controller = new AbortController();
    controller.abort();
    await expect(uploadProjectFile(target, png(), controller.signal)).rejects.toMatchObject({
      name: 'AbortError',
    });
    expect(seen).toHaveLength(0);
  });

  it('フォルダの取得の後で中断されたら、上げずに AbortError を投げる', async () => {
    const controller = new AbortController();
    const seen = stubFetch({
      folders: () => {
        controller.abort();
        return Response.json([{ id: 'root-1' }]);
      },
    });
    await expect(uploadProjectFile(target, png(), controller.signal)).rejects.toMatchObject({
      name: 'AbortError',
    });
    expect(seen.map((request) => request.method)).toEqual(['GET']);
  });

  it('アップロードの失敗の応答の後で中断されても、文言ではなく AbortError を投げる', async () => {
    const controller = new AbortController();
    stubFetch({
      upload: () => {
        controller.abort();
        return Response.json({ message: 'x' }, { status: 413 });
      },
    });
    await expect(uploadProjectFile(target, png(), controller.signal)).rejects.toMatchObject({
      name: 'AbortError',
    });
  });
});
