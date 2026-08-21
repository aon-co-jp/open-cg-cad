//! 既存の実在・検証済みオープンソースCAE/CFDエンジン(CalculiX・OpenFOAM)
//! を子プロセスとして呼び出すための、入出力・実行の薄い統合層。
//!
//! # 設計方針(必読、`lib.rs`冒頭の全体免責事項・タスク指示に基づく)
//!
//! **このモジュール自体は計算エンジンを持たない。** `structural.rs`
//! (簡易せん断建物モデル)・`crash_sim.rs`(質点+ばね+ダンパーモデル)・
//! `tunnel_noise.rs`(断面積分布近似)はいずれも簡易近似モデルであり、
//! 本格的な非線形FEM衝突解析(LS-DYNA相当)・3次元CFD(Navier-Stokes)の
//! 代替ではないことをそれぞれのdocコメントで明記済みだった。本モジュールは
//! それらに代えて、**実在・検証済みのオープンソースエンジンである
//! CalculiX(非線形FEM、`calculix`サブモジュール)・OpenFOAM(CFD、
//! `openfoam`サブモジュール)を子プロセスとして呼び出し**、
//! - open-cg-cadの内部データ(`Building`/`Mesh`)を各エンジンの入力形式へ
//!   変換するエクスポータ
//! - 実行バイナリを子プロセスとして起動するランナー
//! - 各エンジンの出力形式(CalculiXの`.frd`、OpenFOAMのログ/フィールド)
//!   をRust側の構造体へ変換するパーサー
//!
//! という3つの役割に徹する「入出力ブリッジ」である。OpenFOAM/CalculiX
//! 自体の計算機能を模倣・再実装することは一切行わない。
//!
//! # この環境での実在確認結果(2026-08-21、正直な開示)
//!
//! この開発機(Windows 11)には、ネイティブ版・WSL(Ubuntu)版のいずれにも
//! `ccx`(CalculiX)・`simpleFoam`/`blockMesh`(OpenFOAM)は**インストール
//! されていない**(`where ccx`・`wsl -e which ccx`等で確認、いずれも
//! 該当なし)。タスク指示により新規インストールは行っていない。
//! そのため本モジュールの各関数は、**「バイナリが実在すればそのまま
//! 動く」設計としてコードは完成させたが、この環境では実際にソルバーを
//! 起動しての実機検証はできていない**——動くふりを避けるため、
//! バイナリが見つからない場合は[`SolverError::BinaryNotFound`]を
//! 正直に返すことを単体テストで保証している(各サブモジュールの
//! テスト参照)。エクスポータ(入力ファイル生成)・パーサー(出力
//! ファイル解析)は、バイナリの実行を介さずに実際にファイルを生成・
//! 解析できることをこのセッションで実機(この環境上のRustコード実行)
//! 検証済み。
//!
//! # 実行バイナリのパス設定
//!
//! 環境変数で上書き可能(既定値はPATH上のコマンド名をそのまま使う):
//! - `OPEN_CG_CAD_CCX_BIN`(既定`ccx`): CalculiXソルバー本体
//! - `OPEN_CG_CAD_BLOCKMESH_BIN`(既定`blockMesh`): OpenFOAMメッシュ生成
//! - `OPEN_CG_CAD_SIMPLEFOAM_BIN`(既定`simpleFoam`): OpenFOAM定常流ソルバー
//!
//! # 今後ユーザー側でインストールした場合の利用手順
//!
//! 1. CalculiX(Linux/WSL版が一般的、Windowsネイティブ版も配布あり)を
//!    インストールし、`ccx`コマンドがPATH上で実行可能であることを
//!    確認する(`ccx -v`等)。別パスに置く場合は`OPEN_CG_CAD_CCX_BIN`に
//!    フルパスを設定する。
//! 2. OpenFOAM(WSL経由が公式に推奨されている、`OpenFOAM/OpenFOAM-dev`の
//!    公式インストール手順に従う)をインストールし、`blockMesh`・
//!    `simpleFoam`がPATH上で実行可能であることを確認する。
//! 3. 上記のいずれも、本モジュールの`export_*`関数でケースディレクトリ/
//!    入力ファイルを生成し、`run_*`関数を呼べばそのまま動く設計に
//!    なっている(コード変更は不要)。

pub mod calculix;
pub mod openfoam;

