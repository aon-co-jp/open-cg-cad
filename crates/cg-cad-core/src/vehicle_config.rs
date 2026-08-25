//! 自動車CADコンフィグ(旧図面からの改造/エンジン換装/駆動方式変更/
//! ドア数変更/ボディスタイル変更/排出ガス規制適合チェック)。
//!
//! ユーザー指示「古いCADなどの図面から、最新の排ガス規制対応や
//! ヨーロッパ基準の対応、SUVにして、2WDから4WDへ、2ドア⇔4ドア、
//! エンジンの載せ替え」への対応。
//!
//! **⚠️ 正直な開示(最重要)**: (1) 実車の詳細な形状・寸法・特定
//! メーカーの意匠を読み取って改造する機能ではなく、`VehicleConfig`と
//! いう抽象化されたパラメータ(エンジン種別・駆動方式・ドア数・
//! ボディスタイル)を保持・変更するだけの構成管理レイヤーに過ぎない。
//! (2) 特定メーカー・特定車種の意匠・商標(ブランド名等)は模倣・
//! 使用しない——ボディスタイルは"SportCoupe"のような一般名詞のみを
//! 収録する。(3) 排出ガス規制適合チェックは、ユーザーが入力した
//! 実測/想定の排出量数値を、公開されている規制値表と比較するだけの
//! 簡易チェックであり、実車の排出ガス試験(WLTP等)を代替しない。

use thiserror::Error;

/// エンジン種別。研究構想段階(`is_experimental`が`true`)のものは、
/// 実在する物理現象として計算せず、常に`EngineComputeResult::
/// NotImplemented`を返す設計上のプレースホルダーである
/// (`cg-cad-core::structural`の`StructuralCheckResult::NotImplemented`
/// と同じ「動くふりをしない」方針を踏襲)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineType {
    /// ガソリン内燃機関。
    GasolinePetrol,
    /// ディーゼル内燃機関。
    Diesel,
    /// ハイブリッド(内燃機関+モーター)。
    HybridElectric,
    /// バッテリー電気自動車(BEV)。
    BatteryElectric,
    /// 燃料電池(水素、FCEV)。
    FuelCellHydrogen,
    /// **研究構想・未実証**: 米Astron Aerospace社が開発を表明している
    /// シリンダー・ピストンを持たないロータリーエンジン「Omega 1」の
    /// 水素対応版を指す。**正直な開示**: 同社の実際の詳細設計(内部構造・
    /// 特許情報等)は本コードに一切含まれておらず、公開されている
    /// 概要レベルの情報を基にした構成上のプレースホルダーに過ぎない
    /// (同社の知的財産を模倣・再現するものではない)。量産・第三者機関に
    /// よる性能検証が確立された技術ではないため、燃焼・出力等の物理
    /// 計算は一切行わない。
    ResearchConceptPistonlessHydrogenOmega1,
    /// **研究構想・未実証**: ジルコニア(酸化ジルコニウム、ZrO2)製
    /// エンジン。**事実関係(2026-08-25、Google検索で調査・記録)**:
    /// ジルコニアは応力誘起変態という結晶構造変化によって亀裂進行を
    /// 抑える性質を持ち、一般的なセラミック(窒化ケイ素等)より強度・
    /// 破壊靭性(しなやかさ)に優れる。融点は約2,715℃と極めて高い。
    /// 金属を含まない酸化物セラミックであるため、金属材料特有の
    /// 「水素脆化」を起こしにくく、水素環境への耐性という観点では
    /// 有利な材料特性を持つ(ただし高温多湿・腐食性環境下での
    /// 「水熱劣化」という別種の劣化機構は安定化剤の種類により起こり
    /// うる)。**一方で、エンジン全体をジルコニア(または他のセラミック)
    /// のみで構成した「オールセラミックエンジン」は実用化された前例が
    /// 無い**——1980年代に京セラ等が窒化ケイ素を用いたオールセラミック
    /// エンジンの試作・走行テストを行ったが、内部の熱を逃がしにくい等の
    /// 技術的・コスト的課題から実用化には至らなかった(現在は
    /// ディーゼル噴射装置のプランジャーやベアリング等、部分的な耐熱・
    /// 耐摩耗部品としての活用に留まる)。このため本エンジン種別は、
    /// 材料特性としては魅力的だが「エンジン全体をこの材料で作る」という
    /// 構想自体が未実証であるという二重の意味で研究構想段階として扱い、
    /// 物理計算は行わないプレースホルダーとする。
    /// **熱管理問題への対策(2026-08-25追記、Google検索で調査・記録)**:
    /// 実用化されている対策はいずれも「金属部品の表面に施すコーティング
    /// (TBC: 遮熱コーティング)」であり、「エンジンをジルコニアで丸ごと
    /// 作る」話ではない。(1) イットリア安定化ジルコニア(YSZ)を用いて
    /// 高温での相転移(体積急変・割れ)を防止、(2) YSZは他セラミックより
    /// 金属(鉄・ニッケル合金)に熱膨張率が近く、さらに`NiCoCrAlY`等の
    /// ボンドコート層を介して基材との熱膨張差を緩和、(3) 単層ではなく
    /// 多層・傾斜組成コーティングにすることで、多孔質・微細クラック構造
    /// が熱衝撃によるひずみを吸収し剥離を防ぐ。つまり熱管理問題は
    /// 「金属基材+ジルコニアTBCコーティング」というハイブリッド構成
    /// でのみ実用上解決されており、これは既に部分部品(燃焼室・タービン
    /// ブレード等)で実践されている技術であって、本エンジン種別が指す
    /// 「オールセラミック/オールジルコニア構造」の実用化を裏付ける
    /// ものではない——引き続き研究構想として扱う。
    ResearchConceptZirconiaCeramicEngine,
    /// **研究構想・未実証**: 核融合エンジンPHEV。同上、核融合発電は
    /// 現時点で実用化された民生技術ではないため、物理計算は一切行わず
    /// パラメータ保持のみのプレースホルダーとして扱う。
    ResearchConceptFusionPhev,
}

