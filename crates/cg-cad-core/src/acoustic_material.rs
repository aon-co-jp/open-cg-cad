//! 音響特性・音質向上研究対応: 内装材(木材種・吸音パネル等)のデータベースと、
//! 部屋全体の残響時間(RT60)の簡易推定(映画館・コンサートホール・
//! IMAX/4DXシアター・オーディオルーム等、`BuildingType::Theater`/
//! `AudioRoom`/`CinemaComplex`向けの音響設計叩き台、ユーザー指示
//! 「吸音パネルや吸音材や硬い木材から柔らかい木材やその木の種類の
//! DATABASEにする機能を搭載」への対応)。
//!
//! **⚠️ 正直な開示(最重要)**: ここで用いる吸音率(NRC/`alpha`)・
//! ヤンカ硬度は、公開されている建築音響学の教科書・製品カタログで
//! 広く引用される**代表的な参考値**であり、特定製品の実測値・特定の木材
//! サンプルのロット依存差(含水率・木目・仕上げ加工)は一切考慮していない。
//! 残響時間の計算も**Sabineの残響式**(拡散音場を仮定した最も基本的な
//! 近似式、周波数特性・部屋形状・空気吸収は考慮しない)に留まる。
//! 本格的な室内音響シミュレーション(音線追跡・波動音響解析等)ではなく、
//! あくまで「この内装だと残響が長すぎる/短すぎる」という一次スクリーニング
//! 用の参考値として使うこと——最終的な音響設計には音響コンサルタント等の
//! 専門家によるレビューが必要(`cg-cad-core`全体の免責事針と同様)。

/// 内装材(木材種・吸音パネル・その他仕上げ材)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteriorMaterial {
    /// 硬い広葉樹(オーク等)。反射的で残響を伸ばす方向。
    HardwoodOak,
    /// 中程度の硬さの広葉樹(ブナ等)。
    HardwoodBeech,
    /// 針葉樹の中では比較的柔らかいスギ材。
    SoftwoodCedar,
    /// 針葉樹の中でも特に柔らかいキリ材。
    SoftwoodPaulownia,
    /// MDF(中質繊維板、量産内装で多用)。
    Mdf,
    /// コンクリート打ちっぱなし(木材ではないが比較対象として収録)。
    BareConcrete,
    /// 布張りの吸音パネル(スタジオ/劇場で一般的)。
    FabricAbsorptionPanel,
    /// 発泡ウレタン吸音材(いわゆる「たまご型」フォーム等)。
    FoamAbsorber,
    /// 厚手のカーペット敷き。
    Carpet,
    /// 劇場座席(布張り、観客有無を問わない参考値)。
    TheaterSeating,
}

/// 1つの材料に対する物性値。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialProperties {
    /// ヤンカ硬度(kN、木材の硬さの代表指標。木材以外は`None`)。
    pub janka_hardness_kn: Option<f64>,
    /// 500Hz帯を代表値とした吸音率(NRC相当の簡易単一値、0.0〜1.0)。
    pub absorption_coefficient: f64,
}

/// 材料データベース。**正直な開示**: 上記モジュールdocコメントの通り、
/// いずれも文献・カタログの代表値であり、個体差は反映しない。
pub fn properties_of(material: InteriorMaterial) -> MaterialProperties {
    use InteriorMaterial::*;
    match material {
        HardwoodOak => MaterialProperties { janka_hardness_kn: Some(6.0), absorption_coefficient: 0.08 },
        HardwoodBeech => MaterialProperties { janka_hardness_kn: Some(4.9), absorption_coefficient: 0.09 },
        SoftwoodCedar => MaterialProperties { janka_hardness_kn: Some(1.5), absorption_coefficient: 0.11 },
        SoftwoodPaulownia => MaterialProperties { janka_hardness_kn: Some(1.0), absorption_coefficient: 0.13 },
        Mdf => MaterialProperties { janka_hardness_kn: Some(2.2), absorption_coefficient: 0.07 },
        BareConcrete => MaterialProperties { janka_hardness_kn: None, absorption_coefficient: 0.02 },
        FabricAbsorptionPanel => MaterialProperties { janka_hardness_kn: None, absorption_coefficient: 0.85 },
        FoamAbsorber => MaterialProperties { janka_hardness_kn: None, absorption_coefficient: 0.75 },
        Carpet => MaterialProperties { janka_hardness_kn: None, absorption_coefficient: 0.55 },
        TheaterSeating => MaterialProperties { janka_hardness_kn: None, absorption_coefficient: 0.65 },
    }
}

/// 木材種(ヤンカ硬度を持つ材料)のみを、硬い順に並べて返す
/// (「硬い木材から柔らかい木材」というユーザー要望に対応する一覧関数)。
pub fn wood_species_by_hardness_desc() -> Vec<InteriorMaterial> {
    let mut woods: Vec<InteriorMaterial> = [
        InteriorMaterial::HardwoodOak,
        InteriorMaterial::HardwoodBeech,
        InteriorMaterial::SoftwoodCedar,
        InteriorMaterial::SoftwoodPaulownia,
        InteriorMaterial::Mdf,
    ]
    .into_iter()
    .filter(|m| properties_of(*m).janka_hardness_kn.is_some())
    .collect();
    woods.sort_by(|a, b| {
        properties_of(*b)
            .janka_hardness_kn
            .partial_cmp(&properties_of(*a).janka_hardness_kn)
            .unwrap()
    });
    woods
}

