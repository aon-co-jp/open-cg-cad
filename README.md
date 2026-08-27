# open-cg-cad

*English*: [README-English.md](README-English.md)

「AI工務店＆AI建設」機能を提供するCADシステム。不動産検索結果から得られる
複数の土地情報に対応し、日照権・建ぺい率・容積率等の法規制チェック、
3D建築物ボリュームの自動CAD生成、着色、複数工法の提示等を行う。
建物種別として通常の映画館用シアター(`CinemaComplex`)とは別に、一戸建て
住宅規模の4DX/IMAX(`BuildingType::HomeCinema4dxImax`)も区別して扱う。

**2026-08-27追記**: 半導体(CPU/NPU/GPU)・自動車/バイク/新幹線/
リニアモーターカー/航空機(ガソリン/ディーゼル/EV/PHEV等のエンジンを
含む)の旧図面を、指示文または再アップロードした指示書/仕様書と照らし
合わせてAIが再設計提案を生成する機能、GitHub読み書き・VPS(レンタル
サーバー)読み書きエージェントを追加(詳細・正直な制約は
[CLAUDE.md](CLAUDE.md)参照)。`https://easy-web.tokyo/open-cg-cad/`
(本番稼働中、ローカルドライブへダウンロードしての運用を前提とした
デモ・ダウンロード導線は`https://easy-web.tokyo/open-cg-cad/demo`)。

## ⚠️ 重要な免責事項(必読)

**このシステムは、あくまで人間の有資格者(一級建築士・宅地建物取引士等)に
よる最終確認の一歩手前までを代行するドラフト/サンプル生成ツールである。
「法的に確定した設計」「建築確認申請に使える正式な図面」を生成するもの
では断じてなく、必ず有資格者のレビューが必要。**

- 構造計算・耐震シミュレーションは、各階を1自由度に集約した簡易骨組み
  モデル+線形静解析としてMVP実装済み(`cg-cad-core::structural`)だが、
  本格的なFEM・専門の構造計算ソフトウェアの代替ではない。より本格的な
  解析には`cg-cad-core::solver_bridge`経由でCalculiX/OpenFOAM(別途
  ユーザー側でインストールが必要)を呼び出せる設計。
- 法規制パラメータ(建ぺい率・容積率の上限値等)は、Web検索等による
  最新情報の自動取得は行わず、**ユーザーが設定する入力値**として扱う
  (今回のスコープ外、将来課題)。
- 生成される建築物形状は直方体(またはその単純な組み合わせ)に限定
  した、意匠検討の叩き台レベルのドラフトである。

詳細は[CLAUDE.md](CLAUDE.md)を参照。

## 構成

