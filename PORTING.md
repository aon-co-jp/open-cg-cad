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

## 半導体/自動車/GitHub/VPSエージェント機能(2026-08-27新設)の移植ポイント

- `server/src/github_agent.rs`: `reqwest`のみ(rustls-tls経由)、
  プラットフォーム依存無し(open-english側からの移植元も同様)。
- `server/src/vps_agent.rs`: `russh`(0.63、pure Rust SSH実装、
  `ssh2`/libssh2のようなCバインディング・vcpkg等の追加ネイティブ依存
  なし)を使用——移植障壁はRPoem/reqwest同様に少ないはず(未検証、
  open-english側の同一実装も未検証のまま)。VPS側(接続先サーバー)は
  POSIXシェル(`cat`/`cat >`が動く環境)を前提としており、Windows VPS
  への接続は想定していない。
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
  同型——**2026-08-25に実施済み**(`https://easy-web.tokyo/open-cg-cad/`、
  ポート8105、`installer/open-cg-cad.service`、`domains.toml`へ
  `path_prefix = "/open-cg-cad"`のみ追加、`/v1`の追加専有は不要——
  API呼び出しを絶対パスから相対パス化していたため。sibling依存
  〈`open-cuda`はVPS上の共有クローンへシンボリックリンク、
  `open-directx`はVPS上に新規clone〉含め、詳細はCLAUDE.mdの
  HANDOFF〈2026-08-25 続き10〉参照)。SQLite図面DBはVPS上の実行
  ディレクトリに別途新規作成される(ローカル開発機のDBとは別)。

## 新規モジュールの移植性(2026-08-25追加分)

- `acoustic_material`/`vehicle_config`/`rail_vehicle`はいずれも
  `cg-cad-core`内の純粋なRustロジックで、Vulkan/グラフィックス・
  外部プロセス依存は無いため`cg-cad-core`本体と同様プラットフォーム
  非依存のはず(未検証)。
- `manufacturing_export::mesh_to_dxf_r12`(DXF R12出力)も文字列生成
  のみでプラットフォーム依存無し。
- `server`のWebGLプレビュー(`index.html`)はブラウザ側のWebGL 1.0
  実装に依存する(サーバー側はメッシュ座標のJSON化のみ)——主要な
  デスクトップ/モバイルブラウザは広くWebGL 1.0に対応済みのため、
  `cg-cad-render`のVulkan経路(`open-directx`依存)より移植性は高い
  はずだが、実機での複数ブラウザ・複数OS横断検証は未実施
  (Claude Browserツール〈Chromium系〉でのみ確認済み)。

## API 方針: REST 全廃・gRPC / Connect へ (2026-11-04)

### 日本語
- **REST は全廃する。新しい API は REST で作らない。** ブラウザや `curl` から呼ぶ API は **Connect の単純呼び出し**(`POST /<パッケージ>.<Service>/<Method>`、`content-type: application/json`、本文は JSON)、サービス間は **gRPC / gRPC-web**(RPoem の管理API `open_runo.admin.v1.Admin` と同じ流儀)。ストリーミングは、必要になってから。
- **既存の REST は、触るときに順に移す。** 移したら旧 REST の経路は消す。移行が終わるまで残るが、**新規には追加しない**。
- **JSON は RS-JSON に統一**する(読み込みは `from_str_secure` / `parse_secure` でサイズ・深さ・重複キーの上限つき、書き出しは `rust_json::fast`)。ブラウザの WASM に `prost` は入れない(サイズが増えるため)。
- **例外(自分たちの API ではなく、相手の規格が決めているもの)**: ACME、OAuth/OIDC のコールバック、第三者(GitHub など)からの Webhook、ヘルスチェック、静的ファイル配信、OTLP など。これらは規格どおりの HTTP を受ける。
- 参考実装: `RPoem`(`open_runo.admin.v1.Admin`)、`open-easy-web`(`server/src/connect.rs`、`proto/open_easy_web/admin/v1/admin.proto`)。
- ※ このリポジトリのコードが既に方針どおりかどうかは、各リポジトリの現状による。この節は方針の記録であり、コードの変更を意味しない。

