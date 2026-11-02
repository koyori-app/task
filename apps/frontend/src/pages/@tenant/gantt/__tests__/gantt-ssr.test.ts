// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import { GanttChart } from '@koyori-app/arc-vue';

/**
 * arc-vue は onMounted の中で wasm を読むので、server では wasm にも canvas にも触れない。
 * vike の SSR で落ちないことを、server のレンダラで縛る（描画は client の mount の後）。
 */
describe('ガントの SSR', () => {
  it('server で描いても落ちず、wasm を読む前の骨組みを返す', async () => {
    const app = createSSRApp({
      render: () =>
        h(GanttChart, {
          tasks: [
            { id: 'a', title: '設計', progress_pct: 40, start: '2026-10-12', end: '2026-10-16' },
          ],
          today: '2026-10-14',
        }),
    });
    const html = await renderToString(app);
    expect(html).toContain('koyori-gantt');
    expect(html).toContain('koyori-gantt-skeleton');
    expect(html).not.toContain('<svg');
  });
});
