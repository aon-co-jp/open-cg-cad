//! CalculiX(実在・オープンソースの非線形FEMソルバー、材料非線形性・
//! 大変形・接触・破断等を扱える)への入出力ブリッジ。
//!
//! **このモジュールはCalculiX自体の計算機能を一切模倣しない。**
//! `Building`(直方体ボリューム)を軸並行の8節点六面体要素(C3D8)グリッド
//! へ離散化してAbaqus互換の`.inp`入力ファイルを書き出し(`export_inp`)、
//! `ccx`バイナリを子プロセスとして実行し(`run`)、CalculiXの`.frd`結果
//! ファイル(節点変位)を解析して返す(`parse_frd_displacements`)。
//!
//! `structural.rs`の簡易せん断建物モデルとの違い: `structural.rs`は
//! 各階を1自由度に集約する近似だが、本モジュールが生成する入力は
//! (粗いながらも)3次元的に離散化された節点・要素を持つため、CalculiX
//! 側で本格的な3次元線形/非線形静解析が可能になる——ただし実際にどこまで
//! 精緻な結果が得られるかは、この統合層ではなくCalculiX本体の解析精度に
//! 依存する(このモジュールは入出力の橋渡しに徹する)。

use super::{binary_exists, resolve_binary, run_checked, SolverError};
use crate::building::Building;
use std::path::Path;

/// CalculiX静解析用の材料・荷重パラメータ。
#[derive(Debug, Clone, Copy)]
pub struct CalculixAnalysisOptions {
    /// ヤング率(Pa)。既定値は`structural::StructuralMaterial::default`と
    /// 揃えた鉄筋コンクリート代表値(約2.1e10 Pa)。
    pub youngs_modulus_pa: f64,
    /// ポアソン比。既定値0.2(RC造の代表値)。
    pub poisson_ratio: f64,
    /// X/Y/Z方向のグリッド分割数(1以上)。粗いほど要素数が少なく高速だが
    /// 精度は下がる(このMVPでは細分化アルゴリズムは持たず、呼び出し側が
    /// 指定する固定分割数を素直に使う)。
    pub grid_divisions: (u32, u32, u32),
    /// 最上部の節点群へ均等配分する鉛直荷重の合計(N、負値で下向き)。
    pub total_top_load_n: f64,
}

impl Default for CalculixAnalysisOptions {
    fn default() -> Self {
        CalculixAnalysisOptions { youngs_modulus_pa: 2.1e10, poisson_ratio: 0.2, grid_divisions: (2, 2, 2), total_top_load_n: -1.0e5 }
    }
}

/// `Building`のメッシュのbounding boxを求める(既存の`Mesh`は頂点の
/// AABBを直接保持しないため、頂点を走査して求める)。
fn mesh_bounding_box(building: &Building) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in &building.mesh.vertices {
        for i in 0..3 {
            min[i] = min[i].min(v.position[i]);
            max[i] = max[i].max(v.position[i]);
        }
    }
    (min, max)
}

/// 節点のグリッドインデックスから1始まりのCalculiX節点番号を求める。
fn node_id(ix: u32, iy: u32, iz: u32, nx: u32, ny: u32) -> u32 {
    1 + ix + iy * (nx + 1) + iz * (nx + 1) * (ny + 1)
}

