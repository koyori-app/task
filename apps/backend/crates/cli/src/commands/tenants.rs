//! テナント。

use crate::Context;
use crate::cli::TenantsCommand;
use crate::commands::projects::single_line;
use crate::error::Result;
use crate::output::{OutputOptions, print};

pub async fn run(context: &Context, command: TenantsCommand, output: OutputOptions) -> Result<i32> {
    match command {
        TenantsCommand::List => {
            // 一覧はテナントを含まないパス。テナントを引かない（名前の指定に使う前段でもあるため）
            let api = context.connect_token_only()?;
            let mut tenants = api.list_tenants().await?;
            tenants.sort_by(|a, b| a.display_id.cmp(&b.display_id));
            if output.json {
                print(&tenants, output);
            } else {
                // 1 件 1 行の文字列にして渡す（オブジェクトのままでは整形 JSON が出る）
                let lines: Vec<String> = tenants
                    .iter()
                    .map(|tenant| {
                        format!(
                            "{}\t{}\t{}",
                            tenant.id,
                            tenant.display_id,
                            single_line(&tenant.name)
                        )
                    })
                    .collect();
                print(&lines, output);
            }
        }
    }
    Ok(0)
}
