# 認証フォーム 設計書

## 概要

サインアップ・サインインフォームの実装仕様。パスワード表示トグル・強度メーター・バリデーション表示の空間共有を含む。

---

## コンポーネント構成

| ファイル | 役割 |
|---|---|
| `src/components/auth/SignUpForm.vue` | 新規登録フォーム |
| `src/components/auth/SignInForm.vue` | ログインフォーム |
| `src/components/auth/PasswordInput.vue` | パスワード入力（表示/非表示トグル付き） |
| `src/components/auth/PasswordStrengthBar.vue` | パスワード強度バー |
| `src/composables/usePasswordStrength.ts` | 強度判定コンポーザブル（API クライアント） |
| `src/composables/__tests__/usePasswordStrength.test.ts` | 強度判定ユニットテスト |
| `server/elysia.ts` | SSR サーバー（`POST /internal/password-strength` を含む） |


---

## パスワード表示トグル

`PasswordInput` は originui の input-group をベースとした複合コンポーネント。

- デフォルト: `type="password"`
- トグルボタンで `type="text"` に切り替え
- アイコン: Phosphor Icons `PhEye` / `PhEyeSlash`
- `focus` / `blur` イベントを emit し、親フォームが `passwordFocused` を管理する

---

## パスワード強度判定

### アーキテクチャ

zxcvbn-ts は非圧縮約 4.7MB のためクライアントバンドルに乗せない。
スコアリングは Elysia SSR サーバーで行い、クライアントは API 経由で結果を取得する。

```text
クライアント
  └─ usePasswordStrength（watchDebounced 300ms）
       └─ POST /internal/password-strength
            └─ Elysia サーバー（zxcvbn-ts）
                 └─ { strength: 'low' | 'medium' | 'high' | '' }
```

`/internal/` プレフィックスにより Vite dev proxy（`/api/*` → Rust backend）と衝突しない。
dev / prod 両環境で Elysia が直接処理する（`elysia.ts` 経由）。

### エンドポイント

`POST /internal/password-strength`

リクエスト: `{ password: string }`
レスポンス: `{ strength: '' | 'low' | 'medium' | 'high' }`

### スコアマッピング

| zxcvbn score | strength |
|---|---|
| 0–1 | `low` |
| 2–3 | `medium` |
| 4 | `high` |
| 空文字 | `''` |

### 辞書設定

```ts
zxcvbnOptions.setOptions({
  dictionary: {
    ...commonPackage.dictionary,
    jaPasswords: jaPackage.dictionary.commonWords,
  },
  graphs: commonPackage.adjacencyGraphs,
});
```

日本語ローマ字パスワード（`sakura`, `hanako` 等）の低評価のため `jaPasswords` を追加。
英語辞書よりペナルティを低く抑えるため `commonWords` サブセットを使用。

### レースコンディション対策

高速入力時に古いレスポンスが後から届いて上書きするのを防ぐため、seq ID で管理する。

```ts
let seq = 0;

// 空文字リセット時: 進行中リクエストを無効化
seq++;

// fetch 前
const id = ++seq;

// 受信時: stale なら破棄
if (!response.ok || id !== seq) return;
```

---

## SignUpForm 表示ロジック

TanStack Form + ArkType を使用。

| フィールド | バリデーションルール | タイミング |
|---|---|---|
| username | 3文字以上 | onBlur |
| email | 必須 / email 形式 | onBlur |
| password | 8文字以上 | onBlur |

`hasSubmitted` フラグで送信後は即時バリデーションに切り替える。

### ユーザー名フィールド下部の空間共有

`min-h-[1.25rem]` コンテナに以下を排他表示する。

| 優先順 | 条件 | 表示内容 |
|---|---|---|
| 1 | エラーあり かつ `isTouched` | `FieldError` |
| 2 | それ以外 | "3文字以上で設定してください。" |

### パスワードフィールド下部の空間共有

`min-h-[1.5rem]` コンテナに以下を排他表示する。

| 優先順 | 条件 | 表示内容 |
|---|---|---|
| 1 | 1文字以上 かつ（フォーカス中 または エラーなし） | `PasswordStrengthBar` |
| 2 | エラーあり かつ `isTouched` | `FieldError` |
| 3 | それ以外（0文字含む） | "8文字以上で設定してください。" |

フォーカス中はエラーを強度バーで上書きする（入力継続中のユーザーに強度フィードバックを優先）。
0文字のとき常時ヒントを表示することで、フォーカス前から要件をユーザーに伝える。

### PasswordStrengthBar

- 3セグメントのカラーバー（赤 / 黄 / 緑）
- `v-if="strength"` により空文字のとき自身は何も描画しない
- ラベル: 弱い / 普通 / 強い
- `role="meter"` + `aria-valuenow` / `aria-valuemin` / `aria-valuemax` で WAI-ARIA 対応

---

## 今後の課題（次 PR 予定）

- `POST /v1/auth/register` / `/v1/auth/login` API 連携
- 登録失敗（409: ユーザー名・メール重複）をバリデーション空間に表示
- 2FA（login 200 レスポンス）対応: 現状 TODO コメントのみ
