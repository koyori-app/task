//! 応答に載せる列挙値を、知らない値でも読めるように受ける型。
//!
//! CLI は payload の型でそのまま応答を読むので、サーバーが列挙に値を足すと、配布済みの CLI は
//! その値を含む応答を丸ごと読めなくなる（`unknown variant`）。応答の列挙値をこれで包むと、
//! 知らない値は文字列のまま受け、ほかの欄は読める。直列化は中身そのままなので JSON の形も
//! OpenAPI（各欄の `#[schema(value_type = …)]`）も変わらない。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrUnknown<T> {
    Known(T),
    /// この版が知らない値（新しいサーバーが足した値）
    Unknown(String),
}

impl<T> OrUnknown<T> {
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown(_) => None,
        }
    }
}

impl<T> From<T> for OrUnknown<T> {
    fn from(value: T) -> Self {
        Self::Known(value)
    }
}

impl<T: PartialEq> PartialEq<T> for OrUnknown<T> {
    fn eq(&self, other: &T) -> bool {
        self.known() == Some(other)
    }
}

impl<T: fmt::Display> fmt::Display for OrUnknown<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(value) => value.fmt(f),
            Self::Unknown(raw) => f.write_str(raw),
        }
    }
}

impl<T: Serialize> Serialize for OrUnknown<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(value) => value.serialize(serializer),
            Self::Unknown(raw) => raw.serialize(serializer),
        }
    }
}

impl<'de, T: FromStr> Deserialize<'de> for OrUnknown<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(raw.parse().map(Self::Known).unwrap_or(Self::Unknown(raw)))
    }
}

#[cfg(test)]
mod tests {
    use entity::review_findings::FindingState;

    use super::OrUnknown;
    use crate::reviews::SeverityStateCount;

    /// 新しいサーバーが足した状態を含む応答も、知らない値を文字列のまま残して読める。
    #[test]
    fn unknown_values_are_kept_as_strings() {
        let actions: Vec<OrUnknown<FindingState>> =
            serde_json::from_str(r#"["fixed", "brand-new"]"#).expect("deserialize");
        assert_eq!(
            actions,
            vec![
                OrUnknown::Known(FindingState::Fixed),
                OrUnknown::Unknown("brand-new".into()),
            ]
        );
        assert_eq!(actions[1].as_str(), "brand-new");

        let count: SeverityStateCount =
            serde_json::from_str(r#"{"severity": "high", "state": "brand-new", "count": 2}"#)
                .expect("deserialize count");
        assert_eq!(count.state, OrUnknown::Unknown("brand-new".into()));
        assert_eq!(count.count, 2);
    }

    /// 直列化は中身そのまま。包んでも JSON の形は変わらない。
    #[test]
    fn serializes_as_the_plain_value() {
        let known = serde_json::to_value(OrUnknown::Known(FindingState::Open)).expect("serialize");
        assert_eq!(known, serde_json::json!("open"));
        let unknown = serde_json::to_value(OrUnknown::<FindingState>::Unknown("x".into()))
            .expect("serialize");
        assert_eq!(unknown, serde_json::json!("x"));
    }
}
