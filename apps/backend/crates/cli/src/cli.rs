//! コマンド定義。引数と既定値は TypeScript 版の commander 定義をそのまま移した。

use clap::{Parser, Subcommand};

// clap の help はコンパイル時のリテラルなので、entity の値をそのまま埋められない
// （`concat!` はリテラルしか受け取らないため定数にもできない）。写しになるぶん、
// 下の `value_hints_match_the_entity` で entity と一致することを固定する。
macro_rules! priority_hint {
    () => {
        "critical_fire, critical, high, medium, low, trivial"
    };
}

macro_rules! sprint_status_hint {
    () => {
        "planning, active, completed"
    };
}

#[derive(Debug, Parser)]
// 版はビルド時に決まる（`build.rs`。タグからのリリースではタグの版になる）
#[command(name = "task", version = env!("TASK_CLI_VERSION"), about = "Task management CLI")]
pub struct Cli {
    /// Output JSON. `tasks list` also takes fields: --json key,title,status
    #[arg(
        long,
        global = true,
        value_name = "FIELDS",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = ""
    )]
    pub json: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

/// `--json` の指定。項目なしの `--json` は今までどおり全部を出す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonRequest {
    pub enabled: bool,
    /// `--json key,title` の項目。`tasks list` だけが受ける
    pub fields: Option<Vec<String>>,
}

/// 項目を選べるコマンド。位置引数を持たないものだけを入れる（`--json` の次の語を項目と決められる）。
/// 位置引数を持つコマンドで `--json key` と書くと、`key` が位置引数か項目か決められない。
const FIELD_COMMANDS: [[&str; 2]; 1] = [["tasks", "list"]];

/// `tasks list --json key,title` を `--json=key,title` に直す。
///
/// `--json` の値は `=` 付きだけを clap に渡す（`require_equals`）。値なしの `--json` が後ろの
/// 位置引数やサブコマンドを食わないようにするため（`task --json projects list`、
/// `task tasks show --json TASK-1` は今までどおり）。空白区切りは、位置引数の無い
/// `FIELD_COMMANDS` に限り、ここで `=` 付きにまとめる。ほかは一切変えない。
pub fn normalize_json_fields(args: Vec<String>) -> Vec<String> {
    // `-` で始まらない最初の 2 語がコマンドの道筋（`task tasks list`）
    let path: Vec<(usize, &String)> = args
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, arg)| !arg.starts_with('-'))
        .take(2)
        .collect();
    let [(_, first), (end, second)] = path[..] else {
        return args;
    };
    if !FIELD_COMMANDS
        .iter()
        .any(|[a, b]| a == first && b == second)
    {
        return args;
    }
    // コマンドの道筋より後ろの `--json` だけを見る
    let mut out = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        if index > end
            && args[index] == "--json"
            && let Some(next) = args.get(index + 1).filter(|next| !next.starts_with('-'))
        {
            out.push(format!("--json={next}"));
            index += 2;
            continue;
        }
        out.push(args[index].clone());
        index += 1;
    }
    out
}

impl Cli {
    /// コマンドラインを解析する。`--json` の空白区切りの項目を、clap に渡す前に直す。
    pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Self, clap::Error> {
        Self::try_parse_from(normalize_json_fields(args.into_iter().collect()))
    }

