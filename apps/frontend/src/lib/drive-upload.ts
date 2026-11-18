/**
 * 入力欄への貼り付けで使う、プロジェクトの Drive へのアップロード (TASK-137)。
 *
 * 保存先はプロジェクトのルートフォルダで、タスクの添付 (TaskAttachments) と同じ場所である。
 * 読める人の範囲が、本文を読める人 (プロジェクトのメンバー) と揃う。タスクへの添付はしない。
 * サイズと容量の上限はサーバーに任せる (上限の値を返す API が無く、ここで持つと env とずれる)。
 */
import { DRIVE_FILES_PATH, DRIVE_FOLDERS_PATH, fetchClient } from '@/lib/api-vue-query';

export type UploadedFile = { id: string; name: string; mimeType: string };
export type UploadTarget = { tenantId: string; projectId: string };

/**
 * 要求の結果を待つ。通信そのものの失敗（オフライン、中継の切断）は応答でなく例外で届き、文言が
 * ブラウザごとの英語（`Failed to fetch` など）になるので、`undefined` に替えて呼び出し側の文言に任せる。
 * 中断されていたら、成否に依らず中断の理由を投げる。
 */
async function settled<T>(request: Promise<T>, signal?: AbortSignal): Promise<T | undefined> {
  let result: T | undefined;
  try {
    result = await request;
  } catch {
    result = undefined;
  }
  signal?.throwIfAborted();
  return result;
}

/**
 * `file` をプロジェクトのルートフォルダへ上げる。失敗は利用者に出す文言の `Error` で投げる。
 * `signal` が中断されたら、どの段でも中断の理由 (`AbortError`) をそのまま投げる。
 */
export async function uploadProjectFile(
  target: UploadTarget,
  file: File,
  signal?: AbortSignal,
): Promise<UploadedFile> {
  signal?.throwIfAborted();
  if (file.size === 0) throw new Error('空のファイルは貼り付けできません');

  const path = { tenant_id: target.tenantId };
  const folders = await settled(
    fetchClient.GET(DRIVE_FOLDERS_PATH, {
      params: { path, query: { project_id: target.projectId, root_only: true } },
      signal,
    }),
    signal,
  );
  if (!folders || folders.error) throw new Error('保存先を読み込めませんでした');
  const folder = folders.data[0];
  if (!folder) throw new Error('プロジェクトの保存先が見つかりません');

  const form = new FormData();
  form.append('folder_id', folder.id);
  form.append('file', file);
  const uploaded = await settled(
    fetchClient.POST(DRIVE_FILES_PATH, { params: { path }, body: form, signal }),
    signal,
  );
  if (!uploaded) throw new Error('ファイルをアップロードできませんでした');
  if (uploaded.error) {
    throw new Error(
      uploaded.response.status === 413
        ? 'ファイルがサイズ上限または保存容量を超えています'
        : 'ファイルをアップロードできませんでした',
    );
  }
  return { id: uploaded.data.id, name: uploaded.data.name, mimeType: uploaded.data.mime_type };
}
