//! 構造計算・耐震シミュレーション(プレースホルダ、未実装)。
//!
//! **正直な開示**: 構造計算(有限要素法等による応力解析)・耐震性の
//! 実シミュレーションは、専門の構造計算ソフトウェア(構造計算適合性
//! 判定を受けられる専用ツール)が担うべき領域であり、本クレートの
//! MVPスコープでは実装していない。このモジュールは「未実装であることを
//! 明示するプレースホルダ」であり、`StructuralCheckResult::NotImplemented`
//! を返す以外の動作をしない——動くふりのコード(ダミーの数値を返して
//! 「耐震性OK」等と偽装すること)は書いていない。

/// 構造計算・耐震シミュレーションの結果。
#[derive(Debug, Clone, PartialEq)]
pub enum StructuralCheckResult {
    /// 未実装であることを明示する唯一のバリアント。専門の構造計算
    /// ソフトウェアとの連携が必要(将来課題)。
    NotImplemented { reason: &'static str },
}

/// 耐震性・構造計算の簡易チェック(プレースホルダ)。
///
/// 常に`NotImplemented`を返す——呼び出し側は、この結果を「構造的に
/// 安全」であることの根拠として扱ってはならない。
pub fn check_structural_safety() -> StructuralCheckResult {
    StructuralCheckResult::NotImplemented {
        reason: "structural calculation and seismic simulation require dedicated professional \
                 engineering software and a licensed structural engineer's review; not implemented \
                 in this draft-generation MVP",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_check_is_honestly_unimplemented_not_a_fake_pass() {
        let result = check_structural_safety();
        assert_eq!(
            result,
            StructuralCheckResult::NotImplemented {
                reason: "structural calculation and seismic simulation require dedicated professional \
                         engineering software and a licensed structural engineer's review; not implemented \
                         in this draft-generation MVP",
            }
        );
    }
}
