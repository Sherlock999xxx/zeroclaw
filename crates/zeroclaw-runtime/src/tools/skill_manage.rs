//! Skill management tools for the background review fork.
//!
//! Three Tool impls exposed to the forked review agent:
//! - `skills_list`: enumerate installed skills (name, description, version).
//! - `skill_view`: read a single skill's SKILL.md (YAML front-matter + body
//!   preview) plus the names of files in `references/`, `templates/`,
//!   `scripts/`.
//! - `skill_manage`: mutating actions — `patch` (atomically rewrite the
//!   SKILL.md YAML front-matter via SkillImprover), `write_file` (add a file
//!   under `references/|templates/|scripts/`), `archive` (move to `.archive/`).
//!
//! Format follows the agentskills.io / Anthropic Agent Skills standard:
//! single `SKILL.md` per skill, YAML front-matter at top, Markdown body below.
//! These tools are NOT registered in the default tool registry — the review
//! fork builds them on demand so the main agent can't accidentally invoke
//! them.

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use zeroclaw_api::tool::{Tool, ToolResult};

const ARCHIVE_DIRNAME: &str = ".archive";
const ALLOWED_FILE_PREFIXES: &[&str] = &["references/", "templates/", "scripts/"];
const MAX_FILE_BYTES: usize = 256 * 1024;
const BODY_PREVIEW_CHARS: usize = 2_000;

fn skills_root(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("skills")
}

fn resolve_skill_dir(workspace_dir: &Path, slug: &str) -> Result<PathBuf> {
    if slug.is_empty()
        || slug.contains("..")
        || slug.contains('/')
        || slug.contains('\\')
        || slug.starts_with('.')
    {
        bail!("Invalid skill slug: {slug}");
    }
    Ok(skills_root(workspace_dir).join(slug))
}

/// Read-only: list installed skills.
pub struct SkillsListTool {
    workspace_dir: PathBuf,
}

impl SkillsListTool {
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self { workspace_dir }
    }
}

#[async_trait]
impl Tool for SkillsListTool {
    fn name(&self) -> &str {
        "skills_list"
    }

    fn description(&self) -> &str {
        "List installed skills with their name, version, and one-line description. \
         Read-only. Use before `skill_view` or `skill_manage` to find candidate \
         slugs."
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult> {
        let root = skills_root(&self.workspace_dir);
        let entries = match list_skill_entries(&root).await {
            Ok(e) => e,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to read skills directory: {e}")),
                });
            }
        };

        if entries.is_empty() {
            return Ok(ToolResult {
                success: true,
                output: "0 installed skills.".to_string(),
                error: None,
            });
        }

        let mut out = format!("{} installed skills:\n\n", entries.len());
        for (slug, name, description, version) in entries {
            let display_name = if name.is_empty() { &slug } else { &name };
            out.push_str(&format!("- {display_name} v{version} ({slug})\n"));
            if !description.is_empty() {
                out.push_str(&format!("    {description}\n"));
            }
        }
        Ok(ToolResult {
            success: true,
            output: out,
            error: None,
        })
    }
}

