//! Project-independent, infallible lexer for task list expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Status,
    Assignee,
    Label,
    Priority,
    Due,
    No,
    Is,
    In,
    Author,
    Sort,
}
impl Key {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.to_ascii_lowercase().as_str() {
            "status" => Self::Status,
            "assignee" => Self::Assignee,
            "label" => Self::Label,
            "priority" => Self::Priority,
            "due" => Self::Due,
            "no" => Self::No,
            "is" => Self::Is,
            "in" => Self::In,
            "author" => Self::Author,
            "sort" => Self::Sort,
            _ => return None,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Word {
        text: String,
        negated: bool,
    },
    Phrase {
        text: String,
        negated: bool,
    },
    Qualifier {
        key: Key,
        values: Vec<String>,
        negated: bool,
        raw: String,
    },
}

fn separator(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{3000}')
}
fn peek(input: &str, at: usize) -> Option<char> {
    input[at..].chars().next()
}
fn phrase(input: &str, at: &mut usize) -> String {
    *at += 1;
    let mut text = String::new();
    while let Some(c) = peek(input, *at) {
        *at += c.len_utf8();
        if c == '"' {
            break;
        }
        if c == '\\' && matches!(peek(input, *at), Some('"' | '\\')) {
            let escaped = peek(input, *at).expect("checked escape");
            text.push(escaped);
            *at += escaped.len_utf8();
        } else {
            text.push(c);
        }
    }
    text
}
pub fn lex(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut at = 0;
    while let Some(c) = peek(input, at) {
        if separator(c) {
            at += c.len_utf8();
            continue;
        }
        let start = at;
        let negated = c == '-' && peek(input, at + 1).is_some_and(|c| !separator(c));
        if negated {
            at += 1;
        }
        if peek(input, at) == Some('"') {
            tokens.push(Token::Phrase {
                text: phrase(input, &mut at),
                negated,
            });
            continue;
        }
        let body_start = at;
        let key_end = input[at..]
            .char_indices()
            .take_while(|(_, c)| !separator(*c) && *c != ':' && *c != '"')
            .last()
            .map_or(at, |(offset, c)| at + offset + c.len_utf8());
        let key = if peek(input, key_end) == Some(':') {
            Key::parse(&input[at..key_end])
        } else {
            None
        };
        if let Some(key) = key {
            at = key_end + 1;
            let mut values = Vec::new();
            loop {
                let value = if peek(input, at) == Some('"') {
                    phrase(input, &mut at)
                } else {
                    let value_start = at;
                    while let Some(c) = peek(input, at) {
                        if separator(c) || c == ',' || c == '"' {
                            break;
                        }
                        at += c.len_utf8();
                    }
                    input[value_start..at].to_owned()
                };
                values.push(value);
                if peek(input, at) != Some(',') {
                    break;
                }
                at += 1;
            }
            tokens.push(Token::Qualifier {
                key,
                values,
                negated,
                raw: input[start..at].to_owned(),
            });
        } else {
            while let Some(c) = peek(input, at) {
                if separator(c) {
                    break;
                }
                at += c.len_utf8();
            }
            tokens.push(Token::Word {
                text: input[body_start..at].to_owned(),
                negated,
            });
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conditions_phrases_and_unknown_keys() {
        assert_eq!(
            lex("STATUS:\"In Progress\",Done -label:保留 通知 http://x -"),
            vec![
                Token::Qualifier {
                    key: Key::Status,
                    values: vec!["In Progress".into(), "Done".into()],
                    negated: false,
                    raw: "STATUS:\"In Progress\",Done".into()
                },
                Token::Qualifier {
                    key: Key::Label,
                    values: vec!["保留".into()],
                    negated: true,
                    raw: "-label:保留".into()
                },
                Token::Word {
                    text: "通知".into(),
                    negated: false
                },
                Token::Word {
                    text: "http://x".into(),
                    negated: false
                },
                Token::Word {
                    text: "-".into(),
                    negated: false
                },
            ]
        );
    }
    #[test]
    fn phrase_boundaries_and_escapes() {
        assert_eq!(
            lex(r#""a b"c -"a\"b\\c" "open"#),
            vec![
                Token::Phrase {
                    text: "a b".into(),
                    negated: false
                },
                Token::Word {
                    text: "c".into(),
                    negated: false
                },
                Token::Phrase {
                    text: "a\"b\\c".into(),
                    negated: true
                },
                Token::Phrase {
                    text: "open".into(),
                    negated: false
                },
            ]
        );
    }
    #[test]
    fn empty_values_and_fixed_whitespace() {
        assert_eq!(
            lex("label:a,,b\u{3000}label:\u{a0}x\u{2003}y"),
            vec![
                Token::Qualifier {
                    key: Key::Label,
                    values: vec!["a".into(), "".into(), "b".into()],
                    negated: false,
                    raw: "label:a,,b".into()
                },
                Token::Qualifier {
                    key: Key::Label,
                    values: vec!["".into()],
                    negated: false,
                    raw: "label:".into()
                },
                Token::Word {
                    text: "x\u{2003}y".into(),
                    negated: false
                },
            ]
        );
    }
    #[test]
    fn random_inputs_never_panic() {
        let alphabet: Vec<char> = "\"\\-:, \t\n　aか🙂".chars().collect();
        let mut state = 500_u64;
        for _ in 0..2000 {
            let mut input = String::new();
            for _ in 0..64 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                input.push(alphabet[(state >> 32) as usize % alphabet.len()]);
            }
            let _tokens = lex(&input);
        }
    }
}
