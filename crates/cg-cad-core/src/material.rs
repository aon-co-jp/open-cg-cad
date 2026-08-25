//! 建物種別ごとの簡易マテリアル(色)割り当て(段階3: 着色)。
//!
//! **正直な開示**: これは「面ごとに異なるテクスチャを貼る」ような
//! 本格的なマテリアルシステムではなく、建物種別に応じた単色の
//! フラットシェーディング割り当てに留まる(タスク指示の「過度に精緻で
//! なくてよい」方針に沿ったMVPスコープ)。

use crate::building::BuildingType;

/// RGBA(0.0〜1.0)。
pub type Color = [f32; 4];

/// 建物種別に対する既定のマテリアル色を返す。
pub fn default_color_for(building_type: BuildingType) -> Color {
    match building_type {
        BuildingType::House => [0.82, 0.70, 0.55, 1.0], // 木造住宅風の淡いベージュ
        BuildingType::OfficeBuilding => [0.55, 0.65, 0.75, 1.0], // ガラス/スチール風の淡い青灰色
        BuildingType::Theater => [0.35, 0.15, 0.20, 1.0], // 劇場/コンサートホール風の濃い臙脂色
        BuildingType::AudioRoom => [0.20, 0.20, 0.25, 1.0], // 遮音重視の濃いグレー
        BuildingType::CinemaComplex => [0.10, 0.10, 0.30, 1.0], // 4DX/IMAX対応ビル風の濃紺
        BuildingType::HomeCinema4dxImax => [0.30, 0.10, 0.35, 1.0], // 住宅版4DX/IMAX(House寄りだが遮光重視の紫がかった色)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_building_type_has_a_distinct_default_color() {
        let types = [
            BuildingType::House,
            BuildingType::OfficeBuilding,
            BuildingType::Theater,
            BuildingType::AudioRoom,
            BuildingType::CinemaComplex,
            BuildingType::HomeCinema4dxImax,
        ];
        let colors: Vec<Color> = types.iter().map(|&t| default_color_for(t)).collect();
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j], "building types {:?} and {:?} share a color", types[i], types[j]);
            }
        }
    }

    #[test]
    fn colors_are_valid_normalized_rgba() {
        for &t in &[
            BuildingType::House,
            BuildingType::OfficeBuilding,
            BuildingType::Theater,
            BuildingType::AudioRoom,
            BuildingType::CinemaComplex,
            BuildingType::HomeCinema4dxImax,
        ] {
            let c = default_color_for(t);
            for channel in c {
                assert!((0.0..=1.0).contains(&channel));
            }
        }
    }
}