- [`crates/cg-cad-core`](crates/cg-cad-core): 敷地(`Site`)・法規制
  (`ZoningRules`)・建物種別(`BuildingType`)・メッシュ(`Mesh`)・
  マテリアル・建築物ボリューム生成(`generate_building`)のコアロジック。
  以下のモジュールを含む(主要な型・関数はいずれも`cg_cad_core::`直下
  からも参照可能):
  - `structural`: 簡易骨組みモデルによる構造計算・層間変形角判定。
  - `crash_sim`: 車両衝突実験の質点+ばね+ダンパー簡易シミュレーション
    (Car/Motorcycle/Maglev/Bus/Truck/Train対応)。`crosswind`/
    `tire_wet_grip`の参考指標を衝突条件へ変換する簡易橋渡し
    (`EnvironmentConditions`/`apply_environment_conditions`)も提供。
  - `airbag`: ハンドル(運転席)/助手席/カーテン/サイドの4種類の
    エアバッグ展開タイミング・乗員保護効果の簡易シミュレーション。
    `crash_sim`の車体加速度時系列を入力に、乗員1質点+ばね+ダンパーで
    近似する。
  - `tunnel_noise`: トンネル微気圧波(騒音)軽減のための列車先頭形状
    デザイン計算。
  - `solver_bridge`: 既存OSSソルバー(CalculiX/OpenFOAM)への入出力
    ブリッジ(実行バイナリが無い環境では正直にエラーを返す)。
  - `aero`: 車両・列車の簡易空力(揚力/ダウンフォース)計算。
  - `crosswind`: 横風時の転倒モーメント・限界風速の簡易計算。
  - `tire_wet_grip`: タイヤトレッドパターンの雨天時グリップ簡易参考指標。
  - `nwd_control`: 2WD〜8WD以上の駆動トルク配分(トルクベクタリング)
    簡易シミュレーション。
  - `manufacturing_export`: STL(3Dプリンター向け)・ロボットアーム
    経路JSONの製造データ出力に加え、DXF(R12、AutoCAD等主要CADアプリと
    互換のオープンな交換フォーマット)出力(`mesh_to_dxf_r12`)。
    **正直な開示**: AutoCADのネイティブ形式DWG(非公開バイナリ)は
    知的財産リスクを避けるため一切対応せず、今後も対応しない方針。
    DXFはAutodesk自身が公開する仕様であり、AutoCAD/FreeCAD/LibreCAD/
    SolidWorks/Fusion 360/SketchUp/Revit等、世界的に広く使われるCAD
    アプリのほぼ全てが読み込める。三角形は`3DFACE`の4頂点表現
    (4点目=3点目)というDXFの標準的な慣習に従う。
  - `acoustic_material`: 木材種(オーク/ブナ/スギ/キリ)+MDF+
    コンクリート+吸音パネル/フォーム/カーペット/劇場座席、計10種類の
    内装材のヤンカ硬度・吸音率(500Hz帯代表値)を収録した簡易DB。
    部屋の各面(壁/天井/床)への材料割り当てからSabineの残響式
    (`RT60 = 0.161*V/A`)でRT60(残響時間)を推定する
    (`reverberation_time_rt60`)。`Theater`/`AudioRoom`/`CinemaComplex`/
    `HomeCinema4dxImax`向けの音響設計一次スクリーニング用途。
    **正直な開示**: 吸音率・ヤンカ硬度は文献・カタログの代表値であり
    個体差は反映しない、Sabine式は拡散音場を仮定した最も基本的な近似式
    (周波数特性・部屋形状・空気吸収は考慮しない)であり、本格的な室内
    音響シミュレーションの代替ではない。最終判断には音響コンサルタント
    等の専門家レビューが必要。
  - `vehicle_config`: 自動車CADコンフィグ(エンジン換装・2WD⇔4WD・
    2ドア⇔4ドア・SUV化等のボディスタイル変更)と、Euro6/日本ポスト
    新長期/US Tier3の排出ガス規制簡易適合チェック。**正直な開示**:
    Astron Aerospace社のOmega 1(ピストンレス水素ロータリーエンジン)・
    ジルコニアセラミックエンジン・核融合エンジンPHEVは、いずれも
    研究構想・未実証の技術として`is_experimental() == true`の
    プレースホルダーに留め、出力推定は常に`NotImplemented`(動くふりを
    しない設計)を返す。実在技術(ガソリン/ディーゼル/HV/BEV/FCEV)
    のみ簡易出力推定・排出ガス規制チェックが実際に機能する。
  - `rail_vehicle`: 鉄道車両CADコンフィグ。新幹線/リニアモーターカー/
    特急/普通の各種別×単層/二階建て×標準/グリーン車×指定/自由席の
    組み合わせで編成を構成し、座席ピッチ・車両長・1列あたり座席数から
    定員を推定する。**正直な開示**: 二階建ての定員倍率は実在した
    二階建て新幹線(E1系/E4系Max)の公開定員データに基づく参考値
    (約1.7倍)であり、単純な2倍ではない。「普通列車の二階建て」等
    現行の運用実態にない組み合わせも自由に試せる設計(現実との一致を
    強制しない)。
  - `llm_explain`: [`aruaru-llm`](https://github.com/aon-co-jp/aruaru-llm)
    連携による計算結果の自然言語説明。
  - いずれも簡易近似モデルであり、各モジュール冒頭のdocコメントに
    簡略化している点・スコープ外事項を明記している(省略・弱化しない
    方針、詳細は各ソースファイル参照)。
- [`crates/cg-cad-render`](crates/cg-cad-render): `cg-cad-core`が
  生成したメッシュを、[`open-directx`](https://github.com/aon-co-jp/open-directx)
  の既存の実Vulkanグラフィックスパイプライン(実機NVIDIA GT 730検証済み)
  経由で実際にレンダリングする橋渡し。新規のVulkan/シェーダーコードは
  書かず、既存の実装を再利用する。

## 動かし方

```sh
cargo build --workspace --release
cargo test --workspace --release
```

`cg-cad-render`のテストは実Vulkanハードウェアが必要(無い場合は
スキップし、モックでの「成功したふり」はしない)。

## `server`クレート(HTTP API+簡易プレビュー画面、2026-08-25新設)

ユーザー指示「open-englishとopen-cg-cadがハイブリッドに相互作用して
機能させて」への対応の第一歩として、`server/`に最小構成のHTTP
サーバーを新設した(既定 `http://127.0.0.1:4701/`)。

- `POST /v1/generate`: 敷地・法規制・建物種別・階数からドラフト
  建築物ボリュームを生成しJSONで返す。ブラウザ側は簡易2D平面図
  (SVG)としてプレビュー表示するほか、メッシュ座標(`mesh.positions`/
  `mesh.colors`/`mesh.indices`)を使い、ライブラリを一切使わない素の
  WebGL 1.0レンダラー(`index.html`の`initWebGl`/`renderMeshWebGl`)で
  ブラウザ上に3Dプレビュー表示する。**正直な開示**: これは
  `cg-cad-render`(`open-directx`経由の実Vulkanネイティブパイプライン)
  とは完全に独立した別経路——サーバーはメッシュ座標をJSONで返すだけで、
  実際のラスタライズはブラウザのWebGLが行う。WebGPUは本バージョンでは
  未実装(ブラウザ対応状況にまだばらつきがあるため、次回課題)。
  Direct2D/Direct3DはWindowsネイティブAPIでありWebブラウザ経由の
  配布形態とは適合しないため対応しない(ネイティブGPU描画は既存の
  `cg-cad-render`のVulkan経路が担う役割分担)。
- `POST /v1/generate/dxf`: 上記と同じ入力から、DXF(R12、AutoCAD等
  互換のオープン形式)ファイルを生成して返す。フロントエンドには
  「📐 DXF書き出し」ボタンがある。
- `POST /v1/drawings/upload`・`GET /v1/drawings`・
  `GET /v1/drawings/get`・`POST /v1/drawings/analyze`: **利用者自身の
  図面**(半導体・LSI・カスタムIC・自動車部品・建築・橋・トンネル・
  CPU/NPU/GPU・航空機・鉄道車両等、複数ファイル/フォルダ選択に対応)を
  SQLite(`data/open-cg-cad.sqlite3`)へ保存し、利用者が入力した説明文を
  `aruaru-llm`(素のGPT-2)へ渡してテキストベースの助言を得る。保存済み
  図面はダウンロードボタン(拡張子からのMIME推定、PDF含む)で取り出せる。
  **正直な開示(最重要)**: `aruaru-llm`は画像を認識できないため、
  アップロードしたファイル自体は一切解析しない——「AI解説」は説明文
  だけに基づく。他人の著作物の模倣・派生生成は一切行わない(利用者
  自身のファイルの保管庫に限定、著作権配慮のためユーザーとの確認の上で
  この範囲に決定した)。

`server`にはこのほか、[`open-kagaku`](https://github.com/aon-co-jp/open-kagaku)
(化学・市販薬教育シミュレーター、姉妹プロジェクト)への相互リンクを
`open-english`連携と同型のパターン(`localStorage`でお互いのURLを
受け渡し)で設置している。

詳細はCLAUDE.mdのHANDOFF(2026-08-25)参照。

## デプロイ状況

開発用途に留まらず、[`https://easy-web.tokyo/open-cg-cad/`](https://easy-web.tokyo/open-cg-cad/)
へ実際にデプロイ済み(VPS上のsystemdサービス、ポート8105、
`installer/open-cg-cad.service`、`open-web-server`の`domains.toml`へ
`path_prefix = "/open-cg-cad"`を追加)。外部インターネット経由での
疎通・`POST /v1/generate`の実応答を確認済み(詳細はCLAUDE.mdの
HANDOFF〈2026-08-25 続き10〉参照)。

## 依存する既存プロジェクト

- [`open-directx`](https://github.com/aon-co-jp/open-directx):
  3Dレンダリングの実行基盤(`directx-graphics-vulkan`をpath依存で再利用)。
- [`open-cuda`](https://github.com/aon-co-jp/open-cuda): `structural`/
  `crash_sim`/`tunnel_noise`の行列演算を`opencuda-blas`のGEMM実装
  (`CpuDevice`経由)へpath依存させている。実Vulkanデバイス経由への
  切替は現状の規模では効果検証の意義が薄いと判断し見送っている
  (CLAUDE.mdのHANDOFF参照)。
- [`aruaru-llm`](https://github.com/aon-co-jp/aruaru-llm): `llm_explain`
  モジュールが`POST /v1/generate`経由で計算結果の自然言語説明を取得
  する(実サーバーでの検証はCI環境では走らない`#[ignore]`テストとして
  存在、CLAUDE.mdのHANDOFF参照)。
