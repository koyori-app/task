/** 選べる利用者（`UserSummary` と同じ形）。 */
export type SelectableUser = { id: string; username: string; avatar_url?: string | null };

/** 候補の取得の様。取得中・失敗を「0 人」と混ぜぬために分けて受ける。 */
export type UserSelectorState = { loading?: boolean; error?: boolean; onRetry?: () => void };
