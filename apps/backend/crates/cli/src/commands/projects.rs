//! プロジェクト。

use payload::labels::LabelResponse;
use serde::Serialize;

use crate::Context;
use crate::cli::ProjectsCommand;
use crate::error::Result;
use crate::output::{OutputOptions, print};
use crate::resolve::{list_labels, list_projects, list_statuses, resolve_project};

/// 人間向けの一覧はキーで包んで出す（TypeScript 版と同じ形）。
#[derive(Serialize)]
struct ProjectListing<T> {
    projects: T,
}

pub async fn run(
    context: &Context,
    command: ProjectsCommand,
    output: OutputOptions,
) -> Result<i32> {
    let api = &context.connect().await?;
    match command {
        ProjectsCommand::List => {
            let projects = list_projects(api).await?;
            if output.json {
                print(&projects, output);
            } else {
                print(
                    &ProjectListing {
                        projects: &projects,
                    },
                    output,
                );
            }
        }
        // `--status` に何を渡せるかは、プロジェクトごとに違ううえ画面を見ないと分からない。
        // CLI だけで完結できるよう並び順（position 昇順）のまま出す
        ProjectsCommand::Statuses { project } => {
            let project = resolve_project(api, &project).await?;
            let mut statuses = list_statuses(api, project.id).await?;
            statuses.sort_by_key(|status| status.position);
            if output.json {
                print(&statuses, output);
            } else {
                // 人間向けは名前だけ 1 行ずつ。オブジェクトを渡すと render_human が
                // 畳めず（特別扱いは `tasks` だけ）、id や日時まで含む JSON が出て
                // 「`--status` に渡せる名前を知る」という用途に合わなくなる
                let names: Vec<&str> = statuses.iter().map(|s| s.name.as_str()).collect();
                print(&names, output);
            }
        }
        // `--label` に渡せる名前を CLI だけで調べられるようにする。API は並び順を保証しないので
        // 名前の昇順に揃え、出力を安定させる
        ProjectsCommand::Labels { project } => {
            let project = resolve_project(api, &project).await?;
            let mut labels = list_labels(api, project.id).await?;
            // `--label` の名前の照合は大文字小文字を区別しないので、並びも同じ基準にする
            labels.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.name.cmp(&b.name))
            });
            if output.json {
                print(&labels, output);
            } else {
                // オブジェクトを渡すと render_human が畳めず id まで含む JSON になるので、
                // 1 件 1 行の文字列にして渡す
                let lines: Vec<String> = labels.iter().map(format_label_line).collect();
                print(&lines, output);
            }
        }
        ProjectsCommand::Show { key } => print(&resolve_project(api, &key).await?, output),
    }
    Ok(0)
}

/// 人間向けの 1 行: `{name}\t{color}\t{description}`。
/// API は名前と説明の改行・タブを拒まないので、1 件 1 行・タブ区切りの 3 列を崩さないよう
/// どちらも改行とタブを空白に畳む。元の文字列は `--json` で取れる。
fn format_label_line(label: &LabelResponse) -> String {
    format!(
        "{}\t{}\t{}",
        single_line(&label.name),
        label.color,
        single_line(&label.description)
    )
}

fn single_line(text: &str) -> String {
    text.split(['\r', '\n', '\t'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn label(description: &str) -> LabelResponse {
        LabelResponse {
            id: Uuid::nil(),
            name: "bug".to_string(),
            description: description.to_string(),
            color: "#112233".to_string(),
            icon_url: None,
            project_id: None,
        }
    }

    #[test]
    fn formats_a_label_as_name_color_and_description() {
        assert_eq!(
            format_label_line(&label("壊れている")),
            "bug\t#112233\t壊れている"
        );
    }

    #[test]
    fn keeps_a_label_with_a_multiline_description_on_one_line() {
        assert_eq!(
            format_label_line(&label("1 行目\n2 行目\r\n3 行目")),
            "bug\t#112233\t1 行目 2 行目 3 行目"
        );
        assert_eq!(format_label_line(&label("")), "bug\t#112233\t");
    }

    #[test]
    fn folds_tabs_and_newlines_in_the_name_and_description_into_spaces() {
        let mut l = label("a\tb");
        l.name = "bug\nurgent\tnow".to_string();
        assert_eq!(format_label_line(&l), "bug urgent now\t#112233\ta b");
    }
}
