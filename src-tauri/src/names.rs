use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct NameFix {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct NameList {
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub names: Vec<NameFix>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct NamesPending {
    pub names: Vec<NameFix>,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NameDecision {
    pub keep: bool,
    pub names: Vec<NameFix>,
}

pub(crate) fn names_prompt(source: &str) -> String {
    format!(
        "Bạn đọc lời video đã được chép tự động. Tìm tên riêng bị chép sai chính tả.\n\
         from là cụm đúng như trong lời. to là tên viết đúng.\n\
         Chỉ lấy cặp mà bạn chắc là sai. Không có thì trả names rỗng. Tối đa 20.\n\
         Chỉ trả về JSON:\n\
         {{\"names\":[{{\"from\":\"\",\"to\":\"\"}}]}}\n\
         <text>\n{source}\n</text>"
    )
}

pub(crate) fn names_from(value: &Value) -> Option<Vec<NameFix>> {
    let items = value.get("names")?.as_array()?;
    let raw: Vec<NameFix> = items
        .iter()
        .take(20)
        .map(|item| NameFix {
            from: item
                .get("from")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            to: item
                .get("to")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
        .collect();
    Some(usable_names(&raw))
}

pub(crate) fn usable_names(names: &[NameFix]) -> Vec<NameFix> {
    let mut kept = Vec::new();
    for name in names {
        let from = name.from.trim();
        let to = name.to.trim();
        if from.chars().filter(|ch| ch.is_alphabetic()).count() < 2 || to.is_empty() {
            continue;
        }
        if same_fold(from, to) {
            continue;
        }
        if kept
            .iter()
            .any(|item: &NameFix| same_fold(&item.from, from))
        {
            continue;
        }
        kept.push(NameFix {
            from: from.to_string(),
            to: to.to_string(),
        });
    }
    kept
}

pub(crate) fn name_signature(names: &[NameFix]) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = usable_names(names)
        .into_iter()
        .map(|name| (fold(&name.from), name.to))
        .collect();
    pairs.sort();
    pairs
}

pub(crate) fn present_in(text: &str, names: &[NameFix]) -> Vec<NameFix> {
    usable_names(names)
        .into_iter()
        .filter(|name| contains_name(text, &name.from))
        .collect()
}

pub(crate) fn contains_name(text: &str, from: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len()).any(|index| find_at(&chars, index, from).is_some())
}

pub(crate) fn apply_names(text: &str, names: &[NameFix]) -> String {
    let mut rules = usable_names(names);
    rules.sort_by(|left, right| {
        right
            .from
            .chars()
            .count()
            .cmp(&left.from.chars().count())
            .then_with(|| left.from.cmp(&right.from))
    });
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        let hit = rules
            .iter()
            .find_map(|rule| find_at(&chars, index, &rule.from).map(|end| (end, rule.to.as_str())));
        if let Some((end, to)) = hit {
            out.push_str(to);
            index = end;
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }
    out
}

fn find_at(chars: &[char], index: usize, from: &str) -> Option<usize> {
    let needle: Vec<char> = from.chars().collect();
    if needle.is_empty() {
        return None;
    }
    let end = index + needle.len();
    if end > chars.len() {
        return None;
    }
    let prev_ok = index == 0 || !chars[index - 1].is_alphabetic();
    let next_ok = end == chars.len() || !chars[end].is_alphabetic();
    if prev_ok && next_ok && eq_ignore_case(&chars[index..end], &needle) {
        Some(end)
    } else {
        None
    }
}

fn eq_ignore_case(hay: &[char], needle: &[char]) -> bool {
    hay.len() == needle.len()
        && hay.iter().zip(needle).all(|(left, right)| {
            left.to_lowercase().to_string() == right.to_lowercase().to_string()
        })
}

fn fold(text: &str) -> String {
    text.to_lowercase()
}

fn same_fold(left: &str, right: &str) -> bool {
    fold(left) == fold(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn replaces_every_name_and_keeps_the_typed_spelling() {
        let names = vec![NameFix {
            from: "Lina".into(),
            to: "Lynae".into(),
        }];
        assert_eq!(
            apply_names("Lina đi với lina.", &names),
            "Lynae đi với Lynae."
        );
    }

    #[test]
    fn longer_phrase_wins_over_the_shorter_name() {
        let names = vec![
            NameFix {
                from: "Grock".into(),
                to: "Grok".into(),
            },
            NameFix {
                from: "Grock bot".into(),
                to: "Grok bot".into(),
            },
        ];
        assert_eq!(
            apply_names("Grock bot và Grock", &names),
            "Grok bot và Grok"
        );
    }

    #[test]
    fn does_not_cut_a_longer_word_or_rescan_the_replacement() {
        let names = vec![
            NameFix {
                from: "Lina".into(),
                to: "Lynae".into(),
            },
            NameFix {
                from: "bot".into(),
                to: "bott".into(),
            },
            NameFix {
                from: "Grock bot".into(),
                to: "Grok bot".into(),
            },
        ];
        assert_eq!(
            apply_names(
                "Lina Na và Ly Na rồi Lina.",
                &[
                    NameFix {
                        from: "Lina".into(),
                        to: "Lynae".into()
                    },
                    NameFix {
                        from: "Ly Na".into(),
                        to: "Lynae".into()
                    },
                    NameFix {
                        from: "Lina Na".into(),
                        to: "Lynae".into()
                    },
                ]
            ),
            "Lynae và Lynae rồi Lynae."
        );
        assert_eq!(apply_names("Linaria", &names), "Linaria");
        assert_eq!(apply_names("Lina, đi", &names), "Lynae, đi");
        assert_eq!(
            apply_names("Grock bot and bot", &names),
            "Grok bot and bott"
        );
    }

    #[test]
    fn drops_short_blank_and_identical_rows_and_keeps_the_first_spelling() {
        let names = vec![
            NameFix {
                from: " a ".into(),
                to: "b".into(),
            },
            NameFix {
                from: " Lina ".into(),
                to: " lina ".into(),
            },
            NameFix {
                from: "Lina".into(),
                to: "Lynae".into(),
            },
            NameFix {
                from: "lina".into(),
                to: "Other".into(),
            },
            NameFix {
                from: "Grok".into(),
                to: "  ".into(),
            },
        ];
        assert_eq!(
            usable_names(&names),
            vec![NameFix {
                from: "Lina".into(),
                to: "Lynae".into(),
            }]
        );
    }

    #[test]
    fn signature_ignores_order_and_source_case() {
        let left = vec![
            NameFix {
                from: "Grock bot".into(),
                to: "Grok bot".into(),
            },
            NameFix {
                from: "lina".into(),
                to: "Lynae".into(),
            },
        ];
        let right = vec![
            NameFix {
                from: "Lina".into(),
                to: "Lynae".into(),
            },
            NameFix {
                from: "Grock bot".into(),
                to: "Grok bot".into(),
            },
        ];
        assert_eq!(name_signature(&left), name_signature(&right));
        let changed = vec![NameFix {
            from: "Lina".into(),
            to: "Linay".into(),
        }];
        assert_ne!(name_signature(&left), name_signature(&changed));
    }

    #[test]
    fn parser_keeps_twenty_and_only_names_that_occur() {
        let mut items = Vec::new();
        for index in 0..25 {
            items.push(json!({"from": format!("Name{index}"), "to": "X"}));
        }
        items.push(json!({"from": "Lina", "to": "Lynae"}));
        let value = json!({"names": items});
        let parsed = names_from(&value).unwrap();
        assert_eq!(parsed.len(), 20);
        assert!(parsed.iter().all(|item| item.from.starts_with("Name")));
        assert_eq!(
            present_in("nhắc Lina một lần", &parsed),
            Vec::<NameFix>::new()
        );
        let heard =
            names_from(&json!({"names":[{"from":"Lina","to":"Lynae"},{"from":"Nope","to":"Yes"}]}))
                .unwrap();
        assert_eq!(
            present_in("Lina ở đây", &heard),
            vec![NameFix {
                from: "Lina".into(),
                to: "Lynae".into(),
            }]
        );
    }

    #[test]
    fn parser_rejects_a_reply_without_names() {
        assert!(names_from(&json!({"theme": "x"})).is_none());
        assert_eq!(names_from(&json!({"names": []})).unwrap(), Vec::new());
    }

    #[test]
    fn prompt_asks_for_json_and_includes_the_transcript() {
        let prompt = names_prompt("Lina gặp Grock bot");
        assert!(prompt.contains("Lina gặp Grock bot"));
        assert!(prompt.contains("\"names\""));
        assert!(!prompt.contains("Lynae"));
    }
}
