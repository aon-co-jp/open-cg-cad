//! OpenFOAM(実在・オープンソースの3次元CFDツールキット)への入出力
//! ブリッジ。
//!
//! **このモジュールはOpenFOAM自体のNavier-Stokesソルバー機能を一切
//! 模倣しない。** `tunnel_noise.rs`が行っている断面積変化率による1次元
//! 簡易近似の代わりに、実際のOpenFOAMケースディレクトリ(`system/`・
//! `constant/`・`0/`)を生成し(`export_case`)、`blockMesh`→`simpleFoam`
//! を子プロセスとして順に実行し(`run`)、ソルバーのログ出力から
//! 残差(residual)の収束推移を解析して返す(`parse_residuals_from_log`)。
//!
//! **正直な開示(スコープの限定)**: OpenFOAMの結果ファイルは通常
//! バイナリ/ASCIIのVTK的なフィールドデータ(`postProcess`や`foamToVTK`
//! 等の別ツールを介するのが一般的)であり、任意の複雑な流れ場全体を
//! この統合層だけで完全にパースすることは「本格的なCFD自体の再実装」に
//! 近づいてしまうためスコープ外とした(タスク指示の「過大な実装はしない」
//! に基づく判断)。本モジュールが実装するのは、(1)ケースディレクトリを
//! 正しく生成できること、(2)ソルバーを子プロセスとして呼び出せること、
//! (3)ソルバーが標準出力に吐く残差ログという最も基本的な収束情報を
//! 解析できること、の3点に留める。`open-directx`のVulkanレンダリング
//! パイプラインへ完全な3次元流れ場を渡すには、別途`foamToVTK`等の
//! OpenFOAM公式後処理ツールの出力(VTKファイル)を読み込むパーサーを
//! 追加実装する必要があり、これは今後の課題として明記する。

use super::{binary_exists, resolve_binary, run_checked, SolverError};
use std::path::{Path, PathBuf};

/// OpenFOAMケースの直方体計算領域パラメータ。
#[derive(Debug, Clone, Copy)]
pub struct OpenFoamCaseOptions {
    /// 計算領域(直方体)のコーナー座標(m)。
    pub domain_min: [f64; 3],
    pub domain_max: [f64; 3],
    /// X/Y/Z方向のブロック分割数。
    pub cells: (u32, u32, u32),
    /// 流入境界(inlet)の一様流速(m/s、X方向)。
    pub inlet_velocity_mps: f64,
}

impl Default for OpenFoamCaseOptions {
    fn default() -> Self {
        OpenFoamCaseOptions { domain_min: [0.0, 0.0, 0.0], domain_max: [10.0, 2.0, 2.0], cells: (20, 8, 8), inlet_velocity_mps: 30.0 }
    }
}

