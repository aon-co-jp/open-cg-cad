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

- 構造計算・耐震シミュレーションは未実装(専門ソフトウェアとの連携が
  必要、`cg-cad-core::structural`参照)。
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
- [`open-cuda`](https://github.com/aon-co-jp/open-cuda) /
  [`aruaru-llm`](https://github.com/aon-co-jp/aruaru-llm): 将来の
  GPU計算基盤・自然言語による仕様変更インターフェースとしての活用を
  見据えるが、今回のセッションでは直接の技術的依存関係は追加していない。
