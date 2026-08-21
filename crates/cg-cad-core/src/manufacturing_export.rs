//! 自動生産用ロボットアーム・3Dプリンター・工場搬入ロボットへ渡せる
//! 製造データ出力(MVP)。
//!
//! **免責事項(必読、他モジュールと同様に省略・弱化しない)**: 本モジュール
//! が出力するのは、(1) 3Dプリンター向けの標準的なASCII STLメッシュ
//! フォーマット、(2) ロボットアーム向けの単純な3D座標列(JSON形式)の
//! 「経路の参考データ」のみである。実在する特定ロボット制御言語
//! (KUKA KRL・FANUC TP・URScript・G-code等)への変換、衝突回避・
//! 特異点回避・加減速プロファイル・IK(逆運動学)ソルバー等の実際の
//! 制御に必要な処理は一切行っていない——スコープ外として明記する。
//! 実際の工場搬入・自動生産ラインへ投入する前には、各ロボットメーカーの
//! 専用オフラインティーチングソフトウェア・安全担当者によるレビューを
//! 必ず経ること。

use std::fmt::Write as _;

use thiserror::Error;

use crate::mesh::Mesh;

#[derive(Debug, Error, PartialEq)]
pub enum ManufacturingExportError {
    #[error("mesh has no triangles to export")]
    EmptyMesh,
    #[error("index {0} out of bounds for {1} vertices")]
    IndexOutOfBounds(u32, usize),
    #[error("waypoints list must not be empty")]
    EmptyWaypoints,
}

/// メッシュを標準的なASCII STLフォーマットの文字列へ変換する
/// (3Dプリンター向け)。
///
/// フォーマット: `solid <name>` … `facet normal nx ny nz` /
/// `outer loop` / `vertex x y z` ×3 / `endloop` / `endfacet` … `endsolid <name>`。
pub fn mesh_to_stl_ascii(mesh: &Mesh, solid_name: &str) -> Result<String, ManufacturingExportError> {
    if mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3) {
        return Err(ManufacturingExportError::EmptyMesh);
    }

    let mut out = String::new();
    let _ = writeln!(out, "solid {solid_name}");

    for tri in mesh.indices.chunks(3) {
        let (i0, i1, i2) = (tri[0], tri[1], tri[2]);
        for &i in &[i0, i1, i2] {
            if i as usize >= mesh.vertices.len() {
                return Err(ManufacturingExportError::IndexOutOfBounds(
                    i,
                    mesh.vertices.len(),
                ));
            }
        }
        let p0 = mesh.vertices[i0 as usize].position;
        let p1 = mesh.vertices[i1 as usize].position;
        let p2 = mesh.vertices[i2 as usize].position;

        let normal = triangle_normal(p0, p1, p2);

        let _ = writeln!(
            out,
            "  facet normal {:e} {:e} {:e}",
            normal[0], normal[1], normal[2]
        );
        let _ = writeln!(out, "    outer loop");
        for p in [p0, p1, p2] {
            let _ = writeln!(out, "      vertex {:e} {:e} {:e}", p[0], p[1], p[2]);
        }
        let _ = writeln!(out, "    endloop");
        let _ = writeln!(out, "  endfacet");
    }

    let _ = writeln!(out, "endsolid {solid_name}");
    Ok(out)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn triangle_normal(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) -> [f32; 3] {
    let n = cross(sub(p1, p0), sub(p2, p0));
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 {
        [n[0] / len, n[1] / len, n[2] / len]
    } else {
        [0.0, 0.0, 0.0]
    }
}

/// ロボットアーム向けの簡易経路データ(3D座標列)。
///
/// **正直な開示**: 実在するロボット制御言語(KUKA KRL・FANUC TP・
/// URScript等)への対応はスコープ外。ここでの`waypoints`は単純な
/// ワールド座標(m)の通過点列に過ぎず、姿勢(オリエンテーション)・
/// 速度・加減速プロファイル・ツール座標系(TCP)設定は含まない。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RobotArmPath {
    /// この経路データの名称・用途メモ。
    pub name: String,
    /// 座標系の単位(常に"m"を明記して曖昧さを避ける)。
    pub unit: String,
    /// 通過点列(x, y, z、メートル単位)。
    pub waypoints: Vec<[f64; 3]>,
    /// スコープ外事項の明記(出力データを受け取る側が誤解しないように)。
    pub scope_note: String,
}

