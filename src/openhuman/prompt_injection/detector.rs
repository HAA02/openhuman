use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptInjectionVerdict {
    Allow,
    Block,
    Review,
}

impl PromptInjectionVerdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
            Self::Review => "review",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInjectionReason {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptEnforcementAction {
    Allow,
    #[serde(rename = "block")]
    Blocked,
    ReviewBlocked,
}

impl PromptEnforcementAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Blocked => "block",
            Self::ReviewBlocked => "review_blocked",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PromptEnforcementDecision {
    pub verdict: PromptInjectionVerdict,
    pub score: f32,
    pub reasons: Vec<PromptInjectionReason>,
    pub action: PromptEnforcementAction,
    pub prompt_hash: String,
    pub prompt_chars: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct PromptEnforcementContext<'a> {
    pub source: &'a str,
    pub request_id: Option<&'a str>,
    pub user_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct DetectionRule {
    code: &'static str,
    message: &'static str,
    score: f32,
    regex: Regex,
}

trait OptionalClassifier: Send + Sync {
    fn classify(&self, normalized: &NormalizedPrompt) -> Option<(f32, PromptInjectionReason)>;
}

struct HeuristicClassifier;

impl OptionalClassifier for HeuristicClassifier {
    fn classify(&self, normalized: &NormalizedPrompt) -> Option<(f32, PromptInjectionReason)> {
        let mut score = 0.0_f32;
        if normalized.had_zwsp {
            score += 0.08;
        }
        if normalized.has_base64_marker {
            score += 0.08;
        }
        if normalized.has_instruction_override && normalized.has_exfiltration_intent {
            score += 0.20;
        }

        if score <= f32::EPSILON {
            None
        } else {
            Some((
                score.min(0.25),
                PromptInjectionReason {
                    code: "classifier.suspicious_combo".to_string(),
                    message:
                        "Input combines multiple prompt-injection traits (obfuscation + override/exfiltration)."
                            .to_string(),
                },
            ))
        }
    }
}

#[derive(Debug, Clone)]
struct NormalizedPrompt {
    lowered: String,
    collapsed: String,
    compact: String,
    had_zwsp: bool,
    has_base64_marker: bool,
    has_instruction_override: bool,
    has_exfiltration_intent: bool,
}

static SPACE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\s+").expect("prompt injection normalization space regex"));
static BASE64_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[A-Za-z0-9+/]{24,}={0,2}")
        .expect("prompt injection normalization base64 detection regex")
});

