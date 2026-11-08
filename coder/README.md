# Coder テンプレート

task の開発用ワークスペース（Docker）。Coder のホストで Docker が動いていれば使える。

```bash
coder templates push task -d coder
```

## 入っているもの

| 場所 | 中身 |
|---|---|
| イメージ（`build/Dockerfile`） | Rust（rustfmt / clippy）、cargo-nextest、cargo-hakari、mold、Node、pnpm（corepack）、ripgrep、gh、jq、psql、Docker（compose / buildx） |
| ホーム（初回起動時に `main.tf` の `startup_script` が入れる） | Claude Code、Codex、Cursor CLI（`cursor-agent`）、T3 Code（`t3`）、Paradev（`superset` / `paradev`）、リポジトリ（`~/works/task`） |

ホーム（`/home/coder`）と Docker のデータはボリュームに残り、ワークスペースを止めても消えない。
それ以外はイメージを作り直すと元に戻る。

Rust と Node の版は `build/Dockerfile` の `ARG` で決めている。`rust-toolchain.toml` と
`.node-version` を上げたら合わせる。

## 初回にやること

1. Coder の SSH 公開鍵（`coder publickey`）を GitHub に登録する。登録前に起動すると clone が失敗するので、登録後に `git clone git@github.com:koyori-app/task.git ~/works/task` を実行する
2. `claude` を起動してログインし、Codex は `codex login --device-auth`、Cursor CLI は `cursor-agent login` でログインする（Codex は ChatGPT の契約。ログイン情報はホームに残る）。Claude Code は、手元で `claude setup-token` を実行して作ったトークンを Coder のユーザーシークレットに登録しておくと、新しいワークスペースでもログイン済みになる（登録した本人のワークスペースにだけ入る。動いているワークスペースには再起動後に反映される）
   ```bash
   echo -n "<トークン>" | coder secret create claude-token --env CLAUDE_CODE_OAUTH_TOKEN
   ```
3. `superset auth login` のあと `superset start --daemon --auto-update` を実行する。次回の起動からは自動で Paradev のホストとして立ち上がる
4. コミットに署名するなら GPG 鍵を取り込む

## 注意

- コンテナは `privileged` で動く。統合テストのハーネス（testcontainers）と `docker compose` のために、コンテナの中で dockerd を動かしているため。信頼できる利用者だけに使わせること
- T3 Code は nightly を入れている（安定版には CLI の配布物がまだ無い）。`t3 serve` で起動し、ブラウザからは下と同じポート転送で開く（`t3 service install` は systemd が無いので使えない）
- 開発サーバー（3000 / 3400）は `coder port-forward <workspace> --tcp 3000:3000` か、ダッシュボードのポート転送で開く
