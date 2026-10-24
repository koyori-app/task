// 利用者を名で探す規則（メンションの補完）と、候補の並び（利用者の選択 UserSelector）。
//
// - 比べる前に名と入力を揃える: NFKC（全角の英数字を半角に）、小文字、片仮名を平仮名に。
//   日本語の IME のまま全角で打っても、大文字で打っても当たる。
// - 前方一致を先に、次に部分一致。同じ組の中は渡された順を保つ。
//
// 表示名という欄は今の API に無い（UserSummary は id・username・avatar_url だけ）ので、
// 比べるのは username だけ。

export type NamedUser = { id: string; username: string };

const KATAKANA = /[ァ-ヶ]/g;

/** 比べるための形に揃える。 */
export function normalizeUserQuery(text: string): string {
  return text
    .normalize('NFKC')
    .toLowerCase()
    .replace(KATAKANA, (ch) => String.fromCharCode(ch.charCodeAt(0) - 0x60))
    .trim();
}

/** 入力に当たる利用者を、前方一致・部分一致の順に返す。空の入力は全員をそのまま返す。 */
export function rankUsersByQuery<T extends NamedUser>(users: readonly T[], query: string): T[] {
  const q = normalizeUserQuery(query);
  if (q === '') return [...users];
  const prefix: T[] = [];
  const partial: T[] = [];
  for (const user of users) {
    const name = normalizeUserQuery(user.username);
    if (name.startsWith(q)) prefix.push(user);
    else if (name.includes(q)) partial.push(user);
  }
  return [...prefix, ...partial];
}

const collator = new Intl.Collator('ja', { sensitivity: 'base', numeric: true });

/**
 * 候補の並び: 選んだ者、自分、その他。各組の中は名の順（日本語の照合）。
 *
 * 選んだ瞬間に並びが変わると、押した項が指の下から逃げる。呼ぶ側は開いた時に一度だけ
 * 並べ、開いている間はその並びを保つこと。
 */
export function orderUsersForPicker<T extends NamedUser>(
  users: readonly T[],
  selectedIds: readonly string[],
  currentUserId?: string | null,
): T[] {
  const selected = new Set(selectedIds);
  const rank = (user: T) => (selected.has(user.id) ? 0 : user.id === currentUserId ? 1 : 2);
  return [...users].sort((a, b) => rank(a) - rank(b) || collator.compare(a.username, b.username));
}