static DETECTION_RULES: Lazy<Vec<DetectionRule>> = Lazy::new(|| {
    vec![
        DetectionRule {
            code: "override.ignore_previous",
            message: "Attempts to override existing safety or system instructions.",
            score: 0.80,
            regex: Regex::new(
                r"(ignore|disregard|forget|bypass|override|cancel)\s+(the\s+|all\s+|any\s+|every\s+|everything|prior\s+|above\s+|previous\s+|your\s+)?\s*(previous|prior|above|system|safety|filter|filters|guidance|guidelines|instructions|rules|constraints|prompts?|directive|directives|told|you\s+were\s+told)",
            )
            .expect("override.ignore_previous regex"),
        },
        DetectionRule {
            code: "override.role_hijack",
            message: "Attempts to redefine assistant role or policy scope.",
            score: 0.80,
            regex: Regex::new(
                r"(you\s+are\s+(now|no\s+longer)|act\s+as\s+if|stay\s+in\s+character|from\s+(this\s+point|now\s+on)|pretend\s+(you\s+are|it'?s)|step\s+out\s+of\s+your\s+role|developer\s+mode|jailbreak|unrestricted|no\s+(rules|filters|restrictions)|\bdan\b|\bstan\b|gpt[- ]unleashed)",
            )
            .expect("override.role_hijack regex"),
        },
        DetectionRule {
            code: "exfiltrate.system_prompt",
            message: "Attempts to reveal hidden prompts or developer instructions.",
            score: 0.80,
            regex: Regex::new(
                r"(reveal|show|print|dump|leak|display|repeat|quote|output|include)\s+.{0,40}(system|developer|hidden|initial|initialization|tool descriptions?|prompt|instructions?|words above|contents of|prior to|before)",
            )
            .expect("exfiltrate.system_prompt regex"),
        },
        DetectionRule {
            code: "exfiltrate.secrets",
            message: "Attempts to exfiltrate secrets, credentials, or private data.",
            score: 0.50,
            regex: Regex::new(
                r"(api\s*key|api_key|\bsecret\b|\bsecrets\b|env(ironment)?\s+variable|openai\s+key|github\s+token|aws\s+credential|\.ssh|id_rsa|/etc/passwd|/etc/shadow|\.env\b|session\s+cookie|jwt|bearer\s+token)",
            )
            .expect("exfiltrate.secrets regex"),
        },
        DetectionRule {
            code: "tool.abuse",
            message: "Attempts to force unsafe tool usage or policy bypass.",
            score: 0.60,
            regex: Regex::new(
                r"((call|use|run|execute)\s+(the\s+)?(tool|tools?|function|functions?)\s+.*(without\s+approval|even\s+if\s+forbidden|no\s+matter\s+what)|use\s+the\s+\w+\s+tool|run\s+shell|execute\s+the\s+following|drop\s+table|fetch\s+/etc/|curl\s+http|email\s+.{0,60}\bto\b\s+\S+@|post\s+.{0,60}\bto\s+https?://)",
            )
            .expect("tool.abuse regex"),
        },
        DetectionRule {
            code: "exfil.network",
            message: "Attempts to exfiltrate conversation or data over the network.",
            score: 0.75,
            regex: Regex::new(
                r"(encode\s+.{0,40}\bbase64\b|post\s+.{0,40}\battacker|\bexfil\b|send\s+.{0,40}\bto\b\s+evil|without\s+(alerting|notifying)\s+the\s+user)",
            )
            .expect("exfil.network regex"),
        },
        DetectionRule {
            code: "shell.injection",
            message: "Likely shell-command injection payload.",
            score: 0.80,
            regex: Regex::new(
                r"(\brm\s+-rf\b|\bnc\s+-e\b|;\s*(curl|wget|sh|bash|nc|cat)\b|\|\s*(sh|bash)\b|\$\([^)]+\)|`[^`]+`|&&\s*(curl|wget|sh|bash)\b|\|\|\s*(curl|wget|sh|bash)\b|/dev/tcp/|;\s*whoami\b)",
            )
            .expect("shell.injection regex"),
        },
        DetectionRule {
            code: "sql.injection",
            message: "Likely SQL injection payload.",
            score: 0.80,
            regex: Regex::new(
                r"('\s*or\s*'?1'?\s*=\s*'?1|;\s*drop\s+table|union\s+select|;\s*delete\s+from|'\s*--\s*$)",
            )
            .expect("sql.injection regex"),
        },
        DetectionRule {
            code: "path.traversal",
            message: "Path traversal payload (../).",
            score: 0.80,
            regex: Regex::new(r"(\.\./){2,}|read\s+the\s+file\s+\.\.|open\s+\.\.").expect("path.traversal regex"),
        },
        DetectionRule {
            code: "delimiter.bypass",
            message: "Attempts to spoof system / user delimiters.",
            score: 0.80,
            regex: Regex::new(
                r"(<\|im_start\|>|</user>|<system>|###(end_of_user|begin_system|system)###|\[system\]|new\s+directive\s*:|system\s*:\s*new)",
            )
            .expect("delimiter.bypass regex"),
        },
        DetectionRule {
            code: "encoding.bypass",
            message: "Encoding-based instruction smuggling (base64 / rot13 / unicode escapes).",
            score: 0.75,
            regex: Regex::new(
                r"(base64\s*:|rot13\s*:|decode\s+(and|then)\s+(act|obey|comply|run)|(?:\\u[0-9a-fA-F]{4}){4,})",
            )
            .expect("encoding.bypass regex"),
        },
        DetectionRule {
            code: "indirect.injection",
            message: "Attempts to follow instructions embedded in fetched / quoted content.",
            score: 0.75,
            regex: Regex::new(
                r"(follow\s+any\s+instructions\s+(inside|in)\s+(it|the\s+document)|if\s+(the\s+)?(page|document|attachment)\s+(you\s+(fetch|read|see)\s+)?(says|contains)|honor\s+(that|earlier|previous)\s+(decision|agreement))",
            )
            .expect("indirect.injection regex"),
        },
        DetectionRule {
            code: "obedience.test",
            message: "Compliance-conditioning prompt (\"prove you can follow\").",
            score: 0.70,
            regex: Regex::new(
                r#"(say\s+only\s+["'][^"']{1,60}["']|confirm\s+you\s+are\s+(now\s+)?in\s+admin\s+mode|admin\s+mode\s+activated|repeat\s+after\s+me\s+\d+|i\s+(have|will)\s+(obey|no\s+restrictions))"#,
            )
            .expect("obedience.test regex"),
        },
        DetectionRule {
            code: "policy.bypass",
            message: "Hypothetical / fiction framings asking for prohibited output.",
            score: 0.70,
            regex: Regex::new(
                r"(hypothetical\s+for\s+(fiction|writing|research)|purely\s+educational|not\s+actually\s+harmful|administrator\s+already\s+approved|already\s+agreed\s+to\s+bypass|security\s+research\s+project|produce\s+the\s+exploit)",
            )
            .expect("policy.bypass regex"),
        },
        DetectionRule {
            code: "manipulation.emotional",
            message: "Emotional / consequence-framing manipulation.",
            score: 0.70,
            regex: Regex::new(
                r"(if\s+you\s+refuse|lose\s+my\s+job|you\s+must\s+comply|benevolent\s+ai\s+you\s+must|refusing\s+breaks\s+the\s+game)",
            )
            .expect("manipulation.emotional regex"),
        },
        DetectionRule {
            code: "exfiltrate.told_before",
            message: "Probes for what the model was told before the user's first message.",
            score: 0.75,
            regex: Regex::new(
                r"(what\s+were\s+you\s+told\s+before|told\s+before\s+the\s+user|before\s+the\s+user'?s\s+first\s+message|quote\s+it\s+exactly)",
            )
            .expect("exfiltrate.told_before regex"),
        },
        DetectionRule {
            code: "override.forget_everything",
            message: "Memory-wipe instruction (\"forget everything you were told\").",
            score: 0.80,
            regex: Regex::new(
                r"forget\s+everything\s+(you\s+were\s+told|that|so\s+far)",
            )
            .expect("override.forget_everything regex"),
        },
        DetectionRule {
            code: "exfil.credential_harvest",
            message: "Attempts to enumerate or print credentials in scope.",
            score: 0.75,
            regex: Regex::new(
                r"(print\s+(any\s+)?(environment\s+variable|env\s+var)|show\s+me\s+(the\s+)?(openai|github|aws)\s+(key|token|credential|secret)|dump\s+.{0,40}credentials?)",
            )
            .expect("exfil.credential_harvest regex"),
        },
    ]
});

