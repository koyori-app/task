/** 選べる利用者（`UserSummary` と同じ形。担当者候補なら部署・チームも付く）。 */
export type SelectableUser = {
  id: string;
  username: string;
  avatar_url?: string | null;
  department?: string | null;
  team?: string | null;
};

/** 候補の取得の様。取得中・失敗を「0 人」と混ぜぬために分けて受ける。 */
export type UserSelectorState = { loading?: boolean; error?: boolean; onRetry?: () => void };