/// 通過点列からロボットアーム向け簡易経路データを構築し、JSON文字列へ
/// シリアライズする。
pub fn robot_arm_path_to_json(
    name: &str,
    waypoints: &[[f64; 3]],
) -> Result<String, ManufacturingExportError> {
    if waypoints.is_empty() {
        return Err(ManufacturingExportError::EmptyWaypoints);
    }

    let path = RobotArmPath {
        name: name.to_string(),
        unit: "m".to_string(),
        waypoints: waypoints.to_vec(),
        scope_note: "実在するロボット制御言語(KUKA KRL/FANUC TP/URScript等)への変換は未対応。姿勢・速度・加減速プロファイル・TCP設定も含まない参考座標列のみ。".to_string(),
    };

    serde_json::to_string_pretty(&path).map_err(|_| ManufacturingExportError::EmptyWaypoints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Mesh;

    fn single_triangle_mesh() -> Mesh {
        use crate::mesh::MeshVertex;
        let color = [1.0, 1.0, 1.0, 1.0];
        Mesh {
            vertices: vec![
                MeshVertex { position: [0.0, 0.0, 0.0], color },
                MeshVertex { position: [1.0, 0.0, 0.0], color },
                MeshVertex { position: [0.0, 1.0, 0.0], color },
            ],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn rejects_empty_mesh() {
        let mesh = Mesh::new();
        assert_eq!(
            mesh_to_stl_ascii(&mesh, "empty").unwrap_err(),
            ManufacturingExportError::EmptyMesh
        );
    }

    #[test]
    fn stl_has_correct_structural_tokens() {
        let mesh = single_triangle_mesh();
        let stl = mesh_to_stl_ascii(&mesh, "test_solid").unwrap();
        assert!(stl.starts_with("solid test_solid\n"));
        assert!(stl.trim_end().ends_with("endsolid test_solid"));
        assert_eq!(stl.matches("facet normal").count(), 1);
        assert_eq!(stl.matches("outer loop").count(), 1);
        assert_eq!(stl.matches("vertex").count(), 3);
        assert_eq!(stl.matches("endloop").count(), 1);
        assert_eq!(stl.matches("endfacet").count(), 1);
    }

    #[test]
    fn stl_facet_count_matches_triangle_count() {
        let mut mesh = single_triangle_mesh();
        // axis_aligned_boxは12三角形(6面x2)を生成する。
        let box_mesh = Mesh::axis_aligned_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]);
        mesh.merge(&box_mesh);
        let stl = mesh_to_stl_ascii(&mesh, "combined").unwrap();
        let expected_facets = mesh.indices.len() / 3;
        assert_eq!(stl.matches("facet normal").count(), expected_facets);
    }

    #[test]
    fn detects_out_of_bounds_index() {
        let mesh = Mesh {
            vertices: vec![],
            indices: vec![0, 1, 2],
        };
        let err = mesh_to_stl_ascii(&mesh, "bad").unwrap_err();
        matches!(err, ManufacturingExportError::IndexOutOfBounds(_, _));
    }

    #[test]
    fn rejects_empty_waypoints() {
        let err = robot_arm_path_to_json("test", &[]).unwrap_err();
        assert_eq!(err, ManufacturingExportError::EmptyWaypoints);
    }

    #[test]
    fn robot_arm_path_json_round_trips_and_contains_scope_note() {
        let waypoints = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.5]];
        let json = robot_arm_path_to_json("pick_and_place_demo", &waypoints).unwrap();
        assert!(json.contains("pick_and_place_demo"));
        assert!(json.contains("\"unit\": \"m\""));
        assert!(json.contains("KUKA KRL"));

        let parsed: RobotArmPath = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.waypoints.len(), 3);
        assert_eq!(parsed.waypoints[2], [1.0, 1.0, 0.5]);
    }
}