fn write_file(dir: &Path, relative: &str, content: &str) -> Result<(), SolverError> {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

/// OpenFOAMの`simpleFoam`(非圧縮性定常流ソルバー)用の最小限のケース
/// ディレクトリを`case_dir`以下に生成する。生成するファイルはいずれも
/// OpenFOAMの標準テンプレート構成に沿った最小構成
/// (`system/controlDict`・`system/fvSchemes`・`system/fvSolution`・
/// `system/blockMeshDict`・`constant/transportProperties`・`0/U`・`0/p`)。
pub fn export_case(case_dir: &Path, opts: &OpenFoamCaseOptions) -> Result<(), SolverError> {
    let (nx, ny, nz) = opts.cells;
    let [x0, y0, z0] = opts.domain_min;
    let [x1, y1, z1] = opts.domain_max;

    let block_mesh_dict = format!(
        "FoamFile\n{{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object blockMeshDict;\n}}\n\
convertToMeters 1;\n\
vertices\n(\n\
    ({x0} {y0} {z0})\n    ({x1} {y0} {z0})\n    ({x1} {y1} {z0})\n    ({x0} {y1} {z0})\n\
    ({x0} {y0} {z1})\n    ({x1} {y0} {z1})\n    ({x1} {y1} {z1})\n    ({x0} {y1} {z1})\n\
);\n\
blocks\n(\n    hex (0 1 2 3 4 5 6 7) ({nx} {ny} {nz}) simpleGrading (1 1 1)\n);\n\
edges\n(\n);\n\
boundary\n(\n\
    inlet {{ type patch; faces ((0 4 7 3)); }}\n\
    outlet {{ type patch; faces ((1 2 6 5)); }}\n\
    walls {{ type wall; faces ((0 1 5 4) (3 7 6 2) (0 3 2 1) (4 5 6 7)); }}\n\
);\n\
mergePatchPairs\n(\n);\n"
    );
    write_file(case_dir, "system/blockMeshDict", &block_mesh_dict)?;

    let control_dict = "FoamFile\n{ version 2.0; format ascii; class dictionary; object controlDict; }\n\
application     simpleFoam;\nstartFrom       startTime;\nstartTime       0;\nstopAt          endTime;\n\
endTime         1000;\ndeltaT          1;\nwriteControl    timeStep;\nwriteInterval   100;\n\
purgeWrite      0;\nwriteFormat     ascii;\nwritePrecision  6;\nwriteCompression off;\ntimeFormat      general;\n\
timePrecision   6;\nrunTimeModifiable true;\n";
    write_file(case_dir, "system/controlDict", control_dict)?;

    let fv_schemes = "FoamFile\n{ version 2.0; format ascii; class dictionary; object fvSchemes; }\n\
ddtSchemes { default steadyState; }\n\
gradSchemes { default Gauss linear; }\n\
divSchemes { default none; div(phi,U) bounded Gauss upwind; div((nuEff*dev2(T(grad(U))))) Gauss linear; }\n\
laplacianSchemes { default Gauss linear corrected; }\n\
interpolationSchemes { default linear; }\n\
snGradSchemes { default corrected; }\n";
    write_file(case_dir, "system/fvSchemes", fv_schemes)?;

    let fv_solution = "FoamFile\n{ version 2.0; format ascii; class dictionary; object fvSolution; }\n\
solvers { p { solver GAMG; tolerance 1e-06; relTol 0.1; } U { solver smoothSolver; smoother GaussSeidel; tolerance 1e-08; relTol 0.1; } }\n\
SIMPLE { nNonOrthogonalCorrectors 0; residualControl { p 1e-3; U 1e-4; } }\n\
relaxationFactors { fields { p 0.3; } equations { U 0.7; } }\n";
    write_file(case_dir, "system/fvSolution", fv_solution)?;

    let transport_properties = "FoamFile\n{ version 2.0; format ascii; class dictionary; object transportProperties; }\n\
transportModel  Newtonian;\nnu              1.5e-05;\n";
    write_file(case_dir, "constant/transportProperties", transport_properties)?;

    let u_field = format!(
        "FoamFile\n{{ version 2.0; format ascii; class volVectorField; object U; }}\n\
dimensions      [0 1 -1 0 0 0 0];\n\
internalField   uniform (0 0 0);\n\
boundaryField\n{{\n\
    inlet {{ type fixedValue; value uniform ({} 0 0); }}\n\
    outlet {{ type zeroGradient; }}\n\
    walls {{ type noSlip; }}\n\
}}\n",
        opts.inlet_velocity_mps
    );
    write_file(case_dir, "0/U", &u_field)?;

    let p_field = "FoamFile\n{ version 2.0; format ascii; class volScalarField; object p; }\n\
dimensions      [0 2 -2 0 0 0 0];\ninternalField   uniform 0;\n\
boundaryField { inlet { type zeroGradient; } outlet { type fixedValue; value uniform 0; } walls { type zeroGradient; } }\n";
    write_file(case_dir, "0/p", p_field)?;

    Ok(())
}

/// `blockMesh`・`simpleFoam`の両バイナリがこの環境で実行可能かを確認する。
pub fn openfoam_binaries_available() -> (bool, bool) {
    let block_mesh = binary_exists(&resolve_binary("OPEN_CG_CAD_BLOCKMESH_BIN", "blockMesh"));
    let simple_foam = binary_exists(&resolve_binary("OPEN_CG_CAD_SIMPLEFOAM_BIN", "simpleFoam"));
    (block_mesh, simple_foam)
}

/// `case_dir`に対して`blockMesh`→`simpleFoam`を順に実行する。
/// どちらかのバイナリが見つからない場合は[`SolverError::BinaryNotFound`]
/// を返す(動くふりをしない)。
pub fn run(case_dir: &Path) -> Result<String, SolverError> {
    let block_mesh_bin = resolve_binary("OPEN_CG_CAD_BLOCKMESH_BIN", "blockMesh");
    let simple_foam_bin = resolve_binary("OPEN_CG_CAD_SIMPLEFOAM_BIN", "simpleFoam");

    if !binary_exists(&block_mesh_bin) {
        return Err(SolverError::BinaryNotFound(block_mesh_bin.to_string_lossy().to_string()));
    }
    if !binary_exists(&simple_foam_bin) {
        return Err(SolverError::BinaryNotFound(simple_foam_bin.to_string_lossy().to_string()));
    }

    let mut block_mesh_cmd = std::process::Command::new(&block_mesh_bin);
    block_mesh_cmd.current_dir(case_dir);
    run_checked(block_mesh_cmd, &block_mesh_bin.to_string_lossy())?;

    let mut simple_foam_cmd = std::process::Command::new(&simple_foam_bin);
    simple_foam_cmd.current_dir(case_dir);
    let output = run_checked(simple_foam_cmd, &simple_foam_bin.to_string_lossy())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 1回の反復(タイムステップ)における各フィールドの初期残差。
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualEntry {
    pub field: String,
    pub initial_residual: f64,
}

/// `simpleFoam`の標準出力ログから
/// `Solving for <field>, Initial residual = <value>, ...`形式の行を
/// 解析し、残差の推移を返す(OpenFOAMソルバーが実際に出力する典型的な
/// ログ行形式に基づく最小限のパーサー)。
pub fn parse_residuals_from_log(log: &str) -> Vec<ResidualEntry> {
    let mut entries = Vec::new();
    for line in log.lines() {
        let trimmed = line.trim();
        let Some(solving_idx) = trimmed.find("Solving for ") else { continue };
        let after_for = &trimmed[solving_idx + "Solving for ".len()..];
        let Some(comma_idx) = after_for.find(',') else { continue };
        let field = after_for[..comma_idx].trim().to_string();

        let marker = "Initial residual = ";
        let Some(marker_idx) = after_for.find(marker) else { continue };
        let after_marker = &after_for[marker_idx + marker.len()..];
        let value_str: String = after_marker.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == 'E' || *c == '+').collect();
        if let Ok(value) = value_str.parse::<f64>() {
            entries.push(ResidualEntry { field, initial_residual: value });
        }
    }
    entries
}

/// `case_dir`のパス(テスト・呼び出し側での確認用の小さいヘルパー)。
pub fn case_file_path(case_dir: &Path, relative: &str) -> PathBuf {
    case_dir.join(relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_case_writes_all_expected_files() {
        let dir = std::env::temp_dir().join(format!("open_cg_cad_openfoam_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let opts = OpenFoamCaseOptions::default();
        export_case(&dir, &opts).unwrap();

        for relative in ["system/blockMeshDict", "system/controlDict", "system/fvSchemes", "system/fvSolution", "constant/transportProperties", "0/U", "0/p"] {
            let path = case_file_path(&dir, relative);
            assert!(path.is_file(), "expected {relative} to exist at {path:?}");
        }
        let u_content = std::fs::read_to_string(case_file_path(&dir, "0/U")).unwrap();
        assert!(u_content.contains(&format!("({} 0 0)", opts.inlet_velocity_mps)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn openfoam_binaries_are_reported_as_unavailable_in_this_environment() {
        // 2026-08-21時点、この開発機・WSL(Ubuntu)いずれにもOpenFOAMは
        // インストールされていない(モジュールdocコメント記載の実在確認
        // 結果と一致することを保証するリグレッションテスト)。
        let (block_mesh, simple_foam) = openfoam_binaries_available();
        assert!(!block_mesh);
        assert!(!simple_foam);
    }

    #[test]
    fn run_returns_binary_not_found_honestly_when_openfoam_is_absent() {
        let dir = std::env::temp_dir();
        let err = run(&dir).unwrap_err();
        assert!(matches!(err, SolverError::BinaryNotFound(_)), "expected BinaryNotFound, got {err:?}");
    }

    #[test]
    fn parse_residuals_from_log_parses_typical_simplefoam_lines() {
        let log = "\
Time = 1\n\
smoothSolver:  Solving for Ux, Initial residual = 1, Final residual = 0.0123, No Iterations 3\n\
smoothSolver:  Solving for Uy, Initial residual = 0.998, Final residual = 0.0111, No Iterations 3\n\
GAMG:  Solving for p, Initial residual = 1, Final residual = 0.0456, No Iterations 5\n\
time step continuity errors : sum local = 1.2e-05, global = -3.4e-07, cumulative = -3.4e-07\n";
        let entries = parse_residuals_from_log(log);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], ResidualEntry { field: "Ux".to_string(), initial_residual: 1.0 });
        assert_eq!(entries[1].field, "Uy");
        assert!((entries[1].initial_residual - 0.998).abs() < 1e-9);
        assert_eq!(entries[2].field, "p");
    }

    #[test]
    fn parse_residuals_from_log_returns_empty_for_unrelated_text() {
        let entries = parse_residuals_from_log("nothing relevant here\n");
        assert!(entries.is_empty());
    }
}
