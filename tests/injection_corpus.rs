//! Phase 1 V1-2/V1-3 — prompt-injection corpus acceptance gate.
//!
//! Two thresholds (PRD §1.4 NFR-01 + §3.1 V1-2 / V1-3):
//!   * recall ≥ 96 % across the 50 injection patterns
//!   * false-positive rate ≤ 1 % across the benign control set
//!
//! Failure of either gate fails CI for the security domain.

use openhuman_core::openhuman::prompt_injection::{
    enforce_prompt_input, PromptEnforcementAction, PromptEnforcementContext,
};
use serde::Deserialize;

const CORPUS_PATH: &str = "tests/fixtures/injection_corpus.json";
const RECALL_FLOOR: f64 = 0.96;
const FALSE_POSITIVE_CEILING: f64 = 0.01;

#[derive(Debug, Deserialize)]
struct Corpus {
    injections: Vec<InjectionFixture>,
    benign: Vec<BenignFixture>,
}

#[derive(Debug, Deserialize)]
struct InjectionFixture {
    id: String,
    #[allow(dead_code)]
    category: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct BenignFixture {
    id: String,
    text: String,
}

fn load_corpus() -> Corpus {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_PATH);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("invalid {}: {e}", path.display()))
}

fn scan(text: &str) -> PromptEnforcementAction {
    enforce_prompt_input(
        text,
        PromptEnforcementContext {
            source: "tests::injection_corpus",
            request_id: None,
            user_id: None,
            session_id: None,
        },
    )
    .action
}

fn is_block(action: PromptEnforcementAction) -> bool {
    matches!(
        action,
        PromptEnforcementAction::Blocked | PromptEnforcementAction::ReviewBlocked
    )
}

#[test]
fn injection_corpus_recall_floor() {
    let corpus = load_corpus();
    let total = corpus.injections.len();
    assert!(
        total >= 50,
        "corpus must hold ≥ 50 injection samples (have {total})"
    );

    let mut misses: Vec<&str> = Vec::new();
    for sample in &corpus.injections {
        if !is_block(scan(&sample.text)) {
            misses.push(sample.id.as_str());
        }
    }
    let recall = (total - misses.len()) as f64 / total as f64;
    assert!(
        recall >= RECALL_FLOOR,
        "recall {recall:.3} below floor {RECALL_FLOOR:.3}; misses={misses:?}"
    );
}

#[test]
fn injection_corpus_false_positive_ceiling() {
    let corpus = load_corpus();
    let total = corpus.benign.len();
    assert!(
        total >= 20,
        "benign control set must hold ≥ 20 samples (have {total})"
    );

    let mut false_positives: Vec<&str> = Vec::new();
    for sample in &corpus.benign {
        if is_block(scan(&sample.text)) {
            false_positives.push(sample.id.as_str());
        }
    }
    let rate = false_positives.len() as f64 / total as f64;
    assert!(
        rate <= FALSE_POSITIVE_CEILING,
        "false-positive rate {rate:.3} above ceiling {FALSE_POSITIVE_CEILING:.3}; offenders={false_positives:?}"
    );
}

#[test]
fn injection_corpus_ids_are_unique() {
    let corpus = load_corpus();
    let mut all: Vec<&str> = corpus
        .injections
        .iter()
        .map(|s| s.id.as_str())
        .chain(corpus.benign.iter().map(|s| s.id.as_str()))
        .collect();
    all.sort();
    let original = all.len();
    all.dedup();
    assert_eq!(
        original,
        all.len(),
        "corpus ids must be unique across both sections"
    );
}
