//! `aruaru-llm`(既存のHTTP API、`POST /v1/generate`)と連携し、
//! [`crate::structural`]が求めた**計算済みの数値結果**を自然言語の
//! 説明文へ変換する。
//!
//! # LLMの役割(必読、`structural`モジュールのdocコメントと同じ方針)
//!
//! **LLMは構造計算を一切行わない**。層間変形角・判定結果等の数値は
//! すべて[`crate::structural::analyze_structure`](線形代数+
//! `opencuda-blas`のGEMM)が既に計算済みであり、このモジュールが
//! `aruaru-llm`へ渡すのはその計算結果を文章化させるための説明用
//! プロンプトのみである。物理法則に基づく正確な数値計算をLLMに
//! 保証させることはできないため、この役割分担を意図的に維持している。
//!
//! # 免責事項
//!
//! `aruaru-llm`(GPT-2 124M系、事前学習のみで対話ファインチューニング
//! 無し)が生成する説明文は、参考情報に過ぎない。正式な構造計算書では
//! なく、必ず一級建築士等の有資格者による確認が必要——この免責事項は
//! 生成された説明文自体にも常に付加する(`ensure_disclaimer_appended`)。

use crate::crash_sim::CrashSimResult;
use crate::structural::StructuralAnalysisResult;
use crate::tunnel_noise::NoseOptimizationResult;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmExplainError {
    #[error("failed to reach aruaru-llm at {url}: {source}")]
    Request { url: String, #[source] source: reqwest::Error },
    #[error("aruaru-llm returned a non-success status {status} for {url}: {body}")]
    NonSuccessStatus { url: String, status: u16, body: String },
}

#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    prompt: &'a str,
    max_new_tokens: usize,
}

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    completion: String,
    #[allow(dead_code)]
    engine: String,
    #[allow(dead_code)]
    disclosure: String,
}

/// この免責事項は、`aruaru-llm`が生成した説明文の末尾に必ず付加する
/// (`structural`モジュールの免責事項と同一の原則、日本語)。
pub const CG_CAD_DISCLAIMER_JA: &str = "\n\n[免責事項] この説明文はAI(aruaru-llm)による参考情報であり、\
正式な構造計算書ではありません。実際の設計・施工には、必ず一級建築士等の有資格者による確認が必要です。\
また元となる数値は、専門の構造計算ソフトウェアの代替ではない簡易的な線形静解析(せん断建物モデル)による\
ドラフト値です。";

/// [`StructuralAnalysisResult`]から、aruaru-llmへ渡す説明依頼プロンプト
/// (日本語)を組み立てる。**計算はすでに完了しており、ここではその
/// 数値をテキスト化するよう依頼するだけ**であることをプロンプト自体にも
/// 明記し、LLM側が新たに数値を「計算」しようとしないよう誘導する。
pub fn build_explanation_prompt(result: &StructuralAnalysisResult, seismic_intensity_label: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "以下は、Rustの線形代数コードで既に計算済みの簡易構造計算結果です(あなたは計算をやり直す必要はありません)。\
         この数値結果を、専門知識のない施主にも分かりやすい日本語の説明文として要約してください。\
         震度の目安: {seismic_intensity_label}。"
    ));
    for drift in &result.story_drifts {
        lines.push(format!(
            "{}階: 水平変位 {:.4}m, 層間変形角 {:.5} (目安1/{:.0}に対し{}判定)",
            drift.story,
            drift.displacement_m,
            drift.drift_angle,
            1.0 / result.drift_limit,
            if drift.within_limit { "適合" } else { "不適合" },
        ));
    }
    lines.push(format!("全体判定: {}", if result.all_within_limit { "目安の範囲内" } else { "目安を超過する階あり" }));
    lines.join("\n")
}

