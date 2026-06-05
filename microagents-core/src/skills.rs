use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{collections::HashMap, fs, io};
use thiserror::Error;

/// Lazily-initialised path to the global skills directory (`~/.agents/skills`).
pub static GLOBAL_SKILLS_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Return the global skills directory, creating the cached path on first call.
pub fn global_skills_path() -> &'static PathBuf {
    GLOBAL_SKILLS_PATH.get_or_init(|| {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(".agents")
            .join("skills")
    })
}

/// Relative path to the project-local skills directory.
pub const SKILLS_PATH: &str = ".agents/skills";

fn null_as_empty_map<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<HashMap<String, Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<HashMap<String, Value>>::deserialize(deserializer)?;
    match opt {
        Some(m) => Ok(Some(m)),
        None => Ok(Some(HashMap::new())),
    }
}

fn null_as_empty_string<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => Ok(Some(s)),
        None => Ok(Some(String::new())),
    }
}

/// Frontmatter extracted from a skill's `SKILL.md` file.
#[derive(Debug, Serialize, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default, deserialize_with = "null_as_empty_string")]
    compatibility: Option<String>,
    #[serde(
        default,
        rename = "allowed-tools",
        deserialize_with = "null_as_empty_string"
    )]
    allowed_tools: Option<String>,
    #[serde(default, deserialize_with = "null_as_empty_map")]
    metadata: Option<HashMap<String, Value>>,
    #[serde(default, deserialize_with = "null_as_empty_string")]
    license: Option<String>,
}

