import { describe, expect, it } from 'vitest';

/**
 * `<template>` 直下の先頭に HTML コメントを置くと、本番ビルドではコメントがそのまま
 * 描画関数に残るため根がフラグメントになり、呼び出し側から渡した class などの
 * fallthrough 属性が黙って捨てられる。開発時は Vue の dev 専用 `filterSingleRoot` が
 * 単一ルートへ補正するので、通常の描画テストでは検出できない（#725 のモーダルが
 * 本番だけ 2 列にならなかった原因）。
 *
 * コメントは根の要素の内側か `<script setup>` 側へ置くこと。テンプレートがコメント
 * だけで要素を持たないコンポーネント（描画しないもの）は根が変わらないので対象外。
 */
const sources = import.meta.glob('../**/*.vue', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** SFC の根の `<template>` ブロックの中身。行頭の `<template>` だけを見る */
function rootTemplate(source: string): string | null {
  const match = /^<template[^>]*>\n([\s\S]*?)\n^<\/template>/m.exec(source);
  return match ? match[1] : null;
}

/** コメントを除いた残りに、要素やテキストが残るか */
function hasContentBesideComments(fragment: string): boolean {
  return fragment.replace(/<!--[\s\S]*?-->/g, '').trim().length > 0;
}

/**
 * 根の要素と並ぶ HTML コメントがあるか。
 *
 * 先頭でも末尾でも、テンプレート直下のコメントは根の要素の兄弟になり、本番ビルドで
 * 根がフラグメントになる。根の要素の内側のコメントと、要素を持たないテンプレートは対象外。
 */
function hasCommentBesideRootElement(template: string): boolean {
  const body = template.trim();
  if (body.startsWith('<!--')) {
    const end = body.indexOf('-->');
    if (end !== -1 && hasContentBesideComments(body.slice(end + 3))) return true;
  }
  // 根の要素は閉じタグか `/>` で終わるので、末尾が `-->` ならテンプレート直下のコメント
  if (body.endsWith('-->')) {
    const start = body.lastIndexOf('<!--');
    if (start !== -1 && hasContentBesideComments(body.slice(0, start))) return true;
  }
  return false;
}

describe('hasCommentBesideRootElement', () => {
  it.each([
    ['先頭コメント + 要素', '<!-- 説明 -->\n<div />', true],
    ['要素 + 末尾コメント', '<div />\n<!-- 説明 -->', true],
    ['先頭と末尾の両方', '<!-- a -->\n<div />\n<!-- b -->', true],
    ['コメントは根の要素の内側', '<div>\n  <!-- 説明 -->\n</div>', false],
    ['コメントだけ（要素を持たない）', '<!-- 描画しない -->', false],
    ['コメントが複数だけ', '<!-- a -->\n<!-- b -->', false],
    ['コメントが無い', '<div />', false],
  ])('%s', (_, template, expected) => {
    expect(hasCommentBesideRootElement(template)).toBe(expected);
  });
});

describe('SFC のテンプレートの根', () => {
  it('根の要素と並ぶコメント（本番でフラグメント根になる形）を持たない', () => {
    expect(Object.keys(sources).length).toBeGreaterThan(0);

    const offenders = Object.entries(sources)
      .filter(([, source]) => {
        const template = rootTemplate(source);
        return template !== null && hasCommentBesideRootElement(template);
      })
      .map(([path]) => path.replace('../', 'src/'));

    expect(offenders).toEqual([]);
  });
});
