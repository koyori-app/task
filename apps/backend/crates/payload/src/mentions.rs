//! コメント本文のメンション印 `<@user_id>` の取り扱い（純関数だけ。DB には触れぬ）。
//!
//! 本文には利用者の id を `<@xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx>` の形で埋める。名前は持たぬので、
//! 改名しても切れぬ。画面は印を `@今の名` の札に描き、本文が利用者の画面の外へ出る口
//! （webhook・CLI）は [`to_plain`] で印を名に戻してから出す。**印を名に戻す処理はここに一つだけ**
//! 置き、各々の口が同じ関数を通る（口ごとに書くと、戻し忘れの口から id が漏れる）。
//!
//! 拾うのは印だけである。名前の文字 `@alice` は拾わぬ（同名の二人や `bob@example.com` の
//! `example` を宛先にしてしまう誤りを持ち込まぬ）。

use std::collections::HashMap;

use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 印の長さ（`<@` + ハイフン付き UUID 36 字 + `>`）。
const MARKER_LEN: usize = 2 + 36 + 1;

/// 本文中の印を前から順に（開始位置, 終了位置, id）で返す。壊れた印・UUID でない物は飛ばす。
fn markers(body: &str) -> Vec<(usize, usize, Uuid)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + MARKER_LEN <= bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'@' && bytes[i + MARKER_LEN - 1] == b'>' {
            // 36 字がちょうどハイフン付きの UUID の形のときだけ印とする（`Uuid::parse_str` は
            // ハイフン無し・波括弧付き等も通すので、字数と位置で先に絞る）
            let id = &body[i + 2..i + MARKER_LEN - 1];
            let dashed = id.len() == 36
                && id
                    .bytes()
                    .enumerate()
                    .all(|(n, b)| matches!(n, 8 | 13 | 18 | 23) == (b == b'-'));
            if dashed && let Ok(uuid) = Uuid::parse_str(id) {
                out.push((i, i + MARKER_LEN, uuid));
                i += MARKER_LEN;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// 本文に出てくるメンションの id。初めて現れた順で、重複は一つにする。
pub fn extract_ids(body: &str) -> Vec<Uuid> {
    let mut seen = std::collections::HashSet::new();
    markers(body)
        .into_iter()
        .map(|(_, _, id)| id)
        .filter(|id| seen.insert(*id))
        .collect()
}

/// 本文のメンション印を `label(id)` で置き換えた文字列を返す。印以外の字には触れぬ。
pub fn replace_markers(body: &str, mut label: impl FnMut(Uuid) -> String) -> String {
    let found = markers(body);
    if found.is_empty() {
        return body.to_string();
    }
    let mut out = String::with_capacity(body.len());
    let mut last = 0;
    for (start, end, id) in found {
        out.push_str(&body[last..start]);
        out.push_str(&label(id));
        last = end;
    }
    out.push_str(&body[last..]);
    out
}

/// メンションされた者が、いま画面に出してよい状態か。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MentionStatus {
    /// いまそのプロジェクトに入れる者。名前とアイコンを返す。
    Member,
    /// 利用者は在るが、このプロジェクトに入れぬ者（外れた・テナントを抜けた）。
    /// 名前とアイコンは、そのテナントに属している・属しておった者（`service::mentions::tenant_footprint_ids`）
    /// の時だけ返す。印の id を知っただけで、別のテナントの利用者の名を引けぬようにするため。
    Outside,
    /// 利用者の行が無い者（削除。名前は引けぬ）。
    Gone,
}

/// 本文のメンションを描くための、id から引ける利用者の表示情報。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MentionedUser {
    pub status: MentionStatus,
    /// `@今の名` の「今の名」。`member` と、そのテナントに属している・属しておった `outside` で返す。
    /// 別のテナントの利用者・行の無い者は `null`。
    #[schema(nullable)]
    pub name: Option<String>,
    /// アイコンの URL。名前と同じ境で返す（`member` と、名前を返す `outside`）。それ以外と、
    /// アイコンを持たぬ者は `null`。
    #[schema(nullable)]
    pub avatar_url: Option<String>,
}

