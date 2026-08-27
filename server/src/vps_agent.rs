//! VPS(レンタルサーバー)への自動読み書き(SSH鍵経由、2026-08-27新設)。
//!
//! ユーザー指示「半導体/CPU/NPU/GPU・自動車/バイク/新幹線/リニア
//! モーターカー/航空機の古い図面に対し、指示書や再設計図とVPS/
//! レンタルサーバー読み書きで照らし合わせて再設計する」への対応の
//! 一部。VPSへの安全な読み書きの実装は、`open-english/server/src/
//! vps_agent.rs`(2026-08-20新設、実機検証済み)のセキュリティ設計を
//! そのまま移植したもの——車輪の再発明を避け、同じ設計判断を踏襲する。
//!
//! ## セキュリティ設計(open-english側と同一方針、詳細略さず記載)
//!
//! - 秘密鍵のパスは`OPEN_CG_CAD_VPS_SSH_KEY_PATH`環境変数で指定し、
//!   サーバープロセス側でのみ読み込む。ブラウザへは一切送信しない。
//! - 接続先ホスト・ユーザー名も`OPEN_CG_CAD_VPS_HOST`
//!   (`host:port`または`host`、既定ポート22)・`OPEN_CG_CAD_VPS_USER`
//!   環境変数で固定する(リクエストからは受け取らない)。
//! - **任意のシェルコマンド実行は一切許可しない**。実行するのは
//!   「読み込み対象パスの`cat`」「書き込み対象パスへの`cat >`」の
//!   2種類のみ。
//! - 読み書き対象パスは`OPEN_CG_CAD_VPS_ALLOWED_PATHS`環境変数
//!   (セミコロン区切りの絶対パス一覧)で許可されたディレクトリ配下に
//!   限定する。未設定・空なら常に拒否(オプトイン方式)。
//! - シェルインジェクション対策として単一引用符クォート+`..`拒否。
//! - **正直な開示**: ホスト鍵検証(known_hosts照合)は未実装(常に
//!   受理する簡易実装)——信頼できる自社VPSへの直接接続を前提とする
//!   既知の制約(open-english側と同一)。

use anyhow::{bail, Context, Result};
use russh::client::{self, Handle};
use russh::keys::PrivateKeyWithHashAlg;
use russh::ChannelMsg;
use std::sync::Arc;

fn allowed_remote_dirs() -> Vec<String> {
    std::env::var("OPEN_CG_CAD_VPS_ALLOWED_PATHS")
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn validate_remote_path(requested: &str) -> Result<()> {
    if requested.contains("..") {
        bail!("path must not contain '..' / パスに'..'を含めることはできません");
    }
    if !requested.starts_with('/') {
        bail!("path must be an absolute remote path starting with '/' / リモート側の絶対パス('/'始まり)を指定してください");
    }
    let allowed = allowed_remote_dirs();
    if allowed.is_empty() {
        bail!("VPS agent is disabled: set OPEN_CG_CAD_VPS_ALLOWED_PATHS to opt in / VPSエージェントは無効です(OPEN_CG_CAD_VPS_ALLOWED_PATHSで許可パスを設定してください)");
    }
    if !allowed.iter().any(|dir| requested.starts_with(dir.as_str())) {
        bail!("path {requested:?} is outside the allowed remote directories / 許可されたリモートディレクトリの範囲外です");
    }
    Ok(())
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r#"'"'"'"#))
}

struct SshHandler;

impl client::Handler for SshHandler {
    type Error = russh::Error;

    fn check_server_key(&mut self, _server_public_key: &russh::keys::PublicKeyOrCertificate) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        async { Ok(true) }
    }
}

struct VpsConfig {
    host: String,
    port: u16,
    user: String,
    key_path: String,
}

fn config_from_env() -> Result<VpsConfig> {
    let host_raw = std::env::var("OPEN_CG_CAD_VPS_HOST").context("OPEN_CG_CAD_VPS_HOST is not set")?;
    let (host, port) = match host_raw.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().context("invalid port in OPEN_CG_CAD_VPS_HOST")?),
        None => (host_raw, 22u16),
    };
    let user = std::env::var("OPEN_CG_CAD_VPS_USER").context("OPEN_CG_CAD_VPS_USER is not set")?;
    let key_path = std::env::var("OPEN_CG_CAD_VPS_SSH_KEY_PATH").context("OPEN_CG_CAD_VPS_SSH_KEY_PATH is not set")?;
    Ok(VpsConfig { host, port, user, key_path })
}

