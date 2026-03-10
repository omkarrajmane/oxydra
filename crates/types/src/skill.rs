use std::fmt;
use std::path::{Path, PathBuf};

use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::{Deserialize, Serialize};

/// Maximum estimated token count for a single skill body.
/// Estimated as `chars / CHARS_PER_TOKEN`. Skills exceeding this are rejected.
pub const MAX_SKILL_TOKENS: usize = 3000;

/// Character-to-token ratio used for the token estimate.
pub const CHARS_PER_TOKEN: usize = 4;

/// Activation mode for a skill.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillActivation {
    /// Inject when all conditions (required tools ready, env vars set) are met.
    #[default]
    Auto,
    /// Only inject on explicit request (future use).
    Manual,
    /// Always inject regardless of conditions.
    Always,
}

/// YAML frontmatter metadata for a skill file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Unique identifier (kebab-case). Used for deduplication across directories.
    pub name: String,
    /// One-line summary for diagnostics and future UI.
    pub description: String,
    /// Activation mode. Default: `auto`.
    #[serde(default)]
    pub activation: SkillActivation,
    /// Tool names that must be registered **and available** for this skill to activate.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Environment variables that must be set. Values available for `{{VAR}}`
    /// template substitution in the skill body.
    #[serde(default, alias = "env")]
    pub env_vars: Vec<String>,
    /// Ordering when multiple skills are active (lower = earlier in prompt).
    #[serde(default = "default_priority")]
    pub priority: i32,
}

fn default_priority() -> i32 {
    100
}

/// A discovered skill: metadata + markdown body + source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub metadata: SkillMetadata,
    /// Markdown body (everything after the YAML frontmatter).
    pub content: String,
    /// Filesystem path where this skill was loaded from.
    pub source_path: PathBuf,
}

/// A skill whose `{{VAR}}` placeholders have been replaced with env values,
/// ready for injection into the system prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSkill {
    pub name: String,
    pub content: String,
    pub priority: i32,
}

/// Errors from validating skill content.
#[derive(Debug)]
pub enum SkillValidationError {
    /// Frontmatter parsing failed.
    Parse(PathBuf, String),
    /// Content exceeds the token cap.
    TokenCap {
        path: PathBuf,
        estimated: usize,
        max: usize,
    },
}

impl fmt::Display for SkillValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(path, msg) => {
                write!(f, "failed to parse skill at {}: {msg}", path.display())
            }
            Self::TokenCap {
                path,
                estimated,
                max,
            } => write!(
                f,
                "skill at {} exceeds token cap ({estimated} estimated > {max} max)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SkillValidationError {}

/// Parse and validate skill content from a raw string (YAML frontmatter + markdown body).
///
/// Reusable by both the runner skill loader and the agent-authored skill tools.
pub fn validate_skill_content(
    raw: &str,
    source_path: &Path,
) -> Result<Skill, SkillValidationError> {
    let matter = Matter::<YAML>::new();
    let parsed = matter
        .parse::<SkillMetadata>(raw)
        .map_err(|err| SkillValidationError::Parse(source_path.to_path_buf(), err.to_string()))?;

    let metadata: SkillMetadata = parsed.data.ok_or_else(|| {
        SkillValidationError::Parse(
            source_path.to_path_buf(),
            "missing YAML frontmatter".to_owned(),
        )
    })?;

    let content = parsed.content;

    // Token cap enforcement.
    let estimated_tokens = content.len() / CHARS_PER_TOKEN;
    if estimated_tokens > MAX_SKILL_TOKENS {
        return Err(SkillValidationError::TokenCap {
            path: source_path.to_path_buf(),
            estimated: estimated_tokens,
            max: MAX_SKILL_TOKENS,
        });
    }

    Ok(Skill {
        metadata,
        content,
        source_path: source_path.to_path_buf(),
    })
}