impl MentionedUser {
    /// 本文が利用者の画面の外へ出る時の、`@` から始まる表示。
    pub fn plain_label(&self) -> String {
        match (self.status, &self.name) {
            // 名が引ける者は、外れた者・退会した者も名で出す（灰色にするのは画面の札の側）
            (MentionStatus::Member | MentionStatus::Outside, Some(name)) => format!("@{name}"),
            (MentionStatus::Member | MentionStatus::Outside, None) => {
                "@（このプロジェクトの外の利用者）".to_string()
            }
            (MentionStatus::Gone, _) => "@（退会した利用者）".to_string(),
        }
    }
}

/// 本文に出る id から引ける表示情報の表。
pub type MentionMap = HashMap<Uuid, MentionedUser>;

/// 対応表に無い id の表示（表を作る側が引き漏らした時も id は出さぬ）。
const UNKNOWN_LABEL: &str = "@（不明な利用者）";

/// メンション印を `@今の名` に戻した本文。**本文が利用者の画面以外（webhook・CLI）へ出る口は
/// 全てこれを通す。** 表に無い id・名が引けぬ者は、名を伏せた定型の句にする。
pub fn to_plain(body: &str, map: &MentionMap) -> String {
    replace_markers(body, |id| {
        map.get(&id)
            .map(MentionedUser::plain_label)
            .unwrap_or_else(|| UNKNOWN_LABEL.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";

    fn id(s: &str) -> Uuid {
        Uuid::parse_str(s).unwrap()
    }

    #[test]
    fn extracts_marker_ids_in_order_without_duplicates() {
        let body = format!("<@{B}> と <@{A}> と <@{B}>");
        assert_eq!(extract_ids(&body), vec![id(B), id(A)]);
    }

    #[test]
    fn plain_at_names_are_not_mentions() {
        assert!(extract_ids("@alice bob@example.com <@alice>").is_empty());
    }

    #[test]
    fn broken_markers_are_ignored() {
        // 閉じ無し・字数違い・ハイフン無し・波括弧付き・UUID でない字
        for body in [
            format!("<@{A}"),
            format!("<@{}>", &A[..35]),
            format!("<@{}>", A.replace('-', "")),
            format!("<@{{{A}}}>"),
            format!("<@{}>", A.replace('1', "z")),
            format!("< @{A}>"),
        ] {
            assert!(extract_ids(&body).is_empty(), "{body}");
        }
    }

    #[test]
    fn marker_next_to_multibyte_text_is_found() {
        let body = format!("確認お願い<@{A}>さん。");
        assert_eq!(extract_ids(&body), vec![id(A)]);
        let map: MentionMap = [(
            id(A),
            MentionedUser {
                status: MentionStatus::Member,
                name: Some("alice".into()),
                avatar_url: None,
            },
        )]
        .into();
        assert_eq!(to_plain(&body, &map), "確認お願い@aliceさん。");
    }

    #[test]
    fn to_plain_replaces_every_marker_and_never_leaks_ids() {
        let body = format!("<@{A}> <@{B}> <@{A}> <@{}>", Uuid::nil());
        let map: MentionMap = [
            (
                id(A),
                MentionedUser {
                    status: MentionStatus::Member,
                    name: Some("alice".into()),
                    avatar_url: None,
                },
            ),
            (
                id(B),
                MentionedUser {
                    status: MentionStatus::Outside,
                    name: Some("former".into()),
                    avatar_url: None,
                },
            ),
        ]
        .into();
        let plain = to_plain(&body, &map);
        // 外れた者でも、名が引けておれば名で出る。表に無い id は定型の句
        assert_eq!(plain, "@alice @former @alice @（不明な利用者）");
        assert!(!plain.contains("<@") && !plain.contains(A));

        // 名が引けぬ外れた者（別のテナントの利用者）は名を伏せた定型の句
        let stranger: MentionMap = [(
            id(B),
            MentionedUser {
                status: MentionStatus::Outside,
                name: None,
                avatar_url: None,
            },
        )]
        .into();
        assert_eq!(
            to_plain(&format!("<@{B}>"), &stranger),
            "@（このプロジェクトの外の利用者）"
        );
    }

    #[test]
    fn gone_user_has_its_own_label() {
        let user = MentionedUser {
            status: MentionStatus::Gone,
            name: None,
            avatar_url: None,
        };
        assert_eq!(user.plain_label(), "@（退会した利用者）");
    }

    #[test]
    fn body_without_markers_is_unchanged() {
        assert_eq!(
            to_plain("ただの文 <b> @x", &MentionMap::new()),
            "ただの文 <b> @x"
        );
    }
}