### English
- **REST is being abolished. New APIs are not built as REST.** APIs called from browsers or `curl` use **Connect unary calls** (`POST /<package>.<Service>/<Method>`, `content-type: application/json`, JSON body); service-to-service calls use **gRPC / gRPC-web** (the same style as RPoem's admin API `open_runo.admin.v1.Admin`). Streaming only when it is actually needed.
- **Existing REST endpoints are migrated one by one, when they are touched.** The old REST route is removed once migrated. Until migration is finished they remain, but **no new ones are added**.
- **JSON is unified on RS-JSON**: reading with `from_str_secure` / `parse_secure` (size, depth and duplicate-key limits), writing with `rust_json::fast`. `prost` is not put into browser WASM (it would grow the binary).
- **Exceptions (shapes dictated by someone else's standard, not our own API)**: ACME, OAuth/OIDC callbacks, webhooks from third parties (e.g. GitHub), health checks, static file serving, OTLP, and so on. These accept plain HTTP as the standard requires.
- Reference implementations: `RPoem` (`open_runo.admin.v1.Admin`) and `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Note: whether the code in a given repository already follows this policy depends on that repository. This section records the policy; it does not mean the code has been changed.

### Deutsch (KI-Übersetzung, noch nicht von Muttersprachlern geprüft)
- **REST wird abgeschafft. Neue APIs werden nicht als REST gebaut.** APIs, die aus dem Browser oder mit `curl` aufgerufen werden, nutzen **Connect-Unary-Aufrufe** (`POST /<Paket>.<Service>/<Methode>`, `content-type: application/json`, JSON-Body); zwischen Diensten gilt **gRPC / gRPC-web** (wie bei der Admin-API von RPoem, `open_runo.admin.v1.Admin`). Streaming erst, wenn es wirklich nötig ist.
- **Bestehende REST-Endpunkte werden einzeln migriert, sobald man sie anfasst.** Nach der Migration wird die alte REST-Route entfernt. Bis dahin bleiben sie bestehen, es werden aber **keine neuen hinzugefügt**.
- **JSON wird auf RS-JSON vereinheitlicht**: Lesen mit `from_str_secure` / `parse_secure` (Grenzen für Größe, Tiefe und doppelte Schlüssel), Schreiben mit `rust_json::fast`. `prost` kommt nicht in Browser-WASM (die Datei würde größer).
- **Ausnahmen (Formate, die ein fremder Standard vorgibt, nicht unsere eigene API)**: ACME, OAuth/OIDC-Callbacks, Webhooks von Dritten (z. B. GitHub), Health-Checks, statische Dateien, OTLP usw. Diese nehmen schlichtes HTTP entgegen, wie es der Standard verlangt.
- Referenzimplementierungen: `RPoem` (`open_runo.admin.v1.Admin`) und `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Hinweis: Ob der Code eines Repositorys diese Richtlinie schon befolgt, hängt vom jeweiligen Repository ab. Dieser Abschnitt hält die Richtlinie fest; er bedeutet nicht, dass der Code bereits geändert wurde.

### Русский (машинный перевод, носителями языка ещё не проверен)
- **REST полностью отменяется. Новые API не создаются как REST.** API, вызываемые из браузера или через `curl`, используют **унарные вызовы Connect** (`POST /<пакет>.<Service>/<Method>`, `content-type: application/json`, тело — JSON); между сервисами — **gRPC / gRPC-web** (так же, как административный API RPoem, `open_runo.admin.v1.Admin`). Потоковая передача — только когда она действительно нужна.
- **Существующие REST-эндпоинты переносятся по одному, когда до них доходят руки.** После переноса старый REST-маршрут удаляется. До завершения переноса они остаются, но **новые не добавляются**.
- **JSON унифицируется на RS-JSON**: чтение через `from_str_secure` / `parse_secure` (ограничения на размер, глубину и повторяющиеся ключи), запись через `rust_json::fast`. `prost` не добавляется в браузерный WASM (размер вырос бы).
- **Исключения (форматы, заданные чужим стандартом, а не наш собственный API)**: ACME, обратные вызовы OAuth/OIDC, вебхуки сторонних сервисов (например, GitHub), проверки работоспособности, раздача статических файлов, OTLP и т. п. Они принимают обычный HTTP, как требует стандарт.
- Эталонные реализации: `RPoem` (`open_runo.admin.v1.Admin`) и `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Примечание: соответствует ли код конкретного репозитория этой политике, зависит от самого репозитория. Этот раздел фиксирует политику и не означает, что код уже изменён.
