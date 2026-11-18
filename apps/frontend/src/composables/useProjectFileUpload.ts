import { computed, type ComputedRef } from 'vue';

import { uploadProjectFile, type UploadedFile } from '@/lib/drive-upload';

/**
 * 入力欄 (MarkdownEditor / CommentEditor) の `uploadFile` に渡す関数を作る。
 *
 * テナントかプロジェクトが分からない間は `undefined` を返し、貼り付けは今までどおり文字だけを扱う。
 * 上げる先は、関数を呼んだ時点 (貼り付けた時点) のプロジェクトである。
 */
export function useProjectFileUpload(
  tenantId: () => string | null | undefined,
  projectId: () => string | null | undefined,
): ComputedRef<((file: File, signal: AbortSignal) => Promise<UploadedFile>) | undefined> {
  return computed(() => {
    const tenant = tenantId();
    const project = projectId();
    if (!tenant || !project) return undefined;
    return (file: File, signal: AbortSignal) =>
      uploadProjectFile({ tenantId: tenant, projectId: project }, file, signal);
  });
}
