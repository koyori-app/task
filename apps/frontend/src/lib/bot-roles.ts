/**
 * BOT に選ばせてよい役。BOT は Member 相当まで（Admin を除く）に縛る。
 * backend の `TenantRole::allowed_for_bot` と `ProjectRole::allowed_for_bot` と同じ境で、
 * 越えた役を送れば 400（`bot-role-too-strong`）になる。
 */
export function rolesForAccount<T extends { value: string }>(
  roles: readonly T[],
  accountKind: string | null | undefined,
): T[] {
  return accountKind === 'bot' ? roles.filter((role) => role.value !== 'Admin') : [...roles];
}
