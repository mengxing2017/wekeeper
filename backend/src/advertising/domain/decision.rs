//! Immediate single-ad selection; configuration versions are not business inputs.
use super::{Content, compose, measure, targeting::SourceIdentity};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct DecisionInput {
    pub source: SourceIdentity,
    pub original: String,
    pub channels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedAd {
    pub placement_id: Uuid,
    pub material_id: Uuid,
    pub content: Content,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub selected_ad: Option<SelectedAd>,
    pub content: String,
    pub selection_count: Option<i64>,
    pub reason: String,
}

impl Decision {
    pub fn skipped(original: &str, reason: &str) -> Self {
        Self {
            selected_ad: None,
            content: original.to_owned(),
            selection_count: None,
            reason: reason.into(),
        }
    }
}

/// The counter belongs to the business unit, including platform/global ads.
/// Missing or unusable advertising never changes or delays the original message.
pub fn select_one(input: &DecisionInput, count: i64, candidates: Vec<SelectedAd>) -> Decision {
    let mut decision = Decision {
        selected_ad: None,
        content: input.original.clone(),
        selection_count: Some(count),
        reason: "no_available_ad".into(),
    };
    if candidates.is_empty() {
        return decision;
    }
    if count < 0 || count == i64::MAX {
        decision.reason = "ad_counter_unavailable".into();
        return decision;
    }
    let candidate = &candidates[(count as u64 % candidates.len() as u64) as usize];
    if candidate.content.validate().is_err() {
        decision.reason = "invalid_ad_content".into();
        return decision;
    }
    let content = compose(&input.original, Some(&candidate.content));
    if content.chars().count() > 200
        || input
            .channels
            .iter()
            .any(|channel| !measure(&content, channel).valid)
    {
        decision.reason = "ad_content_too_long".into();
        return decision;
    }
    decision.selected_ad = Some(candidate.clone());
    decision.content = content;
    decision.selection_count = Some(count + 1);
    decision.reason = "selected".into();
    decision
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_limit_and_unavailable_ad_preserve_the_original() {
        let mut content = Content {
            name: "广告".into(),
            body: "汉".repeat(140),
            link: None,
        };
        content.validate().unwrap();
        content.body.push('字');
        assert!(content.validate().is_err());
        content.body = "😀".repeat(140);
        content.validate().unwrap();
        content.body.push('a');
        assert!(content.validate().is_err());
        let input = DecisionInput {
            source: SourceIdentity {
                platform_id: Uuid::new_v4(),
                unit_id: Uuid::new_v4(),
            },
            original: "原消息".into(),
            channels: vec!["mobile".into()],
        };
        let invalid = SelectedAd {
            placement_id: Uuid::new_v4(),
            material_id: Uuid::new_v4(),
            content,
        };
        for candidates in [vec![], vec![invalid]] {
            let decision = select_one(&input, 7, candidates);
            assert_eq!(decision.content, input.original);
            assert_eq!(decision.selection_count, Some(7));
            assert!(decision.selected_ad.is_none());
        }
    }

    #[test]
    fn composed_message_limit_counts_unicode_linefeeds_and_links_in_both_channels() {
        let ad = SelectedAd {
            placement_id: Uuid::new_v4(),
            material_id: Uuid::new_v4(),
            content: Content {
                name: "广告".into(),
                body: "😀汉".into(),
                link: Some("https://example.org".into()),
            },
        };
        let overhead = compose("", Some(&ad.content)).chars().count();
        for channel in ["mobile", "wecom"] {
            let mut input = DecisionInput {
                source: SourceIdentity {
                    platform_id: Uuid::new_v4(),
                    unit_id: Uuid::new_v4(),
                },
                original: "汉".repeat(200 - overhead),
                channels: vec![channel.into()],
            };
            let selected = select_one(&input, 7, vec![ad.clone()]);
            assert_eq!(selected.content.chars().count(), 200);
            assert!(selected.selected_ad.is_some());
            input.original.push('😀');
            let skipped = select_one(&input, 7, vec![ad.clone()]);
            assert_eq!(skipped.content, input.original);
            assert!(skipped.selected_ad.is_none());
            assert_eq!(skipped.selection_count, Some(7));
            assert_eq!(skipped.reason, "ad_content_too_long");
        }
    }

    #[test]
    fn selects_one_ad_and_does_not_consume_on_channel_overflow() {
        let mut input = DecisionInput {
            source: SourceIdentity {
                platform_id: Uuid::new_v4(),
                unit_id: Uuid::new_v4(),
            },
            original: "消息".into(),
            channels: vec!["wecom".into()],
        };
        let ads: Vec<_> = ["甲", "乙"]
            .into_iter()
            .map(|body| SelectedAd {
                placement_id: Uuid::new_v4(),
                material_id: Uuid::new_v4(),
                content: Content {
                    name: body.into(),
                    body: body.into(),
                    link: None,
                },
            })
            .collect();
        for count in 0..4 {
            let decision = select_one(&input, count, ads.clone());
            assert_eq!(
                decision.selected_ad.unwrap().material_id,
                ads[count as usize % 2].material_id
            );
            assert_eq!(decision.selection_count, Some(count + 1));
        }
        input.original = "a".repeat(4000);
        let decision = select_one(&input, 4, ads);
        assert_eq!(decision.content, input.original);
        assert_eq!(decision.selection_count, Some(4));
        assert!(decision.selected_ad.is_none());
    }
}

#[cfg(test)]
mod formatting_tests {
    use super::*;

    #[test]
    fn one_added_linefeed_preserves_original_bytes_and_optional_link() {
        for source in [
            "正文",
            "多行\n正文",
            "正文\n",
            "  正文 😀  ",
            "原文\r\n正文",
        ] {
            for link in [None, Some("https://example.org".to_owned())] {
                let ad = Content {
                    name: "管理名".into(),
                    body: "广告".into(),
                    link: link.clone(),
                };
                let suffix = link.map(|v| format!("\n{v}")).unwrap_or_default();
                assert_eq!(
                    compose(source, Some(&ad)),
                    format!("{source}\n广告{suffix}")
                );
                assert_eq!(compose(source, None), source);
                let input = DecisionInput {
                    source: SourceIdentity {
                        platform_id: Uuid::new_v4(),
                        unit_id: Uuid::new_v4(),
                    },
                    original: source.into(),
                    channels: vec!["mobile".into(), "wecom".into()],
                };
                let result = select_one(
                    &input,
                    0,
                    vec![SelectedAd {
                        placement_id: Uuid::new_v4(),
                        material_id: Uuid::new_v4(),
                        content: ad,
                    }],
                );
                if source.contains('\r') {
                    assert_eq!(result.content, source);
                    assert!(result.selected_ad.is_none());
                    assert_eq!(result.selection_count, Some(0));
                } else {
                    assert_eq!(result.content, format!("{source}\n广告{suffix}"));
                }
            }
        }
    }

    #[test]
    fn advertising_decisions_roundtrip_the_shared_contract() {
        let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/advertising-decisions.json"
        ))
        .unwrap();
        for fixture in fixtures {
            let typed: Decision = serde_json::from_value(fixture.clone()).unwrap();
            assert_eq!(serde_json::to_value(typed).unwrap(), fixture);
        }
    }
}
