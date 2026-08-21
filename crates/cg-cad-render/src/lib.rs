//! `cg-cad-core`が生成した[`Mesh`]を、`open-directx`の既存の実Vulkan
//! グラフィックスパイプライン(`directx-graphics-vulkan`、NVIDIA GT 730
//! 実機検証済み)経由で実際にレンダリングする橋渡し。
//!
//! **免責事項**: ここでレンダリングされる画像はドラフト/サンプル
//! ビジュアライゼーションであり、正式な建築パースやレンダリング成果物
//! ではない(`cg-cad-core`クレート全体の免責事項を参照)。
//!
//! # 設計方針(車輪の再発明を避ける)
//!
//! `directx-graphics-vulkan::render_indexed_scene_with_depth_and_read_back`
//! (実レンダーパス+フレームバッファ+`VkGraphicsPipelineCreateInfo`+深度
//! テスト付き実描画、`open-directx`側で既に実機検証済み)をそのまま呼び
//! 出す。頂点/ピクセルシェーダーも`open-directx`の既存の`triangle_vs.dxbc`/
//! `triangle_ps.dxbc`(パススルー、POSITION+COLOR)をそのまま翻訳・再利用
//! し、本クレート側で新たなシェーダーやVulkanコードは一切書かない。
//!
//! 本クレートが新規に実装するのは、`cg-cad-core::Mesh`(ワールド座標系
//! メートル単位)を、既存のレンダラーが要求するNDC座標系
//! [`directx_graphics_vulkan::Vertex`]へ変換する単純な正射影(俯瞰視点、
//! 真上から見下ろす)のみ。

use cg_cad_core::mesh::Mesh;
use directx_graphics_vulkan::{render_indexed_scene_with_depth_and_read_back, Rgba8, Vertex};
use directx_shader_translate::spirv_gen::{translate_pixel_shader, translate_vertex_shader};
use thiserror::Error;

// open-directxが既に実fxc.exeでコンパイル・実機検証済みのパススルー
// 頂点/ピクセルシェーダーをそのまま再利用する(新規シェーダーは書かない)。
const TRIANGLE_VS_DXBC: &[u8] =
    include_bytes!("../../../../open-directx/crates/directx-shader-translate/shaders/triangle_vs.dxbc");
const TRIANGLE_PS_DXBC: &[u8] =
    include_bytes!("../../../../open-directx/crates/directx-shader-translate/shaders/triangle_ps.dxbc");

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("failed to translate the reused triangle_vs.dxbc/triangle_ps.dxbc shaders: {0}")]
    ShaderTranslation(String),
    #[error("mesh has no vertices/indices to render")]
    EmptyMesh,
    #[error("real Vulkan rendering failed: {0}")]
    Vulkan(#[from] directx_graphics_vulkan::GraphicsError),
}

/// [`Mesh`]の全頂点を正射影(俯瞰、真上から見下ろす視点)でNDC座標へ
/// 変換する。
///
/// - NDC x/y ← ワールドx/z(東西・南北)を`half_extent_m`で正規化。
/// - depth ← ワールドy(高さ)を`[0,1]`へ正規化して反転したもの
///   (高いほどdepthが小さくなり、`CompareOp::LESS`の深度テストで
///   「高い場所ほど手前(=真上から見て見える)」という直感的な俯瞰視点を
///   実現する——新規のカメラ/射影行列コードは書かず、深度テストの比較
///   方向だけを利用する単純な設計)。
/// - color ← メッシュの頂点色をそのままパススルー(シェーダー側は
///   COLORセマンティクスを素通しするのみ)。
fn project_top_down(mesh: &Mesh, half_extent_m: f32, max_height_m: f32) -> Vec<Vertex> {
    // メッシュのx/z範囲の中心を原点とみなして正規化する——敷地(Site)の
    // 座標系は原点を左下角とするため、敷地が原点から離れた位置に
    // 配置されていると、中心を考慮せずワールド座標をそのまま
    // half_extent_mで割るだけでは正射影のNDC範囲[-1,1]からはみ出して
    // 意図せずクリップされてしまう(実機テストで実際に検出したバグ)。
    let (min_x, max_x) = mesh.vertices.iter().map(|v| v.position[0]).fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
    let (min_z, max_z) = mesh.vertices.iter().map(|v| v.position[2]).fold((f32::MAX, f32::MIN), |(lo, hi), z| (lo.min(z), hi.max(z)));
    let center_x = (min_x + max_x) / 2.0;
    let center_z = (min_z + max_z) / 2.0;

    mesh.vertices
        .iter()
        .map(|v| {
            let [x, y, z] = v.position;
            let ndc_x = ((x - center_x) / half_extent_m).clamp(-1.0, 1.0);
            // ワールドzが「南北」、画面上で見た目が反転しないようndc_yへそのまま対応させる。
            let ndc_y = ((z - center_z) / half_extent_m).clamp(-1.0, 1.0);
            let normalized_height = if max_height_m > 0.0 { (y / max_height_m).clamp(0.0, 1.0) } else { 0.0 };
            let depth = 1.0 - normalized_height * 0.9; // 0.1..=1.0、高いほど手前(depth小)
            Vertex { pos: [ndc_x, ndc_y, depth], color: v.color }
        })
        .collect()
}

