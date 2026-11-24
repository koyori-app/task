import type { TreeNode } from '@koyori-app/ui-vue';

import type { FileGroup } from '@/lib/review-findings';

const NO_FILE_ID = 'nofile';
const FILE_PREFIX = 'file:';
const DIR_PREFIX = 'dir:';

/**
 * ディレクトリの節のラベル。4 段までは全部、5 段以上は `…/` + 末尾 3 段。
 * リポジトリ直下（空文字）は「（ルート）」。
 */
export function dirLabel(dir: string): string {
  if (dir === '') return '（ルート）';
  const segments = dir.split('/');
  return segments.length <= 4 ? dir : `…/${segments.slice(-3).join('/')}`;
}

export function treeNodeIdOfFile(file: string | null): string {
  return file === null ? NO_FILE_ID : `${FILE_PREFIX}${file}`;
}

/** 節の id からファイルを引く。ディレクトリの節は undefined、`nofile` は null。 */
export function fileOfTreeNode(id: string): string | null | undefined {
  if (id === NO_FILE_ID) return null;
  return id.startsWith(FILE_PREFIX) ? id.slice(FILE_PREFIX.length) : undefined;
}

function badgeOf(group: FileGroup): TreeNode['badge'] {
  return {
    label: `${group.progress.resolved} / ${group.progress.total}`,
    tone: group.progress.tone,
  };
}

/**
 * ファイルの木（1 段）。ファイルを親ディレクトリでまとめ、ディレクトリの節（`dir:<パス>`）の
 * 下にファイルの節（`file:<パス>`）を置く。節の並びは入力の順（右の一覧に最初に出る順）。
 * ファイルの指定が無いまとまりは最後の `nofile` で、`fileCount` に数えない。
 */
export function buildFileTree(groups: readonly FileGroup[]): {
  nodes: TreeNode[];
  fileCount: number;
} {
  const dirs = new Map<string, TreeNode & { children: TreeNode[] }>();
  let noFile: TreeNode | null = null;
  let fileCount = 0;
  for (const group of groups) {
    if (group.file === null) {
      noFile = { id: NO_FILE_ID, label: 'ファイルの指定なし', badge: badgeOf(group) };
      continue;
    }
    fileCount += 1;
    const slash = group.file.lastIndexOf('/');
    const dir = slash < 0 ? '' : group.file.slice(0, slash);
    let node = dirs.get(dir);
    if (!node) {
      const label = dirLabel(dir);
      node = {
        id: `${DIR_PREFIX}${dir}`,
        label,
        ...(label === dir ? {} : { title: dir === '' ? 'リポジトリの直下' : dir }),
        children: [],
      };
      dirs.set(dir, node);
    }
    node.children.push({
      id: treeNodeIdOfFile(group.file),
      label: group.file.slice(slash + 1),
      title: group.file,
      badge: badgeOf(group),
    });
  }
  return { nodes: [...dirs.values(), ...(noFile ? [noFile] : [])], fileCount };
}
