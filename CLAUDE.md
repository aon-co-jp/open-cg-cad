# 設計思想＆開発方針＆開発環境ルール(open-cg-cad)

作業ドライブは`F:\runo`。この節は
[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の`CLAUDE.md`を
正本とし、各プロジェクトへコピーして同期する既存の運用ルール継承方針に
準じる(実機検証必須・正直な開示徹底等の全リポジトリ共通ルールは、
詳細をここに複製せず`open-raid-z/CLAUDE.md`を参照すること)。
GitHubリポジトリ: [aon-co-jp/open-cg-cad](https://github.com/aon-co-jp/open-cg-cad)。

**開発開始日: 2026-08-21**。構想自体は`open-directx/CLAUDE.md`の
2026-08-20〜21付「アイデアメモ」節に記録済み(3Dオンラインゲーム用
モデリングツール構想、「AI不動産×AI工務店」連携構想、「AI工務店＆AI建設」
機能の詳細ワークフロー)。

## ⚠️ 最重要制約(必ず全ドキュメント・コード・UIに一貫して明記すること)

**このシステムは、あくまで人間の有資格者(一級建築士・宅地建物取引士等)
による最終確認の一歩手前までを代行するドラフト/サンプル生成ツールである。
「法的に確定した設計」「建築確認申請に使える正式な図面」を生成するもの
では断じてなく、必ず有資格者のレビューが必要という前提を省略・弱化しない
こと。**この免責事項は`cg-cad-core`クレートのルートdocコメント
(`src/lib.rs`)・README.md/README-English.mdの両方に明記済み。

## このプロジェクトの役割

「AI工務店＆AI建設」機能を提供するCADシステム。不動産検索結果から得られる
複数の土地情報に対応し、(1)日照権・建ぺい率・容積率・耐震性等の関連法規
チェック、(2)3D建築物(オフィスビル・映画館・コンサートホール・
オーディオルーム・4DX/IMAX対応シアタールーム付きビル・一戸建て住宅等)の
CAD自動生成、(3)耐震性等を含む建設シミュレーション、(4)生成CADへの
自動着色、(5)複数工法の提示・選択、(6)顧客向け間取りサンプル提案フロー、
を目指す。

## アーキテクチャ方針

- **Rustワークスペース**(`cg-cad-core`+`cg-cad-render`)。
- `cg-cad-core`: プラットフォーム非依存の純粋ロジック(敷地・法規制
  チェック・メッシュ生成・マテリアル)。Vulkan/グラフィックス依存無し。
- `cg-cad-render`: `open-directx`の既存の実Vulkanグラフィックス
  パイプライン(`directx-graphics-vulkan`、NVIDIA GT 730実機検証済み)を
  path依存で再利用し、新規のVulkan/シェーダーコードを一切書かずに
  メッシュを実際にレンダリングする橋渡し。車輪の再発明を避ける
  (`open-directx/CLAUDE.md`の一貫した方針を踏襲)。
- `open-cuda`/`aruaru-llm`: 今回のセッションでは直接の技術的依存は
  追加していない(将来のGPU計算基盤・自然言語仕様変更インターフェース
  としての活用候補、正直な開示)。

## HANDOFF

- **2026-08-21(続き3) 既存OSSソルバ(OpenFOAM/CalculiX)との連携統合層
  (`solver_bridge`モジュール)を新規実装(ユーザー指示: 本格的なCFD/
  非線形FEM衝突解析ソフトとして開発してほしいという要望に対し、
  「既存の実在・検証済みOSSエンジンを子プロセスとして呼び出し、
  open-cg-cadは入出力・可視化・LLM説明の統合層に徹する」方針で対応)**:
  1. **(a) ソルバーの実在確認結果(正直な開示)**: この開発機(Windows 11)
     には、ネイティブ版・WSL(Ubuntu、`wsl --list --verbose`で存在確認
     済み)版のいずれにも`ccx`(CalculiX)・`blockMesh`/`simpleFoam`
     (OpenFOAM)は**インストールされていない**(`Get-Command ccx`、
     `wsl -e bash -lc "which ccx; which simpleFoam; which openfoam"`
     のいずれも該当なし)。タスク指示により新規インストールは行って
     いない。
  2. **(b) 実装した統合層の内容**: `crates/cg-cad-core/src/
     solver_bridge/`(`mod.rs`+`calculix.rs`+`openfoam.rs`)を新設し、
     `lib.rs`から`pub mod solver_bridge;`として公開した。
     - **入力ファイル変換層**: `calculix::export_inp`が`Building`の
       メッシュbounding boxを軸並行グリッド(既定2x2x2、
       `CalculixAnalysisOptions::grid_divisions`で変更可)のC3D8
       六面体要素へ離散化し、Abaqus互換の`.inp`(`*NODE`/`*ELEMENT`/
       `*MATERIAL`/`*BOUNDARY`/`*CLOAD`/`*STEP`)を文字列として生成する。
       `openfoam::export_case`が`system/{blockMeshDict,controlDict,
       fvSchemes,fvSolution}`・`constant/transportProperties`・
       `0/{U,p}`という`simpleFoam`用の最小限だが構文的に妥当な
       ケースディレクトリを実際にファイルシステムへ書き出す。
     - **子プロセス呼び出し**: `calculix::run`(`ccx -i <job>`)・
       `openfoam::run`(`blockMesh`→`simpleFoam`の順次実行)。
       実行バイナリのパスは環境変数(`OPEN_CG_CAD_CCX_BIN`/
       `OPEN_CG_CAD_BLOCKMESH_BIN`/`OPEN_CG_CAD_SIMPLEFOAM_BIN`、
       既定はPATH上のコマンド名)で上書き可能。**タスク指示は
       `tokio::process::Command`を例示していたが、既存コードベース
       (`reqwest`のblockingクライアント使用等)が非同期ランタイムを
       一切導入していないことと整合させるため、`std::process::
       Command`(同期版)を採用した(正直な設計判断の記録、tokio
       依存を新規追加しない選択)。**
     - **結果ファイルのパーサー**: `calculix::parse_frd_displacements`
       がCalculiXの`.frd`出力の`DISP`ブロック(節点変位)を解析して
       `Vec<NodeDisplacement>`を返す。`openfoam::
       parse_residuals_from_log`が`simpleFoam`の標準出力ログの
       `Solving for <field>, Initial residual = <value>`行を解析して
       `Vec<ResidualEntry>`を返す。**正直な開示(スコープ限定)**:
       OpenFOAM側は、任意の複雑な3次元流れ場全体(通常`foamToVTK`等の
       別ツール経由で得るバイナリ/ASCIIフィールドデータ)の完全な
       パースまでは実装していない——それはOpenFOAM自体の後処理機能の
       再実装に近づいてしまうためスコープ外とした(タスク指示の
       「過大な実装はしない」に基づく判断、`openfoam.rs`冒頭の
       docコメントに明記済み)。`open-directx`のVulkanレンダリング
       パイプラインへの変位・応力分布の橋渡しは、CalculiX側
       (`NodeDisplacement`→`Mesh`頂点変位)は今回のデータ構造として
       実装済みだが、実際に`cg-cad-render`側へ配線する統合(可視化
       パイプラインへの接続)は今回のスコープでは未着手(次回課題)。
  3. **(c) 検証結果(実ソルバーが無い環境でのコードレベル検証)**:
     `cargo test -p cg-cad-core --release`**58件全green**(既存44件+
     `solver_bridge`系14件)。うち`ccx_binary_is_reported_as_unavailable_
     in_this_environment`・`openfoam_binaries_are_reported_as_
     unavailable_in_this_environment`・`run_returns_binary_not_found_
     honestly_when_{ccx,openfoam}_is_absent`の4件は、**この環境で
     実際にバイナリ不在→`SolverError::BinaryNotFound`が返る**ことを
     直接検証するリグレッションテスト(動くふりをしないことの実機
     保証)。加えて`export_inp_contains_expected_node_and_element_
     counts`(生成された`.inp`の節点数・要素数がグリッド分割数から
     計算した期待値と一致)・`export_case_writes_all_expected_files`
     (実際にファイルシステムへ7ファイルすべてが書き出されることを
     `std::fs`で確認)・`parse_frd_displacements_parses_hand_written_
     sample_block`/`parse_residuals_from_log_parses_typical_
     simplefoam_lines`(手書きの実フォーマット準拠サンプルに対する
     パーサーの正しさ)を検証済み。`cargo build --workspace --release`
     ・`cargo clippy -p cg-cad-core --all-targets --release --
     -D warnings`いずれも成功・警告0件(初回`needless_borrow`・
     `ptr_arg`の2件を検出・修正後)。**実際にccx/simpleFoamを起動して
     の計算結果取得は、この環境にバイナリが無いため実施できていない**
     (段階2の実機検証は前提条件が満たされず未達、正直な開示)。
  4. **(d) 今後ユーザー側でOpenFOAM/CalculiXをインストールした場合の
     利用手順**(`solver_bridge/mod.rs`冒頭のdocコメントにも同内容を
     記載済み): (i) CalculiXをインストールし`ccx`がPATH上で実行可能に
     なることを確認(別パスなら`OPEN_CG_CAD_CCX_BIN`にフルパスを設定)、
     (ii) OpenFOAM(WSL経由が一般的)をインストールし`blockMesh`・
     `simpleFoam`がPATH上で実行可能になることを確認、(iii) 上記の
     いずれも、本モジュールの`export_*`→`run`→`parse_*`関数群は
     コード変更無しでそのまま動く設計にしてある。
  5. **簡易フォールバックとしての位置づけの明記**: `lib.rs`冒頭の
     全体免責事項に、既存の`crash_sim`/`tunnel_noise`/`structural`が
     「OSSソルバーが利用できない場合の簡易フォールバック」である旨を
     追記した(削除はしていない、両方使える設計のまま)。
  6. **`git push`は今回も実施していない**(タスク指示通り、ユーザーからの
     明示的な許可待ち)。
  - 次にすべきこと: (1) ユーザーの許可を得た上で`git push`、
    (2) 実際にCalculiX/OpenFOAMをインストールできる環境が用意され次第、
    段階2(実ケースの実行・実計算結果の取得)を実施する、(3)
    `NodeDisplacement`の結果を`cg-cad-render`(`open-directx`のVulkan
    パイプライン)側へ実際に配線し、変位で変形させたメッシュを描画する
    統合、(4) OpenFOAM側の完全なフィールドデータ読み込み
    (`foamToVTK`出力のVTKパーサー等、今回明示的にスコープ外とした
    将来課題)、(5)`crash_sim`(非線形衝突)についても同様にCalculiXの
    陽解法(`*DYNAMIC, EXPLICIT`)ケースへのエクスポートを追加する余地
    がある(今回は静解析`*STATIC`のみ実装)。

- **2026-08-21(続き2) 衝突実験シミュレーション・トンネル微気圧波(騒音)
  デザイン計算シミュレーターを追加実装(`structural.rs`実装パターン
  〈簡易物理モデル+GPU加速行列演算+LLM説明+免責事項徹底〉を踏襲、
  ユーザー指示)**:
  1. **(a) 衝突シミュレーションの実装内容・数値検証結果**: `crates/
     cg-cad-core/src/crash_sim.rs`を新設。車体を複数質点+ばね+ダンパー
     で近似した1次元多質点モデルで、運動方程式`M x'' + C x' + K x = 0`を
     陽解法(中心差分法系)で数値積分し、各質点の変位・速度・加速度の
     時系列、最大加速度(G値換算含む)、最大圧壊変位を求める
     (`simulate_crash`)。安定な時間刻み幅の目安を`stable_time_step_s`
     で提供。乗員保護の参考G値は`VehicleKind::reference_survivable_g`
     として車種別(Car/Motorcycle/Maglev)に用意したが、断定的な法規
     適合判定は行わない設計。**数値検証**: 質量比を極端にして片側を
     事実上の固定端とした2質点系(単振動の解析解が既知)と比較する
     単体テストを実装し、振幅・最大加速度とも相対誤差5%以内で一致を
     確認(`two_mass_undamped_matches_single_harmonic_oscillator_
     period`)。加えて初速が大きいほど最大加速度が大きいこと・減衰が
     あるほど最大圧壊変位が小さくなることも単体テストで検証済み
     (計6テスト、全green)。
  2. **(b) トンネル微気圧波シミュレーションの実装内容・数値検証結果**:
     `crates/cg-cad-core/src/tunnel_noise.rs`を新設。列車先頭形状の
     断面積分布を冪関数`A(x) = A_max*(x/L)^power`で近似
     (`NoseShape`)、その断面積変化率の最大値`max(dA/dx)`を微気圧波
     強度の簡易指標として計算(`micro_pressure_wave_index`、離散前進
     差分演算子との行列-ベクトル積として実装)、複数の`power`候補を
     グリッドサーチして指標最小の形状を提案する
     (`optimize_nose_shape`)。**数値検証**: `power=1`(線形)・
     `power=2`(2次)の断面積分布について解析的な導関数値を手計算し、
     GEMM経由の離散微分結果と相対誤差2%以内で一致することを確認
     (`linear_profile_matches_analytical_gradient`・
     `quadratic_profile_matches_analytical_max_gradient_at_tail`)。
     加えて「powerが大きいほど指標が大きい」「先頭形状が長いほど
     指標が小さい」という工学的に妥当な単調性、グリッドサーチが
     実際に最小指標の候補を選ぶことも単体テストで検証済み
     (計6テスト、全green)。
  3. **(c) GPU加速の実装・確認結果**: 両モジュールとも、`structural.rs`
     と同一パターンで`opencuda_blas::sgemm`(`opencuda_cpu::CpuDevice`
     経由、既存の実機検証済みGEMM実装)へ行列-ベクトル積をpath依存
     させた——`crash_sim`は毎ステップの`K*x`・`C*x`計算、`tunnel_noise`
     は離散微分演算子との積。新規のGPU計算コードは一切書いていない。
     **正直な開示**: `structural.rs`のHANDOFFに記載済みの理由と同様、
     今回のMVP規模(質点数・サンプル数とも小さい)ではCPU経路で十分
     高速であり、実Vulkanデバイスへの切替による効果検証は見送った
     (将来課題として`structural.rs`の既存記載を踏襲)。
  4. **(d) aruaru-llm連携の実装内容・実機検証結果**: `crates/cg-cad-core/
     src/llm_explain.rs`に`build_crash_explanation_prompt`/
     `explain_crash_via_aruaru_llm`・`build_tunnel_noise_explanation_
     prompt`/`explain_tunnel_noise_via_aruaru_llm`を追加(共通の
     `POST /v1/generate`呼び出しロジックを`call_generate`として抽出し
     3経路で共用、重複を避けた)。それぞれ専用の免責事項定数
     (`CRASH_SIM_DISCLAIMER_JA`/`TUNNEL_NOISE_DISCLAIMER_JA`)を説明文
     末尾に必ず付加する。**実際に`aruaru-llm.exe`
     (`F:\runo\aruaru-llm\target\release\aruaru-llm.exe`、ポート4600)を
     起動し、実HTTPリクエストで両シミュレーション結果の説明文が実際に
     生成されることを確認済み**(`cargo test -p cg-cad-core --release --
     -- ignored --nocapture`で`explains_crash_via_real_aruaru_llm_
     server`・`explains_tunnel_noise_via_real_aruaru_llm_server`
     〈構造計算側の既存`explains_via_real_aruaru_llm_server`と合わせて
     計3件〉が実行時間合計約100秒で`ok`)。
  5. **検証結果まとめ**: `cargo build --workspace --release`成功
     (既存クレートへのregression無し)。`cargo test -p cg-cad-core
     --release`**44件全green**(既存27件+`crash_sim`6件+
     `tunnel_noise`6件+`llm_explain`新規5件、`#[ignore]`の実サーバー
     統合テスト3件は上記4番の通り別途実行し全`ok`)。`cargo clippy -p
     cg-cad-core --all-targets --release -- -D warnings`は初回
     `needless_range_loop`警告1件を検出・修正後、警告0件を確認。
  6. **(e) 簡略化している部分(正直な開示、モジュール冒頭のdocコメントにも
     同内容を明記済み)**: 衝突シミュレーションは(i) 3次元ではなく1次元
     (衝突方向のみ)の質点+ばね+ダンパー近似で、材料の塑性変形・破断
     等の非線形挙動は表現していない、(ii) 陽解法(中心差分法系)のため
     ステップ幅が粗いと数値的に不安定になりうる、(iii) 乗員保護の
     参考G値は一般的に引用される目安のオーダーに過ぎず断定的な法規
     適合判定ではない、(iv) リニアモーターカー・バイクも同一モデルの
     延長として扱っており車両ごとの構造特性の違いは反映していない。
     トンネル微気圧波は(v) 本格的な3次元CFD(Navier-Stokes)ではなく
     `max(dA/dx)`という単純な代理指標による1次元近似、(vi) 圧力波の
     伝播・トンネル出口での放射特性等は考慮していない、(vii)
     グリッドサーチという最も単純な総当たり最適化に留まる。
  7. **`git push`は今回も実施していない**(タスク指示通り、ユーザーからの
     明示的な許可待ち)。
  - 次にすべきこと: (1) ユーザーの許可を得た上で`git push`、(2) 実
    Vulkanデバイス経由での`sgemm`実行への切替(現状は`structural.rs`と
    同様CPU経路のみ)、(3)`crash_sim`の3次元化・材料非線形性の考慮、
    (4)`tunnel_noise`のより精密な圧力波動方程式モデルへの拡張、(5)
    生成CAD(`building`/`mesh`モジュール)との連携(例: 生成した建物の
    重量・階高を`crash_sim`/`structural`の入力として自動連携させる)。

- **2026-08-21(続き) 構造計算・耐震シミュレーションを実装(`structural`
  モジュール、それまでの「プレースホルダのみ・常にNotImplemented」
  から実装へ変更)、`aruaru-llm`連携(結果の自然言語説明)も実装・実HTTPで
  検証完了(ユーザー指示: 構造計算・耐震シミュレーションを実際に行える
  ようにし、`open-cuda`のGEMM実装で高速化、`aruaru-llm`も連携させる)**:
  1. **(a) 構造計算エンジンの実装内容・数値検証結果**: `crates/
     cg-cad-core/src/structural.rs`に、各階を1つの水平自由度へ集約した
     「せん断建物(shear building)」簡易骨組みモデル+線形弾性静解析を
     実装した。柱の水平剛性(`12EI/h^3`、両端固定はり近似)から全体
     剛性マトリックス(三重対角)を組み立て、自重に基づく簡易地震荷重
     (震度に応じた水平力の一定係数近似)を外力ベクトルとし、`Kx=F`を
     解いて各階の水平変位・層間変形角を求め、建築基準法施行令第82条の2
     の目安値(1/200)と比較する。**数値検証**: 単純な1自由度ばね系
     (`x=F/K`)・2自由度連立方程式(Cramerの公式による手計算)という
     2つの既知の解析解と、実装した`solve_kx_f_via_gemm`(後述)の出力を
     比較する単体テストを実装し、相対誤差1%以内で一致することを確認
     (`single_story_matches_hand_calculated_spring_deflection`・
     `two_story_matches_hand_calculated_2x2_linear_system`)。加えて
     高層建物ほど変位が大きくなること・震度係数0で変位0になることも
     テストで検証済み。
  2. **(b) GPU加速の実装方法・実機確認結果**: `Kx=F`をヤコビ反復法で
     解く際、行列-ベクトル積`K*x_k`の計算を`opencuda_blas::sgemm`
     (`open-cuda`側の既存GEMM実装、`GemmPath::CpuNaive`、実機検証済み)
     へpath依存させて実行する設計にした(新規にGPU計算コードを書かず
     既存資産を再利用、タスク指示通り)。`cg-cad-core/Cargo.toml`に
     `opencuda-core`/`opencuda-cpu`/`opencuda-blas`をpath依存として
     追加。**正直な開示**: 今回`CpuDevice`(CPUバックエンド)経由で
     `sgemm`を呼ぶ実装・検証に留めた——`sgemm`のシグネチャは`spirv`
     引数でVulkan経路(`GemmPath::VulkanGeneric`)にも切替可能な設計に
     なっているため、実Vulkanデバイス(NVIDIA GT 730)への切替自体は
     `opencuda-vulkan::real::VulkanDevice`を渡し`matmul.spv`を用意
     すれば技術的には可能だが、このMVPの規模(数階〜十数階、n×nの
     小さい行列)ではCPU実行で十分高速であり、GPU実機への切替による
     効果を実証する意義が薄いと判断し、今回はCPU経路での実機検証
     (`cargo test -p cg-cad-core --release`、実際に`opencuda_cpu::
     CpuDevice`上で`launch_kernel`がディスパッチされることを経由して
     計算結果が正しいことを検証)に留めた。将来、大規模な建物(数百階
     クラス等、非現実的だが)や複数建物の一括解析が必要になった場合は
     Vulkan経路への切替が有効になりうる。
  3. **(c) aruaru-llm連携の実装内容・検証結果**: `crates/cg-cad-core/
     src/llm_explain.rs`を新設。`analyze_structure`が返す計算済みの
     数値結果(層間変形角・判定結果)を日本語プロンプトへ整形し
     (`build_explanation_prompt`、「あなたは計算をやり直す必要はない」
     と明記してLLMに計算させようとしないよう誘導)、`aruaru-llm`の
     `POST /v1/generate`(reqwest blockingクライアント)へ渡して説明文を
     取得する。**実際にaruaru-llmサーバー(`F:\runo\aruaru-llm\target\
     release\aruaru-llm.exe`)を起動し、実HTTPリクエストで説明文が
     生成されることを確認済み**(`cargo test -p cg-cad-core --release
     -- --ignored --nocapture explains_via_real_aruaru_llm_server`、
     `ARUARU_LLM_TEST_URL=http://127.0.0.1:4600`、実行時間約65秒
     〈GPT-2 124M・CPU貪欲デコード、`max_new_tokens=96`〉、`ok`)。
     免責事項(`CG_CAD_DISCLAIMER_JA`、「AIによる参考情報であり正式な
     構造計算書ではない、有資格者の確認が必要」)を生成された説明文の
     末尾に必ず付加する設計とし、その付加が実際に行われることもテストで
     検証した。**正直な開示**: LLM(GPT-2 124M系)は事前学習のみで
     対話ファインチューニング無しのため、生成される説明文の品質・
     一貫性は保証されない(`aruaru-llm`自身のCLAUDE.mdに記載の限界と
     同じ)——本連携が実証するのは「数値結果を渡して説明文を生成させる
     配線が実際に機能する」ことであり、生成文の教育的な質の高さを
     主張するものではない。
  4. **(d) 簡略化している部分(正直な開示)**: (i) 本格的な有限要素法
     (3次元ソリッド要素・シェル要素)ではなく、各階1自由度のせん断
     建物モデル(剛床仮定)——柱以外の部材(梁・壁等)の剛性は考慮して
     いない。(ii) 断面・ヤング率は建物種別によらない代表値の定数
     (`StructuralMaterial::default`)であり、実際の構造設計のように
     部材ごとに個別決定していない。(iii) 地震荷重は精密なAi分布・
     応答スペクトル法ではなく、各階重量への一定係数(簡易ベースシア
     係数)近似。(iv) P-Δ効果・非線形挙動(部材降伏・座屈)・時刻歴
     応答解析は一切行っていない。(v) 層間変形角の目安値1/200は
     建築基準法施行令第82条の2に基づく一般的な目安であり、実際の
     許容値は建物用途・構造種別等により異なりうる——本コードはこの
     数値をハードコードした目安判定に過ぎず、法的な確認ではない。
     (vi) `structural`モジュール冒頭・出力の`disclaimer`フィールド・
     LLM説明文の末尾いずれにも、上記(vi)含む簡易性を明記済み。
  5. **検証結果まとめ**: `cargo build --workspace --release`成功
     (`cg-cad-render`含む既存クレートへのregression無し)。
     `cargo test -p cg-cad-core --release` **27件全green**(既存21件+
     新規`structural`4件+`llm_explain`2件、`explains_via_real_aruaru_
     llm_server`は`#[ignore]`のためデフォルトでは走らないが上記3番の
     通り実際に実行し`ok`を確認済み)。`cargo clippy -p cg-cad-core
     --all-targets --release -- -D warnings`警告0件。
  6. **`git push`は今回も実施していない**(タスク指示通り、ユーザーからの
     明示的な許可待ち)。
  - 次にすべきこと: (1) 実Vulkanデバイス(`opencuda-vulkan::real::
    VulkanDevice`)経由での`sgemm`実行への切替・実機ベンチマーク
    (現状はCPU経路のみ検証済み、上記2番参照)、(2) `building.rs`の
    直方体ボリュームから、実際の柱・梁の座標付きノード/要素モデルへ
    より精密に変換する処理(現状は各階の柱本数・断面を一律の代表値と
    して扱っており、`Mesh`の実形状とは直接連動していない)、(3)
    `aruaru-llm`の`/v1/chat`ではなく`/v1/generate`を経由しているため、
    英語プロンプト向けに事前学習されたGPT-2 124Mへ日本語プロンプトを
    渡している非対称がある(`aruaru-llm`自身の既存の開示通り、日本語
    生成品質は保証されない)——将来、日本語対応モデルへの切替が
    `aruaru-llm`側で行われれば恩恵を受けられる設計にしてある。

- **2026-08-21 段階0〜1(MVP)実装、実機Vulkanレンダリングまで検証完了**:
  1. **段階0(リポジトリ初期化)**: `open-raid-z/CLAUDE.md`・
     `open-directx/CLAUDE.md`を参照して方針を確認した上で、Cargo
     ワークスペース(`cg-cad-core`+`cg-cad-render`)を新規作成。README.md・
     README-English.md・PORTING.md・本CLAUDE.mdを新規作成し、免責事項を
     全ファイルに一貫して明記した。
  2. **段階1(最小限の3D形状生成)**:
     - `cg-cad-core::site::Site`: 多角形敷地(シューレース公式による
       面積計算、矩形の簡易コンストラクタ、外接矩形算出)。
     - `cg-cad-core::zoning::ZoningRules`/`check_zoning_compliance`:
       建ぺい率・容積率の入力値ベースのチェック(Web検索等による自動
       取得は今回のスコープ外、正直な開示——`ZoningRules`は完全に
       ユーザー設定可能な入力値として扱う設計)。**タスク指示に明記
       された具体的検証ケース(敷地面積100㎡・建ぺい率60%の場合、
       建築面積60㎡を超える設計は却下される)をユニットテストとして
       実装・green化した**(`zoning::tests::
       footprint_exceeding_60_percent_coverage_on_100m2_site_is_rejected`)。
     - `cg-cad-core::mesh::Mesh`: 頂点+インデックスの三角形メッシュ、
       軸並行直方体(ボックス)生成(`axis_aligned_box`、8頂点・12三角形)。
     - `cg-cad-core::building::generate_building`: 敷地の外接矩形を
       建ぺい率上限に収まるよう面積比でスケールダウンし、階数x階高で
       高さを求め、`check_zoning_compliance`で適合を検証してから直方体
       ボリュームを生成する。不適合の場合は`CadError`で明確に却下し、
       誤ったドラフトを黙って返さない設計。
     - `cg-cad-render`: `open-directx`の`render_indexed_scene_with_depth_
       and_read_back`(既存の実深度テスト付きインデックス描画パイプ
       ライン)をそのまま再利用し、`triangle_vs.dxbc`/`triangle_ps.dxbc`
       (既存の実fxc.exeコンパイル済みパススルーシェーダー)も再翻訳して
       再利用。`project_top_down`(俯瞰視点の正射影、新規実装は座標変換
       のみ)でワールド座標のメッシュをNDC座標へ変換し、実際にVulkanへ
       ディスパッチしてフレームバッファを読み戻す。
  3. **実機検証で発見・修正した実バグ(型チェックのみで完了と報告しない
     方針の徹底、具体例)**: 実機テスト
     `house_render_real_vulkan.rs::generated_house_footprint_actually_
     renders_on_real_vulkan_hardware`を実行したところ、初回は被覆
     ピクセル数676(期待値1706.7の40%以下)で失敗した。原因調査の結果、
     `project_top_down`が敷地の中心座標を考慮せずワールド座標を
     そのまま`half_extent_m`で割ってNDC化しており、`Site::rectangle`が
     原点ではなく敷地の実際の位置(例: (0,0)〜(10,10)の場合中心は
     (5,5))にメッシュを配置するため、NDC範囲`[-1,1]`からメッシュの
     大部分がはみ出して意図せずクリップされていた。メッシュのx/z
     bounding boxの中心を求めて原点補正するよう修正し、再実行した結果
     被覆ピクセル数1764(期待値1706.7の3%以内)まで改善、実際に
     House用マテリアル色(`material::default_color_for`)と一致する
     ピクセルが存在することも確認した。**もし実機テストを書かず
     型チェック・ビルド成功だけで「動いた」と報告していたら、この
     座標変換バグを見逃していた**——実機検証の重要性を裏付ける具体例
     として記録する。
  4. **実際に`cargo test --workspace --release -- --nocapture`で確認した
     結果(誇張なし、実出力そのまま、NVIDIA GeForce GT 730)**:
     ```
     cg-cad-core: 21 tests, all ok
     cg-cad-render (unit): 3 tests, all ok
     cg-cad-render (house_render_real_vulkan, 実機): 1 test, ok
       "OK: cg-cad-core generated a 60.00m2 footprint House (site 100m2,
        coverage 60%), rendered it via open-directx's real Vulkan pipeline
        on the real GPU present on this machine, and 1764 of 64x64
        read-back pixels are non-background (expected ~1706.7 from the
        footprint/viewport area ratio)."
     ```
     `cargo build --workspace --release`/`cargo clippy --workspace
     --all-targets --release -- -D warnings`はいずれも警告0件。
  5. **段階2(複数建物種別)**: `BuildingType`列挙(House/OfficeBuilding/
     Theater/AudioRoom/CinemaComplex)を実装、種別ごとに異なる階高定数
     (`floor_height_m`)を持たせ、ユニットテストで「同じ階数でも建物種別
     によって高さが異なる」ことを検証済み。**正直な開示**: いずれも
     直方体レベルの簡易形状であり(タスク指示通り)、映画館/コンサート
     ホール等の複雑な内部形状(客席の傾斜等)は表現していない。
  6. **段階3(着色)**: `cg-cad-core::material::default_color_for`で
     建物種別ごとの単色マテリアルを割り当て、実機レンダリングテストで
     実際に読み戻したピクセル色がHouse用マテリアル色と一致することを
     確認した(段階3の「実際にレンダリング結果に反映されることを確認
     する」という指示に対応)。**正直な開示**: 面ごとの異なるテクスチャ
     割り当てのような本格的なマテリアルシステムではなく、建物種別に
     応じた単色のフラットシェーディングに留まる。
  7. **段階4(法規制情報の扱い)**: タスク指示通り、Web検索による最新
     法規制の自動取得は実装していない。`ZoningRules`は完全にユーザー
     設定可能な入力値として扱う設計(将来課題として本ファイルに明記)。
  8. **見送った部分とその理由(正直な開示)**:
     - **段階5(AIチャットによる仕様変更)**: 今回は未着手。タスク指示に
       「時間が許せば」「無理に実装しなくてよい」と明記されており、
       段階0〜3の実機検証を伴う実装を優先したため見送った。
       `aruaru-llm`との連携は将来課題として`README.md`に記載済み。
     - **構造計算・耐震シミュレーション**: `cg-cad-core::structural`は
       意図的に「未実装であることを明示するプレースホルダ」のみ実装
       (`StructuralCheckResult::NotImplemented`を常に返す、動くふりの
       コードは書いていない)。専門ソフトウェアとの連携が必要な領域と
       判断し、タスク指示通り実装対象外とした。
     - **複数の工法(重量鉄骨・軽量鉄骨・在来工法等)の提示・選択UI**:
       今回のスコープでは未着手(段階0〜3の3Dモデリング・法規制チェック・
       着色の実証を優先した)。次回課題。
     - **顧客向け間取りサンプル提案フロー・UI**: 今回のスコープでは
       未着手(バックエンドのCADロジック・レンダリング実証を優先)。
     - **`open-cuda`/`aruaru-llm`との直接統合**: 今回は`open-directx`の
       Vulkanレンダリングのみを再利用し、`open-cuda`のGPU計算基盤・
       `aruaru-llm`のLLM推論エンジンへの直接依存は追加していない
       (README.mdに将来候補として明記)。
  9. **`git push`は今回実施していない**(タスク指示通り、ユーザーからの
     明示的な許可待ち)。
  - 次にすべきこと: (1) ユーザーの許可を得た上で`git push`、
    (2) 複数建物種別のより現実的な形状(単純な直方体の組み合わせ、例:
    オフィスビルの複数棟構成)への拡張、(3) 複数工法の提示・選択機能、
    (4) 顧客向け間取りサンプル提案フロー、(5) 余裕があれば段階5
    (`aruaru-llm`連携による自然言語仕様変更プロトタイプ)、(6) 法規制
    自動取得(Web検索連携、今回は明示的にスコープ外とした将来課題)。