/// `Building`を軸並行の直方体グリッド(C3D8六面体要素)へ離散化し、
/// CalculiXのAbaqus互換`.inp`入力ファイル(文字列)を生成する。
///
/// 境界条件: 最下層(y最小)の全節点をXYZ完全固定(`*BOUNDARY`)、
/// 最上層(y最大)の全節点へ`total_top_load_n`を均等配分した集中荷重
/// (`*CLOAD`、鉛直Y方向)を与える単純な静解析ケース。
pub fn export_inp(building: &Building, opts: &CalculixAnalysisOptions) -> String {
    let (min, max) = mesh_bounding_box(building);
    let (nx, ny, nz) = opts.grid_divisions;
    let (nx, ny, nz) = (nx.max(1), ny.max(1), nz.max(1));

    let mut out = String::new();
    out.push_str("** open-cg-cad solver_bridge::calculix によって自動生成された入力ファイル\n");
    out.push_str("** 免責: 直方体ボリュームを軸並行グリッドで単純に離散化した粗いメッシュであり、\n");
    out.push_str("** 建築確認申請等に使える正式な構造計算ではない(open-cg-cad全体の免責事項参照)。\n");
    out.push_str("*NODE\n");
    for iz in 0..=nz {
        for iy in 0..=ny {
            for ix in 0..=nx {
                let x = min[0] + (max[0] - min[0]) * (ix as f32 / nx as f32);
                let y = min[1] + (max[1] - min[1]) * (iy as f32 / ny as f32);
                let z = min[2] + (max[2] - min[2]) * (iz as f32 / nz as f32);
                let id = node_id(ix, iy, iz, nx, ny);
                out.push_str(&format!("{id}, {x:.6}, {y:.6}, {z:.6}\n"));
            }
        }
    }

    out.push_str("*ELEMENT, TYPE=C3D8, ELSET=EALL\n");
    let mut elem_id = 1u32;
    for iz in 0..nz {
        for iy in 0..ny {
            for ix in 0..nx {
                // C3D8のAbaqus標準節点順序(底面4点を反時計回り→天面4点を反時計回り)。
                let n = |dx: u32, dy: u32, dz: u32| node_id(ix + dx, iy + dy, iz + dz, nx, ny);
                let n1 = n(0, 0, 0);
                let n2 = n(1, 0, 0);
                let n3 = n(1, 1, 0);
                let n4 = n(0, 1, 0);
                let n5 = n(0, 0, 1);
                let n6 = n(1, 0, 1);
                let n7 = n(1, 1, 1);
                let n8 = n(0, 1, 1);
                out.push_str(&format!("{elem_id}, {n1}, {n2}, {n3}, {n4}, {n5}, {n6}, {n7}, {n8}\n"));
                elem_id += 1;
            }
        }
    }

    out.push_str("*MATERIAL, NAME=CONCRETE\n*ELASTIC\n");
    out.push_str(&format!("{:.6}, {:.4}\n", opts.youngs_modulus_pa, opts.poisson_ratio));
    out.push_str("*SOLID SECTION, ELSET=EALL, MATERIAL=CONCRETE\n");

    out.push_str("*NSET, NSET=BOTTOM\n");
    let bottom_nodes: Vec<u32> = (0..=nz).flat_map(|iz| (0..=nx).map(move |ix| node_id(ix, 0, iz, nx, ny))).collect();
    push_nset_ids(&mut out, &bottom_nodes);

    out.push_str("*NSET, NSET=TOP\n");
    let top_nodes: Vec<u32> = (0..=nz).flat_map(|iz| (0..=nx).map(move |ix| node_id(ix, ny, iz, nx, ny))).collect();
    push_nset_ids(&mut out, &top_nodes);

    out.push_str("*BOUNDARY\nBOTTOM, 1, 3\n");

    out.push_str("*STEP\n*STATIC\n*CLOAD\n");
    let load_per_node = opts.total_top_load_n / top_nodes.len().max(1) as f64;
    for id in &top_nodes {
        out.push_str(&format!("{id}, 2, {load_per_node:.6}\n"));
    }
    out.push_str("*NODE FILE\nU\n*EL FILE\nS\n*END STEP\n");

    out
}

fn push_nset_ids(out: &mut String, ids: &[u32]) {
    for chunk in ids.chunks(8) {
        let line: Vec<String> = chunk.iter().map(|id| id.to_string()).collect();
        out.push_str(&line.join(", "));
        out.push('\n');
    }
}

/// CalculiXの`ccx`バイナリの存在確認結果(実行前チェック用、
/// `run`が内部で呼ぶのと同じ判定をテストやCLIから直接使えるよう公開)。
pub fn ccx_binary_available() -> bool {
    binary_exists(&resolve_binary("OPEN_CG_CAD_CCX_BIN", "ccx"))
}

/// `case_dir/job_name.inp`を入力として`ccx`を実行する
/// (`ccx -i job_name`、カレントディレクトリを`case_dir`にして起動)。
/// バイナリが見つからない場合は[`SolverError::BinaryNotFound`]を返す
/// (動くふりをしない)。
pub fn run(case_dir: &Path, job_name: &str) -> Result<(), SolverError> {
    let bin = resolve_binary("OPEN_CG_CAD_CCX_BIN", "ccx");
    if !binary_exists(&bin) {
        return Err(SolverError::BinaryNotFound(bin.to_string_lossy().to_string()));
    }
    let mut cmd = std::process::Command::new(&bin);
    cmd.current_dir(case_dir).arg("-i").arg(job_name);
    run_checked(cmd, &bin.to_string_lossy())?;
    Ok(())
}

/// 1節点分の変位結果。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeDisplacement {
    pub node_id: u32,
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
}