/// 部屋の1つの面(壁・天井・床の一部)に割り当てられた材料と面積。
#[derive(Debug, Clone, Copy)]
pub struct SurfaceAssignment {
    pub material: InteriorMaterial,
    pub area_m2: f64,
}

/// 全ての面の吸音率×面積を合算した「等価吸音面積」(Sabine単位、単位はm²)。
pub fn total_absorption_sabins(surfaces: &[SurfaceAssignment]) -> f64 {
    surfaces
        .iter()
        .map(|s| properties_of(s.material).absorption_coefficient * s.area_m2)
        .sum()
}

/// エラー種別。
#[derive(Debug, Clone, PartialEq)]
pub enum AcousticError {
    /// 体積または等価吸音面積が0以下で、残響時間が定義できない。
    InvalidInput(String),
}

/// Sabineの残響式によるRT60(残響時間、単位: 秒)の簡易推定。
/// `RT60 = 0.161 * V / A`(V: 室容積[m³]、A: 等価吸音面積[m²、Sabine単位])。
/// 音響学の教科書で広く引用される最も基本的な近似式(拡散音場仮定)。
pub fn reverberation_time_rt60(volume_m3: f64, surfaces: &[SurfaceAssignment]) -> Result<f64, AcousticError> {
    if volume_m3 <= 0.0 {
        return Err(AcousticError::InvalidInput(format!("volume_m3 must be positive, got {volume_m3}")));
    }
    let total_absorption = total_absorption_sabins(surfaces);
    if total_absorption <= 0.0 {
        return Err(AcousticError::InvalidInput("total absorption must be positive (no surfaces assigned?)".to_string()));
    }
    Ok(0.161 * volume_m3 / total_absorption)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wood_species_are_sorted_from_hard_to_soft() {
        let ordered = wood_species_by_hardness_desc();
        for pair in ordered.windows(2) {
            let a = properties_of(pair[0]).janka_hardness_kn.unwrap();
            let b = properties_of(pair[1]).janka_hardness_kn.unwrap();
            assert!(a >= b, "{:?} ({a}) should be >= {:?} ({b})", pair[0], pair[1]);
        }
        assert_eq!(ordered.first().copied(), Some(InteriorMaterial::HardwoodOak));
        assert_eq!(ordered.last().copied(), Some(InteriorMaterial::SoftwoodPaulownia));
    }

    #[test]
    fn absorption_panels_absorb_far_more_than_bare_concrete() {
        let panel = properties_of(InteriorMaterial::FabricAbsorptionPanel).absorption_coefficient;
        let concrete = properties_of(InteriorMaterial::BareConcrete).absorption_coefficient;
        assert!(panel > concrete * 5.0);
    }

    #[test]
    fn total_absorption_sums_area_weighted_coefficients() {
        let surfaces = [
            SurfaceAssignment { material: InteriorMaterial::BareConcrete, area_m2: 100.0 },
            SurfaceAssignment { material: InteriorMaterial::FabricAbsorptionPanel, area_m2: 20.0 },
        ];
        let expected = 0.02 * 100.0 + 0.85 * 20.0;
        assert!((total_absorption_sabins(&surfaces) - expected).abs() < 1e-9);
    }

    #[test]
    fn rt60_matches_hand_calculated_sabine_example() {
        // 教科書的な例: V=1000m^3, A=100 sabins -> RT60 = 0.161*1000/100 = 1.61s
        let surfaces = [SurfaceAssignment { material: InteriorMaterial::BareConcrete, area_m2: 5000.0 }];
        // BareConcreteのalpha=0.02なので5000*0.02=100 sabinsとなるよう面積を調整。
        let rt60 = reverberation_time_rt60(1000.0, &surfaces).unwrap();
        assert!((rt60 - 1.61).abs() < 1e-6, "expected ~1.61s, got {rt60}");
    }

    #[test]
    fn more_absorption_reduces_reverberation_time() {
        let volume = 2000.0;
        let hard_room = [SurfaceAssignment { material: InteriorMaterial::BareConcrete, area_m2: 600.0 }];
        let treated_room = [
            SurfaceAssignment { material: InteriorMaterial::BareConcrete, area_m2: 400.0 },
            SurfaceAssignment { material: InteriorMaterial::FabricAbsorptionPanel, area_m2: 200.0 },
        ];
        let rt60_hard = reverberation_time_rt60(volume, &hard_room).unwrap();
        let rt60_treated = reverberation_time_rt60(volume, &treated_room).unwrap();
        assert!(rt60_treated < rt60_hard, "adding absorption panels should shorten RT60: hard={rt60_hard}, treated={rt60_treated}");
    }

    #[test]
    fn zero_or_negative_volume_is_rejected_honestly() {
        let surfaces = [SurfaceAssignment { material: InteriorMaterial::FabricAbsorptionPanel, area_m2: 10.0 }];
        assert!(matches!(reverberation_time_rt60(0.0, &surfaces), Err(AcousticError::InvalidInput(_))));
        assert!(matches!(reverberation_time_rt60(-5.0, &surfaces), Err(AcousticError::InvalidInput(_))));
    }

    #[test]
    fn no_surfaces_assigned_is_rejected_honestly() {
        assert!(matches!(reverberation_time_rt60(1000.0, &[]), Err(AcousticError::InvalidInput(_))));
    }
}
