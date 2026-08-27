//! open-cg-cadの最小HTTP API+簡易プレビュー画面(2026-08-25新設)。
//!
//! ユーザー指示「open-englishとopen-cg-cadがハイブリッドに相互作用して
//! 機能させて、open-englishとeasy-web.tokyoからもインストールと画面
//! 遷移可能にして」への対応の第一歩。open-cg-cadはこれまでライブラリ
//! クレート(`cg-cad-core`)のみで、HTTPサーバー・Webフロントエンドが
//! 一切存在しなかった——本クレートはその最小構成として新設した。
//!
//! **正直な開示・今回のスコープ**: (1) `cg-cad-core::generate_building`を
//! 呼び出し、敷地(矩形)・法規制(建ぺい率/容積率)・建物種別・階数から
//! ドラフト建築物ボリュームを生成しJSONで返すAPI、(2) その結果を
//! ブラウザ上で簡易的な2D平面(上から見た敷地・建築面積の矩形)として
//! SVGで表示するだけの最小プレビュー画面、の2点のみを実装した。
//! `cg-cad-core`が持つ本格的な3Dメッシュ(`Building.mesh`)・構造計算・
//! 衝突シミュレーション等はAPIレスポンスに含めていない(今回のスコープ
//! 外、次の段階で追加する)。実際の建築確認申請に使える設計図面ではなく、
//! あくまで意匠検討の叩き台であるという既存の免責事項(`README.md`
//! 「重要な免責事項」参照)は本サーバーでも変わらない。

use cg_cad_core::{generate_building, BuildingType, Site, ZoningRules};
use open_runo_poem_compat::{get, handler_fn, post, Request, Response, Route, Server, StatusCode, TcpListener};
use std::net::SocketAddr;
use std::sync::Arc;

mod drawings_db;
use drawings_db::DrawingsDb;
mod github_agent;
mod vps_agent;

fn rs_json_response(status: StatusCode, value: &impl serde::Serialize) -> Response {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    hyper::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(open_runo_poem_compat::hyper_compat::fixed_body(bytes::Bytes::from(body)))
        .expect("building a response from a fixed set of valid headers cannot fail")
}

fn html_response(body: &str) -> Response {
    hyper::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html; charset=utf-8")
        .body(open_runo_poem_compat::hyper_compat::fixed_body(bytes::Bytes::from(body.as_bytes().to_vec())))
        .expect("building a response from a fixed set of valid headers cannot fail")
}

async fn read_json_body<T: serde::de::DeserializeOwned>(req: Request) -> Result<T, Response> {
    use http_body_util::BodyExt;
    let bytes = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return Err(rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": "failed to read request body"}))),
    };
    serde_json::from_slice::<T>(&bytes).map_err(|e| rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": format!("invalid JSON body: {e}")})))
}

async fn healthz() -> Response {
    rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true}))
}

#[derive(serde::Deserialize)]
struct GenerateRequest {
    width_m: f64,
    depth_m: f64,
    coverage_ratio: f64,
    floor_area_ratio: f64,
    building_type: String,
    floors: u32,
}

fn parse_building_type(s: &str) -> Option<BuildingType> {
    match s {
        "house" => Some(BuildingType::House),
        "office" => Some(BuildingType::OfficeBuilding),
        "theater" => Some(BuildingType::Theater),
        "audio_room" => Some(BuildingType::AudioRoom),
        "cinema_complex" => Some(BuildingType::CinemaComplex),
        "home_cinema_4dx_imax" => Some(BuildingType::HomeCinema4dxImax),
        _ => None,
    }
}