/// Reads SKILL.md front-matter via the same lightweight parser the loader uses
/// (top-level `key: value` pairs only — no nested mappings).
async fn list_skill_entries(
    skills_dir: &Path,
) -> std::io::Result<Vec<(String, String, String, String)>> {
    let mut rd = match tokio::fs::read_dir(skills_dir).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut out = Vec::new();
    while let Some(entry) = rd.next_entry().await? {
        let slug = entry.file_name().to_string_lossy().into_owned();
        if slug.starts_with('.') {
            continue;
        }
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let md_path = entry.path().join("SKILL.md");
        let Ok(content) = tokio::fs::read_to_string(&md_path).await else {
            continue;
        };
        let Some((front, _)) = split_front_matter(&content) else {
            continue;
        };
        let name = front_value(&front, "name").unwrap_or_default();
        let description = front_value(&front, "description").unwrap_or_default();
        let version = front_value(&front, "version").unwrap_or_else(|| "0.0.0".to_string());
        out.push((slug, name, description, version));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Read-only: view a single skill's SKILL.md front-matter + body preview + support files.
pub struct SkillViewTool {
    workspace_dir: PathBuf,
}

impl SkillViewTool {
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self { workspace_dir }
    }
}

#[async_trait]
impl Tool for SkillViewTool {
    fn name(&self) -> &str {
        "skill_view"
    }

    fn description(&self) -> &str {
        "Read a single skill's SKILL.md content (YAML front-matter + body \
         preview) plus the names of its support files under references/, \
         templates/, scripts/. Use this before deciding whether to patch the \
         skill or add a support file."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "slug": {
                    "type": "string",
                    "description": "Skill slug (directory name under workspace/skills/)."
                }
            },
            "required": ["slug"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let slug = args
            .get("slug")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing `slug` argument"))?;

        let skill_dir = match resolve_skill_dir(&self.workspace_dir, slug) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };

        let md_path = skill_dir.join("SKILL.md");
        let md = match tokio::fs::read_to_string(&md_path).await {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Skill '{slug}' not found: {e}")),
                });
            }
        };

        let (front, body) = split_front_matter(&md).unwrap_or((String::new(), md.clone()));
        let support_files = collect_support_files(&skill_dir).await;

        let mut output = format!("# Skill '{slug}'\n\n## Front-matter\n\n```yaml\n{front}\n```\n");
        if !body.trim().is_empty() {
            let truncated = if body.len() > BODY_PREVIEW_CHARS {
                format!("{}…\n[truncated; full body is {} bytes]", &body[..BODY_PREVIEW_CHARS], body.len())
            } else {
                body
            };
            output.push_str(&format!("\n## Body (Markdown)\n\n{truncated}\n"));
        }
        if !support_files.is_empty() {
            output.push_str("\n## Support files\n");
            for path in &support_files {
                output.push_str(&format!("- {path}\n"));
            }
        }

        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }
}

async fn collect_support_files(skill_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for sub in ["references", "templates", "scripts"] {
        let dir = skill_dir.join(sub);
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            out.push(format!("{sub}/{name}"));
        }
    }
    out.sort();
    out
}

/// Mutating: patch a SKILL.md, write a support file, or archive a skill.
pub struct SkillManageTool {
    workspace_dir: PathBuf,
}

impl SkillManageTool {
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self { workspace_dir }
    }
}

#[async_trait]
impl Tool for SkillManageTool {
    fn name(&self) -> &str {
        "skill_manage"
    }

    fn description(&self) -> &str {
        "Mutating operations on installed skills. Actions: `patch` (atomically \
         rewrite SKILL.md — supply the full new file content; the YAML \
         front-matter must have a `name` field), `write_file` (add a file \
         under references/, templates/, or scripts/), `archive` (move to \
         .archive/). All writes go through atomic temp-rename and validation \
         where applicable."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["patch", "write_file", "archive"],
                    "description": "Which mutation to perform."
                },
                "slug": {
                    "type": "string",
                    "description": "Skill slug to operate on."
                },
                "content": {
                    "type": "string",
                    "description": "For `patch`: new SKILL.md body (YAML front-matter + Markdown). For `write_file`: file contents."
                },
                "file_path": {
                    "type": "string",
                    "description": "For `write_file` only: relative path starting with `references/`, `templates/`, or `scripts/`."
                },
                "reason": {
                    "type": "string",
                    "description": "Short human-readable reason recorded in the skill's audit trail."
                }
            },
            "required": ["action", "slug"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing `action` argument"))?;
        let slug = args
            .get("slug")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing `slug` argument"))?;

        match action {
            "patch" => self.patch(slug, &args).await,
            "write_file" => self.write_file(slug, &args).await,
            "archive" => self.archive(slug).await,
            other => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Unknown action '{other}'. Valid: patch, write_file, archive"
                )),
            }),
        }
    }
}

