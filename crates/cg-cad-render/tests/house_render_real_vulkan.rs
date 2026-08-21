//! 実機Vulkanレンダリングテスト: `cg-cad-core`が生成した一戸建て住宅の
//! 直方体ボリュームを、`open-directx`の既存の実グラフィックスパイプライン
//! (`directx-graphics-vulkan`)経由で実際にレンダリングし、フレームバッファ
//! を読み戻して結果を検証する。
//!
//! `open-directx`側の既存の実機テスト群と同じ方針で、実Vulkanデバイス/
//! ドライバが無い環境ではスキップする(モックでの「成功したふり」はしない)。

use cg_cad_core::{generate_building, BuildingType, Site, ZoningRules};
use cg_cad_render::{count_non_background_pixels, render_mesh_top_down_and_read_back};
use directx_graphics_vulkan::Rgba8;

#[test]
fn generated_house_footprint_actually_renders_on_real_vulkan_hardware() {
    // 敷地100m2、建ぺい率60%、容積率200%、平屋(1階)の一戸建て住宅を生成。
    let site = Site::rectangle(10.0, 10.0).expect("valid site");
    let zoning = ZoningRules::new(0.6, 2.0).expect("valid zoning rules");
    let building = generate_building(&site, &zoning, BuildingType::House, 1).expect("zoning-compliant house");

    // 建築面積は敷地面積x建ぺい率(60m2)以内であるはず(cg-cad-core側の
    // zoning検証で既に保証されているが、レンダリング前提としても再確認)。
    assert!(building.footprint_m2 <= 60.0 + 1e-6);

    let width = 64u32;
    let height = 64u32;
    // 敷地の半幅(5m)よりわずかに大きい範囲をビューポートに収める。
    let half_extent_m = 6.0f32;

    let pixels = match render_mesh_top_down_and_read_back(&building.mesh, half_extent_m, width, height) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping real-hardware house render test: no usable real Vulkan graphics device/driver ({e})");
            return;
        }
    };
    assert_eq!(pixels.len(), (width * height) as usize);

    let background = Rgba8 { r: 0, g: 0, b: 0, a: 255 };
    let covered = count_non_background_pixels(&pixels, background);

    // 期待される被覆ピクセル比率: footprint_m2 / (viewport area) =
    // 建築面積 / (2*half_extent_m)^2。UNORM量子化・境界ピクセルの
    // 丸め等を考慮し、緩やかな許容範囲(±50%)で検証する——ピクセル単位の
    // 厳密な一致ではなく「実際に妥当なサイズで描画されている」ことの
    // 実機での直接証拠とする。
    let viewport_area_m2 = (2.0 * half_extent_m as f64).powi(2);
    let expected_ratio = building.footprint_m2 / viewport_area_m2;
    let expected_pixels = expected_ratio * (width * height) as f64;

    println!(
        "OK: cg-cad-core generated a {:.2}m2 footprint House (site 100m2, coverage 60%), rendered it via \
         open-directx's real Vulkan pipeline on the real GPU present on this machine, and {covered} of \
         {}x{} read-back pixels are non-background (expected ~{expected_pixels:.1} from the footprint/viewport \
         area ratio).",
        building.footprint_m2,
        width,
        height
    );

    assert!(covered > 0, "expected at least some pixels to be covered by the rendered building footprint");
    assert!(
        (covered as f64) > expected_pixels * 0.5 && (covered as f64) < expected_pixels * 1.5,
        "covered pixel count {covered} is not within 50% of the expected {expected_pixels:.1} \
         derived from the footprint/viewport area ratio -- rendering may be broken (wrong scale/projection)"
    );

    // 描画されたピクセルの色が、House用マテリアル色
    // (material::default_color_for、量子化込み)と一致することも確認する。
    let expected_color = Rgba8 {
        r: (0.82f32 * 255.0).round() as u8,
        g: (0.70f32 * 255.0).round() as u8,
        b: (0.55f32 * 255.0).round() as u8,
        a: 255,
    };
    let matching_color_pixels =
        pixels.iter().filter(|p| p.r.abs_diff(expected_color.r) <= 2 && p.g.abs_diff(expected_color.g) <= 2 && p.b.abs_diff(expected_color.b) <= 2).count();
    assert!(
        matching_color_pixels > 0,
        "expected at least some read-back pixels to match the House material color {expected_color:?}, found none"
    );
}