/// Errors that can occur while loading a skill from disk.
#[derive(Debug, Error)]
pub enum SkillLoadingError {
    /// The skill file could not be read.
    #[error("Error while reading the skill file")]
    SkillReadError(#[from] io::Error),
    /// The YAML/TOML frontmatter in the skill file is invalid.
    #[error("Error while parsing the skill's frontmatter")]
    SkillFrontMatterError(#[from] markdown_frontmatter::Error),
}

/// Parse a skill's `SKILL.md` and return its description.
///
/// The file is expected to contain YAML frontmatter with at least a
/// `description` field.
pub fn parse_skill(skill_file: &Path) -> Result<String, SkillLoadingError> {
    let content = fs::read_to_string(skill_file)?;
    let (frontmatter, _) = markdown_frontmatter::parse::<SkillFrontmatter>(&content)?;

    Ok(frontmatter.description)
}

/// Locate a skill by name, preferring the local project directory.
///
/// Searches `.agents/skills/{name}` first, then `~/.agents/skills/{name}`.
/// Returns `None` if the skill cannot be found in either location.
pub fn ensure_skill(skill_name: &str) -> Option<PathBuf> {
    let g = global_skills_path().join(skill_name);
    let p = PathBuf::from(SKILLS_PATH).join(skill_name);
    if p.exists() {
        return Some(p);
    } else if g.exists() {
        return Some(g);
    }
    None
}

/// Discover all available skills in both local and global directories.
///
/// Duplicates are removed; local skills shadow global ones.
pub fn find_skills() -> Result<HashMap<String, String>, SkillLoadingError> {
    let g = global_skills_path();
    let p = PathBuf::from(SKILLS_PATH);
    let mut all_skills = HashMap::new();
    if g.exists() {
        let result = fs::read_dir(g)?;
        for entry in result {
            let entry = entry?;
            if entry.path().is_dir() {
                let des = parse_skill(&entry.path().join("SKILL.md"))?;
                all_skills.insert(entry.file_name().to_string_lossy().into_owned(), des);
            }
        }
    }

    if p.exists() {
        let result = fs::read_dir(p)?;
        for entry in result {
            let entry = entry?;
            if entry.path().is_dir() {
                let des = parse_skill(&entry.path().join("SKILL.md"))?;
                all_skills.insert(entry.file_name().to_string_lossy().into_owned(), des);
            }
        }
    }

    Ok(all_skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_skill_md(dir: &std::path::Path, content: &str) {
        let path = dir.join("SKILL.md");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_parse_skill_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_md = tmp.path().join("SKILL.md");
        fs::write(
            &skill_md,
            "---\nname: rust\ndescription: Best practices for Rust\n---\n\n# Rust\n",
        )
        .unwrap();
        let desc = parse_skill(&skill_md).unwrap();
        assert_eq!(desc, "Best practices for Rust");
    }

    #[test]
    fn test_parse_skill_with_all_frontmatter_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_md = tmp.path().join("SKILL.md");
        fs::write(
            &skill_md,
            "---\n\
            name: python\n\
            description: Python skill\n\
            compatibility: \">=3.10\"\n\
            allowed-tools: read,write\n\
            metadata:\n\
              foo: bar\n\
            license: MIT\n\
            ---\n\n\
            # Python\n",
        )
        .unwrap();
        let desc = parse_skill(&skill_md).unwrap();
        assert_eq!(desc, "Python skill");
    }

    #[test]
    fn test_parse_skill_missing_frontmatter_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_md = tmp.path().join("SKILL.md");
        fs::write(&skill_md, "# No frontmatter\n").unwrap();
        let err = parse_skill(&skill_md).unwrap_err();
        assert!(
            matches!(err, SkillLoadingError::SkillFrontMatterError(_)),
            "expected frontmatter error, got {:?}",
            err
        );
    }

    #[test]
    fn test_parse_skill_missing_file_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("missing").join("SKILL.md");
        let err = parse_skill(&missing).unwrap_err();
        assert!(
            matches!(err, SkillLoadingError::SkillReadError(_)),
            "expected read error, got {:?}",
            err
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_ensure_skill_prefers_local_path() {
        let tmp = tempfile::tempdir().unwrap();
        let local_skills = tmp.path().join(".agents").join("skills").join("test-skill");
        fs::create_dir_all(&local_skills).unwrap();
        fs::write(
            local_skills.join("SKILL.md"),
            "---\nname: test\ndescription: local\n---\n",
        )
        .unwrap();

        // Override global path to something else so local wins
        let global_skills = tmp.path().join("global").join("skills").join("test-skill");
        fs::create_dir_all(&global_skills).unwrap();
        fs::write(
            global_skills.join("SKILL.md"),
            "---\nname: test\ndescription: global\n---\n",
        )
        .unwrap();

        // We can't easily override OnceLock in tests, but we can at least verify
        // that a local path is returned when it exists by temporarily changing CWD
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = ensure_skill("test-skill");
        std::env::set_current_dir(original).unwrap();

        let path = result.unwrap();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains(".agents"));
        assert!(path_str.contains("skills"));
        assert!(path_str.contains("test-skill"));
    }

    #[test]
    fn test_ensure_skill_not_found() {
        let result = ensure_skill("definitely-nonexistent-skill-12345");
        assert!(result.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn test_find_skills_local_only() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_dir = tmp.path().join(".agents").join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let skill_a = skills_dir.join("skill-a");
        fs::create_dir(&skill_a).unwrap();
        write_skill_md(&skill_a, "---\nname: skill-a\ndescription: Skill A\n---\n");

        let skill_b = skills_dir.join("skill-b");
        fs::create_dir(&skill_b).unwrap();
        write_skill_md(&skill_b, "---\nname: skill-b\ndescription: Skill B\n---\n");

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let skills = find_skills().unwrap();
        std::env::set_current_dir(original).unwrap();

        // Only count skills that came from our temp directory
        let local_skills: Vec<_> = skills
            .into_iter()
            .filter(|(name, _)| name.starts_with("skill-"))
            .collect();
        assert_eq!(local_skills.len(), 2);
        let mut names: Vec<_> = local_skills.into_iter().map(|(n, _)| n).collect();
        names.sort();
        assert_eq!(names, vec!["skill-a", "skill-b"]);
    }

    #[test]
    #[serial_test::serial]
    fn test_find_skills_empty_when_no_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let skills = find_skills().unwrap();
        std::env::set_current_dir(original).unwrap();
        // Only count skills that came from our temp directory
        let local_skills: Vec<_> = skills
            .into_iter()
            .filter(|(name, _)| name.starts_with("test-"))
            .collect();
        assert!(local_skills.is_empty());
    }

    #[test]
    fn test_skill_frontmatter_null_handling() {
        let yaml = r#"---
name: null-test
description: Handles nulls
compatibility: null
allowed-tools: null
metadata: null
license: null
---
"#;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("SKILL.md");
        fs::write(&path, yaml).unwrap();

        let desc = parse_skill(&path).unwrap();
        assert_eq!(desc, "Handles nulls");
    }
}
