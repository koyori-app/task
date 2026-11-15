//! `task --version` が名乗る版を決める。
//!
//! リリースはリポジトリ共通の `v*` タグで行うので、CLI だけが独自の版を持つと
//! タグと `--version` がずれる。タグから `TASK_CLI_VERSION` を渡してもらい、
//! 無ければクレートの版を使う（手元のビルドはこちら）。

fn main() {
    let version = std::env::var("TASK_CLI_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    // 空でない TASK_CLI_VERSION が渡されたビルドだけをリリースとして扱う。
    // CI はタグの push のときだけ渡し、PR・手動実行・手元ではクレートの版を使う。
    let from_release = std::env::var("TASK_CLI_VERSION")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    // 配布物の名前（`task-<版>-<target>.tar.gz`）に使う
    let target = std::env::var("TARGET").unwrap_or_default();

    println!("cargo:rustc-env=TASK_CLI_VERSION={version}");
    println!("cargo:rustc-env=TASK_CLI_TARGET={target}");
    println!(
        "cargo:rustc-env=TASK_CLI_VERSION_SOURCE={}",
        if from_release { "env" } else { "crate" }
    );
    println!("cargo:rerun-if-env-changed=TASK_CLI_VERSION");
}
