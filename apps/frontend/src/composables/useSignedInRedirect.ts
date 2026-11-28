import { useQueryClient } from '@tanstack/vue-query';
import { meQueryOptions } from '@/lib/api-vue-query';

/**
 * サインイン・登録の画面で、既にログイン済みなら戻り先へ進める。
 *
 * ログイン中に OAuth の開始を開くと、backend は連携の追加とみなして再認証を求め、
 * `reauth-required` の JSON を画面に出してしまう。共有された `/signin?redirect_after=…` を
 * ログイン中に開いたときと、別のタブでログインした後に古いサインイン画面のボタンを押したときに起きる。
 *
 * /me を取り直し、通れば戻り先へ置き換えて true を返す。拒否（401 / 403。2FA の途中を含む）や
 * 通信の失敗は、ログイン済みと確かめられないので false を返し、呼び出し側はサインインを続ける。
 */
export function useSignedInRedirect() {
  const queryClient = useQueryClient();

  async function redirectIfSignedIn(redirectAfter: string): Promise<boolean> {
    try {
      await queryClient.fetchQuery({ ...meQueryOptions(), staleTime: 0 });
    } catch {
      return false;
    }
    window.location.replace(redirectAfter);
    return true;
  }

  return { redirectIfSignedIn };
}