/// `POST /v1/generate`: 敷地・法規制・建物種別・階数からドラフト建築物
/// ボリュームを生成する。**正直な開示**: 返すのは`footprint_m2`(建築
/// 面積)・`total_floor_area_m2`(延床面積)・敷地/建築面積の矩形寸法
/// (簡易2Dプレビュー描画用)のみ——3Dメッシュそのものは含めていない
/// (今回のスコープ外)。
/// `generate`/`generate_dxf`共通の検証+生成ロジック(重複を避けるため抽出)。
/// メッシュをブラウザのWebGLで直接描画できるようフラットな配列(頂点位置
/// ×3、頂点色RGBA×4、インデックス)へ変換する(優先順位4番目
/// 「WebGL/WebGPU対応」への第一歩、2026-08-25新設)。**正直な開示**:
/// (1) `cg-cad-render`(Vulkanネイティブパイプライン、`open-directx`
/// 経由)とは別の独立した経路——サーバー側でメッシュ生成のみ行い、
/// 実際のラスタライズはブラウザのWebGL(クライアント側)が行う設計。
/// (2) WebGPUは本バージョンでは未実装(ブラウザのWebGPU対応状況に
/// まだばらつきがあるため、まずより広くサポートされているWebGLを優先
/// した、次回以降の課題)。(3) Direct2D/Direct3DはWindowsネイティブAPIで
/// あり、Webブラウザ経由のこのサーバーの配布形態とは適合しない
/// (ネイティブGPU描画は既存の`cg-cad-render`のVulkan経路が担う)。
fn mesh_to_json(mesh: &cg_cad_core::mesh::Mesh) -> serde_json::Value {
    let mut positions = Vec::with_capacity(mesh.vertices.len() * 3);
    let mut colors = Vec::with_capacity(mesh.vertices.len() * 4);
    for v in &mesh.vertices {
        positions.extend_from_slice(&v.position);
        colors.extend_from_slice(&v.color);
    }
    serde_json::json!({ "positions": positions, "colors": colors, "indices": mesh.indices })
}