fn optional_classifier() -> Option<Box<dyn OptionalClassifier>> {
    let choice = env::var("OPENHUMAN_PROMPT_INJECTION_CLASSIFIER")
        .unwrap_or_else(|_| "off".to_string())
        .to_ascii_lowercase();
    match choice.as_str() {
        "heuristic" => Some(Box::new(HeuristicClassifier)),
        _ => None,
    }
}

fn normalize_prompt(input: &str) -> NormalizedPrompt {
    let lowered = input.to_lowercase();
    let had_zwsp = lowered.chars().any(|ch| {
        matches!(
            ch,
            '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
        )
    });
    let has_base64_marker = BASE64_RE.is_match(&lowered);

    let mut buffer = String::with_capacity(lowered.len());
    for ch in lowered.chars() {
        let mapped = match ch {
            '0' => 'o',
            '1' => 'i',
            '3' => 'e',
            '4' => 'a',
            '5' => 's',
            '7' => 't',
            '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}' => ' ',
            other if other.is_ascii_alphanumeric() || other.is_whitespace() => other,
            _ => ' ',
        };
        buffer.push(mapped);
    }
    let collapsed = SPACE_RE.replace_all(buffer.trim(), " ").into_owned();
    let compact: String = collapsed.chars().filter(|ch| !ch.is_whitespace()).collect();

    let has_instruction_override = collapsed.contains("ignore previous instructions")
        || collapsed.contains("ignore all previous instructions")
        || compact.contains("ignoreallpreviousinstructions")
        || compact.contains("ignorepreviousinstructions");
    let has_exfiltration_intent = collapsed.contains("system prompt")
        || collapsed.contains("developer instructions")
        || collapsed.contains("hidden prompt")
        || collapsed.contains("reveal");

    NormalizedPrompt {
        lowered,
        collapsed,
        compact,
        had_zwsp,
        has_base64_marker,
        has_instruction_override,
        has_exfiltration_intent,
    }
}

