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
