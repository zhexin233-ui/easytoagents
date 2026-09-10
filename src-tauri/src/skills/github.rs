//! 公开 GitHub 单目录 Skill 的固定提交、受限下载边界。

use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use percent_encoding::percent_decode_str;
use reqwest::{header, redirect::Policy, Client, Proxy, Response, StatusCode, Url};
use serde::Deserialize;
use tempfile::TempDir;

use crate::error::{AppError, ErrorCode};

const GITHUB_API: &str = "https://api.github.com/";
const GITHUB_RAW: &str = "https://raw.githubusercontent.com/";
const USER_AGENT: &str = "EasyToAgents/0.1 GitHub-Skill-Import";
const MAX_REF_CANDIDATES: usize = 16;
const MAX_REQUESTS: usize = 4_128;
const MAX_METADATA_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_METADATA_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_FILES: usize = 4_096;
const MAX_DEPTH: usize = 32;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SKILL_MD_BYTES: u64 = 512 * 1024;
const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_RELATIVE_PATH_BYTES: usize = 1_024;
const OVERALL_TIMEOUT: Duration = Duration::from_secs(120);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub(crate) struct DownloadedGithubSkill {
    directory: TempDir,
    normalized_url: String,
}

impl DownloadedGithubSkill {
    pub(crate) fn path(&self) -> &Path {
        self.directory.path()
    }

