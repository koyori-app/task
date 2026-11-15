/** v-html の容れ物の中だけをカードの起点にする。 */
export const TASK_REF_CARD_SELECTOR = 'a.kfm-task-ref';
export type TaskRefCardTarget = { projectKey: string; seqKey: string; href: string };
export function taskRefCardTarget(
  trigger: Element,
  tenantDisplayId: string,
): TaskRefCardTarget | null {
  const href = trigger.getAttribute('href');
  if (!href) return null;
  const parts = href.split('/');
  const [, tenant, projects, projectKey, tasks, seqKey] = parts;
  if (
    parts.length !== 6 ||
    parts[0] !== '' ||
    tenant !== tenantDisplayId ||
    projects !== 'projects' ||
    tasks !== 'tasks' ||
    !/^[A-Z][A-Z0-9]{1,9}$/.test(projectKey) ||
    !new RegExp(`^${projectKey}-[0-9]+$`).test(seqKey)
  )
    return null;
  return { projectKey, seqKey, href };
}
export function hasTaskRefCards(html: string): boolean {
  return html.includes('kfm-task-ref');
}