/// `aruaru-llm`の`POST /v1/generate`を実際に呼び出し、説明文を取得する。
///
/// `base_url`: 例 `http://127.0.0.1:4600`(`aruaru-llm`のCLAUDE.md記載の
/// 既定ポート)。同期(blocking)HTTPクライアントを使う——本クレート
/// (`cg-cad-core`)は非同期ランタイムに依存しない設計のため。
pub fn explain_via_aruaru_llm(base_url: &str, result: &StructuralAnalysisResult, seismic_intensity_label: &str) -> Result<String, LlmExplainError> {
    let prompt = build_explanation_prompt(result, seismic_intensity_label);
    let completion = call_generate(base_url, &prompt)?;
    Ok(format!("{}{}", completion, CG_CAD_DISCLAIMER_JA))
}

/// この免責事項は、`aruaru-llm`が生成した衝突シミュレーション説明文の
/// 末尾に必ず付加する。
pub const CRASH_SIM_DISCLAIMER_JA: &str = "\n\n[免責事項] この説明文はAI(aruaru-llm)による参考情報であり、\
正式な衝突安全性評価報告書ではありません。実際の車両設計・安全性認証には、必ず有資格の衝突安全性評価技術者による\
確認が必要です。また元となる数値は、専門の衝突解析ソフトウェア(LS-DYNA等の非線形FEM)の代替ではない簡易的な\
質点+ばね+ダンパーモデルによるドラフト値です。";

/// この免責事項は、`aruaru-llm`が生成したトンネル微気圧波説明文の末尾に
/// 必ず付加する。
pub const TUNNEL_NOISE_DISCLAIMER_JA: &str = "\n\n[免責事項] この説明文はAI(aruaru-llm)による参考情報であり、\
正式なCFD解析報告書ではありません。実際の車両設計には、必ず鉄道工学の有資格技術者による確認が必要です。\
また元となる数値は、専門の数値流体力学(CFD)ソフトウェアの代替ではない、断面積変化率を指標とする簡易的な\
1次元近似によるドラフト値です。";

/// [`CrashSimResult`]から、aruaru-llmへ渡す説明依頼プロンプト(日本語)を
/// 組み立てる(`build_explanation_prompt`と同じ方針、計算はやり直させない)。
pub fn build_crash_explanation_prompt(result: &CrashSimResult, vehicle_label: &str) -> String {
    format!(
        "以下は、Rustの数値積分コードで既に計算済みの簡易衝突シミュレーション結果です\
         (あなたは計算をやり直す必要はありません)。この数値結果を、専門知識のない読者にも\
         分かりやすい日本語の説明文として要約してください。対象車両: {vehicle_label}。\n\
         最大加速度: {:.2} m/s^2 ({:.1} G)\n\
         最大圧壊変位: {:.4} m",
        result.max_acceleration_mps2, result.max_acceleration_g, result.max_crush_displacement_m,
    )
}

/// `aruaru-llm`の`POST /v1/generate`を実際に呼び出し、衝突シミュレーション
/// 結果の説明文を取得する(`explain_via_aruaru_llm`と同じHTTP連携方式)。
pub fn explain_crash_via_aruaru_llm(base_url: &str, result: &CrashSimResult, vehicle_label: &str) -> Result<String, LlmExplainError> {
    let prompt = build_crash_explanation_prompt(result, vehicle_label);
    let completion = call_generate(base_url, &prompt)?;
    Ok(format!("{}{}", completion, CRASH_SIM_DISCLAIMER_JA))
}

/// [`NoseOptimizationResult`]から、aruaru-llmへ渡す説明依頼プロンプト
/// (日本語)を組み立てる。
pub fn build_tunnel_noise_explanation_prompt(result: &NoseOptimizationResult) -> String {
    let mut lines = Vec::new();
    lines.push(
        "以下は、Rustのコードで既に計算済みの簡易トンネル微気圧波シミュレーション結果です\
         (あなたは計算をやり直す必要はありません)。この数値結果を、専門知識のない読者にも\
         分かりやすい日本語の説明文として要約してください。"
            .to_string(),
    );
    for c in &result.candidates {
        lines.push(format!("先頭形状パラメータ power={:.1}: 微気圧波指標={:.4}", c.shape_power, c.micro_pressure_wave_index));
    }
    lines.push(format!(
        "最も指標が小さい(=なめらか)推奨形状: power={:.1} (指標={:.4})",
        result.best.shape_power, result.best.micro_pressure_wave_index
    ));
    lines.join("\n")
}

