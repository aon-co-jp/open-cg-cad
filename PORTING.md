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

## `server`クレート(2026-08-25新設)の移植ポイント

- `open_runo_poem_compat`(RPoem)ベースのため、プラットフォーム依存は
  RPoem自体の対応状況に従う(`cg-cad-render`と違いVulkan/グラフィックス
  依存は無いため、移植障壁自体はより少ないはず、未検証)。
- SQLite永続化(`rusqlite`、`bundled` feature)を使用——利用者環境に
  システムSQLiteが入っているかに依存しない設計で、この点は
  `open-english/server`の`db.rs`と同じ方針。
- `aruaru-llm`連携は`reqwest`でHTTP経由(既存の`cg-cad-core::
  llm_explain`と同じパターン)。接続先は`http://localhost:4600`固定
  (リクエストボディの`aruaru_llm_base`で上書き可能)。
- 図面ファイル(BLOB)はSQLiteへそのまま保存する設計のため、大量の
  大きな図面ファイルを扱う運用には向かない(1件10MiB上限、
  `MAX_DRAWING_BYTES`)——将来、外部ストレージ(S3互換等)への移行を
  検討する場合は`drawings_db.rs`の`insert`/`get`のみを差し替えれば
  よい設計にしてある。
- easy-web.tokyo(VPS)へのデプロイ手順は、`open-english`で実施した
  手順(`git clone`+`cargo build --release`+systemdサービス化+
  `open-web-server`の`domains.toml`への`path_prefix`追加、詳細は
  `open-english/CLAUDE.md`のHANDOFF〈2026-08-25(続き6)〉参照)と
  同型になる見込み——**今回は未実施**(次回の課題)。