fn build_from_generate_request(body: &GenerateRequest) -> Result<cg_cad_core::building::Building, Box<Response>> {
    let site = Site::rectangle(body.width_m, body.depth_m)
        .map_err(|e| Box::new(rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": format!("invalid site: {e}")}))))?;
    let zoning = ZoningRules::new(body.coverage_ratio, body.floor_area_ratio)
        .map_err(|e| Box::new(rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": format!("invalid zoning rules: {e}")}))))?;
    let building_type = parse_building_type(&body.building_type).ok_or_else(|| {
        Box::new(rs_json_response(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({"error": "building_type must be one of: house, office, theater, audio_room, cinema_complex, home_cinema_4dx_imax"}),
        ))
    })?;
    generate_building(&site, &zoning, building_type, body.floors)
        .map_err(|e| Box::new(rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": format!("could not generate building: {e}")}))))
}

async fn generate(req: Request) -> Response {
    let body: GenerateRequest = match read_json_body(req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let building = match build_from_generate_request(&body) {
        Ok(b) => b,
        Err(resp) => return *resp,
    };

    // 簡易2Dプレビュー用: 建築面積(footprint_m2)を敷地と同じアスペクト比の
    // 矩形とみなして、敷地の中心に配置した寸法を返す(実際のfootprintの
    // 形状は敷地形状に応じて変わり得るが、矩形敷地のMVPではこの近似で
    // 十分——将来任意多角形敷地に対応する際は要見直し)。
    let site_area_m2 = body.width_m * body.depth_m;
    let scale = if site_area_m2 > 0.0 { (building.footprint_m2 / site_area_m2).sqrt() } else { 0.0 };
    let footprint_width_m = body.width_m * scale;
    let footprint_depth_m = body.depth_m * scale;

    rs_json_response(
        StatusCode::OK,
        &serde_json::json!({
            "ok": true,
            "footprint_m2": building.footprint_m2,
            "total_floor_area_m2": building.total_floor_area_m2,
            "floors": building.floors,
            "building_type": body.building_type,
            "site_width_m": body.width_m,
            "site_depth_m": body.depth_m,
            "footprint_width_m": footprint_width_m,
            "footprint_depth_m": footprint_depth_m,
            "mesh": mesh_to_json(&building.mesh),
            "disclosure_en": "This is a draft massing/volume estimate only (rectangular footprint approximation), not a permit-ready design. A licensed architect must review any real project.",
            "disclosure_ja": "これは意匠検討の叩き台(矩形footprintによる近似)であり、建築確認申請に使える設計図面ではありません。実際のプロジェクトには有資格者(一級建築士等)のレビューが必須です。",
        }),
    )
}

/// 生成した建築物ボリュームをDXF(R12、AutoCAD等の主要CADアプリが読める
/// 公開フォーマット)としてエクスポートする(ユーザー指示「CADは、
/// AUTO-CADなどのアプリなど世界中の有名なCADアプリのDATAと互換性を
/// 保って」への対応、2026-08-25新設)。**正直な開示**: DWG(AutoCADの
/// 非公開バイナリ形式)には対応しない——`manufacturing_export::
/// mesh_to_dxf_r12`のdocコメント参照。
async fn generate_dxf(req: Request) -> Response {
    let body: GenerateRequest = match read_json_body(req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let building = match build_from_generate_request(&body) {
        Ok(b) => b,
        Err(resp) => return *resp,
    };
    match cg_cad_core::manufacturing_export::mesh_to_dxf_r12(&building.mesh) {
        Ok(dxf) => rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true, "dxf": dxf})),
        Err(e) => rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("dxf export failed: {e}")})),
    }
}

// ----------------------------------------------------------------------------
// 図面のアップロード・一覧・取得・AI解説(2026-08-25新設)。
// **正直な開示**: `aruaru-llm`は画像を認識できないテキスト生成モデル
// (素のGPT-2)であり、アップロードされた図面ファイル自体は解析しない
// ——「AI解説」は利用者が併せて入力した説明文に基づく助言に過ぎない。
// 他人の著作物の図面を模倣・派生生成する機能は実装していない(利用者
// 自身のファイルの保管庫+テキストベースの助言に限定)。
// ----------------------------------------------------------------------------

/// アップロード1件あたりの上限(10MiB)。無制限に受け付けてメモリ/DB
/// 容量を圧迫しないための単純な上限(world-labの`ComputeLimits`と同じ
/// 「まず単純な上限を設ける」という考え方)。
const MAX_DRAWING_BYTES: usize = 10 * 1024 * 1024;

#[derive(serde::Deserialize)]
struct UploadDrawingRequest {
    filename: String,
    category: String,
    description: String,
    data_base64: String,
}

async fn upload_drawing(req: Request, db: Arc<DrawingsDb>) -> Response {
    let body: UploadDrawingRequest = match read_json_body(req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    use base64::Engine;
    let data = match base64::engine::general_purpose::STANDARD.decode(&body.data_base64) {
        Ok(d) => d,
        Err(e) => return rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": format!("invalid base64 data: {e}")})),
    };
    if data.len() > MAX_DRAWING_BYTES {
        return rs_json_response(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({"error": format!("file too large ({} bytes, limit {} bytes)", data.len(), MAX_DRAWING_BYTES)}),
        );
    }
    if body.filename.trim().is_empty() {
        return rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": "filename is required"}));
    }
    match db.insert(&body.filename, &body.category, &body.description, &data) {
        Ok(id) => rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true, "id": id})),
        Err(e) => rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("db insert failed: {e}")})),
    }
}

async fn list_drawings(db: Arc<DrawingsDb>) -> Response {
    match db.list() {
        Ok(list) => rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true, "drawings": list})),
        Err(e) => rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("db list failed: {e}")})),
    }
}

async fn get_drawing(req: Request, db: Arc<DrawingsDb>) -> Response {
    let id: i64 = match query_param(&req, "id").and_then(|s| s.parse().ok()) {
        Some(v) => v,
        None => return rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": "query parameter 'id' (integer) is required"})),
    };
    match db.get(id) {
        Ok(Some(d)) => rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true, "drawing": d})),
        Ok(None) => rs_json_response(StatusCode::NOT_FOUND, &serde_json::json!({"ok": false, "error": "not found"})),
        Err(e) => rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("db get failed: {e}")})),
    }
}