/// `aruaru-llm`の`POST /v1/generate`を実際に呼び出し、トンネル微気圧波
/// シミュレーション結果の説明文を取得する。
pub fn explain_tunnel_noise_via_aruaru_llm(base_url: &str, result: &NoseOptimizationResult) -> Result<String, LlmExplainError> {
    let prompt = build_tunnel_noise_explanation_prompt(result);
    let completion = call_generate(base_url, &prompt)?;
    Ok(format!("{}{}", completion, TUNNEL_NOISE_DISCLAIMER_JA))
}

/// `aruaru-llm`の`POST /v1/generate`を呼び出す共通処理
/// (`explain_via_aruaru_llm`から抽出、3つの説明関数で重複させない)。
fn call_generate(base_url: &str, prompt: &str) -> Result<String, LlmExplainError> {
    let url = format!("{}/v1/generate", base_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("reqwest blocking client must build with a simple timeout config");
    let response = client
        .post(&url)
        .json(&GenerateRequest { prompt, max_new_tokens: 96 })
        .send()
        .map_err(|source| LlmExplainError::Request { url: url.clone(), source })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(LlmExplainError::NonSuccessStatus { url, status: status.as_u16(), body });
    }

    let parsed: GenerateResponse = response.json().map_err(|source| LlmExplainError::Request { url: url.clone(), source })?;
    Ok(parsed.completion)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural::StoryDriftResult;

    fn sample_result() -> StructuralAnalysisResult {
        StructuralAnalysisResult {
            story_drifts: vec![
                StoryDriftResult { story: 1, displacement_m: 0.01, drift_angle: 0.0033, within_limit: true },
                StoryDriftResult { story: 2, displacement_m: 0.02, drift_angle: 0.0033, within_limit: true },
            ],
            all_within_limit: true,
            drift_limit: crate::structural::DEFAULT_DRIFT_LIMIT,
            disclaimer: "test",
        }
    }

    #[test]
    fn prompt_includes_all_story_numbers_and_does_not_ask_llm_to_recompute() {
        let result = sample_result();
        let prompt = build_explanation_prompt(&result, "震度6強相当");
        assert!(prompt.contains("1階"));
        assert!(prompt.contains("2階"));
        assert!(prompt.contains("既に計算済み"));
        assert!(prompt.contains("震度6強相当"));
    }

    #[test]
    fn disclaimer_is_always_appendable_and_non_empty() {
        assert!(!CG_CAD_DISCLAIMER_JA.trim().is_empty());
        assert!(CG_CAD_DISCLAIMER_JA.contains("有資格者"));
    }

    #[test]
    fn crash_prompt_includes_key_numbers_and_does_not_ask_llm_to_recompute() {
        use crate::crash_sim::CrashSimResult;
        let result = CrashSimResult {
            steps: vec![],
            max_acceleration_mps2: 588.0,
            max_acceleration_g: 60.0,
            max_crush_displacement_m: 0.35,
            disclaimer: "test",
        };
        let prompt = build_crash_explanation_prompt(&result, "普通乗用車");
        assert!(prompt.contains("60.0"));
        assert!(prompt.contains("0.3500") || prompt.contains("0.35"));
        assert!(prompt.contains("既に計算済み"));
        assert!(prompt.contains("普通乗用車"));
    }

    #[test]
    fn crash_disclaimer_is_non_empty_and_mentions_qualified_engineer() {
        assert!(!CRASH_SIM_DISCLAIMER_JA.trim().is_empty());
        assert!(CRASH_SIM_DISCLAIMER_JA.contains("有資格"));
    }

    #[test]
    fn tunnel_noise_prompt_includes_best_candidate_and_does_not_ask_llm_to_recompute() {
        use crate::tunnel_noise::NoseShapeCandidateResult;
        let result = NoseOptimizationResult {
            candidates: vec![
                NoseShapeCandidateResult { shape_power: 1.0, micro_pressure_wave_index: 0.8 },
                NoseShapeCandidateResult { shape_power: 2.0, micro_pressure_wave_index: 1.6 },
            ],
            best: NoseShapeCandidateResult { shape_power: 1.0, micro_pressure_wave_index: 0.8 },
            disclaimer: "test",
        };
        let prompt = build_tunnel_noise_explanation_prompt(&result);
        assert!(prompt.contains("既に計算済み"));
        assert!(prompt.contains("power=1.0"));
        assert!(prompt.contains("推奨形状"));
    }

    #[test]
    fn tunnel_noise_disclaimer_is_non_empty_and_mentions_qualified_engineer() {
        assert!(!TUNNEL_NOISE_DISCLAIMER_JA.trim().is_empty());
        assert!(TUNNEL_NOISE_DISCLAIMER_JA.contains("有資格"));
    }

    /// 実aruaru-llmサーバーが到達可能な場合のみ実行する統合テスト
    /// (実行方法: `ARUARU_LLM_TEST_URL=http://127.0.0.1:4600 cargo test
    /// -p cg-cad-core --release -- --ignored --nocapture`)。CI環境や
    /// サーバー未起動時は`#[ignore]`によりデフォルトではスキップされる
    /// ——スキップを誤魔化さず、実行方法をテスト内にも明記する。
    #[test]
    #[ignore = "requires a running aruaru-llm server; see ARUARU_LLM_TEST_URL"]
    fn explains_via_real_aruaru_llm_server() {
        let base_url = std::env::var("ARUARU_LLM_TEST_URL").unwrap_or_else(|_| "http://127.0.0.1:4600".to_string());
        let result = sample_result();
        let explanation = explain_via_aruaru_llm(&base_url, &result, "震度6強相当").expect("aruaru-llm request must succeed");
        assert!(!explanation.trim().is_empty());
        assert!(explanation.contains("有資格者"), "explanation must always carry the disclaimer: {explanation}");
    }

    /// 実aruaru-llmサーバーへの衝突シミュレーション説明文生成の統合テスト
    /// (実行方法は`explains_via_real_aruaru_llm_server`と同じ)。
    #[test]
    #[ignore = "requires a running aruaru-llm server; see ARUARU_LLM_TEST_URL"]
    fn explains_crash_via_real_aruaru_llm_server() {
        use crate::crash_sim::CrashSimResult;
        let base_url = std::env::var("ARUARU_LLM_TEST_URL").unwrap_or_else(|_| "http://127.0.0.1:4600".to_string());
        let result = CrashSimResult {
            steps: vec![],
            max_acceleration_mps2: 588.0,
            max_acceleration_g: 60.0,
            max_crush_displacement_m: 0.35,
            disclaimer: "test",
        };
        let explanation = explain_crash_via_aruaru_llm(&base_url, &result, "普通乗用車").expect("aruaru-llm request must succeed");
        assert!(!explanation.trim().is_empty());
        assert!(explanation.contains("有資格"), "explanation must always carry the disclaimer: {explanation}");
    }

    /// 実aruaru-llmサーバーへのトンネル微気圧波説明文生成の統合テスト
    /// (実行方法は`explains_via_real_aruaru_llm_server`と同じ)。
    #[test]
    #[ignore = "requires a running aruaru-llm server; see ARUARU_LLM_TEST_URL"]
    fn explains_tunnel_noise_via_real_aruaru_llm_server() {
        use crate::tunnel_noise::NoseShapeCandidateResult;
        let base_url = std::env::var("ARUARU_LLM_TEST_URL").unwrap_or_else(|_| "http://127.0.0.1:4600".to_string());
        let result = NoseOptimizationResult {
            candidates: vec![
                NoseShapeCandidateResult { shape_power: 1.0, micro_pressure_wave_index: 0.8 },
                NoseShapeCandidateResult { shape_power: 2.0, micro_pressure_wave_index: 1.6 },
            ],
            best: NoseShapeCandidateResult { shape_power: 1.0, micro_pressure_wave_index: 0.8 },
            disclaimer: "test",
        };
        let explanation = explain_tunnel_noise_via_aruaru_llm(&base_url, &result).expect("aruaru-llm request must succeed");
        assert!(!explanation.trim().is_empty());
        assert!(explanation.contains("有資格"), "explanation must always carry the disclaimer: {explanation}");
    }
}