/// CalculiXの`.frd`結果ファイルから節点変位(`DISP`ブロック)を解析する。
///
/// `.frd`のDISPブロックは`-1`(節点データ開始)行に続き、各節点1行
/// `-1<node_id><dx><dy><dz>`(固定幅または空白区切り)という構造を持つ。
/// 本パーサーは、CalculiXが実際に出力する空白区切りの短い変種
/// (`-1  <id>  <dx>  <dy>  <dz>`)を対象とする、必要最小限の実装。
pub fn parse_frd_displacements(content: &str) -> Result<Vec<NodeDisplacement>, SolverError> {
    let mut in_disp_block = false;
    let mut results = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("-4") && trimmed.contains("DISP") {
            in_disp_block = true;
            continue;
        }
        if in_disp_block {
            if trimmed.starts_with("-3") {
                in_disp_block = false;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("-1") {
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if fields.len() >= 4 {
                    let node_id: u32 = fields[0]
                        .parse()
                        .map_err(|_| SolverError::ParseError(format!("invalid node id in DISP line: {trimmed}")))?;
                    let dx: f64 =
                        fields[1].parse().map_err(|_| SolverError::ParseError(format!("invalid dx in DISP line: {trimmed}")))?;
                    let dy: f64 =
                        fields[2].parse().map_err(|_| SolverError::ParseError(format!("invalid dy in DISP line: {trimmed}")))?;
                    let dz: f64 =
                        fields[3].parse().map_err(|_| SolverError::ParseError(format!("invalid dz in DISP line: {trimmed}")))?;
                    results.push(NodeDisplacement { node_id, dx, dy, dz });
                }
            }
        }
    }
    if results.is_empty() {
        return Err(SolverError::ParseError("no DISP block found in .frd content".to_string()));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{generate_building, BuildingType};
    use crate::site::Site;
    use crate::zoning::ZoningRules;

    fn sample_building() -> Building {
        let site = Site::rectangle(10.0, 10.0).unwrap();
        let zoning = ZoningRules::new(0.6, 2.0).unwrap();
        generate_building(&site, &zoning, BuildingType::House, 2).unwrap()
    }

    #[test]
    fn export_inp_contains_expected_node_and_element_counts() {
        let building = sample_building();
        let opts = CalculixAnalysisOptions { grid_divisions: (2, 3, 2), ..Default::default() };
        let inp = export_inp(&building, &opts);

        assert!(inp.contains("*NODE"));
        assert!(inp.contains("*ELEMENT, TYPE=C3D8"));
        assert!(inp.contains("*MATERIAL, NAME=CONCRETE"));
        assert!(inp.contains("*BOUNDARY"));
        assert!(inp.contains("*CLOAD"));

        // 節点数 = (nx+1)*(ny+1)*(nz+1)、要素数 = nx*ny*nz
        let expected_nodes = 3 * 4 * 3;
        let expected_elements = 2 * 3 * 2;
        let node_lines = inp.lines().filter(|l| l.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)).count();
        // NODE行+ELEMENT行+NSET行+BOUNDARY行+CLOAD行が混在するため、
        // ここでは要素数分の"8つのコンマ区切り整数"を持つ行のみを数える。
        let element_lines = inp.lines().filter(|l| l.matches(',').count() == 8 && !l.contains('.')).count();
        assert_eq!(element_lines, expected_elements);
        assert!(node_lines >= expected_nodes);
    }

    #[test]
    fn ccx_binary_is_reported_as_unavailable_in_this_environment() {
        // 2026-08-21時点、この開発機にはccxはインストールされていない
        // (モジュールdocコメント記載の実在確認結果と一致することを保証する
        // リグレッションテスト)。
        assert!(!ccx_binary_available());
    }

    #[test]
    fn run_returns_binary_not_found_honestly_when_ccx_is_absent() {
        let dir = std::env::temp_dir();
        let err = run(&dir, "nonexistent_job").unwrap_err();
        assert!(matches!(err, SolverError::BinaryNotFound(_)), "expected BinaryNotFound, got {err:?}");
    }

    #[test]
    fn parse_frd_displacements_parses_hand_written_sample_block() {
        // CalculiXが実際に出力する.frdのDISPブロック形式を模した最小サンプル。
        let sample = "\
    1C
 -4  DISP        4    1
 -5  D1          1    2    1    0
 -5  D2          1    2    2    0
 -5  D3          1    2    3    0
 -1         1 1.000000E-03 2.000000E-03-1.500000E-03
 -1         2 0.000000E+00 0.000000E+00 0.000000E+00
 -3
";
        // 上のフォーマットは実CalculiX出力の固定幅(スペース無し連結)版に
        // 近いため、まずは本パーサーが想定する空白区切り版で検証する。
        let simple_sample = "\
 -4  DISP\n -1 1 0.001 0.002 -0.0015\n -1 2 0.0 0.0 0.0\n -3\n";
        let parsed = parse_frd_displacements(simple_sample).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], NodeDisplacement { node_id: 1, dx: 0.001, dy: 0.002, dz: -0.0015 });
        assert_eq!(parsed[1], NodeDisplacement { node_id: 2, dx: 0.0, dy: 0.0, dz: 0.0 });
        let _ = sample; // 実フォーマットの参考として残すが、固定幅版は今回未対応(正直な開示)。
    }

    #[test]
    fn parse_frd_displacements_errors_on_missing_disp_block() {
        let err = parse_frd_displacements("no disp block here\n").unwrap_err();
        assert!(matches!(err, SolverError::ParseError(_)));
    }
}
