/**
 * 招待の承諾画面（`/invitations/accept`）。
 * 規則は apps/backend/docs/tenant-project-authz.md の「招待」。
 */

/** サインイン後にこの画面へ戻すための印（`lib/one-time-notice.ts`）。値は戻り先のパス。 */
export const INVITATION_ACCEPT_RETURN = 'task:invitation-accept-return';

/** 印の値が承諾画面のパスであるときだけ戻り先として使う（外部や別画面へは飛ばさない）。 */
export function invitationAcceptReturnPath(value: string | null): string | null {
  return value?.startsWith('/invitations/accept?') ? value : null;
}
