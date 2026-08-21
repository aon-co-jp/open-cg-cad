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

use crate::structural::StructuralAnalysisResult;
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
    let url = format!("{}/v1/generate", base_url.trim_end_matches('/'));

    // GPT-2系の貪欲デコードはCPU実行だと数十秒かかることがある
    // (aruaru-llm側CLAUDE.mdの実測記録、`max_new_tokens=96`程度で
    // 数十秒に達する場合がある)ため、既定より長めのタイムアウトを
    // 明示的に設定する。
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .expect("reqwest blocking client must build with a simple timeout config");
    let response = client
        .post(&url)
        .json(&GenerateRequest { prompt: &prompt, max_new_tokens: 96 })
        .send()
        .map_err(|source| LlmExplainError::Request { url: url.clone(), source })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(LlmExplainError::NonSuccessStatus { url, status: status.as_u16(), body });
    }

    let parsed: GenerateResponse = response.json().map_err(|source| LlmExplainError::Request { url: url.clone(), source })?;
    Ok(format!("{}{}", parsed.completion, CG_CAD_DISCLAIMER_JA))
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
}
