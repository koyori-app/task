import { describe, it, expect } from 'vitest';
import { taskRefCardTarget, hasTaskRefCards, TASK_REF_CARD_SELECTOR } from '../task-ref-card';
import { renderDescription } from '@/lib/markup-renderer';
const anchor = (href?: string) => {
  const a = document.createElement('a');
  if (href) a.setAttribute('href', href);
  return a;
};
describe('task ref target', () => {
  it('同じテナントの正しいパスだけを受ける', () => {
    const href = '/acme/projects/TASK/tasks/TASK-58';
    expect(taskRefCardTarget(anchor(href), 'acme')).toEqual({
      projectKey: 'TASK',
      seqKey: 'TASK-58',
      href,
    });
    for (const value of [
      undefined,
      '/other/projects/TASK/tasks/TASK-58',
      '/acme/wrong/TASK/tasks/TASK-58',
      '/acme/projects/ENG/tasks/TASK-1',
      '/acme/projects/TASK/tasks/TASK-x',
      href + '/extra',
      href + '?x=1',
    ])
      expect(taskRefCardTarget(anchor(value), 'acme')).toBeNull();
    expect(hasTaskRefCards('<a class="kfm-task-ref">')).toBe(true);
    expect(hasTaskRefCards('<p>plain</p>')).toBe(false);
  });
  it('描画器が出すリンクを読み取れる', async () => {
    const html = await renderDescription(
      'TASK-1 （ENG-2） **A1-22**\n\n> [!NOTE]\n> TASK-3\n\n`TASK-4` [TASK-5](https://example.com)',
      { taskRef: { tenant: 'acme' } },
    );
    const root = document.createElement('div');
    root.innerHTML = html;
    const links = [...root.querySelectorAll(TASK_REF_CARD_SELECTOR)];
    expect(links).toHaveLength(4);
    for (const a of links) expect(taskRefCardTarget(a, 'acme')?.seqKey).toBe(a.textContent);
  });
});
