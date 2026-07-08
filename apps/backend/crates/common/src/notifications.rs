//! 通知イベントタイプの定数定義。
//!
//! DTO のバリデーションとハンドラー/ワーカーの双方から参照されるため、
//! 依存グラフの最下層である common に置く。

pub const TYPE_ASSIGNED: &str = "assigned";
pub const TYPE_MENTIONED: &str = "mentioned";
pub const TYPE_STATUS_CHANGED: &str = "status_changed";
pub const TYPE_COMMENT_ADDED: &str = "comment_added";

/// バリデーションに使用する既知のイベントタイプ一覧。
/// `pr_merged` / `deadline_soon` はまだハンドラ未実装だが将来の拡張のため登録済み。
pub const KNOWN_EVENT_TYPES: &[&str] = &[
    TYPE_ASSIGNED,
    TYPE_MENTIONED,
    TYPE_STATUS_CHANGED,
    TYPE_COMMENT_ADDED,
    "deadline_soon",
    "pr_merged",
];

pub const DEFAULT_IN_APP_EVENTS: &[&str] = &[
    TYPE_ASSIGNED,
    TYPE_MENTIONED,
    TYPE_STATUS_CHANGED,
    "deadline_soon",
    TYPE_COMMENT_ADDED,
    "pr_merged",
];
