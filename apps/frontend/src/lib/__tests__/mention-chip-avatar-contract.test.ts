import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  USER_AVATAR_INACTIVE_CLASS,
  USER_AVATAR_INITIALS_CLASS,
  USER_AVATAR_SIZES,
} from '@/lib/user-avatar';

/**
 * メンションの札のアバター（`remark-kfm-mention/style.css` の `.kfm-mention-avatar`）が、UserAvatar の xs の写しで
 * あることを縛る。札は v-html と CodeMirror の widget でも描くので Vue の部品を置けず、値を CSS で写しておる。
 * 期待の値は `lib/user-avatar.ts` の定数（Tailwind の class）から引くので、どちらの側を替えても落ちる。
 */
const CSS = readFileSync(resolve(__dirname, '../remark-kfm-mention/style.css'), 'utf8');

/** 選択子の並び（`,` 区切り）に `selector` をちょうど含む規則の宣言（`prop` → 値）。 */
function declarations(selector: string): Record<string, string> {
  const source = CSS.replace(/\/\*[\s\S]*?\*\//g, '');
  for (const m of source.matchAll(/([^{}]+)\{([^}]*)\}/g)) {
    const selectors = m[1].split(',').map((s) => s.trim().replace(/\s+/g, ' '));
    if (!selectors.includes(selector)) continue;
    return Object.fromEntries(
      m[2]
        .split(';')
        .map((d) => d.trim())
        .filter(Boolean)
        .map((d) => {
          const at = d.indexOf(':');
          return [d.slice(0, at).trim(), d.slice(at + 1).trim()];
        }),
    );
  }
  throw new Error(`${selector} の規則が見つからぬ`);
}

/** 写しに使う Tailwind の class を、その CSS の宣言へ直す（ここに無い class は写せぬので落とす）。 */
function tailwind(classes: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const cls of classes.split(/\s+/).filter(Boolean)) {
    let m: RegExpExecArray | null;
    if ((m = /^size-(\d+(?:\.\d+)?)$/.exec(cls))) {
      out.width = out.height = `${Number(m[1]) / 4}rem`;
    } else if ((m = /^text-\[(\d+px)\]$/.exec(cls))) {
      out['font-size'] = m[1];
    } else if ((m = /^bg-([a-z-]+)$/.exec(cls))) {
      out['background-color'] = `var(--${m[1]})`;
    } else if ((m = /^text-([a-z-]+)$/.exec(cls))) {
      out.color = `var(--${m[1]})`;
    } else if ((m = /^opacity-(\d+)$/.exec(cls))) {
      out.opacity = String(Number(m[1]) / 100);
    } else if (cls === 'grayscale') {
      out.filter = 'grayscale(1)';
    } else {
      throw new Error(`写し方を知らぬ class: ${cls}`);
    }
  }
  return out;
}

describe('札のアバターは UserAvatar の xs の写し', () => {
  const avatar = declarations('.kfm-mention-avatar');

  it('大きさと頭文字の字の大きさは USER_AVATAR_SIZES.xs', () => {
    expect(avatar).toMatchObject(tailwind(USER_AVATAR_SIZES.xs));
  });

  it('頭文字の地と字の色は USER_AVATAR_INITIALS_CLASS', () => {
    expect(avatar).toMatchObject(tailwind(USER_AVATAR_INITIALS_CLASS));
  });

  it('字の太さは UserAvatar と同じく置かれた所に従う（札の側で決めぬ）', () => {
    expect(avatar).not.toHaveProperty('font-weight');
  });

  it('外れた者・退会した者の灰色は USER_AVATAR_INACTIVE_CLASS', () => {
    const inactive = tailwind(USER_AVATAR_INACTIVE_CLASS);
    for (const state of ['outside', 'gone']) {
      expect(declarations(`.kfm-mention-${state} .kfm-mention-avatar`)).toMatchObject(inactive);
    }
  });

  it('画像の置き方は UserAvatar と同じく丸いっぱい（object-fit で切り抜かぬ）', () => {
    const img = declarations('.kfm-mention-avatar > img');
    expect(img).toMatchObject({ width: '100%', height: '100%' });
    expect(img).not.toHaveProperty('object-fit');
  });
});