    pub(crate) fn normalized_url(&self) -> &str {
        &self.normalized_url
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubTreeLink {
    owner: String,
    repository: String,
    ambiguous_segments: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CommitResponse {
    sha: String,
    commit: CommitBody,
}

#[derive(Debug, Deserialize)]
struct CommitBody {
    tree: CommitTree,
}

#[derive(Debug, Deserialize)]
struct CommitTree {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct TreeEntry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    size: Option<u64>,
}

#[derive(Debug)]
struct ResolvedGithubTree {
    owner: String,
    repository: String,
    directory: Vec<String>,
    commit_sha: String,
    root_tree_sha: String,
    normalized_url: String,
}

#[derive(Debug, Clone)]
struct DownloadFile {
    relative: PathBuf,
    segments: Vec<String>,
    expected_size: u64,
}

struct GithubDownloader {
    client: Client,
    api_base: Url,
    raw_base: Url,
    deadline: Instant,
    requests: usize,
    metadata_bytes: usize,
}

/// `proxy` 由调用方显式注入（发布进程在 setup 里从 shell 环境读一次），
/// 下载本身不读进程环境，与适配器"显式环境注入"的原则一致。
pub(crate) async fn download_github_skill(
    input: &str,
    proxy: Option<&str>,
) -> Result<DownloadedGithubSkill, AppError> {
    let link = parse_github_tree_link(input)?;
    let mut client_builder = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .redirect(Policy::none())
        // 避免 macOS headless/沙箱中读取系统动态代理存储；只使用显式注入的代理。
        .no_proxy();
    if let Some(proxy) = proxy.map(str::trim).filter(|value| !value.is_empty()) {
        client_builder = client_builder.proxy(
            Proxy::all(proxy)
                .map_err(|_| AppError::invalid_input("proxy", "HTTP(S)/ALL_PROXY 配置无效"))?,
        );
    }
    let client = client_builder.build().map_err(|_| download_error())?;
    let mut downloader = GithubDownloader {
        client,
        api_base: Url::parse(GITHUB_API).map_err(|_| download_error())?,
        raw_base: Url::parse(GITHUB_RAW).map_err(|_| download_error())?,
        deadline: Instant::now() + OVERALL_TIMEOUT,
        requests: 0,
        metadata_bytes: 0,
    };
    downloader.download(link).await
}

impl GithubDownloader {
    async fn download(&mut self, link: GithubTreeLink) -> Result<DownloadedGithubSkill, AppError> {
        let resolved = self.resolve(link).await?;
        let subtree_sha = self.find_subtree(&resolved).await?;
        let files = self.list_files_for(&resolved, &subtree_sha).await?;
        let directory = tempfile::Builder::new()
            .prefix("easytoagents-github-skill-")
            .tempdir()
            .map_err(|_| download_error())?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .map_err(|_| download_error())?;
        for file in &files {
            self.download_file(&resolved, file, directory.path())
                .await?;
        }
        Ok(DownloadedGithubSkill {
            directory,
            normalized_url: resolved.normalized_url,
        })
    }

    async fn resolve(&mut self, link: GithubTreeLink) -> Result<ResolvedGithubTree, AppError> {
        let max_split = link.ambiguous_segments.len().saturating_sub(1);
        if max_split == 0 || link.ambiguous_segments.len() > MAX_DEPTH + MAX_REF_CANDIDATES {
            return Err(AppError::invalid_input(
                "url",
                "GitHub 链接的 ref 或目录层级无法安全解析",
            ));
        }
        for split in (1..=max_split.min(MAX_REF_CANDIDATES)).rev() {
            let reference = &link.ambiguous_segments[..split];
            let endpoint = api_url(
                &self.api_base,
                &[
                    "repos",
                    &link.owner,
                    &link.repository,
                    "commits",
                    &reference.join("/"),
                ],
            )?;
            let response = self.request(endpoint, true).await?;
            if matches!(
                response.status(),
                StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
            ) {
                continue;
            }
            let commit: CommitResponse = self.read_json(response).await?;
            if !is_commit_sha(&commit.sha) || !is_git_sha(&commit.commit.tree.sha) {
                return Err(download_error());
            }
            let directory = link.ambiguous_segments[split..].to_vec();
            if directory.len() > MAX_DEPTH {
                return Err(AppError::invalid_input(
                    "url",
                    "GitHub Skill 目录层级超出限制",
                ));
            }
            let normalized_url =
                normalized_tree_url(&link.owner, &link.repository, reference, &directory)?;
            return Ok(ResolvedGithubTree {
                owner: link.owner,
                repository: link.repository,
                directory,
                commit_sha: commit.sha,
                root_tree_sha: commit.commit.tree.sha,
                normalized_url,
            });
        }
        Err(AppError::not_found("githubSkill", "GitHub ref 或目录"))
    }

    async fn find_subtree(&mut self, resolved: &ResolvedGithubTree) -> Result<String, AppError> {
        let mut tree_sha = resolved.root_tree_sha.clone();
        for segment in &resolved.directory {
            let endpoint = api_url(
                &self.api_base,
                &[
                    "repos",
                    &resolved.owner,
                    &resolved.repository,
                    "git",
                    "trees",
                    &tree_sha,
                ],
            )?;
            let response = self.request(endpoint, false).await?;
            let tree: TreeResponse = self.read_json(response).await?;
            if tree.truncated {
                return Err(AppError::invalid_input("url", "GitHub 目录元数据不完整"));
            }
            let entry = tree
                .tree
                .into_iter()
                .find(|entry| entry.path == *segment)
                .ok_or_else(|| AppError::not_found("githubSkill", "GitHub Skill 目录"))?;
            if entry.kind != "tree" || entry.mode != "040000" || !is_git_sha(&entry.sha) {
                return Err(AppError::invalid_input("url", "GitHub 链接不是普通目录"));
            }
            tree_sha = entry.sha;
        }
        Ok(tree_sha)
    }

    async fn list_files_for(
        &mut self,
        resolved: &ResolvedGithubTree,
        subtree_sha: &str,
    ) -> Result<Vec<DownloadFile>, AppError> {
        let mut endpoint = api_url(
            &self.api_base,
            &[
                "repos",
                &resolved.owner,
                &resolved.repository,
                "git",
                "trees",
                subtree_sha,
            ],
        )?;
        endpoint.query_pairs_mut().append_pair("recursive", "1");
        let response = self.request(endpoint, false).await?;
        let tree: TreeResponse = self.read_json(response).await?;
        validate_tree_files(tree)
    }

    async fn download_file(
        &mut self,
        resolved: &ResolvedGithubTree,
        file: &DownloadFile,
        destination_root: &Path,
    ) -> Result<(), AppError> {
        let mut path_segments = vec![
            resolved.owner.clone(),
            resolved.repository.clone(),
            resolved.commit_sha.clone(),
        ];
        path_segments.extend(resolved.directory.clone());
        path_segments.extend(file.segments.clone());
        let segment_refs = path_segments.iter().map(String::as_str).collect::<Vec<_>>();
        let endpoint = api_url(&self.raw_base, &segment_refs)?;
        let response = self.request(endpoint, false).await?;
        let bytes = read_limited(response, file.expected_size as usize + 1).await?;
        if bytes.len() as u64 != file.expected_size {
            return Err(download_error());
        }
        let destination = destination_root.join(&file.relative);
        if let Some(parent) = destination.parent() {
            create_private_directories(destination_root, parent)?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&destination)
            .map_err(|_| download_error())?;
        output.write_all(&bytes).map_err(|_| download_error())?;
        output.sync_all().map_err(|_| download_error())?;
        Ok(())
    }

    async fn request(&mut self, url: Url, allow_not_found: bool) -> Result<Response, AppError> {
        self.requests = self.requests.checked_add(1).ok_or_else(download_error)?;
        if self.requests > MAX_REQUESTS {
            return Err(AppError::invalid_input(
                "url",
                "GitHub 下载请求数量超出限制",
            ));
        }
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(timeout_error)?;
        let response = self
            .client
            .get(url)
            .header(header::USER_AGENT, USER_AGENT)
            .header(header::ACCEPT, "application/vnd.github+json")
            .timeout(remaining.min(REQUEST_TIMEOUT))
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    timeout_error()
                } else {
                    download_error()
                }
            })?;
        if response.status().is_success()
            || (allow_not_found
                && matches!(
                    response.status(),
                    StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
                ))
        {
            return Ok(response);
        }
        if response.status() == StatusCode::TOO_MANY_REQUESTS
            || (response.status() == StatusCode::FORBIDDEN
                && response
                    .headers()
                    .get("x-ratelimit-remaining")
                    .is_some_and(|value| value == "0"))
        {
            return Err(rate_limit_error());
        }
        if response.status() == StatusCode::NOT_FOUND {
            return Err(AppError::not_found("githubSkill", "GitHub Skill 目录"));
        }
        Err(download_error())
    }

