//! Scoped filtering is pure: immutable input, deterministic normalization and bounded evidence.
use super::{Fault, Result, digest, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, OnceLock},
};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;
pub const ALGORITHM: &str = "nfkc-fullfold-whitespace/2";
pub const UNICODE_VERSION: &str = "18.0.0";
pub const COMPOSER: &str = "source-text/1";
pub fn normalize(value: &str) -> String {
    static FOLD: OnceLock<BTreeMap<char, String>> = OnceLock::new();
    let table = FOLD.get_or_init(|| {
        include_str!("unicode/CaseFolding-18.0.0.txt")
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('#').next()?.split(';').map(str::trim);
                let code = parts.next()?;
                let kind = parts.next()?;
                let mapping = parts.next()?;
                if !matches!(kind, "C" | "F") {
                    return None;
                }
                Some((
                    char::from_u32(u32::from_str_radix(code, 16).ok()?)?,
                    mapping
                        .split_whitespace()
                        .map(|v| {
                            char::from_u32(u32::from_str_radix(v, 16).expect("fixed Unicode data"))
                                .expect("Unicode scalar")
                        })
                        .collect(),
                ))
            })
            .collect()
    });
    let folded: String = value
        .nfkc()
        .flat_map(|c| {
            table
                .get(&c)
                .map(|s| s.chars().collect::<Vec<_>>())
                .unwrap_or_else(|| vec![c])
        })
        .collect();
    folded.nfkc().collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub word: String,
    pub enabled: bool,
}
pub fn validate(entries: &[Entry]) -> Result<()> {
    ensure(entries.len() <= 2000, "keyword_limit_exceeded")?;
    let mut seen = BTreeSet::new();
    for entry in entries {
        let text = normalize(&entry.word);
        ensure(
            !text.trim().is_empty() && entry.word.len() <= 512 && text.chars().count() <= 128,
            "invalid_keyword",
        )?;
        ensure(seen.insert(text), "duplicate_keyword")?;
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub scope_id: String,
    pub version_id: Uuid,
    pub word: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScopeVersion {
    pub scope_id: String,
    pub revision: i64,
    pub version_id: Uuid,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Context {
    pub source_id: Uuid,
    pub route_id: String,
    pub unit_id: Option<Uuid>,
    pub channel: Option<String>,
    pub workspace_id: Option<Uuid>,
    pub versions: Vec<ScopeVersion>,
}
impl Context {
    pub fn fingerprint(&self) -> String {
        digest(&serde_json::to_vec(&(ALGORITHM, UNICODE_VERSION, self)).expect("fixed context"))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub rule_id: String,
    pub field: String,
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evaluation {
    pub input_digest: String,
    pub normalized_digest: String,
    pub total_hits: i64,
    pub matched_rules: Vec<Rule>,
    pub samples: Vec<Sample>,
    pub truncated: bool,
}
impl Evaluation {
    pub fn passed(&self) -> bool {
        self.total_hits == 0
    }
}
struct Matcher {
    engine: aho_corasick::AhoCorasick,
    groups: Vec<Vec<usize>>,
}
static CACHE: OnceLock<Mutex<BTreeMap<String, Arc<Matcher>>>> = OnceLock::new();
pub fn evaluate(rules: &[Rule], fields: &[(&str, &str)]) -> Result<Evaluation> {
    ensure(rules.len() <= 10000, "keyword_limit_exceeded")?;
    ensure(
        fields.iter().map(|(_, s)| s.len()).sum::<usize>() <= super::MAX_BODY * 2,
        "keyword_input_too_large",
    )?;
    let normalized: Vec<(&str, String)> = fields
        .iter()
        .map(|(name, text)| (*name, normalize(text)))
        .collect();
    let mut result = Evaluation {
        input_digest: digest(&serde_json::to_vec(fields)?),
        normalized_digest: digest(&serde_json::to_vec(&normalized)?),
        total_hits: 0,
        matched_rules: vec![],
        samples: vec![],
        truncated: false,
    };
    if rules.is_empty() {
        return Ok(result);
    }
    let key = digest(&serde_json::to_vec(&(ALGORITHM, rules))?);
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let cached = cache
        .lock()
        .map_err(|_| Fault("keyword_engine_unavailable"))?
        .get(&key)
        .cloned();
    let matcher = if let Some(cached) = cached {
        cached
    } else {
        let mut patterns: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, rule) in rules.iter().enumerate() {
            let word = normalize(&rule.word);
            ensure(!word.trim().is_empty(), "invalid_keyword")?;
            patterns.entry(word).or_default().push(index);
        }
        let engine = aho_corasick::AhoCorasick::new(patterns.keys())
            .map_err(|_| Fault("keyword_engine_unavailable"))?;
        let matcher = Arc::new(Matcher {
            engine,
            groups: patterns.into_values().collect(),
        });
        let mut entries = cache
            .lock()
            .map_err(|_| Fault("keyword_engine_unavailable"))?;
        if entries.len() >= 8 {
            entries.clear();
        }
        entries.insert(key, matcher.clone());
        matcher
    };
    let mut matched = BTreeSet::new();
    let mut work = 0usize;
    for (field, text) in &normalized {
        let mut offsets = vec![0; text.len() + 1];
        let mut compact = String::new();
        let mut compact_starts = Vec::new();
        let mut compact_ends = Vec::new();
        let mut count = 0;
        for (byte, character) in text.char_indices() {
            offsets[byte] = count;
            if !character.is_whitespace() {
                compact.push(character);
                for _ in 0..character.len_utf8() {
                    compact_starts.push(count);
                    compact_ends.push(count + 1);
                }
            }
            count += 1;
        }
        offsets[text.len()] = count;
        let mut occurrences = BTreeSet::new();
        for (candidate, compressed) in [(text.as_str(), false), (compact.as_str(), true)] {
            if compressed && compact.len() == text.len() {
                continue;
            }
            for hit in matcher.engine.find_overlapping_iter(candidate) {
                work += 1;
                ensure(work <= 2_000_000, "keyword_engine_budget_exceeded")?;
                let (start, end) = if compressed {
                    (compact_starts[hit.start()], compact_ends[hit.end() - 1])
                } else {
                    (offsets[hit.start()], offsets[hit.end()])
                };
                for &index in &matcher.groups[hit.pattern().as_usize()] {
                    work += 1;
                    ensure(work <= 2_000_000, "keyword_engine_budget_exceeded")?;
                    if !occurrences.insert((index, start, end)) {
                        continue;
                    }
                    result.total_hits += 1;
                    matched.insert(index);
                    if result.samples.len() < 100 {
                        result.samples.push(Sample {
                            rule_id: rules[index].id.clone(),
                            field: (*field).into(),
                            start,
                            end,
                        });
                    }
                }
            }
        }
    }
    result.matched_rules = matched.into_iter().map(|i| rules[i].clone()).collect();
    result.truncated = result.total_hits > result.samples.len() as i64;
    Ok(result)
}
pub fn compose(title: Option<&str>, content: Option<&str>, url: &str) -> String {
    format!(
        "{}\n{}\n{}",
        title.unwrap_or_default(),
        content.unwrap_or_default(),
        url
    )
    .replace("\r\n", "\n")
    .replace('\r', "\n")
}
/// Candidate adapters keep slot identities, ordering and cursor transactions; filtering never consumes a slot.
#[derive(Debug, Serialize, Deserialize)]
pub struct CandidateDecision {
    pub slot_id: Uuid,
    pub advertisement_version: Uuid,
    pub eligible: bool,
    pub evaluation: Evaluation,
}
pub fn check_candidate(
    rules: &[Rule],
    slot_id: Uuid,
    advertisement_version: Uuid,
    final_text: &str,
) -> Result<CandidateDecision> {
    let evaluation = evaluate(rules, &[("final", final_text)])?;
    Ok(CandidateDecision {
        slot_id,
        advertisement_version,
        eligible: evaluation.passed(),
        evaluation,
    })
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub expected_revision: i64,
    pub reason: String,
    pub entries: Vec<Entry>,
}
pub enum Command {
    Scopes,
    Versions {
        scope: String,
        page: i64,
    },
    Publish {
        scope: String,
        input: Publication,
    },
    Checks {
        route: String,
        receipt: Uuid,
        page: i64,
    },
    Hits {
        route: String,
        receipt: Uuid,
        check: Uuid,
        page: i64,
    },
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rule(word: &str) -> Rule {
        Rule {
            id: word.into(),
            scope_id: "global".into(),
            version_id: Uuid::nil(),
            word: word.into(),
        }
    }
    #[test]
    fn unicode_version_and_full_folding() {
        assert_eq!(unicode_normalization::UNICODE_VERSION, (18, 0, 0));
        assert_eq!(
            normalize("ＳＴＲＡＳＳＥ Straße K Σς"),
            "strasse strasse k σσ"
        );
        assert_eq!(normalize("A  B\nC"), "a  b\nc");
    }
    #[test]
    fn bounded_overlapping_evidence_and_field_boundaries() {
        let e = evaluate(&[rule("aa")], &[("title", &"a".repeat(110))]).unwrap();
        assert_eq!(e.total_hits, 109);
        assert_eq!(e.samples.len(), 100);
        assert!(e.truncated);
        assert!(
            evaluate(&[rule("ab")], &[("title", "a"), ("content", "b")])
                .unwrap()
                .passed()
        );
    }
    #[test]
    fn matches_across_whitespace_with_original_positions_and_no_duplicate_hits() {
        let result = evaluate(
            &[rule("诈骗"), rule("骗术")],
            &[("content", "诈骗 诈 \n骗\t术")],
        )
        .unwrap();
        assert_eq!(result.total_hits, 3);
        assert!(
            result
                .samples
                .iter()
                .any(|s| s.rule_id == "诈骗" && (s.start, s.end) == (0, 2))
        );
        assert!(
            result
                .samples
                .iter()
                .any(|s| s.rule_id == "诈骗" && (s.start, s.end) == (3, 7))
        );
        assert!(
            result
                .samples
                .iter()
                .any(|s| s.rule_id == "骗术" && (s.start, s.end) == (6, 9))
        );
        assert!(
            evaluate(&[rule("诈骗")], &[("content", "诈\u{200b}骗")])
                .unwrap()
                .passed()
        );
        assert!(
            evaluate(&[rule("诈骗")], &[("title", "诈"), ("content", "骗")])
                .unwrap()
                .passed()
        );
    }
    #[test]
    fn actual_composition_and_candidate_exclusion() {
        let rules = vec![rule("禁止")];
        assert!(
            !check_candidate(&rules, Uuid::nil(), Uuid::nil(), "原文禁禁止广告")
                .unwrap()
                .eligible
        );
        assert!(
            check_candidate(&rules, Uuid::nil(), Uuid::nil(), "原文正常广告")
                .unwrap()
                .eligible
        );
        assert_eq!(
            compose(Some("T"), Some("C"), "https://e.test"),
            "T\nC\nhttps://e.test"
        );
    }
}

/// Public evidence DTO, shared by HTTP serialization and contract fixtures.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRecord {
    pub id: Uuid,
    pub receipt_id: Uuid,
    pub stage: String,
    pub payload_fingerprint: String,
    pub context: Context,
    pub effective_fingerprint: String,
    pub input_digest: String,
    pub normalized_digest: String,
    pub algorithm: String,
    pub unicode_version: String,
    pub outcome: String,
    pub total_hits: i64,
    pub truncated: bool,
    pub created_at: String,
}
#[cfg(test)]
mod contract_tests {
    use super::*;
    #[test]
    fn public_keyword_contract_roundtrips_and_executes_fixed_unicode_vectors() {
        let v: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-keywords.json"
        ))
        .unwrap();
        let record: CheckRecord = serde_json::from_value(v["check"].clone()).unwrap();
        assert_eq!(serde_json::to_value(record).unwrap(), v["check"]);
        let publication: Publication = serde_json::from_value(v["publication"].clone()).unwrap();
        validate(&publication.entries).unwrap();
        assert_eq!(serde_json::to_value(publication).unwrap(), v["publication"]);
        for vector in v["normalization"].as_array().unwrap() {
            assert_eq!(
                normalize(vector["input"].as_str().unwrap()),
                vector["normalized"]
            );
        }
        for field in ["outcome", "force_pass", "workspace_id"] {
            let mut forged = v["publication"].clone();
            forged[field] = serde_json::json!(true);
            assert!(serde_json::from_value::<Publication>(forged).is_err());
        }
    }
}