impl SkillManageTool {
    async fn patch(&self, slug: &str, args: &Value) -> Result<ToolResult> {
        let skill_dir = match resolve_skill_dir(&self.workspace_dir, slug) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };
        if !skill_dir.join("SKILL.md").exists() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Skill '{slug}' not found (no SKILL.md)")),
            });
        }
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("`patch` requires `content`"))?;
        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Skill review");

        let mut improver = crate::skills::improver::SkillImprover::new(
            self.workspace_dir.clone(),
            zeroclaw_config::schema::SkillImprovementConfig {
                enabled: true,
                cooldown_secs: 0,
                ..Default::default()
            },
        );
        match improver.improve_skill(slug, content, reason).await {
            Ok(_) => Ok(ToolResult {
                success: true,
                output: format!("Patched skill '{slug}'."),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Patch failed: {e}")),
            }),
        }
    }

    async fn write_file(&self, slug: &str, args: &Value) -> Result<ToolResult> {
        let skill_dir = match resolve_skill_dir(&self.workspace_dir, slug) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };
        if !skill_dir.exists() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Skill '{slug}' not found")),
            });
        }
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("`write_file` requires `file_path`"))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("`write_file` requires `content`"))?;

        if !ALLOWED_FILE_PREFIXES
            .iter()
            .any(|prefix| file_path.starts_with(prefix))
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "file_path must start with one of: {}",
                    ALLOWED_FILE_PREFIXES.join(", ")
                )),
            });
        }
        if file_path.contains("..") || file_path.contains('\0') {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("file_path contains forbidden segment".to_string()),
            });
        }
        if content.len() > MAX_FILE_BYTES {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "content exceeds {MAX_FILE_BYTES} bytes ({} given)",
                    content.len()
                )),
            });
        }

        let target = skill_dir.join(file_path);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // Reject anything that escapes the skill directory after canonicalisation.
        let canonical_skill_dir = skill_dir
            .canonicalize()
            .unwrap_or_else(|_| skill_dir.clone());
        let canonical_target_parent = target
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_else(|| skill_dir.clone());
        if !canonical_target_parent.starts_with(&canonical_skill_dir) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("file_path escapes skill directory".to_string()),
            });
        }

        tokio::fs::write(&target, content.as_bytes()).await?;
        Ok(ToolResult {
            success: true,
            output: format!("Wrote {file_path} for skill '{slug}'."),
            error: None,
        })
    }

    async fn archive(&self, slug: &str) -> Result<ToolResult> {
        let skill_dir = match resolve_skill_dir(&self.workspace_dir, slug) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };
        if !skill_dir.exists() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Skill '{slug}' not found")),
            });
        }
        let archive_dir = skills_root(&self.workspace_dir).join(ARCHIVE_DIRNAME);
        tokio::fs::create_dir_all(&archive_dir).await?;
        let target = archive_dir.join(slug);
        let final_target = if target.exists() {
            let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
            archive_dir.join(format!("{slug}-{stamp}"))
        } else {
            target
        };
        tokio::fs::rename(&skill_dir, &final_target).await?;
        Ok(ToolResult {
            success: true,
            output: format!(
                "Archived skill '{slug}' to {}",
                final_target.display()
            ),
            error: None,
        })
    }
}

// ─── YAML front-matter helpers (file-local copies; identical to improver) ───

fn split_front_matter(content: &str) -> Option<(String, String)> {
    let normalized = content.replace("\r\n", "\n");
    let rest = normalized.strip_prefix("---\n")?;
    if let Some(idx) = rest.find("\n---\n") {
        Some((rest[..idx].to_string(), rest[idx + 5..].to_string()))
    } else {
        rest.strip_suffix("\n---")
            .map(|front| (front.to_string(), String::new()))
    }
}

