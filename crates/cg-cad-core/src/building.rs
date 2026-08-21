//! 建築物ボリュームの自動生成(MVP、段階1・段階2)。
//!
//! **免責事項(必読)**: このモジュールが生成する`Building`は、あくまで
//! 「敷地・法規制パラメータ・建物種別・階数」という入力から導出される
//! **簡易的な直方体ボリュームのドラフト**であり、建築確認申請に使える
//! 正式な設計図面ではない。日照権の実シミュレーション・耐震性の構造
//! 計算・避難経路計画等は一切行っていない(段階4のスコープ外事項参照)。
//! 実際の建築には、一級建築士等の有資格者による設計・確認が必須。

use crate::mesh::Mesh;
use crate::site::Site;
use crate::zoning::{check_zoning_compliance, ZoningError, ZoningRules};
use thiserror::Error;

/// 対応する建物種別(段階2: 複数建物種別への対応)。
/// **正直な開示**: いずれも過度に精緻な形状ではなく、直方体または
/// 直方体の単純な組み合わせレベルに留める(タスク指示通り)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingType {
    /// 一戸建て住宅(低層、1〜2階想定)。
    House,
    /// オフィスビル(中〜高層の直方体)。
    OfficeBuilding,
    /// 映画館・コンサートホール(大空間、低層だが階高が高い)。
    Theater,
    /// オーディオルーム付き住宅・小規模施設(遮音重視、低層)。
    AudioRoom,
    /// 4DX/IMAX対応シアタールーム付きビル(複合施設、中層)。
    CinemaComplex,
}