    pub fn json_request(&self) -> crate::error::Result<JsonRequest> {
        let Some(value) = &self.json else {
            return Ok(JsonRequest {
                enabled: false,
                fields: None,
            });
        };
        if value.is_empty() {
            return Ok(JsonRequest {
                enabled: true,
                fields: None,
            });
        }
        let fields: Vec<String> = value
            .split(',')
            .map(|field| field.trim().to_string())
            .collect();
        if fields.iter().any(String::is_empty) {
            return Err(crate::error::CliError::validation(
                "--json: empty field name",
            ));
        }
        Ok(JsonRequest {
            enabled: true,
            fields: Some(fields),
        })
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Authentication commands
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Manage ~/.config/task/config.yaml
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Project commands
    Projects {
        #[command(subcommand)]
        command: ProjectsCommand,
    },
    /// Task commands
    Tasks {
        // 一番大きい派生（`Update` の項目群）に enum 全体の大きさが引きずられるので箱に入れる
        #[command(subcommand)]
        command: Box<TasksCommand>,
    },
    /// My Tasks commands
    My {
        #[command(subcommand)]
        command: MyCommand,
    },
    /// Sprint commands
    Sprints {
        #[command(subcommand)]
        command: SprintsCommand,
    },
    /// Review findings commands
    Review {
        #[command(subcommand)]
        command: ReviewCommand,
    },
    /// Notification commands
    Notifications {
        #[command(subcommand)]
        command: NotificationsCommand,
    },
    /// Tenant commands
    Tenants {
        #[command(subcommand)]
        command: TenantsCommand,
    },
    /// Replace this binary with the latest release (verifies the SHA-256)
    SelfUpdate,
}

#[derive(Debug, Subcommand)]
pub enum TenantsCommand {
    /// List the tenants this token can be used in (needs the read:tenant scope)
    List,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Show the personal access token in use and its holder (tenant_ids lists the tenants it can be used in)
    Whoami,
    /// Save personal access token to config
    Token {
        /// Token value (omit to read from stdin)
        token: Option<String>,
    },
    /// Remove token from local config
    Logout,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// List all config values
    List,
    /// Get a config value
    Get {
        /// api_url | token | tenant_id
        key: String,
    },
    /// Set a config value
    Set {
        /// api_url | token | tenant_id
        key: String,
        /// Value to store
        value: String,
    },
    /// Remove a config value
    Unset {
        /// api_url | token | tenant_id
        key: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectsCommand {
    /// List projects
    List,
    /// List the statuses a project's tasks can be in
    Statuses {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// List the labels a project's tasks can have (name, color, description)
    Labels {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// List the types a project's tasks can have (name, color)
    Types {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Show a project by key or UUID
    Show {
        /// Project key or UUID
        key: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum TasksCommand {
    /// List tasks in a project
    List {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        #[arg(long, help = concat!("Filter by priority (", priority_hint!(), ")"))]
        priority: Option<String>,
        /// Filter by status name
        #[arg(long)]
        status: Option<String>,
        /// Filter by label name
        #[arg(long)]
        label: Option<String>,
        /// Filter by type name (or UUID)
        #[arg(long = "type")]
        task_type: Option<String>,
        /// Filter by assignee username (or UUID)
        #[arg(long)]
        assignee: Option<String>,
        /// Only tasks nobody is assigned to
        #[arg(long, conflicts_with = "assignee")]
        unassigned: bool,
        /// Filter by milestone name (or UUID)
        #[arg(long)]
        milestone: Option<String>,
        /// Filter by sprint name (or UUID)
        #[arg(long)]
        sprint: Option<String>,
        /// Filter by parent task (KEY-N or UUID)
        #[arg(long)]
        parent: Option<String>,
        /// Only tasks planned to start on or after this date (YYYY-MM-DD)
        #[arg(long, value_name = "YYYY-MM-DD")]
        planned_start_from: Option<chrono::NaiveDate>,
        /// Only tasks planned to start on or before this date (YYYY-MM-DD)
        #[arg(long, value_name = "YYYY-MM-DD")]
        planned_start_to: Option<chrono::NaiveDate>,
        /// List archived tasks instead of active ones
        #[arg(long)]
        archived: bool,
        /// created_at_desc | priority_asc | deadline_asc
        #[arg(long)]
        sort: Option<String>,
        /// Tasks per page (1-200)
        #[arg(long, default_value_t = 50)]
        limit: u64,
        /// Page number, 1-based
        #[arg(long, default_value_t = 1)]
        page: u64,
    },
    /// Search tasks by text
    Search {
        /// Search query
        query: String,
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// Hits per page (1-100)
        #[arg(long, default_value_t = 20)]
        limit: u64,
        /// Page number, 1-based
        #[arg(long, default_value_t = 1)]
        page: u64,
    },
    /// Create a task
    Create {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// Task title
        #[arg(long)]
        title: String,
        /// Task description (must not be empty)
        #[arg(long)]
        description: Option<String>,
        /// Read the description from a file (`-` for stdin; an empty file is refused)
        #[arg(long, conflicts_with = "description")]
        description_file: Option<String>,
        #[arg(long, help = concat!("Task priority. Accepted values: ", priority_hint!()))]
        priority: Option<String>,
        /// Status name
        #[arg(long)]
        status: Option<String>,
        #[command(flatten)]
        fields: TaskFieldArgs,
    },
    /// Create many tasks at once from a JSON array (all or none)
    ///
    /// Each element takes the same names and values as `tasks create`: title, description,
    /// priority, status, labels, assignees, parent, soft_deadline, hard_deadline,
    /// planned_start, estimate, progress, milestone, sprint, type. Every name is resolved before
    /// anything is sent; the first one that cannot be resolved stops the command with
    /// `tasks[i]: ...` (i is 0-based).
    BulkCreate {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// JSON file with an array of tasks, or - for stdin
        file: String,
        /// Status name for elements without `status` (default: the project's default status)
        #[arg(long)]
        status: Option<String>,
    },
    /// Show a task
    Show {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// Print the description as stored, keeping mention markers (`<@user-id>`) instead of
        /// `@name`; use it to edit the description and pass it back to `tasks update`
        #[arg(long)]
        raw: bool,
    },
    /// Update a task
    Update {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// New title
        #[arg(long)]
        title: Option<String>,
        /// New description (an empty value is refused; use --clear-description to remove it)
        #[arg(long)]
        description: Option<String>,
        /// Read the new description from a file (`-` for stdin; an empty file is refused)
        #[arg(long, conflicts_with = "description")]
        description_file: Option<String>,
        /// Status name
        #[arg(long)]
        status: Option<String>,
        #[arg(long, help = concat!("Priority (", priority_hint!(), ")"))]
        priority: Option<String>,
        #[command(flatten)]
        fields: TaskFieldArgs,
        #[command(flatten)]
        clears: TaskClearArgs,
        /// Move the task into the archive
        #[arg(long)]
        archive: bool,
        /// Bring the task back from the archive
        #[arg(long, conflicts_with = "archive")]
        unarchive: bool,
    },
    /// Mark a task as done
    Complete {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Add a comment to a task (posting only; to read comments use `tasks comments`)
    Comment {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Comment body (omit to read from stdin)
        body: Option<String>,
        /// Read the comment body from a file (`-` for stdin)
        #[arg(long, conflicts_with = "body")]
        body_file: Option<String>,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// List the comments of a task, oldest first (replies are indented under their parent)
    Comments {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Delete a task
    Delete {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Pull requests linked to a task
    Pr {
        #[command(subcommand)]
        command: TaskPrCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum TaskPrCommand {
    /// List the pull requests linked to a task, and the project's repository
    List {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Link a pull request of the project's GitHub repository to a task
    Link {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Pull request number
        #[arg(value_parser = clap::value_parser!(i32).range(1..))]
        number: i32,
        /// Complete the task when the pull request is merged
        #[arg(long)]
        closes: bool,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Unlink a pull request from a task (removes the link from every source)
    Unlink {
        /// Task ref (KEY-N or UUID)
        task_ref: String,
        /// Pull request number
        #[arg(value_parser = clap::value_parser!(i32).range(1..))]
        number: i32,
        /// Project key when using UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
}

/// 作成と更新で同じ綴りにする項目。片方だけ増えると使う側が覚え直しになる。
#[derive(Debug, clap::Args)]
pub struct TaskFieldArgs {
    /// Soft deadline (RFC 3339, or YYYY-MM-DD for the start of that day in UTC)
    #[arg(long)]
    pub soft_deadline: Option<String>,
    /// Hard deadline (RFC 3339, or YYYY-MM-DD for the start of that day in UTC)
    #[arg(long)]
    pub hard_deadline: Option<String>,
    /// Planned start date (YYYY-MM-DD)
    #[arg(long, value_name = "YYYY-MM-DD")]
    pub planned_start: Option<chrono::NaiveDate>,
    /// Estimate in minutes (1 or more)
    #[arg(long)]
    pub estimate: Option<i32>,
    /// Progress percentage (0-100)
    #[arg(long)]
    pub progress: Option<i16>,
    /// Parent task (KEY-N or UUID). A new task takes the parent's deadlines and planned start unless set here
    #[arg(long)]
    pub parent: Option<String>,
    /// Milestone name (or UUID)
    #[arg(long)]
    pub milestone: Option<String>,
    /// Sprint name (or UUID)
    #[arg(long)]
    pub sprint: Option<String>,
    /// Type name (or UUID)
    #[arg(long = "type")]
    pub task_type: Option<String>,
    /// Label name; repeat to set several (replaces the current labels)
    #[arg(long = "label")]
    pub labels: Vec<String>,
    /// Label to add, keeping the others; repeat to add several
    #[arg(long = "add-label", conflicts_with = "labels")]
    pub add_labels: Vec<String>,
    /// Label to remove, keeping the others; repeat to remove several
    #[arg(long = "remove-label", conflicts_with = "labels")]
    pub remove_labels: Vec<String>,
    /// Assignee username; repeat to set several (replaces the current assignees)
    #[arg(long = "assignee")]
    pub assignees: Vec<String>,
}

/// 明示的な解除。値の指定と混ぜられないよう `conflicts_with` を張る。
#[derive(Debug, clap::Args)]
pub struct TaskClearArgs {
    /// Remove the description (the only way to empty it)
    #[arg(long, conflicts_with_all = ["description", "description_file"])]
    pub clear_description: bool,
    /// Remove the soft deadline
    #[arg(long, conflicts_with = "soft_deadline")]
    pub clear_soft_deadline: bool,
    /// Remove the hard deadline
    #[arg(long, conflicts_with = "hard_deadline")]
    pub clear_hard_deadline: bool,
    /// Remove the planned start date
    #[arg(long, conflicts_with = "planned_start")]
    pub clear_planned_start: bool,
    /// Remove the estimate
    #[arg(long, conflicts_with = "estimate")]
    pub clear_estimate: bool,
    /// Detach from the parent task
    #[arg(long, conflicts_with = "parent")]
    pub clear_parent: bool,
    /// Detach from the milestone
    #[arg(long, conflicts_with = "milestone")]
    pub clear_milestone: bool,
    /// Detach from the sprint
    #[arg(long, conflicts_with = "sprint")]
    pub clear_sprint: bool,
    /// Remove the type
    #[arg(long, conflicts_with = "task_type")]
    pub clear_type: bool,
    /// Remove every assignee
    #[arg(long, conflicts_with = "assignees")]
    pub clear_assignees: bool,
}

#[derive(Debug, Subcommand)]
pub enum MyCommand {
    /// List tasks assigned to me
    List {
        /// today | week | no_due_date | overdue | all
        #[arg(long, default_value = "all")]
        filter: String,
        /// Only tasks that are not done
        #[arg(long)]
        open: bool,
        /// Tasks per page (1-200)
        #[arg(long, default_value_t = 50)]
        limit: u64,
        /// Page number, 1-based
        #[arg(long, default_value_t = 1)]
        page: u64,
    },
    /// Complete a personal or assigned task by ref (e.g. ME-3)
    Complete {
        /// Task ref (ME-N, KEY-N, or UUID)
        task_ref: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum SprintsCommand {
    /// List sprints
    List {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        #[arg(long, help = concat!("Filter by sprint status (", sprint_status_hint!(), ")"))]
        status: Option<String>,
    },
    /// Show sprint details
    Show {
        /// Sprint UUID
        id: String,
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Start a sprint
    Start {
        /// Sprint UUID
        id: String,
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
    /// Complete a sprint
    Complete {
        /// Sprint UUID
        id: String,
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// Move incomplete tasks to backlog
        #[arg(long)]
        backlog: bool,
    },
    /// Show sprint burndown data
    Burndown {
        /// Sprint UUID or name
        id: String,
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum NotificationsCommand {
    /// List my notifications (newest first; read state does not affect the order)
    List {
        /// Only unread notifications
        #[arg(long)]
        unread: bool,
        /// Notifications to fetch (1-100)
        #[arg(long, default_value_t = 50)]
        limit: u64,
    },
    /// Mark one notification as read
    Read {
        /// Notification UUID
        id: String,
    },
    /// Mark every visible notification as read
    ReadAll,
    /// Show or update the notification settings of a project
    Settings {
        /// Project key or UUID (a key also needs read:project to look up; default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// In-app event types, comma separated (replaces the current list)
        #[arg(long = "in-app", value_name = "types")]
        in_app: Option<String>,
        /// Email event types, comma separated (replaces the current list)
        #[arg(long, value_name = "types")]
        email: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ReviewCommand {
    /// Submit one review round (findings are created together)
    Submit {
        /// JSON file with the round, or - for stdin
        file: String,
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// PR number (overrides `pr` in the JSON)
        #[arg(long)]
        pr: Option<String>,
    },
    /// Carry unresolved findings over from another PR (e.g. after re-opening a merged PR)
    CarryOver {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// PR number to carry findings from
        #[arg(long = "from-pr")]
        from_pr: String,
        /// PR number to carry findings to (a PR of the current integration)
        #[arg(long = "to-pr")]
        to_pr: String,
        /// Repository of the source PR (default: the current integration; "" for rounds recorded before it)
        #[arg(long = "from-repo", value_name = "owner/name")]
        from_repo: Option<String>,
    },
    /// List findings for a PR
    List {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// PR number
        #[arg(long)]
        pr: String,
        /// Repository to read (default: the current integration; "" for rounds recorded before it)
        #[arg(long, value_name = "owner/name")]
        repo: Option<String>,
        /// Filter by state (open,fixing,fixed,verified,deferred,rejected)
        #[arg(long, value_name = "states")]
        state: Option<String>,
        /// Filter by severity (high,medium,low,nit)
        #[arg(long, value_name = "severities")]
        severity: Option<String>,
    },
    /// List review rounds for a PR
    Rounds {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// PR number
        #[arg(long)]
        pr: String,
        /// Repository to read (default: the current integration; "" for rounds recorded before it)
        #[arg(long, value_name = "owner/name")]
        repo: Option<String>,
    },
    /// Move a finding to a new state
    Resolve {
        /// Finding UUID
        id: String,
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// New state (open,fixing,fixed,verified,deferred,rejected)
        #[arg(long)]
        state: String,
        /// Why the state changed (kept in the history)
        #[arg(long, value_name = "text")]
        note: Option<String>,
    },
    /// Edit the title, body, or location of a finding you submitted, while it is still open.
    /// The severity cannot be changed; reject the finding and submit it again instead
    Edit {
        /// Finding UUID
        id: String,
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// New title
        #[arg(long)]
        title: Option<String>,
        /// New body (markdown)
        #[arg(long, conflicts_with = "body_file")]
        body: Option<String>,
        /// Read the new body from a file (`-` for stdin)
        #[arg(long, value_name = "path")]
        body_file: Option<String>,
        /// New file of the location
        #[arg(long, conflicts_with = "clear_file")]
        file: Option<String>,
        /// Remove the file of the location
        #[arg(long)]
        clear_file: bool,
        /// New line of the location (1 or greater)
        #[arg(long, conflicts_with = "clear_line")]
        line: Option<String>,
        /// Remove the line of the location
        #[arg(long)]
        clear_line: bool,
    },
    /// Show the merge verdict for a PR (exits 1 unless it is reviewed, clean, and up to date)
    Summary {
        /// Project key or UUID (default: `project` in .koyori-task.yaml)
        #[arg(long)]
        project: Option<String>,
        /// PR number
        #[arg(long)]
        pr: String,
        /// Repository to read (default: the current integration; "" for rounds recorded before it)
        #[arg(long, value_name = "owner/name")]
        repo: Option<String>,
        /// Commit to compare with the reviewed HEAD (default: git rev-parse HEAD)
        #[arg(long, value_name = "sha")]
        head: Option<String>,
        /// Do not compare the reviewed HEAD with the working tree
        #[arg(long = "no-head-check")]
        no_head_check: bool,
        /// Accept a summary from a project without a GitHub integration
        #[arg(long)]
        allow_unlinked: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// help に並べた候補は entity の写し。entity 側が増減したらここで落として気付く。
    #[test]
    fn value_hints_match_the_entity() {
        use entity::sprints::SprintStatus;
        use entity::tasks::TaskPriority;
        use sea_orm::ActiveEnum;

        assert_eq!(priority_hint!(), TaskPriority::values().join(", "));
        assert_eq!(sprint_status_hint!(), SprintStatus::values().join(", "));
    }

    #[test]
    fn the_command_tree_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn accepts_the_json_flag_before_or_after_the_subcommand() {
        for argv in [
            vec!["task", "--json", "projects", "list"],
            vec!["task", "projects", "list", "--json"],
        ] {
            let cli = Cli::try_parse_from(argv).unwrap();
            assert!(cli.json.is_some());
        }
    }

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn json_fields_are_joined_to_the_flag_only_for_commands_that_take_them() {
        for (input, expected) in [
            (
                &[
                    "task",
                    "tasks",
                    "list",
                    "--project",
                    "APP",
                    "--json",
                    "key,title",
                ][..],
                &[
                    "task",
                    "tasks",
                    "list",
                    "--project",
                    "APP",
                    "--json=key,title",
                ][..],
            ),
            (
                &[
                    "task",
                    "tasks",
                    "list",
                    "--json",
                    "key,title",
                    "--project",
                    "APP",
                ][..],
                &[
                    "task",
                    "tasks",
                    "list",
                    "--json=key,title",
                    "--project",
                    "APP",
                ][..],
            ),
            // 行末、または次がフラグなら項目なし
            (
                &["task", "tasks", "list", "--json"][..],
                &["task", "tasks", "list", "--json"][..],
            ),
            (
                &["task", "tasks", "list", "--json", "--project", "APP"][..],
                &["task", "tasks", "list", "--json", "--project", "APP"][..],
            ),
            // コマンドの前の `--json` と、位置引数を持つコマンドは変えない
            (
                &["task", "--json", "tasks", "list"][..],
                &["task", "--json", "tasks", "list"][..],
            ),
            (
                &["task", "tasks", "show", "--json", "APP-1"][..],
                &["task", "tasks", "show", "--json", "APP-1"][..],
            ),
            (
                &["task", "tasks", "comment", "APP-1", "--json", "body"][..],
                &["task", "tasks", "comment", "APP-1", "--json", "body"][..],
            ),
            (
                &["task", "projects", "list", "--json", "x"][..],
                &["task", "projects", "list", "--json", "x"][..],
            ),
        ] {
            assert_eq!(
                normalize_json_fields(args(input)),
                args(expected),
                "{input:?}"
            );
        }
    }

    #[test]
    fn the_json_request_separates_a_bare_flag_from_fields() {
        let request = |words: &[&str]| Cli::parse_args(args(words)).unwrap().json_request();
        let fields = request(&[
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--json",
            "key,title",
        ])
        .unwrap();
        assert!(fields.enabled);
        assert_eq!(
            fields.fields,
            Some(vec!["key".to_string(), "title".to_string()])
        );

        let bare = request(&["task", "tasks", "list", "--project", "APP", "--json"]).unwrap();
        assert!(bare.enabled);
        assert_eq!(bare.fields, None);

        let none = request(&["task", "tasks", "list", "--project", "APP"]).unwrap();
        assert!(!none.enabled);

        let err = request(&[
            "task",
            "tasks",
            "list",
            "--project",
            "APP",
            "--json=key,,title",
        ])
        .unwrap_err();
        assert_eq!(err.exit_code, 2);
    }

    /// 値なしの `--json` は、後ろのサブコマンドや位置引数を食わない。
    #[test]
    fn a_bare_json_flag_does_not_swallow_the_next_word() {
        let cli = Cli::parse_args(args(&["task", "--json", "projects", "list"])).unwrap();
        assert_eq!(cli.json.as_deref(), Some(""));
        assert!(matches!(cli.command, Command::Projects { .. }));
        let cli = Cli::parse_args(args(&["task", "tasks", "show", "--json", "APP-1"])).unwrap();
        assert_eq!(cli.json.as_deref(), Some(""));
        let Command::Tasks { command } = cli.command else {
            panic!("expected tasks");
        };
        assert!(matches!(*command, TasksCommand::Show { ref task_ref, .. } if task_ref == "APP-1"));
    }

    #[test]
    fn tasks_comments_takes_a_task_ref() {
        assert!(Cli::try_parse_from(["task", "tasks", "comments", "APP-1"]).is_ok());
        assert!(Cli::try_parse_from(["task", "tasks", "comments"]).is_err());
    }

    #[test]
    fn tenants_list_is_a_command() {
        assert!(Cli::try_parse_from(["task", "tenants", "list"]).is_ok());
    }

    #[test]
    fn head_check_defaults_to_on_and_is_removed_only_by_the_explicit_flag() {
        let on =
            Cli::try_parse_from(["task", "review", "summary", "--project", "APP", "--pr", "1"])
                .unwrap();
        let off = Cli::try_parse_from([
            "task",
            "review",
            "summary",
            "--project",
            "APP",
            "--pr",
            "1",
            "--no-head-check",
        ])
        .unwrap();

        let flag = |cli: Cli| match cli.command {
            Command::Review {
                command: ReviewCommand::Summary { no_head_check, .. },
            } => no_head_check,
            _ => unreachable!(),
        };
        assert!(!flag(on));
        assert!(flag(off));
    }

    #[test]
    fn requires_the_pr_option_and_accepts_a_missing_project() {
        // `--project` は `.koyori-task.yaml` から補えるので解析では求めない（無ければ実行時に終了 2）
        assert!(Cli::try_parse_from(["task", "review", "list", "--pr", "1"]).is_ok());
        assert!(Cli::try_parse_from(["task", "review", "list", "--project", "APP"]).is_err());
    }

    /// 本文の指定と解除を同時に受けると、API が解除を優先して渡した本文が黙って消える。
    #[test]
    fn refuses_a_new_description_together_with_clearing_it() {
        let base = ["task", "tasks", "update", "APP-1"];
        for value in [
            vec!["--description", "new"],
            vec!["--description-file", "body.md"],
        ] {
            let argv: Vec<&str> = base
                .iter()
                .copied()
                .chain(value)
                .chain(["--clear-description"])
                .collect();
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?}");
        }
        // 解除だけ、本文だけならそれぞれ通る
        assert!(
            Cli::try_parse_from(["task", "tasks", "update", "APP-1", "--clear-description"])
                .is_ok()
        );
        assert!(
            Cli::try_parse_from(["task", "tasks", "update", "APP-1", "--description", "new"])
                .is_ok()
        );
    }

    /// 担当者の解除も値の指定と混ぜられない。混ざると「置き換え」と「全員外す」の
    /// どちらが勝つかが綴りから読めなくなる。
    #[test]
    fn rejects_clearing_and_setting_assignees_together() {
        assert!(
            Cli::try_parse_from([
                "task",
                "tasks",
                "update",
                "APP-1",
                "--assignee",
                "alice",
                "--clear-assignees",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from(["task", "tasks", "update", "APP-1", "--clear-assignees"]).is_ok()
        );
        assert!(
            Cli::try_parse_from(["task", "tasks", "update", "APP-1", "--assignee", "alice"])
                .is_ok()
        );
    }

    /// `--unassigned` は `--assignee` と同時に指定できない。
    #[test]
    fn rejects_unassigned_with_an_assignee_filter() {
        let base = ["task", "tasks", "list", "--project", "APP"];
        let with = |extra: &[&str]| Cli::try_parse_from(base.iter().chain(extra.iter()).copied());
        assert!(with(&["--assignee", "alice", "--unassigned"]).is_err());
        assert!(with(&["--unassigned"]).is_ok());
        assert!(with(&["--assignee", "alice"]).is_ok());
    }

    /// 開始予定日は日付だけを受け、解除と同時には指定できない。
    #[test]
    fn parses_a_planned_start_date_and_refuses_it_with_its_clear() {
        let base = ["task", "tasks", "update", "APP-1"];
        let parse = |extra: &[&str]| {
            Cli::try_parse_from(base.iter().chain(extra).copied().collect::<Vec<_>>())
        };
        let cli = parse(&["--planned-start", "2026-10-16"]).unwrap();
        let Command::Tasks { command, .. } = cli.command else {
            panic!("expected tasks");
        };
        let TasksCommand::Update { fields, .. } = *command else {
            panic!("expected tasks update");
        };
        assert_eq!(
            fields.planned_start,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 16)
        );
        assert!(parse(&["--planned-start", "2026-10-16T00:00:00Z"]).is_err());
        assert!(parse(&["--planned-start", "2026-10-16", "--clear-planned-start"]).is_err());
        assert!(parse(&["--clear-planned-start"]).is_ok());
    }

    /// API は 0 以下の番号を 400 で返すだけなので、引数の段階で断る。
    #[test]
    fn takes_only_a_positive_pull_request_number() {
        for action in ["link", "unlink"] {
            for bad in ["0", "-1", "abc"] {
                assert!(
                    Cli::try_parse_from(["task", "tasks", "pr", action, "APP-1", bad]).is_err(),
                    "{action} {bad}"
                );
            }
            assert!(Cli::try_parse_from(["task", "tasks", "pr", action, "APP-1", "1"]).is_ok());
        }
    }

    #[test]
    fn keeps_an_empty_repo_filter_as_a_value() {
        let cli = Cli::try_parse_from([
            "task",
            "review",
            "rounds",
            "--project",
            "APP",
            "--pr",
            "1",
            "--repo",
            "",
        ])
        .unwrap();
        match cli.command {
            Command::Review {
                command: ReviewCommand::Rounds { repo, .. },
            } => {
                assert_eq!(repo.as_deref(), Some(""));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn tasks_show_takes_raw_flag() {
        let raw_of = |argv: &[&str]| {
            let Command::Tasks { command, .. } = Cli::try_parse_from(argv).unwrap().command else {
                panic!("expected tasks");
            };
            let TasksCommand::Show { raw, .. } = *command else {
                panic!("expected tasks show");
            };
            raw
        };
        assert!(raw_of(&["task", "tasks", "show", "APP-1", "--raw"]));
        assert!(!raw_of(&["task", "tasks", "show", "APP-1"]));
    }
}
