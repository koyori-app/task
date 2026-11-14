import { queryErrorStatus } from '@/lib/api-vue-query';

/** 管理者が行う操作。エラーの説明を操作ごとに変えるための識別子。 */
export type AdminOperation =
  | 'createUser'
  | 'changeAdmin'
  | 'suspend'
  | 'unsuspend'
  | 'passwordReset'
  | 'reset2fa'
  | 'deletePasskey'
  | 'disconnectOAuth'
  | 'updateSettings';

const FORBIDDEN: Record<AdminOperation, string> = {
  createUser: '管理者権限がないため、ユーザーを作成できません。',
  changeAdmin:
    '管理者権限を変更できません。自分自身の権限は外せません。また、最後の管理者は降格できません。',
  suspend: '最後の管理者は停止できません。先に別の利用者を管理者にしてください。',
  unsuspend: '管理者権限がないため、停止を解除できません。',
  passwordReset: '管理者権限がないため、リセットメールを送れません。',
  reset2fa: '管理者権限がないため、2FA をリセットできません。',
  deletePasskey: '管理者権限がないため、パスキーを削除できません。',
  disconnectOAuth: '管理者権限がないため、OAuth 連携を解除できません。',
  updateSettings: '管理者権限がないため、設定を変更できません。',
};

const NOT_FOUND: Record<AdminOperation, string> = {
  createUser: '作成先が見つかりません。',
  changeAdmin: '対象のユーザーが見つかりません。すでに削除された可能性があります。',
  suspend: '対象のユーザーが見つかりません。すでに削除された可能性があります。',
  unsuspend: '対象のユーザーが見つかりません。すでに削除された可能性があります。',
  passwordReset: '対象のユーザーが見つかりません。すでに削除された可能性があります。',
  reset2fa: '対象のユーザーが見つかりません。すでに削除された可能性があります。',
  deletePasskey: '対象のパスキーが見つかりません。すでに削除されている可能性があります。',
  disconnectOAuth: '対象の OAuth 連携が見つかりません。すでに解除されている可能性があります。',
  updateSettings: 'システム設定が見つかりません。',
};

const CONFLICT: Partial<Record<AdminOperation, string>> = {
  createUser: '同じメールアドレスまたはユーザー名の利用者がすでにいます。',
  passwordReset: '同じ宛先へ短時間に送りすぎています。しばらく待ってから再度お試しください。',
};

/**
 * 操作の失敗を、操作ごとの 403 / 404 / 409 の意味で説明する。
 * backend の拒否理由は操作ごとに違うため、HTTP 状態だけの共通文言にしない。
 */
export function adminErrorMessage(operation: AdminOperation, error: unknown): string {
  switch (queryErrorStatus(error)) {
    case 403:
      return FORBIDDEN[operation];
    case 404:
      return NOT_FOUND[operation];
    case 409:
      return (
        CONFLICT[operation] ?? '他の操作と競合しました。画面を更新してからやり直してください。'
      );
    case 400:
    case 422:
      return '入力内容を確認してください。';
    default:
      return '操作に失敗しました。時間をおいて再度お試しください。';
  }
}

type TargetUser = { username: string; email: string };

/** 確認ダイアログに出す対象。取り違えを避けるため、名前とメールを必ず並べる。 */
export function targetLabel(user: TargetUser): string {
  return `${user.username}（${user.email}）`;
}