impl EngineType {
    /// 実在・検証済みの技術か、研究構想・未実証のプレースホルダーかを返す。
    pub fn is_experimental(self) -> bool {
        matches!(
            self,
            EngineType::ResearchConceptPistonlessHydrogenOmega1
                | EngineType::ResearchConceptZirconiaCeramicEngine
                | EngineType::ResearchConceptFusionPhev
        )
    }
}

/// 駆動方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drivetrain {
    TwoWheelDrive,
    FourWheelDrive,
}

/// ドア数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorCount {
    Two,
    Four,
}

/// ボディスタイル(一般名詞のみ、特定メーカー・特定車種名は使わない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyStyle {
    Sedan,
    Suv,
    SportCoupe,
    Hatchback,
    Wagon,
}

/// 車両コンフィグ(旧図面から読み取った/新規入力した抽象パラメータ)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleConfig {
    pub engine: EngineType,
    pub drivetrain: Drivetrain,
    pub doors: DoorCount,
    pub body_style: BodyStyle,
}

impl VehicleConfig {
    /// エンジンを載せ替える(他パラメータは変更しない)。
    pub fn with_engine(self, engine: EngineType) -> Self {
        VehicleConfig { engine, ..self }
    }

    /// 駆動方式を変更する(2WD⇔4WD)。
    pub fn with_drivetrain(self, drivetrain: Drivetrain) -> Self {
        VehicleConfig { drivetrain, ..self }
    }

    /// ドア数を変更する(2ドア⇔4ドア)。
    pub fn with_doors(self, doors: DoorCount) -> Self {
        VehicleConfig { doors, ..self }
    }

    /// ボディスタイルを変更する(例: SUV化)。
    pub fn with_body_style(self, body_style: BodyStyle) -> Self {
        VehicleConfig { body_style, ..self }
    }
}

/// エンジン計算結果。研究構想エンジンは常に`NotImplemented`を返す。
#[derive(Debug, Clone, PartialEq)]
pub enum EngineComputeResult {
    /// 実在技術の簡易出力推定(kW、非常に粗い参考値)。
    EstimatedOutputKw(f64),
    /// 研究構想・未実証のため計算未実装(理由付き)。
    NotImplemented(String),
}

