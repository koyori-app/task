export function isRowInteractiveTarget(event: Event) {
  const target = event.target as HTMLElement | null;
  if (!target) return false;
  return Boolean(target.closest('button, a, input, [role="checkbox"]'));
}

export function handleRowKeydownNavigate(event: KeyboardEvent, navigate: () => void) {
  if (event.key !== 'Enter' && event.key !== ' ') return;
  if (isRowInteractiveTarget(event)) return;
  event.preventDefault();
  navigate();
}