    async fn read_json<T: for<'de> Deserialize<'de>>(
        &mut self,
        response: Response,
    ) -> Result<T, AppError> {
        let bytes = read_limited(response, MAX_METADATA_RESPONSE_BYTES).await?;
        self.metadata_bytes = self
            .metadata_bytes
            .checked_add(bytes.len())
            .ok_or_else(download_error)?;
        if self.metadata_bytes > MAX_METADATA_TOTAL_BYTES {
            return Err(AppError::invalid_input("url", "GitHub 元数据大小超出限制"));
        }
        serde_json::from_slice(&bytes).map_err(|_| download_error())
    }
}

fn validate_tree_files(tree: TreeResponse) -> Result<Vec<DownloadFile>, AppError> {
    if tree.truncated {
        return Err(AppError::invalid_input("url", "GitHub 目录元数据不完整"));
    }
    if tree.tree.len() > MAX_FILES {
        return Err(AppError::invalid_input("url", "Skill 文件数量超出限制"));
    }
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    for entry in tree.tree {
        let segments = validate_remote_relative_path(&entry.path)?;
        match (entry.kind.as_str(), entry.mode.as_str()) {
            ("tree", "040000") => continue,
            ("blob", "100644" | "100755") => {}
            ("blob", "120000") => {
                return Err(AppError::invalid_input(
                    "url",
                    "GitHub Skill 不允许符号链接",
                ))
            }
            ("commit", _) | (_, "160000") => {
                return Err(AppError::invalid_input("url", "GitHub Skill 不允许子模块"))
            }
            _ => {
                return Err(AppError::invalid_input(
                    "url",
                    "GitHub Skill 含不支持的文件类型",
                ))
            }
        }
        if !is_git_sha(&entry.sha) {
            return Err(download_error());
        }
        let size = entry
            .size
            .ok_or_else(|| AppError::invalid_input("url", "GitHub 文件大小信息缺失"))?;
        if size > MAX_FILE_BYTES {
            return Err(AppError::invalid_input("url", "Skill 单文件大小超出限制"));
        }
        if entry.path == "SKILL.md" && size > MAX_SKILL_MD_BYTES {
            return Err(AppError::invalid_input("url", "SKILL.md 超出大小限制"));
        }
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| AppError::invalid_input("url", "Skill 总大小超出限制"))?;
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(AppError::invalid_input("url", "Skill 总大小超出限制"));
        }
        files.push(DownloadFile {
            relative: segments.iter().collect(),
            segments,
            expected_size: size,
        });
    }
    if !files
        .iter()
        .any(|file| file.relative == Path::new("SKILL.md"))
    {
        return Err(AppError::invalid_input(
            "SKILL.md",
            "Skill 目录缺少 SKILL.md",
        ));
    }
    files.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(files)
}