/// エンジン種別から簡易出力推定を返す(実在技術のみ、非常に粗い参考値
/// ——排気量やモーター定格等の詳細パラメータを持たないため、
/// あくまで「このエンジン種別は動く」ことのデータ配線検証用)。
pub fn estimate_engine_output_kw(engine: EngineType) -> EngineComputeResult {
    match engine {
        EngineType::GasolinePetrol => EngineComputeResult::EstimatedOutputKw(110.0),
        EngineType::Diesel => EngineComputeResult::EstimatedOutputKw(90.0),
        EngineType::HybridElectric => EngineComputeResult::EstimatedOutputKw(135.0),
        EngineType::BatteryElectric => EngineComputeResult::EstimatedOutputKw(150.0),
        EngineType::FuelCellHydrogen => EngineComputeResult::EstimatedOutputKw(130.0),
        EngineType::ResearchConceptPistonlessHydrogenOmega1 => EngineComputeResult::NotImplemented(
            "Astron Aerospace社のOmega 1(ピストンレス水素ロータリーエンジン)構想は研究構想・未実証のため出力計算は未実装です。公開概要情報を基にしたパラメータ入力の保持のみ対応しています。".to_string(),
        ),
        EngineType::ResearchConceptZirconiaCeramicEngine => EngineComputeResult::NotImplemented(
            "ジルコニアは材料としては高強度・高靭性・水素脆化を起こしにくい(金属を含まないため)という有利な特性を持ちますが、エンジン全体をジルコニア/セラミックのみで構成する「オールセラミックエンジン」自体が実用化された前例が無い(1980年代の京セラ等の試作も熱管理課題で断念)研究構想のため、出力計算は未実装です。パラメータ入力の保持のみ対応しています。".to_string(),
        ),
        EngineType::ResearchConceptFusionPhev => EngineComputeResult::NotImplemented(
            "核融合エンジンPHEVは研究構想・未実証(民生実用化技術ではない)のため出力計算は未実装です。パラメータ入力の保持のみ対応しています。".to_string(),
        ),
    }
}

/// 排出ガス規制(ユーザー入力の実測/想定値と比較する簡易チェック用の
/// 規制値。`zoning::ZoningRules`と同じ「入力値ベース、自動取得はしない」
/// 設計方針)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmissionStandard {
    /// EU Euro 6(乗用車、ガソリン/ディーゼル)。
    Euro6,
    /// 日本 ポスト新長期規制。
    JapanPostNewLongTerm,
    /// 米国 Tier 3。
    UsTier3,
}

/// 規制値(g/km、代表的な公開値の簡易近似。**正直な開示**: 実際の規制は
/// 車両重量区分・試験モード(WLTC等)により細分化されるが、ここでは
/// 単一の代表値のみを収録する簡易チェック)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmissionLimits {
    pub nox_g_per_km: f64,
    pub pm_g_per_km: f64,
}

