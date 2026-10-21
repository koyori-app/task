import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// メンションの札は、描き手に mentions（札の表）を渡すと描かれ、mentionTenant（道の
// テナント）も渡すとプロフィールへの link になる。表だけ渡して道を渡し忘れると、
// その経路だけ札が素の span になる。PR #963 では、全画面のコメントと分割ビューの
// 説明文で、この渡し忘れが一つずつ見つかった。
// 経路が増えても同じ漏れが出ないよう、src と server の下で描き手を呼ぶ所を全て拾い、
// 表を渡す描画は必ず道のテナントも渡すことを源で確かめる。
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FRONTEND_DIR = path.resolve(__dirname, '../../..');
const SOURCE_FILES = ['src', 'server']
  .flatMap((dir) => fs.globSync(path.join(FRONTEND_DIR, dir, '**/*.{ts,vue}')))
  .filter((file) => !/(__tests__|\.test\.|\.stories\.)/.test(file));

/** `await renderDescription(...)` の引数の文字を、呼ぶ file の道と組にして返す。 */
function renderCalls(): { file: string; args: string }[] {
  const calls: { file: string; args: string }[] = [];
  for (const file of SOURCE_FILES) {
    const source = fs.readFileSync(file, 'utf8');
    for (const match of source.matchAll(/await\s+renderDescription\(/g)) {
      // 括弧の深さを数えて、呼び出しの閉じ括弧までを引数とする
      let depth = 1;
      let end = match.index + match[0].length;
      while (depth > 0 && end < source.length) {
        if (source[end] === '(') depth += 1;
        if (source[end] === ')') depth -= 1;
        end += 1;
      }
      const args = source
        .slice(match.index + match[0].length, end - 1)
        // 注釈の中の語で判じないよう、行の注釈を除く
        .replace(/\/\/.*$/gm, '');
      calls.push({ file: path.relative(FRONTEND_DIR, file), args });
    }
  }
  return calls;
}

const passesMentions = (args: string) => /\bmentions\b\s*[:,}\n]/.test(args);
const passesMentionTenant = (args: string) => /\bmentionTenant\b/.test(args);

describe('メンションの札を描く経路', () => {
  it('描き手を呼ぶ所を全て拾えている（探し方が空振りしていない）', () => {
    const files = renderCalls().map(({ file }) => file);
    // 全画面の説明文（SSR）、分割ビューの説明文（server の口）、コメント、レビューの指摘、
    // ドキュメント（SSR）。ドキュメントとレビューの指摘は札の表を渡さず、札を描かない
    expect(files).toEqual(
      expect.arrayContaining([
        'src/pages/@tenant/projects/@projectKey/tasks/@taskId/+data.ts',
        'src/pages/@tenant/projects/@projectKey/docs/@fileId/+data.ts',
        'server/middlewares/render-description.ts',
        'src/components/tasks/CommentBody.vue',
        'src/components/reviews/ReviewFindingBody.vue',
      ]),
    );
  });

  it('札の表を渡す描画は、どれも道のテナントも渡す', () => {
    const calls = renderCalls().filter(({ args }) => passesMentions(args));
    // 表を渡す経路が見つからなければ、この確かめは何も縛らない
    expect(calls.length).toBeGreaterThanOrEqual(3);
    const missing = calls.filter(({ args }) => !passesMentionTenant(args)).map(({ file }) => file);
    expect(missing).toEqual([]);
  });
});
