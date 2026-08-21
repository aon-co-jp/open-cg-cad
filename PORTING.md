# PORTING.md(open-cg-cad)

現時点(2026-08-21着手)ではRustワークスペースとして単一プラットフォーム
(Windows、実機NVIDIA GT 730)でのみ実機検証済み。

- `cg-cad-core`はプラットフォーム非依存の純粋なRustロジック
  (Vulkan/グラフィックス依存無し)——他OSへの移植障壁は無いはず
  (未検証)。
- `cg-cad-render`は`open-directx`の`directx-graphics-vulkan`をそのまま
  再利用しているため、移植性は`open-directx`自体の対応状況に従う
  (`open-directx/README.md`の「Platform & vendor support matrix」参照、
  Linux/Android/macOS/iOSはコード上はブロック要因なしだが実機未検証)。

`aruaru-llm`連携(`llm_explain`モジュール)は実装済み(`cg-cad-core`が
`reqwest`でHTTP経由呼び出すのみで、プラットフォーム依存の追加は無い)。
`cg-cad-core::solver_bridge`が呼び出すCalculiX(`ccx`)/OpenFOAM
(`blockMesh`/`simpleFoam`)は、この開発機にはインストールされておらず
未検証(Windowsネイティブ・WSL Ubuntuいずれも不在を確認済み、
CLAUDE.mdのHANDOFF参照)——各ソルバー自体のプラットフォーム対応状況に
移植性が従う。

将来、Web検索による法規制自動取得を追加する場合は、別途このファイルに
移植・依存関係の情報を追記する。