fn query_param(req: &Request, key: &str) -> Option<String> {
    let query = req.uri().query()?;
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next()?;
        let v = it.next().unwrap_or("");
        if k == key {
            return Some(urlencoding_decode_simple(v));
        }
    }
    None
}

/// 依存クレート追加を避けるための最小限のURLデコード(`%XX`と`+`のみ対応)。
fn urlencoding_decode_simple(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(byte);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(serde::Deserialize)]
struct AnalyzeDrawingRequest {
    id: i64,
    aruaru_llm_base: Option<String>,
}

/// **正直な開示**: `aruaru-llm`(素のGPT-2、`/v1/generate`)は画像を
/// 認識できないため、ここで送るのはDBに保存済みの`description`
/// (利用者自身が入力した図面の説明文)のみ——アップロードされた
/// ファイル自体の画素・図形情報は一切解析しない。この開示はAPI
/// レスポンス自体にも含める(黙って解析したふりをしない)。
async fn analyze_drawing(req: Request, db: Arc<DrawingsDb>) -> Response {
    let body: AnalyzeDrawingRequest = match read_json_body(req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let drawing = match db.get(body.id) {
        Ok(Some(d)) => d,
        Ok(None) => return rs_json_response(StatusCode::NOT_FOUND, &serde_json::json!({"ok": false, "error": "drawing not found"})),
        Err(e) => return rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("db get failed: {e}")})),
    };
    if drawing.description.trim().is_empty() {
        return rs_json_response(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({"ok": false, "error": "this drawing has no text description to analyze (the AI cannot see the file itself) / この図面には解析対象の説明文がありません(AIはファイル自体を見ることはできません)"}),
        );
    }
    let base = body.aruaru_llm_base.unwrap_or_else(|| "http://localhost:4600".to_string());
    let prompt = format!(
        "You are a mechanical/electronics design reviewer. A user uploaded a drawing named \"{}\" (category: {}) with this description written by the user: \"{}\". \
Based only on this text description (you cannot see the actual drawing image), point out possible issues and suggest improvements. Trainer:",
        drawing.filename, drawing.category, drawing.description
    );
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build() {
        Ok(c) => c,
        Err(e) => return rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("client build failed: {e}")})),
    };
    let resp = match client
        .post(format!("{}/v1/generate", base.trim_end_matches('/')))
        .json(&serde_json::json!({"prompt": prompt, "max_new_tokens": 96}))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return rs_json_response(
                StatusCode::BAD_GATEWAY,
                &serde_json::json!({"ok": false, "error": format!("could not reach aruaru-llm at {base}: {e}")}),
            )
        }
    };
    if !resp.status().is_success() {
        return rs_json_response(StatusCode::BAD_GATEWAY, &serde_json::json!({"ok": false, "error": format!("aruaru-llm returned HTTP {}", resp.status())}));
    }
    let data: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return rs_json_response(StatusCode::BAD_GATEWAY, &serde_json::json!({"ok": false, "error": format!("invalid response from aruaru-llm: {e}")})),
    };
    let completion = data.get("completion").and_then(|v| v.as_str()).unwrap_or("(no completion field in aruaru-llm response)");
    let disclosure_en = "Honest disclosure: this analysis is generated by a base GPT-2 model from your text description only — it does not visually inspect the uploaded file, and quality/accuracy is not guaranteed.";
    let disclosure_ja = "正直な開示: この解説は素のGPT-2モデルが、あなたが入力した説明文のみから生成したものです——アップロードされたファイル自体を視覚的に検査してはおらず、品質・正確性は保証されません。";
    let analysis = format!("{completion}\n\n{disclosure_en}\n{disclosure_ja}");
    if let Err(e) = db.save_analysis(body.id, &analysis) {
        return rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("failed to save analysis: {e}")}));
    }
    rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true, "analysis": analysis}))
}