impl BuildingType {
    /// 1フロアあたりの階高(メートル)。実際の階高は工法・用途により
    /// 幅があるが、MVPでは種別ごとの代表値を定数として扱う(この定数値
    /// 自体の妥当性は有資格者の確認が必要、単なるドラフト生成用の
    /// プレースホルダ)。
    pub fn floor_height_m(&self) -> f64 {
        match self {
            BuildingType::House => 3.0,
            BuildingType::OfficeBuilding => 3.5,
            BuildingType::Theater => 6.0,        // 大空間のため階高を大きく
            BuildingType::AudioRoom => 3.2,
            BuildingType::CinemaComplex => 5.5,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum CadError {
    #[error("zoning compliance check failed: {0}")]
    Zoning(#[from] ZoningError),
    #[error("floors must be at least 1, got {0}")]
    InvalidFloors(u32),
}

/// 生成された建築物ボリューム。
#[derive(Debug, Clone)]
pub struct Building {
    pub mesh: Mesh,
    pub building_type: BuildingType,
    pub floors: u32,
    pub footprint_m2: f64,
    pub total_floor_area_m2: f64,
}

/// 敷地・法規制・建物種別・階数から、直方体ベースの建築物ボリュームを
/// 生成する(段階1のMVPロジック)。
///
/// 手順:
/// 1. 敷地の外接矩形(bounding box)を求める。
/// 2. 建ぺい率の上限に収まるよう、外接矩形を面積比でスケールダウンした
///    footprint(建築面積)を敷地の中心に配置する。
/// 3. 階数 x 階高 で建物高さを求め、footprint x 階数 で延床面積を求める。
/// 4. `zoning::check_zoning_compliance`で建ぺい率・容積率の適合を検証
///    する——不適合なら誤ったドラフトを黙って返さず`CadError`を返す。
pub fn generate_building(site: &Site, zoning: &ZoningRules, building_type: BuildingType, floors: u32) -> Result<Building, CadError> {
    if floors == 0 {
        return Err(CadError::InvalidFloors(floors));
    }

    let site_area = site.area_m2();
    let max_footprint = zoning.max_building_footprint_m2(site_area);

    let ((min_x, min_y), (max_x, max_y)) = site.bounding_box();
    let bbox_width = max_x - min_x;
    let bbox_depth = max_y - min_y;
    let bbox_area = bbox_width * bbox_depth;

    // 外接矩形の面積が建ぺい率上限を超える場合、面積比の平方根で
    // 縦横を均等にスケールダウンする(縦横比を保ったまま面積だけを
    // 目標値に合わせる、単純だが妥当な既定の縮小方法)。
    let scale = if bbox_area > max_footprint { (max_footprint / bbox_area).sqrt() } else { 1.0 };
    let footprint_width = bbox_width * scale;
    let footprint_depth = bbox_depth * scale;
    let footprint_m2 = footprint_width * footprint_depth;

    let height_m = building_type.floor_height_m() * floors as f64;
    let total_floor_area_m2 = footprint_m2 * floors as f64;

    check_zoning_compliance(site_area, zoning, footprint_m2, total_floor_area_m2)?;

    // 敷地中心にfootprintを配置(x=東西, z=南北)。yは高さ方向(0が地面)。
    let center_x = (min_x + max_x) / 2.0;
    let center_z = (min_y + max_y) / 2.0; // Siteのyをワールドのz(南北)へ対応させる
    let half_w = footprint_width / 2.0;
    let half_d = footprint_depth / 2.0;

    let color = crate::material::default_color_for(building_type);
    let mesh = Mesh::axis_aligned_box(
        [(center_x - half_w) as f32, 0.0, (center_z - half_d) as f32],
        [(center_x + half_w) as f32, height_m as f32, (center_z + half_d) as f32],
        color,
    );

    Ok(Building { mesh, building_type, floors, footprint_m2, total_floor_area_m2 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_house_within_zoning_limits() {
        let site = Site::rectangle(10.0, 10.0).unwrap(); // 100m2
        let zoning = ZoningRules::new(0.6, 2.0).unwrap();
        let building = generate_building(&site, &zoning, BuildingType::House, 2).unwrap();
        assert!(building.footprint_m2 <= zoning.max_building_footprint_m2(site.area_m2()) + 1e-9);
        assert!(building.total_floor_area_m2 <= zoning.max_total_floor_area_m2(site.area_m2()) + 1e-9);
        assert_eq!(building.mesh.triangle_count(), 12);
    }

    #[test]
    fn rejects_too_many_floors_that_exceed_floor_area_ratio() {
        let site = Site::rectangle(10.0, 10.0).unwrap(); // 100m2, bbox=footprint(scale=1 if within coverage)
        let zoning = ZoningRules::new(0.6, 1.0).unwrap(); // 容積率100%→上限100m2、footprint=60m2なら1階(60m2)はOK、2階(120m2)はNG
        let result = generate_building(&site, &zoning, BuildingType::House, 2);
        assert!(matches!(result, Err(CadError::Zoning(ZoningError::TotalFloorAreaExceedsRatio { .. }))));
    }

    #[test]
    fn rejects_zero_floors() {
        let site = Site::rectangle(10.0, 10.0).unwrap();
        let zoning = ZoningRules::new(0.6, 2.0).unwrap();
        let result = generate_building(&site, &zoning, BuildingType::House, 0);
        assert_eq!(result.err(), Some(CadError::InvalidFloors(0)));
    }

    #[test]
    fn footprint_is_scaled_down_to_respect_coverage_ratio_when_bbox_exceeds_it() {
        // 敷地bboxそのまま(100m2)だと建ぺい率60%の上限(60m2)を超えるため、
        // footprintは縮小されるはず。
        let site = Site::rectangle(10.0, 10.0).unwrap();
        let zoning = ZoningRules::new(0.6, 5.0).unwrap();
        let building = generate_building(&site, &zoning, BuildingType::OfficeBuilding, 1).unwrap();
        assert!(building.footprint_m2 < site.area_m2());
        assert!((building.footprint_m2 - 60.0).abs() < 1e-6);
    }

    #[test]
    fn different_building_types_get_different_heights_for_the_same_floor_count() {
        let site = Site::rectangle(20.0, 20.0).unwrap();
        let zoning = ZoningRules::new(0.5, 10.0).unwrap();
        let house = generate_building(&site, &zoning, BuildingType::House, 1).unwrap();
        let theater = generate_building(&site, &zoning, BuildingType::Theater, 1).unwrap();
        let house_height = house.mesh.vertices.iter().map(|v| v.position[1]).fold(0.0f32, f32::max);
        let theater_height = theater.mesh.vertices.iter().map(|v| v.position[1]).fold(0.0f32, f32::max);
        assert!(theater_height > house_height);
    }
}
