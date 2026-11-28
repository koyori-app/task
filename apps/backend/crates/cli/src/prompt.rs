//! 端末への問い合わせ。`task init` だけが使う。
//!
//! 問いの文は標準エラーに出す（標準出力は結果と `--json` のためにあける）。
//! テストは台本どおりに答える実装を差し込む。

use std::io::{BufRead, IsTerminal, Write};

use crate::error::{CliError, Result};
use crate::text_input::read_stdin;

/// 答えの `None` は入力の終わり（EOF）。
pub trait Prompt {
    /// 問い合わせてよいか（標準入力が端末か）。偽なら `line` と `secret` を呼ばない
    fn interactive(&self) -> bool;
    /// 1 行を聞く。行末の改行は落とす
    fn line(&self, label: &str) -> Result<Option<String>>;
    /// 画面に出さずに 1 行を聞く（トークン）
    fn secret(&self, label: &str) -> Result<Option<String>>;
    /// 端末でない標準入力を丸ごと読む。端末なら `None`
    fn piped(&self, what: &str) -> Result<Option<String>>;
}

/// 実プロセスの端末。
pub struct TerminalPrompt;

fn show(label: &str) -> Result<()> {
    let mut stderr = std::io::stderr();
    stderr
        .write_all(label.as_bytes())
        .and_then(|()| stderr.flush())
        .map_err(|err| CliError::new(format!("Cannot write to stderr: {err}")))
}

fn without_line_end(mut line: String) -> String {
    while line.ends_with(['\n', '\r']) {
        line.pop();
    }
    line
}

impl Prompt for TerminalPrompt {
    fn interactive(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn line(&self, label: &str) -> Result<Option<String>> {
        show(label)?;
        let mut line = String::new();
        let read = std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|err| CliError::new(format!("Cannot read from the terminal: {err}")))?;
        Ok((read > 0).then(|| without_line_end(line)))
    }

    fn secret(&self, label: &str) -> Result<Option<String>> {
        show(label)?;
        match rpassword::read_password() {
            Ok(value) => Ok(Some(without_line_end(value))),
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(err) => Err(CliError::new(format!(
                "Cannot read from the terminal: {err}"
            ))),
        }
    }

    fn piped(&self, what: &str) -> Result<Option<String>> {
        read_stdin(what)
    }
}

/// 端末でなく、何も読めない。`Context::new` の既定（テストが実プロセスの標準入力を読まないため）。
pub(crate) struct NoPrompt;

impl Prompt for NoPrompt {
    fn interactive(&self) -> bool {
        false
    }

    fn line(&self, _label: &str) -> Result<Option<String>> {
        Ok(None)
    }

    fn secret(&self, _label: &str) -> Result<Option<String>> {
        Ok(None)
    }

    fn piped(&self, _what: &str) -> Result<Option<String>> {
        Ok(None)
    }
}