/// [`Mesh`]を実Vulkan(`open-directx`の既存パイプライン)で俯瞰レンダリング
/// し、読み戻したRGBA8ピクセル列を返す。
///
/// `half_extent_m`はビューポートに収める正射影の半幅(メートル)——
/// 呼び出し側が敷地サイズ等から適切な値を渡す想定(このクレート自体は
/// カメラの自動フィッティングは行わない、MVPスコープ)。
pub fn render_mesh_top_down_and_read_back(
    mesh: &Mesh,
    half_extent_m: f32,
    width: u32,
    height: u32,
) -> Result<Vec<Rgba8>, RenderError> {
    if mesh.vertices.is_empty() || mesh.indices.is_empty() {
        return Err(RenderError::EmptyMesh);
    }

    let vs = translate_vertex_shader(TRIANGLE_VS_DXBC).map_err(|e| RenderError::ShaderTranslation(e.to_string()))?;
    let ps = translate_pixel_shader(TRIANGLE_PS_DXBC).map_err(|e| RenderError::ShaderTranslation(e.to_string()))?;

    let max_height_m = mesh.vertices.iter().map(|v| v.position[1]).fold(0.0f32, f32::max);
    let vertices = project_top_down(mesh, half_extent_m, max_height_m);

    let pixels = render_indexed_scene_with_depth_and_read_back(
        &vs.spirv_words,
        &ps.spirv_words,
        &vertices,
        &mesh.indices,
        width,
        height,
    )?;
    Ok(pixels)
}

/// 読み戻したピクセル列のうち、背景色(クリア色、黒)ではないピクセル数を
/// 数える——「実際に何か描画された」ことの単純な検証ヘルパー。
pub fn count_non_background_pixels(pixels: &[Rgba8], background: Rgba8) -> usize {
    pixels.iter().filter(|p| **p != background).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_top_down_maps_a_unit_box_center_to_ndc_origin() {
        let mesh = Mesh::axis_aligned_box([-1.0, 0.0, -1.0], [1.0, 2.0, 1.0], [1.0, 0.0, 0.0, 1.0]);
        let projected = project_top_down(&mesh, 2.0, 2.0);
        assert_eq!(projected.len(), mesh.vertices.len());
        // コーナー(-1,0,-1)はndc(-0.5,-0.5)、depth=1.0(y=0で高さ最低)になるはず。
        let corner = projected[0];
        assert!((corner.pos[0] - (-0.5)).abs() < 1e-6);
        assert!((corner.pos[1] - (-0.5)).abs() < 1e-6);
        assert!((corner.pos[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_empty_mesh_without_touching_vulkan() {
        let empty = Mesh { vertices: vec![], indices: vec![] };
        let result = render_mesh_top_down_and_read_back(&empty, 10.0, 4, 4);
        assert!(matches!(result, Err(RenderError::EmptyMesh)));
    }

    #[test]
    fn count_non_background_pixels_counts_correctly() {
        let bg = Rgba8 { r: 0, g: 0, b: 0, a: 255 };
        let pixels = vec![bg, Rgba8 { r: 200, g: 0, b: 0, a: 255 }, bg, Rgba8 { r: 0, g: 200, b: 0, a: 255 }];
        assert_eq!(count_non_background_pixels(&pixels, bg), 2);
    }
}
