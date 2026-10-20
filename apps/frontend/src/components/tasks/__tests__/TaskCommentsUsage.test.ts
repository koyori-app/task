import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// TaskComments は mentionTenant が無いと、コメントの札をプロフィールへの link に
// せず span で出す。分割ビューだけ渡して全画面で渡し忘れた事がある（PR #963）。
// 使う側が増えても同じ漏れが出ないよう、src の下の全ての <TaskComments> が
// :mention-tenant を渡していることを源で確かめる。
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC_DIR = path.resolve(__dirname, '../../..');
const VUE_FILES = fs.globSync(path.join(SRC_DIR, '**/*.vue'));

/** `<TaskComments ...>` の開きタグを、使う file の道と組にして返す。 */
function taskCommentsTags(): { file: string; tag: string }[] {
  const found: { file: string; tag: string }[] = [];
  for (const file of VUE_FILES) {
    const source = fs.readFileSync(file, 'utf8');
    for (const match of source.matchAll(/<TaskComments\b[^>]*>/g)) {
      found.push({ file: path.relative(SRC_DIR, file), tag: match[0] });
    }
  }
  return found;
}

describe('TaskComments の使い所', () => {
  it('分割ビューと全画面の両方で使われている（探し方が空振りしていない）', () => {
    const files = taskCommentsTags().map(({ file }) => file);
    expect(files).toContain('components/tasks/TaskDetailPane.vue');
    expect(files).toContain('pages/@tenant/projects/@projectKey/tasks/@taskId/+Page.vue');
  });

  it('どの使い所も :mention-tenant を渡す', () => {
    const missing = taskCommentsTags()
      .filter(({ tag }) => !/\s:mention-tenant="[^"]+"/.test(tag))
      .map(({ file }) => file);
    expect(missing).toEqual([]);
  });
});