fn parse_github_tree_link(input: &str) -> Result<GithubTreeLink, AppError> {
    let url = Url::parse(input.trim())
        .map_err(|_| AppError::invalid_input("url", "请输入有效的 GitHub Skill 目录链接"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::invalid_input(
            "url",
            "仅支持不含凭据、端口、查询或片段的 github.com HTTPS 目录链接",
        ));
    }
    let raw_segments = url
        .path_segments()
        .ok_or_else(|| AppError::invalid_input("url", "GitHub Skill 链接路径无效"))?
        .collect::<Vec<_>>();
    if raw_segments.len() < 5 || raw_segments.get(2) != Some(&"tree") {
        return Err(AppError::invalid_input(
            "url",
            "链接必须是 github.com/{owner}/{repo}/tree/{ref}/{path}",
        ));
    }
    let segments = raw_segments
        .into_iter()
        .map(decode_url_segment)
        .collect::<Result<Vec<_>, _>>()?;
    let owner = segments[0].clone();
    let repository = segments[1].clone();
    if !valid_repository_component(&owner) || !valid_repository_component(&repository) {
        return Err(AppError::invalid_input("url", "GitHub 仓库名称无效"));
    }
    Ok(GithubTreeLink {
        owner,
        repository,
        ambiguous_segments: segments[3..].to_vec(),
    })
}

fn decode_url_segment(segment: &str) -> Result<String, AppError> {
    let decoded = percent_decode_str(segment)
        .decode_utf8()
        .map_err(|_| AppError::invalid_input("url", "GitHub 链接必须使用有效 UTF-8"))?
        .into_owned();
    if decoded.is_empty()
        || decoded == "."
        || decoded == ".."
        || decoded.contains('/')
        || decoded.contains('\\')
        || decoded.contains('\0')
    {
        return Err(AppError::invalid_input(
            "url",
            "GitHub 链接含不安全路径片段",
        ));
    }
    Ok(decoded)
}

fn valid_repository_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn normalized_tree_url(
    owner: &str,
    repository: &str,
    reference: &[String],
    directory: &[String],
) -> Result<String, AppError> {
    let mut url = Url::parse("https://github.com/").map_err(|_| download_error())?;
    {
        let mut segments = url.path_segments_mut().map_err(|_| download_error())?;
        segments.pop_if_empty();
        segments.push(owner).push(repository).push("tree");
        for segment in reference.iter().chain(directory) {
            segments.push(segment);
        }
    }
    Ok(url.to_string())
}

fn validate_remote_relative_path(path: &str) -> Result<Vec<String>, AppError> {
    if path.len() > MAX_RELATIVE_PATH_BYTES {
        return Err(AppError::invalid_input("url", "Skill 相对路径过长"));
    }
    let relative = Path::new(path);
    if relative.is_absolute() {
        return Err(AppError::invalid_input("url", "GitHub 返回了不安全路径"));
    }
    let mut segments = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| AppError::invalid_input("url", "GitHub 路径必须是 UTF-8"))?;
                if value.contains('\\') || value.contains('\0') {
                    return Err(AppError::invalid_input("url", "GitHub 返回了不安全路径"));
                }
                segments.push(value.to_owned());
            }
            _ => return Err(AppError::invalid_input("url", "GitHub 返回了不安全路径")),
        }
    }
    if segments.is_empty() || segments.len() > MAX_DEPTH {
        return Err(AppError::invalid_input("url", "Skill 目录层级超出限制"));
    }
    Ok(segments)
}

fn api_url(base: &Url, segments: &[&str]) -> Result<Url, AppError> {
    let mut url = base.clone();
    {
        let mut path = url.path_segments_mut().map_err(|_| download_error())?;
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
    }
    Ok(url)
}

async fn read_limited(mut response: Response, limit: usize) -> Result<Vec<u8>, AppError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AppError::invalid_input("url", "GitHub 响应大小超出限制"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| download_error())? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(AppError::invalid_input("url", "GitHub 响应大小超出限制"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn create_private_directories(root: &Path, target: &Path) -> Result<(), AppError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| AppError::invalid_input("url", "GitHub 下载路径越界"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(AppError::invalid_input("url", "GitHub 下载路径越界"));
        };
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => fs::set_permissions(&current, fs::Permissions::from_mode(0o700))
                .map_err(|_| download_error())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.is_dir()) {
                    return Err(download_error());
                }
            }
            Err(_) => return Err(download_error()),
        }
    }
    Ok(())
}

fn is_git_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_commit_sha(value: &str) -> bool {
    is_git_sha(value)
}

fn download_error() -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        "GitHub Skill 下载失败，请稍后重试",
        true,
    )
}

fn timeout_error() -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        "GitHub Skill 下载超时，请稍后重试",
        true,
    )
}

