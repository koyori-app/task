//! 通知イベントタイプの定数定義。
//!
//! DTO のバリデーションとハンドラー/ワーカーの双方から参照されるため、
//! 依存グラフの最下層である common に置く。

pub const TYPE_ASSIGNED: &str = "assigned";
pub const TYPE_MENTIONED: &str = "mentioned";
pub const TYPE_STATUS_CHANGED: &str = "status_changed";
pub const TYPE_COMMENT_ADDED: &str = "comment_added";
pub const TYPE_REVIEW_ROUND_CREATED: &str = "review_round_created";
pub const TYPE_REVIEW_FINDING_CHANGED: &str = "review_finding_changed";
/// プロジェクトの全ラウンドを購読する印。通知種別ではなく受信者の選び方にだけ効く
/// （直接の関係者でない人にも `review_round_created` を届ける。既定 OFF）。
pub const TYPE_REVIEW_ROUND_ANY: &str = "review_round_any";

/// バリデーションに使用する既知のイベントタイプ一覧。
/// `pr_merged` / `deadline_soon` はまだハンドラ未実装だが将来の拡張のため登録済み。
pub const KNOWN_EVENT_TYPES: &[&str] = &[
    TYPE_ASSIGNED,
    TYPE_MENTIONED,
    TYPE_STATUS_CHANGED,
    TYPE_COMMENT_ADDED,
    "deadline_soon",
    "pr_merged",
    TYPE_REVIEW_ROUND_CREATED,
    TYPE_REVIEW_FINDING_CHANGED,
    TYPE_REVIEW_ROUND_ANY,
];

pub const DEFAULT_IN_APP_EVENTS: &[&str] = &[
    TYPE_ASSIGNED,
    TYPE_MENTIONED,
    TYPE_STATUS_CHANGED,
    "deadline_soon",
    TYPE_COMMENT_ADDED,
    "pr_merged",
    // レビューの購読印（`review_round_any`）は既定に入れない。
    // 入れるとプロジェクト全員が全ラウンドを受け取る
    TYPE_REVIEW_ROUND_CREATED,
    TYPE_REVIEW_FINDING_CHANGED,
];
