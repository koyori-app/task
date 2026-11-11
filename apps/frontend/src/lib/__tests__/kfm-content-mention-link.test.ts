// メンションの札（a.koyori-mention）が、層の外の `.kfm-content a`（青い字・下線）に負けないことを固定する。
// koyori の CSS は components 層に入り、層の外の規則は詳細度に関係なく層に勝つ。
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CSS = fs.readFileSync(path.join(__dirname, '../remark-gfm/style.css'), 'utf8');

/** コメントを剥いだ CSS からセレクタ（カンマ分解済み）を列挙する */
const selectorsOf = (css: string): string[] =>
  css
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .split('}')
    .map((block) => block.split('{')[0]?.trim() ?? '')
    .flatMap((selector) => selector.split(','))
    .map((selector) => selector.trim())
    .filter((selector) => selector.length > 0);

/** 選択子の最後の複合セレクタが要素 `a` で始まるか */
const LAST_COMPOUND_IS_A = /(?:^|[\s>+~])a(?![\w-])[^\s>+~]*$/;

/** 選択子の最後の複合セレクタ（結合子より後ろ） */
const lastCompound = (selector: string): string => selector.split(/[\s>+~]+/).pop() ?? '';

/** 最後の複合セレクタが `a` で始まるのに、その複合セレクタで札を除いていない選択子 */
const linkRulesMissingChipExclusion = (selectors: string[]): string[] =>
  selectors.filter(
    (s) => LAST_COMPOUND_IS_A.test(s) && !lastCompound(s).includes(':not(.koyori-mention)'),
  );

describe('.kfm-content a は札（a.koyori-mention）を外す', () => {
  it('検査器の対照: 札を外さない `.kfm-content a` は違反になる', () => {
    expect(linkRulesMissingChipExclusion(['.kfm-content a'])).toEqual(['.kfm-content a']);
    expect(linkRulesMissingChipExclusion(['.kfm-content a:not(.koyori-mention)'])).toEqual([]);
    expect(linkRulesMissingChipExclusion(['.kfm-content a:not(.koyori-mention):hover'])).toEqual(
      [],
    );
    expect(linkRulesMissingChipExclusion(['.kfm-content a:not(.koyori-mention) span'])).toEqual([]);
    expect(linkRulesMissingChipExclusion(['.kfm-content :not(.koyori-mention) a'])).toEqual([
      '.kfm-content :not(.koyori-mention) a',
    ]);
  });

  it('remark-gfm/style.css の a の規則はすべて :not(.koyori-mention) を持つ', () => {
    const selectors = selectorsOf(CSS);
    expect(selectors.filter((s) => LAST_COMPOUND_IS_A.test(s)).length).toBeGreaterThanOrEqual(1);
    expect(linkRulesMissingChipExclusion(selectors)).toEqual([]);
  });
});