// ----------------------------------------------------------------------------
// 旧図面の再設計(2026-08-27新設)。ユーザー指示「半導体やCPUやNPUやGPU、
// 自動車やバイクや新幹線やリニアモーターカーや航空機などの…古い図面に
// 対して言葉や命令や、再度UPLOADする指示書や設計図などと照らし合わせて、
// 図面を再設計出来る機能」への対応。
//
// **正直な開示(最重要、analyze_drawingと同じ制約)**: `aruaru-llm`は
// 画像を認識できないテキスト生成モデル(素のGPT-2)である。ここで行える
// 「再設計」は、(1)旧図面のアップロード時に利用者自身が入力した説明文
// (テキスト)と、(2)利用者が今回入力する指示文、または再アップロードした
// 指示書/仕様書図面の説明文(テキスト)の**2つのテキストを比較**して、
// GPT-2に改善案の文章を生成させるものに限られる——半導体のダイ設計・
// 自動車/航空機の実際のCAD形状そのものを機械的に変形・再計算する機能
// ではない。物理シミュレーション(既存の`crash_sim`/`structural`/
// `aero`等)と組み合わせた自動再設計は今回のスコープ外(HANDOFF参照)。
// ----------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct RedesignDrawingRequest {
    old_drawing_id: i64,
    /// 利用者が直接入力する再設計指示(自由記述)。`spec_drawing_id`と
    /// 併用可能(両方あれば両方の内容をプロンプトへ含める)。
    instruction_text: Option<String>,
    /// 再度アップロードした指示書/仕様書/新設計図(`drawings`テーブルの
    /// 別レコードID)。これも説明文(テキスト)のみが参照される。
    spec_drawing_id: Option<i64>,
    aruaru_llm_base: Option<String>,
}

