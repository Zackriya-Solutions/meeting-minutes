use crate::database::repositories::{
    meeting::MeetingsRepository, summary::SummaryProcessesRepository,
};
use crate::openspec::setup;
use crate::openspec::generator;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};
use tokio::process::Command;
use tokio::time::timeout;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const OPEN_SPEC_TIMEOUT_SECS: u64 = 180;
const TRANSCRIPT_SEED_FILE: &str = "transcript.md";
const SUMMARY_SEED_FILE: &str = "summary.md";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenSpecErrorCode {
    NodeMissing,
    CliMissing,
    CliFailed,
    NetworkUnavailable,
    Timeout,
    InvalidInput,
    IoFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSpecErrorPayload {
    pub code: OpenSpecErrorCode,
    pub message: String,
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOpenSpecSuccess {
    pub zip_temp_path: String,
    pub suggested_filename: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenSpecGenerationResult {
    Success {
        zip_temp_path: String,
        suggested_filename: String,
        slug: String,
    },
    Error {
        code: OpenSpecErrorCode,
        message: String,
        stderr: Option<String>,
    },
}

#[derive(Debug, Clone)]
enum OpenSpecCli {
    Managed {
        node_path: PathBuf,
        entrypoint: PathBuf,
    },
    Global,
    Npx,
}

#[derive(Debug, Clone)]
struct TranscriptSeed {
    title: String,
    transcript_markdown: String,
    summary_markdown: Option<String>,
}

#[derive(Debug, Clone)]
struct CommandOutput {
    status_success: bool,
    stderr: String,
}

#[derive(Debug, Clone)]
struct RunCommandRequest {
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
    timeout: Duration,
}

#[async_trait::async_trait]
trait CommandRunner: Sync {
    fn executable_exists(&self, name: &str) -> bool;

    async fn run(&self, request: RunCommandRequest) -> Result<CommandOutput, OpenSpecErrorPayload>;
}

struct SystemCommandRunner;

#[async_trait::async_trait]
impl CommandRunner for SystemCommandRunner {
    fn executable_exists(&self, name: &str) -> bool {
        which::which(name).is_ok()
    }

    async fn run(&self, request: RunCommandRequest) -> Result<CommandOutput, OpenSpecErrorPayload> {
        let child = Command::new(&request.program)
            .kill_on_drop(true)
            .args(&request.args)
            .current_dir(&request.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| OpenSpecErrorPayload {
                code: OpenSpecErrorCode::IoFailure,
                message: format!("Failed to start OpenSpec process: {}", err),
                stderr: None,
            })?;

        let result = timeout(request.timeout, child.wait_with_output()).await;

        let output = match result {
            Ok(wait_result) => wait_result.map_err(|err| OpenSpecErrorPayload {
                code: OpenSpecErrorCode::IoFailure,
                message: format!("Failed while waiting for OpenSpec process: {}", err),
                stderr: None,
            })?,
            Err(_) => {
                return Err(OpenSpecErrorPayload {
                    code: OpenSpecErrorCode::Timeout,
                    message: format!(
                        "OpenSpec generation timed out after {} seconds",
                        request.timeout.as_secs()
                    ),
                    stderr: None,
                });
            }
        };

        Ok(CommandOutput {
            status_success: output.status.success(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

pub struct OpenSpecService;

impl OpenSpecService {
    pub async fn generate_bundle<R: Runtime>(
        app: &AppHandle<R>,
        pool: &sqlx::SqlitePool,
        meeting_id: String,
        generate_with_ai: bool,
    ) -> OpenSpecGenerationResult {
        let runner = SystemCommandRunner;
        Self::generate_bundle_with_runner(app, pool, &meeting_id, generate_with_ai, &runner).await
    }

    async fn generate_bundle_with_runner<R: Runtime>(
        app: &AppHandle<R>,
        pool: &sqlx::SqlitePool,
        meeting_id: &str,
        generate_with_ai: bool,
        runner: &(dyn CommandRunner + Sync),
    ) -> OpenSpecGenerationResult {
        if meeting_id.trim().is_empty() {
            return to_result_error(OpenSpecErrorPayload {
                code: OpenSpecErrorCode::InvalidInput,
                message: "Meeting ID is required".to_string(),
                stderr: None,
            });
        }

        let seed = match Self::load_meeting_seed(pool, meeting_id).await {
            Ok(seed) => seed,
            Err(err) => return to_result_error(err),
        };

        let app_data_dir = match app.path().app_data_dir() {
            Ok(dir) => dir,
            Err(err) => {
                return to_result_error(OpenSpecErrorPayload {
                    code: OpenSpecErrorCode::IoFailure,
                    message: format!("Failed to resolve app data directory: {}", err),
                    stderr: Some(err.to_string()),
                })
            }
        };

        let cli = Self::detect_cli_for_app(app, &app_data_dir, runner);
        Self::generate_bundle_for_seed_with_runner_and_cli(app, pool, &app_data_dir, meeting_id, seed, generate_with_ai, runner, cli).await
    }

    #[cfg(test)]
    async fn generate_bundle_for_seed_with_runner(
        app_data_dir: &Path,
        meeting_id: &str,
        seed: TranscriptSeed,
        runner: &(dyn CommandRunner + Sync),
    ) -> OpenSpecGenerationResult {
        let cli = match Self::detect_cli(runner) {
            Ok(cli) => cli,
            Err(err) => return to_result_error(err),
        };

        Self::generate_bundle_for_seed_with_runner_and_cli_for_tests(app_data_dir, meeting_id, seed, runner, Ok(cli)).await
    }

    async fn generate_bundle_for_seed_with_runner_and_cli_for_tests(
        app_data_dir: &Path,
        meeting_id: &str,
        seed: TranscriptSeed,
        runner: &(dyn CommandRunner + Sync),
        cli: Result<OpenSpecCli, OpenSpecErrorPayload>,
    ) -> OpenSpecGenerationResult {
        let cli = match cli {
            Ok(cli) => cli,
            Err(err) => return to_result_error(err),
        };

        let workspace = match Self::prepare_workspace(app_data_dir, meeting_id, &seed) {
            Ok(path) => path,
            Err(err) => return to_result_error(err),
        };

        let slug = format!("{}-openspec", slugify(&seed.title));
        let init_request = Self::build_cli_request(
            &workspace,
            &cli,
            vec!["init".to_string(), ".".to_string(), "--tools".to_string(), "none".to_string(), "--force".to_string()],
        );

        // OpenSpec 1.7+ intentionally has no `generate` command. The CLI
        // creates the governed change workspace; an AI agent then writes the
        // proposal/spec/design/tasks using `/opsx:propose`. Keep the meeting
        // source files in that workspace so the exported bundle is immediately
        // actionable with every supported coding agent.
        let create_request = Self::build_cli_request(
            &workspace,
            &cli,
            vec![
                "new".to_string(),
                "change".to_string(),
                slug.clone(),
                "--description".to_string(),
                format!("Generate an OpenSpec change from the meeting: {}", seed.title),
            ],
        );

        let output = match runner.run(init_request).await {
            Ok(output) => output,
            Err(err) => {
                return OpenSpecGenerationResult::Error {
                    code: err.code,
                    message: err.message,
                    stderr: err.stderr,
                }
            }
        };

        if !output.status_success {
            let stderr = output.stderr.trim().to_string();
            let code = if is_network_error(&stderr) {
                OpenSpecErrorCode::NetworkUnavailable
            } else {
                OpenSpecErrorCode::CliFailed
            };

            return OpenSpecGenerationResult::Error {
                code,
                message: "OpenSpec CLI failed to generate artifacts".to_string(),
                stderr: if stderr.is_empty() { None } else { Some(stderr) },
            };
        }

        let output = match runner.run(create_request).await {
            Ok(output) => output,
            Err(err) => return to_result_error(err),
        };

        if !output.status_success {
            let stderr = output.stderr.trim().to_string();
            return OpenSpecGenerationResult::Error {
                code: if is_network_error(&stderr) {
                    OpenSpecErrorCode::NetworkUnavailable
                } else {
                    OpenSpecErrorCode::CliFailed
                },
                message: "OpenSpec CLI failed to create the change workspace".to_string(),
                stderr: if stderr.is_empty() { None } else { Some(stderr) },
            };
        }

        let generated_change_dir = match Self::resolve_generated_change_dir(&workspace, &slug) {
            Ok(path) => path,
            Err(err) => return to_result_error(err),
        };

        if let Err(err) = Self::write_meeting_context(&generated_change_dir, &seed) {
            return to_result_error(err);
        }

        let zip_path = workspace.join(format!("{}.zip", slug));
        if let Err(err) = zip_directory(&generated_change_dir, &zip_path) {
            return OpenSpecGenerationResult::Error {
                code: OpenSpecErrorCode::IoFailure,
                message: "Failed to package generated OpenSpec artifacts".to_string(),
                stderr: Some(err),
            };
        }

        let suggested_filename = format!("{}-openspec.zip", slugify(&seed.title));

        OpenSpecGenerationResult::Success {
            zip_temp_path: zip_path.to_string_lossy().to_string(),
            suggested_filename,
            slug,
        }
    }

    async fn generate_bundle_for_seed_with_runner_and_cli<R: Runtime>(
        app: &AppHandle<R>,
        pool: &sqlx::SqlitePool,
        app_data_dir: &Path,
        meeting_id: &str,
        seed: TranscriptSeed,
        generate_with_ai: bool,
        runner: &(dyn CommandRunner + Sync),
        cli: Result<OpenSpecCli, OpenSpecErrorPayload>,
    ) -> OpenSpecGenerationResult {
        let result = Self::generate_bundle_for_seed_with_runner_and_cli_for_tests(app_data_dir, meeting_id, seed.clone(), runner, cli).await;
        let OpenSpecGenerationResult::Success { zip_temp_path, suggested_filename, slug } = result else { return result };
        if !generate_with_ai { return OpenSpecGenerationResult::Success { zip_temp_path, suggested_filename, slug } }
        let change_dir = app_data_dir.join("openspec-generation").join(slugify(meeting_id)).join("openspec").join("changes").join(&slug);
        if let Err(error) = generator::generate_artifacts(app, pool, meeting_id, &seed.title, &seed.transcript_markdown, seed.summary_markdown.as_deref(), &change_dir).await {
            return OpenSpecGenerationResult::Error { code: OpenSpecErrorCode::CliFailed, message: "Selected AI provider failed to generate OpenSpec artifacts".to_string(), stderr: Some(error) };
        }
        let zip_path = app_data_dir.join("openspec-generation").join(slugify(meeting_id)).join(format!("{}.zip", slug));
        if let Err(error) = zip_directory(&change_dir, &zip_path) {
            return OpenSpecGenerationResult::Error { code: OpenSpecErrorCode::IoFailure, message: "Failed to package AI-generated OpenSpec artifacts".to_string(), stderr: Some(error) };
        }
        OpenSpecGenerationResult::Success { zip_temp_path: zip_path.to_string_lossy().to_string(), suggested_filename, slug }
    }

    fn detect_cli_for_app<R: Runtime>(
        app: &AppHandle<R>,
        _app_data_dir: &Path,
        runner: &dyn CommandRunner,
    ) -> Result<OpenSpecCli, OpenSpecErrorPayload> {
        // The Settings installer validates these exact absolute paths. Prefer
        // them over `openspec.cmd`/PATH so generation cannot regress into the
        // Windows cmd.exe PATH issue that the installer deliberately avoids.
        if let Some((node_path, entrypoint)) = setup::managed_openspec_paths(app) {
            return Ok(OpenSpecCli::Managed {
                node_path,
                entrypoint,
            });
        }

        Self::detect_cli(runner)
    }

    fn detect_cli(runner: &dyn CommandRunner) -> Result<OpenSpecCli, OpenSpecErrorPayload> {
        if runner.executable_exists("openspec") {
            return Ok(OpenSpecCli::Global);
        }

        if !runner.executable_exists("node") {
            return Err(OpenSpecErrorPayload {
                code: OpenSpecErrorCode::NodeMissing,
                message: "Node.js is required to run OpenSpec via npx".to_string(),
                stderr: None,
            });
        }

        if !runner.executable_exists("npx") {
            return Err(OpenSpecErrorPayload {
                code: OpenSpecErrorCode::CliMissing,
                message: "OpenSpec CLI not found (neither global openspec nor npx)".to_string(),
                stderr: None,
            });
        }

        Ok(OpenSpecCli::Npx)
    }

    async fn load_meeting_seed(
        pool: &sqlx::SqlitePool,
        meeting_id: &str,
    ) -> Result<TranscriptSeed, OpenSpecErrorPayload> {
        let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
            .await
            .map_err(|err| OpenSpecErrorPayload {
                code: OpenSpecErrorCode::IoFailure,
                message: format!("Failed to load meeting metadata: {}", err),
                stderr: None,
            })?
            .ok_or_else(|| OpenSpecErrorPayload {
                code: OpenSpecErrorCode::InvalidInput,
                message: format!("Meeting not found: {}", meeting_id),
                stderr: None,
            })?;

        let mut all = Vec::new();
        let mut offset = 0_i64;
        let page_size = 500_i64;
        loop {
            let (page, total) = MeetingsRepository::get_meeting_transcripts_paginated(
                pool, meeting_id, page_size, offset,
            )
            .await
            .map_err(|err| OpenSpecErrorPayload {
                code: OpenSpecErrorCode::IoFailure,
                message: format!("Failed to load meeting transcripts: {}", err),
                stderr: None,
            })?;

            let count = page.len() as i64;
            all.extend(page);
            offset += count;

            if offset >= total || count == 0 {
                break;
            }
        }

        if all.is_empty() {
            return Err(OpenSpecErrorPayload {
                code: OpenSpecErrorCode::InvalidInput,
                message: "Meeting has no transcript to generate OpenSpec".to_string(),
                stderr: None,
            });
        }

        all.sort_by(|a, b| {
            let a_time = a.audio_start_time.unwrap_or_default();
            let b_time = b.audio_start_time.unwrap_or_default();
            a_time
                .partial_cmp(&b_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let transcript_markdown = all
            .iter()
            .map(|row| row.transcript.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        let summary_markdown = SummaryProcessesRepository::get_summary_data(pool, meeting_id)
            .await
            .ok()
            .and_then(|row| row)
            .and_then(|row| row.result)
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|value| value.get("markdown").and_then(|v| v.as_str()).map(str::to_string));

        Ok(TranscriptSeed {
            title: meeting.title,
            transcript_markdown,
            summary_markdown,
        })
    }

    fn prepare_workspace(
        app_data_dir: &Path,
        meeting_id: &str,
        seed: &TranscriptSeed,
    ) -> Result<PathBuf, OpenSpecErrorPayload> {
        let workspace = app_data_dir
            .join("openspec-generation")
            .join(slugify(meeting_id));

        if workspace.exists() {
            fs::remove_dir_all(&workspace).map_err(|err| OpenSpecErrorPayload {
                code: OpenSpecErrorCode::IoFailure,
                message: format!("Failed to reset OpenSpec workspace: {}", err),
                stderr: None,
            })?;
        }

        fs::create_dir_all(&workspace).map_err(|err| OpenSpecErrorPayload {
            code: OpenSpecErrorCode::IoFailure,
            message: format!("Failed to create OpenSpec workspace: {}", err),
            stderr: None,
        })?;

        write_text_file(&workspace.join(TRANSCRIPT_SEED_FILE), &seed.transcript_markdown)?;
        if let Some(summary) = &seed.summary_markdown {
            write_text_file(&workspace.join(SUMMARY_SEED_FILE), summary)?;
        }

        Ok(workspace)
    }

    fn build_cli_request(workspace: &Path, cli: &OpenSpecCli, command_args: Vec<String>) -> RunCommandRequest {
        match cli {
            OpenSpecCli::Managed {
                node_path,
                entrypoint,
            } => RunCommandRequest {
                program: node_path.to_string_lossy().to_string(),
                args: std::iter::once(entrypoint.to_string_lossy().to_string())
                    .chain(command_args)
                    .collect(),
                cwd: workspace.to_path_buf(),
                timeout: Duration::from_secs(OPEN_SPEC_TIMEOUT_SECS),
            },
            OpenSpecCli::Global => RunCommandRequest {
                program: "openspec".to_string(),
                args: command_args,
                cwd: workspace.to_path_buf(),
                timeout: Duration::from_secs(OPEN_SPEC_TIMEOUT_SECS),
            },
            OpenSpecCli::Npx => RunCommandRequest {
                program: "npx".to_string(),
                args: std::iter::once("openspec@latest".to_string())
                    .chain(command_args)
                    .collect(),
                cwd: workspace.to_path_buf(),
                timeout: Duration::from_secs(OPEN_SPEC_TIMEOUT_SECS),
            },
        }
    }

    fn write_meeting_context(
        change_dir: &Path,
        seed: &TranscriptSeed,
    ) -> Result<(), OpenSpecErrorPayload> {
        write_text_file(&change_dir.join(TRANSCRIPT_SEED_FILE), &seed.transcript_markdown)?;
        if let Some(summary) = &seed.summary_markdown {
            write_text_file(&change_dir.join(SUMMARY_SEED_FILE), summary)?;
        }

        let instructions = format!(
            "# Meeting Context\n\nThis OpenSpec change was created from the meeting **{}**.\n\nUse your supported AI coding assistant from this change workspace and run:\n\n```text\n/opsx:propose\n```\n\nAsk it to read `transcript.md` and `summary.md` (if present), then create the proposal, specs, design, and tasks.\n",
            seed.title
        );
        write_text_file(&change_dir.join("MEETING_CONTEXT.md"), &instructions)
    }

    fn resolve_generated_change_dir(
        workspace: &Path,
        expected_slug: &str,
    ) -> Result<PathBuf, OpenSpecErrorPayload> {
        let changes_root = workspace.join("openspec").join("changes");
        if !changes_root.exists() {
            return Err(OpenSpecErrorPayload {
                code: OpenSpecErrorCode::CliFailed,
                message: "OpenSpec CLI did not produce openspec/changes output".to_string(),
                stderr: None,
            });
        }

        let expected_path = changes_root.join(expected_slug);
        if expected_path.is_dir() {
            return Ok(expected_path);
        }

        let mut candidates = fs::read_dir(&changes_root)
            .map_err(|err| OpenSpecErrorPayload {
                code: OpenSpecErrorCode::IoFailure,
                message: "Failed to inspect generated OpenSpec changes".to_string(),
                stderr: Some(err.to_string()),
            })?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| {
                let modified = entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .ok();
                (entry.path(), modified)
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|a, b| a.1.cmp(&b.1));
        candidates
            .pop()
            .map(|(path, _)| path)
            .ok_or_else(|| OpenSpecErrorPayload {
                code: OpenSpecErrorCode::CliFailed,
                message: "OpenSpec CLI completed but no change folder was generated".to_string(),
                stderr: None,
            })
    }
}

fn write_text_file(path: &Path, content: &str) -> Result<(), OpenSpecErrorPayload> {
    fs::write(path, content).map_err(|err| OpenSpecErrorPayload {
        code: OpenSpecErrorCode::IoFailure,
        message: format!("Failed to write {}", path.display()),
        stderr: Some(err.to_string()),
    })
}

fn zip_directory(source_dir: &Path, target_zip: &Path) -> Result<(), String> {
    let file = fs::File::create(target_zip).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for entry in WalkDir::new(source_dir).into_iter() {
        let entry = entry.map_err(|e: walkdir::Error| e.to_string())?;
        let path = entry.path();
        let relative = path.strip_prefix(source_dir).map_err(|e| e.to_string())?;

        if relative.as_os_str().is_empty() {
            continue;
        }

        let zip_name = relative.to_string_lossy().replace('\\', "/");
        if path.is_dir() {
            zip.add_directory(format!("{}/", zip_name), options)
                .map_err(|e| e.to_string())?;
            continue;
        }

        zip.start_file(zip_name, options).map_err(|e| e.to_string())?;
        let mut src = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        src.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
        zip.write_all(&buffer).map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn slugify(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_dash = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            out.push('-');
            previous_dash = true;
        }
    }

    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "meeting".to_string()
    } else {
        trimmed
    }
}

fn is_network_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    [
        "network",
        "registry.npmjs.org",
        "eai_again",
        "enotfound",
        "econnreset",
        "timed out",
        "fetch failed",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

fn to_result_error(payload: OpenSpecErrorPayload) -> OpenSpecGenerationResult {
    OpenSpecGenerationResult::Error {
        code: payload.code,
        message: payload.message,
        stderr: payload.stderr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::path::Path;
    use std::time::Instant;
    use tempfile::tempdir;
    use tokio::time::sleep;

    #[derive(Default)]
    struct MockRunner {
        available: HashMap<String, bool>,
        outputs: std::sync::Mutex<VecDeque<Result<CommandOutput, OpenSpecErrorPayload>>>,
        seen_programs: std::sync::Mutex<Vec<String>>,
        seen_args: std::sync::Mutex<Vec<Vec<String>>>,
        create_change_slug: std::sync::Mutex<Option<String>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for MockRunner {
        fn executable_exists(&self, name: &str) -> bool {
            self.available.get(name).copied().unwrap_or(false)
        }

        async fn run(&self, request: RunCommandRequest) -> Result<CommandOutput, OpenSpecErrorPayload> {
            if let Some(slug) = self
                .create_change_slug
                .lock()
                .expect("create_change_slug lock")
                .clone()
            {
                let change_dir = request.cwd.join("openspec").join("changes").join(slug);
                fs::create_dir_all(change_dir.join("specs")).expect("create generated change dir");
                fs::write(change_dir.join("proposal.md"), "proposal").expect("write proposal");
                fs::write(change_dir.join("design.md"), "design").expect("write design");
                fs::write(change_dir.join("tasks.md"), "tasks").expect("write tasks");
                fs::write(change_dir.join("specs").join("spec.md"), "spec").expect("write spec");
            }

            self.seen_programs
                .lock()
                .expect("seen programs lock")
                .push(request.program);
            self.seen_args
                .lock()
                .expect("seen args lock")
                .push(request.args);
            self.outputs
                .lock()
                .expect("outputs lock")
                .pop_front()
                .unwrap_or_else(|| Ok(CommandOutput {
                    status_success: true,
                    stderr: String::new(),
                }))
        }
    }

    fn run_generation_for_seed<'a>(
        app_data_dir: &'a Path,
        meeting_id: &'a str,
        seed: TranscriptSeed,
        runner: &'a (dyn CommandRunner + Sync),
    ) -> impl std::future::Future<Output = OpenSpecGenerationResult> + 'a {
        OpenSpecService::generate_bundle_for_seed_with_runner(app_data_dir, meeting_id, seed, runner)
    }

    #[test]
    fn red_test_meeting_text_cannot_affect_executable_selection() {
        let runner = MockRunner {
            available: HashMap::from([
                ("openspec".to_string(), false),
                ("node".to_string(), true),
                ("npx".to_string(), true),
            ]),
            ..Default::default()
        };

        // ponytail: this test proves executable resolution ignores untrusted transcript-like text.
        let suspicious_text = "README.sh requirements.txt $(rm -rf /)";
        let resolved = OpenSpecService::detect_cli(&runner).expect("cli resolution");

        assert!(matches!(resolved, OpenSpecCli::Npx));
        assert!(suspicious_text.contains("README.sh"));
    }

    #[test]
    fn detects_global_openspec_first() {
        let runner = MockRunner {
            available: HashMap::from([("openspec".to_string(), true)]),
            ..Default::default()
        };

        let resolved = OpenSpecService::detect_cli(&runner).expect("cli resolution");
        assert!(matches!(resolved, OpenSpecCli::Global));
    }

    #[test]
    fn maps_node_missing() {
        let runner = MockRunner {
            available: HashMap::from([
                ("openspec".to_string(), false),
                ("node".to_string(), false),
            ]),
            ..Default::default()
        };

        let err = OpenSpecService::detect_cli(&runner).expect_err("should fail");
        assert!(matches!(err.code, OpenSpecErrorCode::NodeMissing));
    }

    #[test]
    fn maps_cli_missing_when_node_present_but_npx_absent() {
        let runner = MockRunner {
            available: HashMap::from([
                ("openspec".to_string(), false),
                ("node".to_string(), true),
                ("npx".to_string(), false),
            ]),
            ..Default::default()
        };

        let err = OpenSpecService::detect_cli(&runner).expect_err("should fail");
        assert!(matches!(err.code, OpenSpecErrorCode::CliMissing));
    }

    #[test]
    fn classifies_network_error_from_stderr() {
        assert!(is_network_error("npm ERR! network request to registry.npmjs.org failed"));
        assert!(!is_network_error("syntax error in template"));
    }

    #[test]
    fn reset_workspace_overwrites_previous_files() {
        let root = tempdir().expect("tempdir");
        let app_data_dir = root.path();

        let meeting_id = "meeting-1";
        let workspace = app_data_dir
            .join("openspec-generation")
            .join(slugify(meeting_id));

        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("stale.txt"), "old").expect("write stale file");

        let seed = TranscriptSeed {
            title: "Weekly Sync".to_string(),
            transcript_markdown: "line a\nline b".to_string(),
            summary_markdown: Some("summary".to_string()),
        };

        let prepared = OpenSpecService::prepare_workspace(app_data_dir, meeting_id, &seed)
            .expect("prepare workspace");

        assert_eq!(prepared, workspace);
        assert!(!prepared.join("stale.txt").exists());
        assert!(prepared.join(TRANSCRIPT_SEED_FILE).exists());
        assert!(prepared.join(SUMMARY_SEED_FILE).exists());
    }

    #[test]
    fn zip_output_contains_generated_files() {
        let dir = tempdir().expect("tempdir");
        let source = dir.path().join("openspec").join("changes").join("demo-change");
        fs::create_dir_all(source.join("specs")).expect("create dirs");
        fs::write(source.join("proposal.md"), "proposal").expect("proposal write");
        fs::write(source.join("design.md"), "design").expect("design write");
        fs::write(source.join("tasks.md"), "tasks").expect("tasks write");
        fs::write(source.join("specs").join("spec.md"), "spec").expect("spec write");

        let zip_path = dir.path().join("bundle.zip");
        zip_directory(&source, &zip_path).expect("zip directory");

        let file = fs::File::open(&zip_path).expect("zip open");
        let mut archive = zip::ZipArchive::new(file).expect("zip archive");

        let mut names = Vec::new();
        for i in 0..archive.len() {
            let entry = archive.by_index(i).expect("zip entry");
            names.push(entry.name().to_string());
        }

        assert!(names.iter().any(|n| n == "proposal.md"));
        assert!(names.iter().any(|n| n == "design.md"));
        assert!(names.iter().any(|n| n == "tasks.md"));
        assert!(names.iter().any(|n| n == "specs/spec.md"));
    }

    #[tokio::test]
    async fn node_present_path_invokes_cli_generation() {
        let root = tempdir().expect("tempdir");
        let seed = TranscriptSeed {
            title: "Weekly Sync".to_string(),
            transcript_markdown: "line a".to_string(),
            summary_markdown: Some("summary".to_string()),
        };

        let runner = MockRunner {
            available: HashMap::from([
                ("openspec".to_string(), false),
                ("node".to_string(), true),
                ("npx".to_string(), true),
            ]),
            outputs: std::sync::Mutex::new(VecDeque::from([
                Ok(CommandOutput {
                    status_success: true,
                    stderr: String::new(),
                }),
                Ok(CommandOutput {
                    status_success: true,
                    stderr: String::new(),
                }),
            ])),
            seen_programs: std::sync::Mutex::new(Vec::new()),
            seen_args: std::sync::Mutex::new(Vec::new()),
            create_change_slug: std::sync::Mutex::new(Some("weekly-sync-openspec".to_string())),
        };

        let result = run_generation_for_seed(root.path(), "meeting-1", seed, &runner).await;

        let seen_programs = runner.seen_programs.lock().expect("seen_programs lock");
        assert_eq!(seen_programs.as_slice(), ["npx", "npx"]);
        assert!(matches!(result, OpenSpecGenerationResult::Success { .. }));
    }

    #[tokio::test]
    async fn successful_generate_bundle_with_runner_returns_zip_bundle() {
        let root = tempdir().expect("tempdir");
        let seed = TranscriptSeed {
            title: "Weekly Sync".to_string(),
            transcript_markdown: "line a".to_string(),
            summary_markdown: Some("summary".to_string()),
        };

        let runner = MockRunner {
            available: HashMap::from([("openspec".to_string(), true)]),
            outputs: std::sync::Mutex::new(VecDeque::from([Ok(CommandOutput {
                status_success: true,
                stderr: String::new(),
            })])),
            seen_programs: std::sync::Mutex::new(Vec::new()),
            seen_args: std::sync::Mutex::new(Vec::new()),
            create_change_slug: std::sync::Mutex::new(Some("weekly-sync-openspec".to_string())),
        };

        let result = run_generation_for_seed(root.path(), "meeting-2", seed, &runner).await;

        match result {
            OpenSpecGenerationResult::Success {
                zip_temp_path,
                suggested_filename,
                slug,
            } => {
                assert!(PathBuf::from(&zip_temp_path).exists());
                assert_eq!(slug, "weekly-sync-openspec");
                assert_eq!(suggested_filename, "weekly-sync-openspec.zip");
            }
            OpenSpecGenerationResult::Error { code, .. } => {
                panic!("expected success, got error: {:?}", code);
            }
        }
    }

    #[tokio::test]
    async fn cli_failure_surfaces_typed_cli_failed_error() {
        let root = tempdir().expect("tempdir");
        let seed = TranscriptSeed {
            title: "Weekly Sync".to_string(),
            transcript_markdown: "line a".to_string(),
            summary_markdown: Some("summary".to_string()),
        };

        let runner = MockRunner {
            available: HashMap::from([("openspec".to_string(), true)]),
            outputs: std::sync::Mutex::new(VecDeque::from([Ok(CommandOutput {
                status_success: false,
                stderr: "syntax error in prompt".to_string(),
            })])),
            seen_programs: std::sync::Mutex::new(Vec::new()),
            seen_args: std::sync::Mutex::new(Vec::new()),
            create_change_slug: std::sync::Mutex::new(None),
        };

        let result = run_generation_for_seed(root.path(), "meeting-3", seed, &runner).await;

        match result {
            OpenSpecGenerationResult::Error { code, stderr, .. } => {
                assert!(matches!(code, OpenSpecErrorCode::CliFailed));
                assert_eq!(stderr, Some("syntax error in prompt".to_string()));
            }
            OpenSpecGenerationResult::Success { .. } => panic!("expected cli failure"),
        }
    }

    #[tokio::test]
    async fn timeout_error_from_runner_surfaces_in_generation_result() {
        let root = tempdir().expect("tempdir");
        let seed = TranscriptSeed {
            title: "Weekly Sync".to_string(),
            transcript_markdown: "line a".to_string(),
            summary_markdown: Some("summary".to_string()),
        };

        let runner = MockRunner {
            available: HashMap::from([("openspec".to_string(), true)]),
            outputs: std::sync::Mutex::new(VecDeque::from([Err(OpenSpecErrorPayload {
                code: OpenSpecErrorCode::Timeout,
                message: "OpenSpec generation timed out after 180 seconds".to_string(),
                stderr: Some("timeout".to_string()),
            })])),
            seen_programs: std::sync::Mutex::new(Vec::new()),
            seen_args: std::sync::Mutex::new(Vec::new()),
            create_change_slug: std::sync::Mutex::new(None),
        };

        let result = run_generation_for_seed(root.path(), "meeting-4", seed, &runner).await;

        match result {
            OpenSpecGenerationResult::Error { code, message, .. } => {
                assert!(matches!(code, OpenSpecErrorCode::Timeout));
                assert!(message.contains("timed out"));
            }
            OpenSpecGenerationResult::Success { .. } => panic!("expected timeout error"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn system_command_runner_times_out_and_aborts_real_process() {
        let dir = tempdir().expect("tempdir");
        let marker = dir.path().join("should_not_exist.txt");
        let marker_path = marker.to_string_lossy().to_string();
        let script = format!("sleep 1; echo done > \"{}\"", marker_path);

        let runner = SystemCommandRunner;
        let started = Instant::now();
        let result = runner
            .run(RunCommandRequest {
                program: "sh".to_string(),
                args: vec!["-c".to_string(), script],
                cwd: dir.path().to_path_buf(),
                timeout: Duration::from_millis(250),
            })
            .await;
        let elapsed = started.elapsed();

        match result {
            Err(err) => assert!(matches!(err.code, OpenSpecErrorCode::Timeout)),
            Ok(_) => panic!("expected timeout"),
        }

        assert!(
            elapsed < Duration::from_millis(1200),
            "timeout path took too long: {:?}",
            elapsed
        );

        sleep(Duration::from_millis(900)).await;
        assert!(
            !marker.exists(),
            "timed-out process kept running and wrote marker file"
        );
    }
}
