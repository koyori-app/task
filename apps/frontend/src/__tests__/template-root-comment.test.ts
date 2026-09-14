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

function startsWithCommentBeforeElement(template: string): boolean {
  const body = template.trimStart();
  if (!body.startsWith('<!--')) return false;
  const end = body.indexOf('-->');
  if (end === -1) return false;
  return body.slice(end + 3).trim().length > 0;
}

describe('SFC のテンプレートの根', () => {
  it('先頭コメント + 要素（本番でフラグメント根になる形）を持たない', () => {
    expect(Object.keys(sources).length).toBeGreaterThan(0);

    const offenders = Object.entries(sources)
      .filter(([, source]) => {
        const template = rootTemplate(source);
        return template !== null && startsWithCommentBeforeElement(template);
      })
      .map(([path]) => path.replace('../', 'src/'));

    expect(offenders).toEqual([]);
  });
});