async fn connect(cfg: &VpsConfig) -> Result<Handle<SshHandler>> {
    let ssh_config = Arc::new(client::Config::default());
    let mut session = client::connect(ssh_config, (cfg.host.as_str(), cfg.port), SshHandler).await.context("failed to connect to VPS over SSH")?;
    let key_pair = russh::keys::load_secret_key(&cfg.key_path, None).with_context(|| format!("failed to load SSH private key from {}", cfg.key_path))?;
    let key = PrivateKeyWithHashAlg::new(Arc::new(key_pair), None);
    let auth_result = session.authenticate_publickey(&cfg.user, key).await.context("SSH publickey authentication failed")?;
    if !matches!(auth_result, client::AuthResult::Success) {
        bail!("SSH authentication was rejected by the VPS / VPS側でSSH認証が拒否されました");
    }
    Ok(session)
}

/// 固定コマンド(`cat <path>`)のみを実行し、標準出力全体を返す。
pub async fn read_file(requested_path: &str) -> Result<String> {
    validate_remote_path(requested_path)?;
    let cfg = config_from_env()?;
    let session = connect(&cfg).await?;
    let mut channel = session.channel_open_session().await.context("failed to open SSH channel")?;
    let command = format!("cat {}", shell_quote(requested_path));
    channel.exec(true, command.as_bytes()).await.context("failed to exec remote command")?;

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_status: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
            ChannelMsg::ExtendedData { ref data, .. } => stderr.extend_from_slice(data),
            ChannelMsg::ExitStatus { exit_status: code } => exit_status = Some(code),
            ChannelMsg::Eof | ChannelMsg::Close => {}
            _ => {}
        }
    }
    match exit_status {
        Some(0) => Ok(String::from_utf8(stdout).context("remote file content is not valid UTF-8 (binary files are not supported by this endpoint)")?),
        Some(code) => bail!("remote `cat` exited with status {code}: {}", String::from_utf8_lossy(&stderr)),
        None => bail!("remote command did not report an exit status"),
    }
}

/// 固定コマンド(`cat > <path>`)のみを実行し、内容を書き込む。
pub async fn write_file(requested_path: &str, content: &str) -> Result<()> {
    validate_remote_path(requested_path)?;
    let cfg = config_from_env()?;
    let session = connect(&cfg).await?;
    let mut channel = session.channel_open_session().await.context("failed to open SSH channel")?;
    let command = format!("cat > {}", shell_quote(requested_path));
    channel.exec(true, command.as_bytes()).await.context("failed to exec remote command")?;
    channel.data(content.as_bytes()).await.context("failed to write to remote stdin")?;
    channel.eof().await.context("failed to send EOF to remote command")?;

    let mut stderr = Vec::new();
    let mut exit_status: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::ExtendedData { ref data, .. } => stderr.extend_from_slice(data),
            ChannelMsg::ExitStatus { exit_status: code } => exit_status = Some(code),
            ChannelMsg::Eof | ChannelMsg::Close => {}
            _ => {}
        }
    }
    match exit_status {
        Some(0) => Ok(()),
        Some(code) => bail!("remote write exited with status {code}: {}", String::from_utf8_lossy(&stderr)),
        None => bail!("remote command did not report an exit status"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn rejects_paths_outside_allowed_dirs_without_env() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::remove_var("OPEN_CG_CAD_VPS_ALLOWED_PATHS");
        assert!(validate_remote_path("/etc/passwd").is_err());
    }

    #[test]
    fn rejects_dotdot_traversal() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("OPEN_CG_CAD_VPS_ALLOWED_PATHS", "/home/deploy/designs");
        }
        assert!(validate_remote_path("/home/deploy/designs/../../etc/passwd").is_err());
        unsafe {
            std::env::remove_var("OPEN_CG_CAD_VPS_ALLOWED_PATHS");
        }
    }

    #[test]
    fn accepts_paths_within_allowed_dir() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("OPEN_CG_CAD_VPS_ALLOWED_PATHS", "/home/deploy/designs");
        }
        assert!(validate_remote_path("/home/deploy/designs/engine.json").is_ok());
        unsafe {
            std::env::remove_var("OPEN_CG_CAD_VPS_ALLOWED_PATHS");
        }
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("it's a path"), r#"'it'"'"'s a path'"#);
    }
}
