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

将来、Web検索による法規制自動取得や自然言語仕様変更(`aruaru-llm`連携)を
追加する場合は、それぞれ別途このファイルに移植・依存関係の情報を追記する。
