//! Git ホスティング↔タスク連携のホスト中立な処理（docs/features/tasks/9.github-tasks.md）。
//!
//! ホスト固有の語彙（GitHub の installation 等）はここへ持ち込まない。
//! 受信口がペイロードを正規化してから渡す。

pub mod commits;
pub mod events;
pub mod task_refs;