fn front_value(front: &str, key: &str) -> Option<String> {
    for line in front.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim() == key {
            let v = v.trim();
            let unquoted = v.trim_matches('"').trim_matches('\'');
            return Some(unquoted.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    async fn write_skill(workspace: &Path, slug: &str, md: &str) {
        let dir = workspace.join("skills").join(slug);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("SKILL.md"), md).await.unwrap();
    }

    const VALID_SKILL: &str = "---\nname: deploy\ndescription: Run a production deploy\nversion: \"0.1.0\"\n---\n\n# Deploy\nDoes a production deploy.\n";

    // ─── skills_list ────────────────────────────────────────

    #[tokio::test]
    async fn skills_list_empty_when_no_skills() {
        let dir = tempdir();
        let tool = SkillsListTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("0 installed"));
    }

    #[tokio::test]
    async fn skills_list_enumerates_installed_skills() {
        let dir = tempdir();
        write_skill(dir.path(), "deploy", VALID_SKILL).await;
        write_skill(
            dir.path(),
            "test-runner",
            "---\nname: test-runner\ndescription: Run the test suite\nversion: \"0.2.0\"\n---\n\nBody\n",
        )
        .await;

        let tool = SkillsListTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("deploy"));
        assert!(result.output.contains("test-runner"));
        assert!(result.output.contains("0.1.0"));
        assert!(result.output.contains("0.2.0"));
    }

    #[tokio::test]
    async fn skills_list_skips_archive_dir() {
        let dir = tempdir();
        write_skill(dir.path(), "active", VALID_SKILL).await;
        let archive_path = dir.path().join("skills").join(".archive").join("old-skill");
        tokio::fs::create_dir_all(&archive_path).await.unwrap();
        tokio::fs::write(archive_path.join("SKILL.md"), VALID_SKILL)
            .await
            .unwrap();

        let tool = SkillsListTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("active"));
        assert!(!result.output.contains("old-skill"));
    }

    // ─── skill_view ─────────────────────────────────────────

    #[tokio::test]
    async fn skill_view_rejects_path_traversal() {
        let dir = tempdir();
        let tool = SkillViewTool::new(dir.path().to_path_buf());
        for bad in ["../etc/passwd", "..", "foo/bar", ".hidden", ""] {
            let result = tool
                .execute(json!({ "slug": bad }))
                .await
                .expect("execute should not error");
            assert!(!result.success, "expected rejection for slug {bad:?}");
        }
    }

    #[tokio::test]
    async fn skill_view_returns_front_matter_and_body() {
        let dir = tempdir();
        write_skill(dir.path(), "deploy", VALID_SKILL).await;

        let tool = SkillViewTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({ "slug": "deploy" })).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("name: deploy"));
        assert!(result.output.contains("Run a production deploy"));
        assert!(result.output.contains("Does a production deploy"));
    }

    #[tokio::test]
    async fn skill_view_lists_support_files() {
        let dir = tempdir();
        let skill_dir = dir.path().join("skills").join("deploy");
        tokio::fs::create_dir_all(skill_dir.join("references")).await.unwrap();
        tokio::fs::create_dir_all(skill_dir.join("scripts")).await.unwrap();
        tokio::fs::write(skill_dir.join("SKILL.md"), VALID_SKILL).await.unwrap();
        tokio::fs::write(skill_dir.join("references").join("api.md"), "...").await.unwrap();
        tokio::fs::write(skill_dir.join("scripts").join("verify.sh"), "...").await.unwrap();

        let tool = SkillViewTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({ "slug": "deploy" })).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("references/api.md"));
        assert!(result.output.contains("scripts/verify.sh"));
    }

    // ─── skill_manage: patch ────────────────────────────────

    const IMPROVED_SKILL: &str = "---\nname: deploy\ndescription: Run a production deploy (now with a pre-flight check)\nversion: \"0.1.1\"\n---\n\n# Deploy\nDoes a production deploy.\nRuns a pre-flight check first.\n";

    #[tokio::test]
    async fn skill_manage_patch_atomically_updates_md() {
        let dir = tempdir();
        write_skill(dir.path(), "deploy", VALID_SKILL).await;
        let tool = SkillManageTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "action": "patch",
                "slug": "deploy",
                "content": IMPROVED_SKILL,
                "reason": "User noted missing pre-flight check",
            }))
            .await
            .unwrap();
        assert!(result.success, "patch failed: {:?}", result.error);

        let on_disk = tokio::fs::read_to_string(
            dir.path().join("skills").join("deploy").join("SKILL.md"),
        )
        .await
        .unwrap();
        assert!(on_disk.contains("pre-flight check"));
        assert!(on_disk.contains("0.1.1"));
        assert!(on_disk.contains("updated_at:"));
        assert!(on_disk.contains("improvement_reason:"));
        assert!(on_disk.contains("User noted missing pre-flight check"));
        assert!(on_disk.contains("<!-- Improvement:"));
        assert!(
            !dir.path()
                .join("skills")
                .join("deploy")
                .join(".SKILL.md.tmp")
                .exists()
        );
    }

    #[tokio::test]
    async fn skill_manage_patch_rejects_invalid_content() {
        let dir = tempdir();
        write_skill(dir.path(), "deploy", VALID_SKILL).await;
        let tool = SkillManageTool::new(dir.path().to_path_buf());

        // No front-matter → validation rejects.
        let result = tool
            .execute(json!({
                "action": "patch",
                "slug": "deploy",
                "content": "just markdown, no yaml front-matter",
                "reason": "broken",
            }))
            .await
            .unwrap();
        assert!(!result.success);
        let on_disk = tokio::fs::read_to_string(
            dir.path().join("skills").join("deploy").join("SKILL.md"),
        )
        .await
        .unwrap();
        assert_eq!(on_disk, VALID_SKILL);
    }

    #[tokio::test]
    async fn skill_manage_patch_rejects_missing_skill() {
        let dir = tempdir();
        let tool = SkillManageTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "patch",
                "slug": "nonexistent",
                "content": IMPROVED_SKILL,
                "reason": "n/a",
            }))
            .await
            .unwrap();
        assert!(!result.success);
    }

    // ─── skill_manage: write_file ───────────────────────────

    #[tokio::test]
    async fn skill_manage_write_file_creates_references_md() {
        let dir = tempdir();
        write_skill(dir.path(), "deploy", VALID_SKILL).await;
        let tool = SkillManageTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "action": "write_file",
                "slug": "deploy",
                "file_path": "references/staging-quirks.md",
                "content": "# Staging quirks\n\n- env DEPLOY_TOKEN must be set\n",
            }))
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);

        let written = tokio::fs::read_to_string(
            dir.path()
                .join("skills")
                .join("deploy")
                .join("references")
                .join("staging-quirks.md"),
        )
        .await
        .unwrap();
        assert!(written.contains("DEPLOY_TOKEN"));
    }

    #[tokio::test]
    async fn skill_manage_write_file_rejects_bad_prefix() {
        let dir = tempdir();
        write_skill(dir.path(), "deploy", VALID_SKILL).await;
        let tool = SkillManageTool::new(dir.path().to_path_buf());

        for bad in [
            "SKILL.md",
            "../../etc/passwd",
            "secrets/key.pem",
            "references/../../etc/passwd",
            "/etc/passwd",
        ] {
            let result = tool
                .execute(json!({
                    "action": "write_file",
                    "slug": "deploy",
                    "file_path": bad,
                    "content": "nope",
                }))
                .await
                .unwrap();
            assert!(!result.success, "expected rejection for {bad:?}");
        }
        let md = tokio::fs::read_to_string(
            dir.path().join("skills").join("deploy").join("SKILL.md"),
        )
        .await
        .unwrap();
        assert_eq!(md, VALID_SKILL);
    }

    #[tokio::test]
    async fn skill_manage_write_file_enforces_size_cap() {
        let dir = tempdir();
        write_skill(dir.path(), "deploy", VALID_SKILL).await;
        let tool = SkillManageTool::new(dir.path().to_path_buf());

        let oversized = "x".repeat(MAX_FILE_BYTES + 1);
        let result = tool
            .execute(json!({
                "action": "write_file",
                "slug": "deploy",
                "file_path": "references/big.md",
                "content": oversized,
            }))
            .await
            .unwrap();
        assert!(!result.success);
    }

    // ─── skill_manage: archive ──────────────────────────────

    #[tokio::test]
    async fn skill_manage_archive_moves_skill() {
        let dir = tempdir();
        write_skill(dir.path(), "obsolete", VALID_SKILL).await;
        let tool = SkillManageTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({ "action": "archive", "slug": "obsolete" }))
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);

        assert!(!dir.path().join("skills").join("obsolete").exists());
        assert!(
            dir.path()
                .join("skills")
                .join(".archive")
                .join("obsolete")
                .join("SKILL.md")
                .exists()
        );
    }

    #[tokio::test]
    async fn skill_manage_archive_does_not_clobber_existing_archive() {
        let dir = tempdir();
        write_skill(dir.path(), "obsolete", VALID_SKILL).await;
        let archive_dir = dir.path().join("skills").join(".archive").join("obsolete");
        tokio::fs::create_dir_all(&archive_dir).await.unwrap();
        tokio::fs::write(archive_dir.join("SKILL.md"), VALID_SKILL).await.unwrap();

        let tool = SkillManageTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({ "action": "archive", "slug": "obsolete" }))
            .await
            .unwrap();
        assert!(result.success);

        assert!(archive_dir.join("SKILL.md").exists());
        let entries: Vec<_> = std::fs::read_dir(dir.path().join("skills").join(".archive"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(entries.iter().any(|e| e.starts_with("obsolete-")));
    }

    #[tokio::test]
    async fn skill_manage_rejects_unknown_action() {
        let dir = tempdir();
        write_skill(dir.path(), "deploy", VALID_SKILL).await;
        let tool = SkillManageTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({ "action": "nuke", "slug": "deploy" }))
            .await
            .unwrap();
        assert!(!result.success);
    }
}