use std::path::{Path, PathBuf};
use std::process::Output;
use thiserror::Error;

/// ソルバー連携の共通エラー型。
#[derive(Debug, Error)]
pub enum SolverError {
    /// 実行バイナリが見つからない(PATH上にも指定パスにも存在しない)。
    /// **正直な開示のための最重要バリアント**——バイナリが無い環境では
    /// 必ずこれを返し、計算が行われたかのような結果を捏造しない。
    #[error("solver binary not found: {0} (set the corresponding OPEN_CG_CAD_*_BIN env var, or install it; see solver_bridge module docs)")]
    BinaryNotFound(String),
    /// 子プロセスの起動・入出力に関するI/Oエラー。
    #[error("io error while running solver: {0}")]
    Io(#[from] std::io::Error),
    /// 子プロセスが非ゼロ終了コードで終了した。
    #[error("solver process exited with non-zero status {status:?}, stderr: {stderr}")]
    ProcessFailed { status: Option<i32>, stderr: String },
    /// 出力ファイルの解析に失敗した(想定される形式と一致しない)。
    #[error("failed to parse solver output: {0}")]
    ParseError(String),
}

/// 環境変数からバイナリパスを解決する(未設定なら既定のコマンド名を
/// そのまま使う、PATH解決はOS/`std::process::Command`に委ねる)。
fn resolve_binary(env_var: &str, default_name: &str) -> PathBuf {
    std::env::var(env_var).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(default_name))
}

/// バイナリが実際にこの環境で実行可能かを判定する(`--help`相当を
/// 軽量に叩く方法は各ソルバーで挙動が異なるため、ここでは
/// `which`/`where`相当の存在確認をOS非依存に行う: 絶対パスならファイル
/// 存在チェック、コマンド名のみならPATH上を走査する)。
fn binary_exists(bin: &Path) -> bool {
    if bin.components().count() > 1 || bin.is_absolute() {
        return bin.is_file();
    }
    let name = bin.to_string_lossy();
    let candidates: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), format!("{name}.bat"), name.to_string()]
    } else {
        vec![name.to_string()]
    };
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(sep) {
            for cand in &candidates {
                if PathBuf::from(dir).join(cand).is_file() {
                    return true;
                }
            }
        }
    }
    false
}

/// 子プロセスを実行し、非ゼロ終了コードなら[`SolverError::ProcessFailed`]
/// を返す共通ヘルパー。
fn run_checked(mut cmd: std::process::Command, binary_label: &str) -> Result<Output, SolverError> {
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            SolverError::BinaryNotFound(binary_label.to_string())
        } else {
            SolverError::Io(e)
        }
    })?;
    if !output.status.success() {
        return Err(SolverError::ProcessFailed { status: output.status.code(), stderr: String::from_utf8_lossy(&output.stderr).to_string() });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_binary_uses_env_var_when_set() {
        std::env::set_var("OPEN_CG_CAD_TEST_BIN_VAR", "/some/custom/path/ccx");
        let resolved = resolve_binary("OPEN_CG_CAD_TEST_BIN_VAR", "ccx");
        assert_eq!(resolved, PathBuf::from("/some/custom/path/ccx"));
        std::env::remove_var("OPEN_CG_CAD_TEST_BIN_VAR");
    }

    #[test]
    fn resolve_binary_falls_back_to_default_name_when_unset() {
        std::env::remove_var("OPEN_CG_CAD_TEST_BIN_VAR_UNSET");
        let resolved = resolve_binary("OPEN_CG_CAD_TEST_BIN_VAR_UNSET", "ccx");
        assert_eq!(resolved, PathBuf::from("ccx"));
    }

    #[test]
    fn binary_exists_is_false_for_definitely_absent_binary() {
        let bin = PathBuf::from("open_cg_cad_definitely_nonexistent_solver_binary_xyz");
        assert!(!binary_exists(&bin));
    }

    #[test]
    fn run_checked_reports_binary_not_found_honestly() {
        let cmd = std::process::Command::new("open_cg_cad_definitely_nonexistent_solver_binary_xyz");
        let err = run_checked(cmd, "open_cg_cad_definitely_nonexistent_solver_binary_xyz").unwrap_err();
        assert!(matches!(err, SolverError::BinaryNotFound(_)), "expected BinaryNotFound, got {err:?}");
    }
}
