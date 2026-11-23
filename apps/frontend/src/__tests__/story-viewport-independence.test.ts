import { describe, expect, it } from 'vitest';

/**
 * ストーリーの play は、実際の画面幅を前提にしない。ストーリーを走らせる経路
 * （vitest の storybook プロジェクト・test-runner・外部の VRT）は画面幅の扱いがそれぞれ違い、
 * 画面幅に依る検査はどれかで必ず落ちる（TASK-524 で、狭い幅を media query で確かめる
 * ストーリーを足し、test-runner の側で幅を合わせにいって別の副作用が出た）。
 * 狭さは枠の幅（container query のもの）か prop（media query のもの）で作る。
 */
const stories = import.meta.glob('../../stories/**/*.stories.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const testRunner = Object.values(
  import.meta.glob('../../.storybook/test-runner.ts', {
    query: '?raw',
    import: 'default',
    eager: true,
  }) as Record<string, string>,
)[0];

describe('ストーリーは画面幅に依らない', () => {
  it('ストーリーを読めている', () => {
    expect(Object.keys(stories).length).toBeGreaterThan(10);
    expect(testRunner).toBeDefined();
  });

  it('play で幅の media query（max-width / min-width）を引かない', () => {
    const offenders = Object.entries(stories).flatMap(([path, source]) =>
      [...source.matchAll(/matchMedia\(([^)]*)\)/g)]
        .filter((match) => /(max|min)-width/.test(match[1]))
        .map((match) => `${path}: ${match[0]}`),
    );

    expect(offenders).toEqual([]);
  });

  it('test-runner は画面幅を ARGOS の撮影の 1 か所でしか変えない', () => {
    expect(testRunner.match(/setViewportSize\(/g)).toHaveLength(1);
  });
});
