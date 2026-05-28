use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{collections::HashMap, fs, io};
use thiserror::Error;

pub static GLOBAL_SKILLS_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn global_skills_path() -> &'static PathBuf {
    GLOBAL_SKILLS_PATH.get_or_init(|| {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(".agents")
            .join("skills")
    })
}

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

#[derive(Debug, Error)]
pub enum SkillLoadingError {
    #[error("Error while reading the skill file")]
    SkillReadError(#[from] io::Error),
    #[error("Error while parsing the skill's frontmatter")]
    SkillFrontMatterError(#[from] markdown_frontmatter::Error),
}

pub fn parse_skill(skill_file: &PathBuf) -> Result<String, SkillLoadingError> {
    let content = fs::read_to_string(skill_file)?;
    let (frontmatter, _) = markdown_frontmatter::parse::<SkillFrontmatter>(&content)?;

    Ok(frontmatter.description)
}

pub fn ensure_skill(skill_name: &str) -> Option<PathBuf> {
    let g = global_skills_path().join(skill_name);
    let p = PathBuf::from(SKILLS_PATH).join(skill_name);
    if p.exists() {
        return Some(p);
    } else if g.exists() {
        return Some(g);
    }
    return None;
}

pub fn find_skills() -> Result<Vec<(String, String)>, SkillLoadingError> {
    let g = global_skills_path();
    let p = PathBuf::from(SKILLS_PATH);
    let mut all_skills = vec![];
    if g.exists() {
        let result = fs::read_dir(g)?;
        for entry in result {
            let entry = entry?;
            if entry.path().is_dir() {
                let des = parse_skill(&entry.path().join("SKILL.md"))?;
                all_skills.push((entry.file_name().to_str().unwrap().to_string(), des));
            }
        }
    }

    if p.exists() {
        let result = fs::read_dir(p)?;
        for entry in result {
            let entry = entry?;
            if entry.path().is_dir() {
                let des = parse_skill(&entry.path().join("SKILL.md"))?;
                all_skills.push((entry.file_name().to_str().unwrap().to_string(), des));
            }
        }
    }

    Ok(all_skills)
}
