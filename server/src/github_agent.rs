//! GitHubへの自動読み書き(2026-08-27新設)。
//!
//! 半導体/CPU/NPU/GPU・自動車/バイク/新幹線/リニアモーターカー/航空機の
//! 設計データ(図面・仕様書)をGitHubリポジトリへ保存・読み出しするため。
//! `open-english/server/src/github_agent.rs`(2026-08-20新設、実機検証
//! 済み)の設計をそのまま移植——Contents API(1ファイル単位の取得・
//! 作成・更新)を使用、Git Data API(低レベルAPI)は今回のスコープには
//! 過大なため不採用という判断も踏襲する。
//!
//! ## セキュリティ設計
//! - トークンはリクエストの都度クライアントから受け取り、このプロセスの
//!   メモリ上でのみ使う——ディスクへ平文永続化しない。
//! - トークン自体をログへ出力しない。

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const GITHUB_API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = "open-cg-cad-server";

#[derive(Debug, Deserialize)]
struct ContentsGetResponse {
    content: String,
    encoding: String,
    sha: String,
}

pub struct FileContent {
    pub content: String,
    pub sha: String,
}

/// `GET /repos/{owner}/{repo}/contents/{path}` — ファイル1件を取得する。
pub async fn read_file(owner: &str, repo: &str, path: &str, branch: Option<&str>, token: Option<&str>) -> Result<FileContent> {
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build().context("failed to build HTTP client")?;
    let mut url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/contents/{path}");
    if let Some(branch) = branch {
        url.push_str("?ref=");
        url.push_str(&urlencode(branch));
    }
    let mut req = client.get(&url).header("accept", "application/vnd.github+json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let resp = req.send().await.context("GitHub API request failed")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("GitHub API returned {status}: {body}");
    }
    let parsed: ContentsGetResponse = resp.json().await.context("failed to parse GitHub API response")?;
    if parsed.encoding != "base64" {
        bail!("unexpected encoding from GitHub API: {}", parsed.encoding);
    }
    let cleaned: String = parsed.content.chars().filter(|c| !c.is_whitespace()).collect();
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(cleaned).context("failed to base64-decode file content")?;
    let content = String::from_utf8(bytes).context("file content is not valid UTF-8 (binary files are not supported by this endpoint)")?;
    Ok(FileContent { content, sha: parsed.sha })
}

#[derive(Debug, Serialize)]
struct ContentsPutRequest<'a> {
    message: &'a str,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct ContentsPutResponse {
    commit: CommitInfo,
}

#[derive(Debug, Deserialize)]
struct CommitInfo {
    #[allow(dead_code)]
    sha: String,
    html_url: Option<String>,
}

/// `PUT /repos/{owner}/{repo}/contents/{path}` — 新規作成または更新の
/// コミットを1件作成する。
pub async fn commit_file(owner: &str, repo: &str, path: &str, content: &str, message: &str, branch: Option<&str>, sha: Option<&str>, token: &str) -> Result<Option<String>> {
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build().context("failed to build HTTP client")?;
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/contents/{path}");
    use base64::Engine;
    let body = ContentsPutRequest { message, content: base64::engine::general_purpose::STANDARD.encode(content.as_bytes()), sha, branch };
    let resp = client
        .put(&url)
        .header("accept", "application/vnd.github+json")
        .header("authorization", format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .context("GitHub API request failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("GitHub API commit returned {status}: {text}");
    }
    let parsed: ContentsPutResponse = resp.json().await.context("failed to parse GitHub API commit response")?;
    Ok(parsed.commit.html_url)
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
