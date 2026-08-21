# open-cg-cad

*English*: [README-English.md](README-English.md)

「AI工務店＆AI建設」機能を提供するCADシステム。不動産検索結果から得られる
複数の土地情報に対応し、日照権・建ぺい率・容積率等の法規制チェック、
3D建築物ボリュームの自動CAD生成、着色、複数工法の提示等を行う。

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
    経路JSONの製造データ出力。
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
