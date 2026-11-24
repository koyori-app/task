import { describe, expect, it } from 'vitest';

import type { FileGroup } from '@/lib/review-findings';
import { buildFileTree, dirLabel, fileOfTreeNode, treeNodeIdOfFile } from '@/lib/review-file-tree';

function group(file: string | null, resolved = 0, total = 1): FileGroup {
  return {
    file,
    findings: [],
    progress: { resolved, total, tone: resolved === total ? 'success' : 'danger' },
  };
}

describe('review file tree', () => {
  it('ディレクトリは 4 段まで全部、5 段からは末尾 3 段、直下は「（ルート）」', () => {
    expect(dirLabel('apps/frontend/src/lib')).toBe('apps/frontend/src/lib');
    expect(dirLabel('apps/frontend/src/pages/@tenant/projects/@projectKey/tasks')).toBe(
      '…/projects/@projectKey/tasks',
    );
    expect(dirLabel('a/b/c/d/e')).toBe('…/c/d/e');
    expect(dirLabel('')).toBe('（ルート）');
  });

  it('同じディレクトリのファイルを 1 つの節にまとめ、節は入力の順に並べる', () => {
    const { nodes, fileCount } = buildFileTree([
      group('src/lib/b.ts'),
      group('src/pages/+Page.vue'),
      group('src/lib/a.ts'),
      group('README.md'),
    ]);
    expect(nodes.map((n) => n.id)).toEqual(['dir:src/lib', 'dir:src/pages', 'dir:']);
    expect(nodes[0].children?.map((n) => n.id)).toEqual(['file:src/lib/b.ts', 'file:src/lib/a.ts']);
    expect(nodes[0].children?.map((n) => n.label)).toEqual(['b.ts', 'a.ts']);
    expect(nodes[2].label).toBe('（ルート）');
    expect(fileCount).toBe(4);
  });

  it('省略したディレクトリの節は title に全体を持つ', () => {
    const { nodes } = buildFileTree([group('a/b/c/d/e/f.ts')]);
    expect(nodes[0]).toMatchObject({ label: '…/c/d/e', title: 'a/b/c/d/e' });
  });

  it('ファイルの節の件数は「解決 / 全体」と色', () => {
    const { nodes } = buildFileTree([
      { file: 'src/a.ts', findings: [], progress: { resolved: 2, total: 3, tone: 'warning' } },
    ]);
    expect(nodes[0].children?.[0].badge).toEqual({ label: '2 / 3', tone: 'warning' });
  });

  it('ファイルの指定なしは最後の nofile で、ファイルの数に数えない', () => {
    const { nodes, fileCount } = buildFileTree([group(null), group('src/a.ts')]);
    expect(nodes.map((n) => n.id)).toEqual(['dir:src', 'nofile']);
    expect(nodes[1].label).toBe('ファイルの指定なし');
    expect(nodes[1].children).toBeUndefined();
    expect(fileCount).toBe(1);
  });

  it('節の id とファイルを往復できる', () => {
    expect(fileOfTreeNode(treeNodeIdOfFile('src/a.ts'))).toBe('src/a.ts');
    expect(fileOfTreeNode(treeNodeIdOfFile(null))).toBeNull();
    expect(treeNodeIdOfFile(null)).toBe('nofile');
    expect(fileOfTreeNode('dir:src')).toBeUndefined();
  });
});