pub fn emission_limits_for(standard: EmissionStandard) -> EmissionLimits {
    match standard {
        EmissionStandard::Euro6 => EmissionLimits { nox_g_per_km: 0.08, pm_g_per_km: 0.0045 },
        EmissionStandard::JapanPostNewLongTerm => EmissionLimits { nox_g_per_km: 0.05, pm_g_per_km: 0.005 },
        EmissionStandard::UsTier3 => EmissionLimits { nox_g_per_km: 0.03, pm_g_per_km: 0.003 },
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum EmissionCheckError {
    #[error("measured NOx {measured} g/km must be non-negative")]
    NegativeNox { measured: f64 },
    #[error("measured PM {measured} g/km must be non-negative")]
    NegativePm { measured: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmissionCheckResult {
    pub compliant: bool,
    pub nox_margin_g_per_km: f64,
    pub pm_margin_g_per_km: f64,
}

/// ユーザーが入力した実測/想定の排出量(g/km)を、指定の規制値と比較する。
/// バッテリー電気自動車(BEV)・燃料電池車(FCEV)はテールパイプ排出が
/// 無いため常に適合とする(この2種別は`measured_*`引数を無視する)。
pub fn check_emission_compliance(
    engine: EngineType,
    standard: EmissionStandard,
    measured_nox_g_per_km: f64,
    measured_pm_g_per_km: f64,
) -> Result<EmissionCheckResult, EmissionCheckError> {
    if matches!(engine, EngineType::BatteryElectric | EngineType::FuelCellHydrogen) {
        return Ok(EmissionCheckResult { compliant: true, nox_margin_g_per_km: f64::INFINITY, pm_margin_g_per_km: f64::INFINITY });
    }
    if measured_nox_g_per_km < 0.0 {
        return Err(EmissionCheckError::NegativeNox { measured: measured_nox_g_per_km });
    }
    if measured_pm_g_per_km < 0.0 {
        return Err(EmissionCheckError::NegativePm { measured: measured_pm_g_per_km });
    }
    let limits = emission_limits_for(standard);
    let nox_margin = limits.nox_g_per_km - measured_nox_g_per_km;
    let pm_margin = limits.pm_g_per_km - measured_pm_g_per_km;
    Ok(EmissionCheckResult { compliant: nox_margin >= 0.0 && pm_margin >= 0.0, nox_margin_g_per_km: nox_margin, pm_margin_g_per_km: pm_margin })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> VehicleConfig {
        VehicleConfig { engine: EngineType::GasolinePetrol, drivetrain: Drivetrain::TwoWheelDrive, doors: DoorCount::Four, body_style: BodyStyle::Sedan }
    }

    #[test]
    fn engine_swap_changes_only_the_engine_field() {
        let original = base_config();
        let swapped = original.with_engine(EngineType::BatteryElectric);
        assert_eq!(swapped.engine, EngineType::BatteryElectric);
        assert_eq!(swapped.drivetrain, original.drivetrain);
        assert_eq!(swapped.doors, original.doors);
        assert_eq!(swapped.body_style, original.body_style);
    }

    #[test]
    fn drivetrain_conversion_2wd_to_4wd() {
        let cfg = base_config().with_drivetrain(Drivetrain::FourWheelDrive);
        assert_eq!(cfg.drivetrain, Drivetrain::FourWheelDrive);
    }

    #[test]
    fn door_count_conversion_and_suv_body_style_change() {
        let cfg = base_config().with_doors(DoorCount::Two).with_body_style(BodyStyle::Suv);
        assert_eq!(cfg.doors, DoorCount::Two);
        assert_eq!(cfg.body_style, BodyStyle::Suv);
    }

    #[test]
    fn real_engines_return_an_output_estimate() {
        for engine in [EngineType::GasolinePetrol, EngineType::Diesel, EngineType::HybridElectric, EngineType::BatteryElectric, EngineType::FuelCellHydrogen] {
            assert!(!engine.is_experimental());
            assert!(matches!(estimate_engine_output_kw(engine), EngineComputeResult::EstimatedOutputKw(kw) if kw > 0.0));
        }
    }

    #[test]
    fn research_concept_engines_are_honestly_not_implemented() {
        for engine in [
            EngineType::ResearchConceptPistonlessHydrogenOmega1,
            EngineType::ResearchConceptZirconiaCeramicEngine,
            EngineType::ResearchConceptFusionPhev,
        ] {
            assert!(engine.is_experimental());
            match estimate_engine_output_kw(engine) {
                EngineComputeResult::NotImplemented(reason) => assert!(!reason.is_empty()),
                other => panic!("expected NotImplemented for experimental engine, got {other:?}"),
            }
        }
    }

    #[test]
    fn battery_electric_and_fuel_cell_are_always_emission_compliant() {
        for engine in [EngineType::BatteryElectric, EngineType::FuelCellHydrogen] {
            let result = check_emission_compliance(engine, EmissionStandard::Euro6, 999.0, 999.0).unwrap();
            assert!(result.compliant);
        }
    }

    #[test]
    fn gasoline_within_euro6_limits_is_compliant() {
        let result = check_emission_compliance(EngineType::GasolinePetrol, EmissionStandard::Euro6, 0.05, 0.003).unwrap();
        assert!(result.compliant);
        assert!(result.nox_margin_g_per_km > 0.0);
        assert!(result.pm_margin_g_per_km > 0.0);
    }

    #[test]
    fn old_diesel_exceeding_euro6_limits_is_rejected() {
        // 旧CAD由来の古いディーゼルの想定実測値(Euro 6以前の水準)。
        let result = check_emission_compliance(EngineType::Diesel, EmissionStandard::Euro6, 0.18, 0.02).unwrap();
        assert!(!result.compliant);
        assert!(result.nox_margin_g_per_km < 0.0);
    }

    #[test]
    fn negative_measured_emissions_are_rejected_honestly() {
        assert_eq!(
            check_emission_compliance(EngineType::GasolinePetrol, EmissionStandard::Euro6, -1.0, 0.0),
            Err(EmissionCheckError::NegativeNox { measured: -1.0 })
        );
        assert_eq!(
            check_emission_compliance(EngineType::GasolinePetrol, EmissionStandard::Euro6, 0.0, -1.0),
            Err(EmissionCheckError::NegativePm { measured: -1.0 })
        );
    }

    #[test]
    fn stricter_standards_have_lower_or_equal_limits_than_euro6() {
        let euro6 = emission_limits_for(EmissionStandard::Euro6);
        let us_tier3 = emission_limits_for(EmissionStandard::UsTier3);
        assert!(us_tier3.nox_g_per_km <= euro6.nox_g_per_km);
    }
}
