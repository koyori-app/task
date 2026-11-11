/**
 * ドキュメント新規作成時の本文テンプレート。
 *
 * 種類は本文の初期値を選ぶためだけのもので、ファイルにもどこにも保存しない
 * （実体は Drive の `.md` ファイルで、種類を持つ列は無い）。
 */
export type DocumentTemplate = {
  id: string;
  label: string;
  body: (title: string) => string;
};

export const DOCUMENT_TEMPLATES: readonly DocumentTemplate[] = [
  {
    id: 'feature-spec',
    label: '機能仕様',
    body: (title) => `# ${title}\n\n## 背景\n\n## 目的\n\n## 仕様\n\n## 対象外\n\n## 未決事項\n`,
  },
  {
    id: 'api-spec',
    label: 'API 仕様',
    body: (title) =>
      `# ${title}\n\n## エンドポイント\n\n| メソッド | パス | 説明 |\n| --- | --- | --- |\n|  |  |  |\n\n## リクエスト\n\n## レスポンス\n\n## エラー\n`,
  },
  {
    id: 'screen-spec',
    label: '画面仕様',
    body: (title) =>
      `# ${title}\n\n## URL\n\n## 表示内容\n\n## 操作\n\n## 状態（空・読み込み中・エラー）\n`,
  },
  {
    id: 'minutes',
    label: '議事録',
    body: (title) => `# ${title}\n\n- 日時:\n- 参加者:\n\n## 議題\n\n## 決定事項\n\n## TODO\n`,
  },
  {
    id: 'runbook',
    label: '手順書',
    body: (title) => `# ${title}\n\n## 前提\n\n## 手順\n\n1.\n\n## 確認\n\n## 戻し方\n`,
  },
  {
    id: 'memo',
    label: 'メモ',
    body: (title) => `# ${title}\n`,
  },
];