fn prompt_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    hex::encode(digest)
}

fn analyze_prompt(input: &str) -> (PromptInjectionVerdict, f32, Vec<PromptInjectionReason>) {
    let normalized = normalize_prompt(input);

    let mut score = 0.0_f32;
    let mut reasons: Vec<PromptInjectionReason> = Vec::new();

    if normalized.has_instruction_override {
        score += 0.46;
        reasons.push(PromptInjectionReason {
            code: "override.obfuscated_instruction".to_string(),
            message: "Detected obfuscated instruction-override phrase.".to_string(),
        });
    }
    if normalized.has_exfiltration_intent {
        score += 0.24;
        reasons.push(PromptInjectionReason {
            code: "exfiltration.intent".to_string(),
            message: "Detected exfiltration-focused prompt intent.".to_string(),
        });
    }

    for rule in DETECTION_RULES.iter() {
        if rule.regex.is_match(&normalized.lowered)
            || rule.regex.is_match(&normalized.collapsed)
            || rule.regex.is_match(&normalized.compact)
        {
            score += rule.score;
            reasons.push(PromptInjectionReason {
                code: rule.code.to_string(),
                message: rule.message.to_string(),
            });
        }
    }

    if let Some(classifier) = optional_classifier() {
        if let Some((classifier_score, reason)) = classifier.classify(&normalized) {
            score += classifier_score;
            reasons.push(reason);
        }
    }

    score = score.min(1.0);
    let verdict = if score >= 0.70 {
        PromptInjectionVerdict::Block
    } else if score >= 0.45 {
        PromptInjectionVerdict::Review
    } else {
        PromptInjectionVerdict::Allow
    };

    (verdict, score, reasons)
}

pub fn enforce_prompt_input(
    input: &str,
    context: PromptEnforcementContext<'_>,
) -> PromptEnforcementDecision {
    let (verdict, score, reasons) = analyze_prompt(input);
    let action = match verdict {
        PromptInjectionVerdict::Allow => PromptEnforcementAction::Allow,
        PromptInjectionVerdict::Block => PromptEnforcementAction::Blocked,
        PromptInjectionVerdict::Review => PromptEnforcementAction::ReviewBlocked,
    };

    let hash = prompt_hash(input);
    let prompt_chars = input.chars().count();
    let reason_codes: Vec<String> = reasons.iter().map(|r| r.code.clone()).collect();

    tracing::info!(
        source = context.source,
        request_id = context.request_id.unwrap_or("unknown"),
        user_id = context.user_id.unwrap_or("unknown"),
        session_id = context.session_id.unwrap_or("unknown"),
        verdict = verdict.as_str(),
        score = score,
        reasons = %reason_codes.join(","),
        action = action.as_str(),
        prompt_hash = %hash,
        prompt_chars = prompt_chars,
        "[prompt_injection] detection verdict"
    );

    PromptEnforcementDecision {
        verdict,
        score,
        reasons,
        action,
        prompt_hash: hash,
        prompt_chars,
    }
}