fn rate_limit_error() -> AppError {
    AppError::new(
        ErrorCode::Conflict,
        "GitHub API 请求频率已达上限，请稍后重试",
        true,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use reqwest::{redirect::Policy, Client, Url};
    use tempfile::tempdir;

    use super::{
        download_github_skill, normalized_tree_url, parse_github_tree_link,
        validate_remote_relative_path, validate_tree_files, GithubDownloader, TreeEntry,
        TreeResponse, OVERALL_TIMEOUT,
    };
    use crate::{app::AppPaths, db::Database, skills::import_downloaded_github_skill};

    #[test]
    fn parses_public_tree_link_without_guessing_ref_boundary() {
        let parsed = parse_github_tree_link(
            "https://github.com/vercel-labs/skills/tree/feature%2Funsafe/skills/find-skills",
        );
        assert!(parsed.is_err());

        let parsed = parse_github_tree_link(
            "https://github.com/vercel-labs/skills/tree/feature/topic/skills/find-skills",
        )
        .unwrap();
        assert_eq!(parsed.owner, "vercel-labs");
        assert_eq!(parsed.repository, "skills");
        assert_eq!(
            parsed.ambiguous_segments,
            ["feature", "topic", "skills", "find-skills"]
        );
    }

    #[test]
    fn rejects_credentials_ports_queries_and_unsafe_segments() {
        for url in [
            "http://github.com/a/b/tree/main/skill",
            "https://user@github.com/a/b/tree/main/skill",
            "https://github.com:444/a/b/tree/main/skill",
            "https://github.com/a/b/tree/main/skill?raw=1",
            "https://github.com/a/b/blob/main/SKILL.md",
            "https://github.com/a/b/tree/main/%2e%2e",
        ] {
            assert!(parse_github_tree_link(url).is_err(), "{url}");
        }
    }

    #[test]
    fn remote_paths_are_single_relative_utf8_tree_paths() {
        assert_eq!(
            validate_remote_relative_path("references/说明.md").unwrap(),
            ["references", "说明.md"]
        );
        for path in ["", "/absolute", "../escape", "a/../../escape", "a\\b"] {
            assert!(validate_remote_relative_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn normalizes_resolved_ref_and_validates_complete_tree_metadata() {
        let normalized = normalized_tree_url(
            "acme",
            "repo",
            &["feature".to_owned(), "topic".to_owned()],
            &["skills".to_owned(), "demo".to_owned()],
        )
        .unwrap();
        assert_eq!(
            normalized,
            "https://github.com/acme/repo/tree/feature/topic/skills/demo"
        );

        let files = validate_tree_files(TreeResponse {
            truncated: false,
            tree: vec![
                TreeEntry {
                    path: "SKILL.md".to_owned(),
                    mode: "100644".to_owned(),
                    kind: "blob".to_owned(),
                    sha: "a".repeat(40),
                    size: Some(20),
                },
                TreeEntry {
                    path: "references/info.txt".to_owned(),
                    mode: "100644".to_owned(),
                    kind: "blob".to_owned(),
                    sha: "b".repeat(40),
                    size: Some(8),
                },
            ],
        })
        .unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].relative.to_string_lossy(), "references/info.txt");

        for (kind, mode) in [("blob", "120000"), ("commit", "160000")] {
            assert!(validate_tree_files(TreeResponse {
                truncated: false,
                tree: vec![TreeEntry {
                    path: "SKILL.md".to_owned(),
                    mode: mode.to_owned(),
                    kind: kind.to_owned(),
                    sha: "c".repeat(40),
                    size: Some(8),
                }],
            })
            .is_err());
        }
    }

    #[test]
    fn local_http_fixture_downloads_every_file_from_one_fixed_commit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let commit_sha = "c".repeat(40);
        let root_tree_sha = "a".repeat(40);
        let subtree_sha = "b".repeat(40);
        let skill_md = b"---\nname: mock-skill\ndescription: Mock skill\n---\n# Mock\n".to_vec();
        let asset = b"complete fixture asset".to_vec();
        let (base, requests, server) = spawn_http_fixture(vec![
            (
                "/repos/acme/repo/commits/main".to_owned(),
                format!(
                    r#"{{"sha":"{commit_sha}","commit":{{"tree":{{"sha":"{root_tree_sha}"}}}}}}"#,
                )
                .into_bytes(),
            ),
            (
                format!("/repos/acme/repo/git/trees/{root_tree_sha}"),
                format!(
                    r#"{{"tree":[{{"path":"demo","mode":"040000","type":"tree","sha":"{subtree_sha}"}}],"truncated":false}}"#,
                )
                .into_bytes(),
            ),
            (
                format!("/repos/acme/repo/git/trees/{subtree_sha}?recursive=1"),
                format!(
                    r#"{{"tree":[{{"path":"SKILL.md","mode":"100644","type":"blob","sha":"{}","size":{}}},{{"path":"references/asset.txt","mode":"100644","type":"blob","sha":"{}","size":{}}}],"truncated":false}}"#,
                    "d".repeat(40),
                    skill_md.len(),
                    "e".repeat(40),
                    asset.len(),
                )
                .into_bytes(),
            ),
            (
                format!("/acme/repo/{commit_sha}/demo/SKILL.md"),
                skill_md.clone(),
            ),
            (
                format!("/acme/repo/{commit_sha}/demo/references/asset.txt"),
                asset.clone(),
            ),
        ]);
        let client = Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .unwrap();
        let mut downloader = GithubDownloader {
            client,
            api_base: base.clone(),
            raw_base: base,
            deadline: Instant::now() + OVERALL_TIMEOUT,
            requests: 0,
            metadata_bytes: 0,
        };

        let downloaded = tauri::async_runtime::block_on(downloader.download(
            parse_github_tree_link("https://github.com/acme/repo/tree/main/demo")?,
        ))?;
        assert_eq!(
            downloaded.normalized_url(),
            "https://github.com/acme/repo/tree/main/demo"
        );
        assert_eq!(fs::read(downloaded.path().join("SKILL.md"))?, skill_md);
        assert_eq!(
            fs::read(downloaded.path().join("references/asset.txt"))?,
            asset
        );
        drop(downloaded);
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 5);
        assert!(requests
            .iter()
            .filter(|request| request.starts_with("/acme/repo/"))
            .all(|request| request.contains(&commit_sha) && !request.contains("/main/")));
        Ok(())
    }

    fn spawn_http_fixture(
        responses: Vec<(String, Vec<u8>)>,
    ) -> (Url, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        let server = thread::spawn(move || {
            for _ in 0..responses.len() {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut chunk = [0_u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = stream.read(&mut chunk).unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..read]);
                }
                let request = String::from_utf8(request).unwrap();
                let path = request.split_whitespace().nth(1).unwrap().to_owned();
                observed.lock().unwrap().push(path.clone());
                let body = responses
                    .iter()
                    .find_map(|(expected, body)| (expected == &path).then_some(body.as_slice()))
                    .unwrap_or_default();
                let status = if body.is_empty() {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
        });
        (
            Url::parse(&format!("http://{address}/")).unwrap(),
            requests,
            server,
        )
    }

    #[test]
    #[ignore = "需要访问公开 GitHub；仅用于发布前真实链接验收"]
    fn real_anthropic_pdf_imports_complete_resources_into_isolated_library() {
        verify_real_public_example(
            "https://github.com/anthropics/skills/tree/main/skills/pdf",
            true,
        );
    }

    #[test]
    #[ignore = "需要访问公开 GitHub；仅用于发布前真实链接验收"]
    fn real_vercel_find_skills_imports_complete_resources_into_isolated_library() {
        verify_real_public_example(
            "https://github.com/vercel-labs/skills/tree/main/skills/find-skills",
            false,
        );
    }

    fn verify_real_public_example(url: &str, expects_extra_resources: bool) {
        let temporary = tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let paths = AppPaths::from_data_root(root.join("private/app-data")).unwrap();
        let mut database = Database::open(&paths).unwrap();
        println!("验收公开链接：{url}");
        let downloaded = tauri::async_runtime::block_on(download_github_skill(url, None)).unwrap();
        let downloaded_files = count_files(downloaded.path());
        assert!(downloaded_files >= 1, "真实示例至少必须包含 SKILL.md");
        if expects_extra_resources {
            assert!(
                downloaded_files > 1,
                "该真实示例必须包含 SKILL.md 之外的资源"
            );
        }
        let imported = import_downloaded_github_skill(
            &mut database,
            &paths,
            downloaded.path(),
            downloaded.normalized_url(),
        )
        .unwrap();
        assert_eq!(imported.source_path, url);
        assert_eq!(
            count_files(std::path::Path::new(&imported.central_path)),
            downloaded_files
        );
    }

    fn count_files(root: &std::path::Path) -> usize {
        fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .map(|path| if path.is_dir() { count_files(&path) } else { 1 })
            .sum()
    }
}