async fn redesign_drawing(req: Request, db: Arc<DrawingsDb>) -> Response {
    let body: RedesignDrawingRequest = match read_json_body(req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let old = match db.get(body.old_drawing_id) {
        Ok(Some(d)) => d,
        Ok(None) => return rs_json_response(StatusCode::NOT_FOUND, &serde_json::json!({"ok": false, "error": "old_drawing_id not found"})),
        Err(e) => return rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("db get failed: {e}")})),
    };

    let mut spec_text = body.instruction_text.clone().unwrap_or_default();
    let mut spec_source_note = String::new();
    if let Some(spec_id) = body.spec_drawing_id {
        match db.get(spec_id) {
            Ok(Some(spec)) => {
                if !spec_text.trim().is_empty() {
                    spec_text.push_str("\n\n");
                }
                spec_text.push_str(&spec.description);
                spec_source_note = format!(" + spec drawing #{} (\"{}\")", spec.id, spec.filename);
            }
            Ok(None) => return rs_json_response(StatusCode::NOT_FOUND, &serde_json::json!({"ok": false, "error": "spec_drawing_id not found"})),
            Err(e) => return rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("db get failed: {e}")})),
        }
    }

    if spec_text.trim().is_empty() {
        return rs_json_response(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({"ok": false, "error": "instruction_text and/or spec_drawing_id (with a non-empty description) is required / instruction_textまたはspec_drawing_id(説明文入り)のいずれかが必要です"}),
        );
    }
    if old.description.trim().is_empty() {
        return rs_json_response(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({"ok": false, "error": "the old drawing has no text description to compare against (the AI cannot see the file itself) / 旧図面に比較対象の説明文がありません(AIはファイル自体を見ることはできません)"}),
        );
    }

    let base = body.aruaru_llm_base.unwrap_or_else(|| "http://localhost:4600".to_string());
    let prompt = format!(
        "You are a design reviewer for semiconductors (CPU/NPU/GPU), automobiles, motorcycles, high-speed trains, maglev trains, and aircraft (including their engines: gasoline, diesel, EV, PHEV). \
An old design named \"{}\" (category: {}) has this description written by its designer: \"{}\". \
A new instruction or specification document says: \"{}\". \
Based only on these two text descriptions (you cannot see the actual drawing images), propose what should change in a redesign, and list concrete differences. Trainer:",
        old.filename, old.category, old.description, spec_text
    );
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)).build() {
        Ok(c) => c,
        Err(e) => return rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("client build failed: {e}")})),
    };
    let resp = match client
        .post(format!("{}/v1/generate", base.trim_end_matches('/')))
        .json(&serde_json::json!({"prompt": prompt, "max_new_tokens": 128}))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return rs_json_response(
                StatusCode::BAD_GATEWAY,
                &serde_json::json!({"ok": false, "error": format!("could not reach aruaru-llm at {base}: {e}")}),
            )
        }
    };
    if !resp.status().is_success() {
        return rs_json_response(StatusCode::BAD_GATEWAY, &serde_json::json!({"ok": false, "error": format!("aruaru-llm returned HTTP {}", resp.status())}));
    }
    let data: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return rs_json_response(StatusCode::BAD_GATEWAY, &serde_json::json!({"ok": false, "error": format!("invalid response from aruaru-llm: {e}")})),
    };
    let completion = data.get("completion").and_then(|v| v.as_str()).unwrap_or("(no completion field in aruaru-llm response)");
    let disclosure_en = "Honest disclosure: this redesign proposal is generated by a base GPT-2 model from text descriptions only — it does not visually inspect either uploaded file, does not perform any physical/electrical/aerodynamic simulation, and does not modify the actual CAD geometry. It is narrative guidance only, and quality/accuracy is not guaranteed.";
    let disclosure_ja = "正直な開示: この再設計提案は素のGPT-2モデルが、テキストの説明文のみから生成したものです——アップロードされたどちらのファイルも視覚的に検査しておらず、物理/電気/空力シミュレーションも一切行っておらず、実際のCAD形状データを変形するものでもありません。あくまで文章による助言に過ぎず、品質・正確性は保証されません。";
    let proposal = format!("{completion}\n\n{disclosure_en}\n{disclosure_ja}");

    let redesign_filename = format!("{} (redesign proposal{})", old.filename, spec_source_note);
    let id = match db.insert_redesign(&redesign_filename, &old.category, &old.description, old.id, &spec_text, &proposal) {
        Ok(id) => id,
        Err(e) => return rs_json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({"ok": false, "error": format!("failed to save redesign: {e}")})),
    };

    rs_json_response(
        StatusCode::OK,
        &serde_json::json!({
            "ok": true,
            "id": id,
            "old_drawing_id": old.id,
            "proposal": proposal,
        }),
    )
}

// ----------------------------------------------------------------------------
// GitHub/VPSレンタルサーバーへの図面データ読み書き(2026-08-27新設)。
// セキュリティ設計の詳細は`github_agent.rs`/`vps_agent.rs`のモジュールdoc
// 参照(open-english/server側の実機検証済み設計をそのまま移植)。
// ----------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct GithubReadRequest {
    owner: String,
    repo: String,
    path: String,
    branch: Option<String>,
    token: Option<String>,
}

async fn agent_github_read(req: Request) -> Response {
    let body: GithubReadRequest = match read_json_body(req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match github_agent::read_file(&body.owner, &body.repo, &body.path, body.branch.as_deref(), body.token.as_deref()).await {
        Ok(file) => rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true, "content": file.content, "sha": file.sha})),
        Err(e) => rs_json_response(StatusCode::BAD_GATEWAY, &serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

#[derive(serde::Deserialize)]
struct GithubCommitRequest {
    owner: String,
    repo: String,
    path: String,
    content: String,
    message: String,
    branch: Option<String>,
    sha: Option<String>,
    token: String,
}

async fn agent_github_commit(req: Request) -> Response {
    let body: GithubCommitRequest = match read_json_body(req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match github_agent::commit_file(&body.owner, &body.repo, &body.path, &body.content, &body.message, body.branch.as_deref(), body.sha.as_deref(), &body.token).await {
        Ok(html_url) => rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true, "html_url": html_url})),
        Err(e) => rs_json_response(StatusCode::BAD_GATEWAY, &serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

async fn agent_vps_read(req: Request) -> Response {
    let path = match query_param(&req, "path") {
        Some(p) => p,
        None => return rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"error": "query parameter 'path' is required"})),
    };
    match vps_agent::read_file(&path).await {
        Ok(content) => rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true, "content": content})),
        Err(e) => rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

#[derive(serde::Deserialize)]
struct VpsWriteRequest {
    path: String,
    content: String,
}

async fn agent_vps_write(req: Request) -> Response {
    let body: VpsWriteRequest = match read_json_body(req).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    match vps_agent::write_file(&body.path, &body.content).await {
        Ok(()) => rs_json_response(StatusCode::OK, &serde_json::json!({"ok": true})),
        Err(e) => rs_json_response(StatusCode::BAD_REQUEST, &serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

const INDEX_HTML: &str = include_str!("index.html");

async fn index() -> Response {
    html_response(INDEX_HTML)
}

fn bind_addr() -> SocketAddr {
    std::env::var("OPEN_CG_CAD_SERVER_BIND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| "127.0.0.1:4701".parse().unwrap())
}

#[tokio::main]
async fn main() {
    let addr = bind_addr();
    let root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let db = Arc::new(DrawingsDb::open(drawings_db::db_path(&root)).expect("failed to open local SQLite DB (data/open-cg-cad.sqlite3)"));

    let db_upload = Arc::clone(&db);
    let db_list = Arc::clone(&db);
    let db_get = Arc::clone(&db);
    let db_analyze = Arc::clone(&db);
    let db_redesign = Arc::clone(&db);
    let app = Route::new()
        .at("/", get(handler_fn(move |_req, _p| async move { index().await })))
        .at("/healthz", get(handler_fn(move |_req, _p| async move { healthz().await })))
        .at("/v1/generate", post(handler_fn(move |req, _p| async move { generate(req).await })))
        .at("/v1/generate/dxf", post(handler_fn(move |req, _p| async move { generate_dxf(req).await })))
        .at("/v1/drawings/upload", post(handler_fn(move |req, _p| { let db = Arc::clone(&db_upload); async move { upload_drawing(req, db).await } })))
        .at("/v1/drawings", get(handler_fn(move |_req, _p| { let db = Arc::clone(&db_list); async move { list_drawings(db).await } })))
        .at("/v1/drawings/get", get(handler_fn(move |req, _p| { let db = Arc::clone(&db_get); async move { get_drawing(req, db).await } })))
        .at("/v1/drawings/analyze", post(handler_fn(move |req, _p| { let db = Arc::clone(&db_analyze); async move { analyze_drawing(req, db).await } })))
        .at("/v1/drawings/redesign", post(handler_fn(move |req, _p| { let db = Arc::clone(&db_redesign); async move { redesign_drawing(req, db).await } })))
        .at("/v1/agent/github/read", post(handler_fn(move |req, _p| async move { agent_github_read(req).await })))
        .at("/v1/agent/github/commit", post(handler_fn(move |req, _p| async move { agent_github_commit(req).await })))
        .at("/v1/agent/vps/read", get(handler_fn(move |req, _p| async move { agent_vps_read(req).await })))
        .at("/v1/agent/vps/write", post(handler_fn(move |req, _p| async move { agent_vps_write(req).await })));

    println!("conversation-free drawings DB: {}", drawings_db::db_path(&root).display());
    println!("open-cg-cad server listening on http://{addr}/");
    let (bound_addr, handle) = Server::new(TcpListener::bind(addr)).run(app).await.expect("failed to bind local server (is the port already in use?)");
    println!("bound to http://{bound_addr}/");
    handle.await.expect("server task panicked");
}
