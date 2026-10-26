//! 型付きのAPI宣言(RPoemのopen-runo-openapi)。`/api/docs`・`/api/openapi.json`・
//! Cosmo互換SDL・Tauriマニフェスト・TSクライアントの元になる。
//!
//! リクエスト型は実際のハンドラが読む構造体そのもの(`super::*Request`)。
//! レスポンスはハンドラが`json!`で組み立てる形をここで型として写したもの
//! (ハンドラの返却形を変えたらここも直す)。

use super::{
    AnalyzeDrawingRequest, GenerateRequest, GithubCommitRequest, GithubReadRequest, MergeDrawingsRequest,
    RedesignDrawingRequest, UploadDrawingRequest, VpsWriteRequest,
};
use crate::drawings_db::{DrawingFull, DrawingSummary};
use open_runo_poem_compat::openapi::{Operation as Op, Spec};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Serialize, JsonSchema)]
struct ErrorBody {
    ok: Option<bool>,
    error: String,
}

#[derive(Serialize, JsonSchema)]
struct OkBody {
    ok: bool,
}

#[derive(Serialize, JsonSchema)]
struct Mesh {
    positions: Vec<f32>,
    colors: Vec<f32>,
    indices: Vec<u32>,
}

#[derive(Serialize, JsonSchema)]
struct GenerateResponse {
    ok: bool,
    footprint_m2: f64,
    total_floor_area_m2: f64,
    floors: u32,
    building_type: String,
    site_width_m: f64,
    site_depth_m: f64,
    footprint_width_m: f64,
    footprint_depth_m: f64,
    mesh: Mesh,
    disclosure_en: String,
    disclosure_ja: String,
}

#[derive(Serialize, JsonSchema)]
struct DxfResponse {
    ok: bool,
    dxf: String,
}

#[derive(Serialize, JsonSchema)]
struct UploadResponse {
    ok: bool,
    id: i64,
}

#[derive(Serialize, JsonSchema)]
struct ListDrawingsResponse {
    ok: bool,
    drawings: Vec<DrawingSummary>,
}

#[derive(Serialize, JsonSchema)]
struct GetDrawingResponse {
    ok: bool,
    drawing: DrawingFull,
}

#[derive(Serialize, JsonSchema)]
struct AnalyzeResponse {
    ok: bool,
    analysis: String,
}

#[derive(Serialize, JsonSchema)]
struct RedesignResponse {
    ok: bool,
    id: i64,
    old_drawing_id: i64,
    proposal: String,
}

#[derive(Serialize, JsonSchema)]
struct MergeResponse {
    ok: bool,
    id: i64,
    merged_from: Vec<i64>,
    proposal: String,
}

#[derive(Serialize, JsonSchema)]
struct GithubReadResponse {
    ok: bool,
    content: String,
    sha: String,
}

#[derive(Serialize, JsonSchema)]
struct GithubCommitResponse {
    ok: bool,
    html_url: String,
}

#[derive(Serialize, JsonSchema)]
struct VpsReadResponse {
    ok: bool,
    content: String,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct VpsReadQuery {
    /// 読むファイルのパス
    path: String,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct GetDrawingQuery {
    /// 図面ID
    id: i64,
}

pub fn spec() -> Spec {
    Spec::new("open-cg-cad server", env!("CARGO_PKG_VERSION"))
        .description("建築ボリューム生成・図面DB・AI解析/再設計/合成・GitHub/VPS連携のAPI")
        .op(Op::get("/").id("index").summary("トップ画面(HTML)").public().response_empty(200, "text/html"))
        .op(Op::get("/healthz").summary("生存確認").public().ok::<OkBody>("alive"))
        .op(Op::post("/v1/generate").summary("敷地・法規から建築ボリュームを生成").public().body::<GenerateRequest>().ok::<GenerateResponse>("生成した").response::<ErrorBody>(400, "入力不正"))
        .op(Op::post("/v1/generate/dxf").summary("生成した建築ボリュームをDXF(R12)で出力").public().body::<GenerateRequest>().ok::<DxfResponse>("DXF").response::<ErrorBody>(500, "出力失敗"))
        .op(Op::post("/v1/drawings/upload").summary("図面をアップロード(base64)").public().body::<UploadDrawingRequest>().ok::<UploadResponse>("保存した").response::<ErrorBody>(400, "入力不正"))
        .op(Op::get("/v1/drawings").summary("図面の一覧").public().ok::<ListDrawingsResponse>("一覧"))
        .op(Op::get("/v1/drawings/get").summary("図面を1件取得").public().query::<GetDrawingQuery>().ok::<GetDrawingResponse>("図面").response::<ErrorBody>(404, "見つからない"))
        .op(Op::post("/v1/drawings/analyze").summary("図面の説明文をAIで解析").public().body::<AnalyzeDrawingRequest>().ok::<AnalyzeResponse>("解析結果").response::<ErrorBody>(404, "図面なし").response::<ErrorBody>(502, "aruaru-llmに届かない"))
        .op(Op::post("/v1/drawings/redesign").summary("旧図面と指示から再設計案を作る").public().body::<RedesignDrawingRequest>().ok::<RedesignResponse>("再設計案").response::<ErrorBody>(404, "図面なし").response::<ErrorBody>(502, "aruaru-llmに届かない"))
        .op(Op::post("/v1/drawings/merge").summary("複数図面を1つの合成案にする").public().body::<MergeDrawingsRequest>().ok::<MergeResponse>("合成案").response::<ErrorBody>(400, "件数不正").response::<ErrorBody>(502, "aruaru-llmに届かない"))
        .op(Op::post("/v1/agent/github/read").summary("GitHubのファイルを読む").public().body::<GithubReadRequest>().ok::<GithubReadResponse>("内容とsha").response::<ErrorBody>(502, "GitHubエラー"))
        .op(Op::post("/v1/agent/github/commit").summary("GitHubへファイルをコミット").public().body::<GithubCommitRequest>().ok::<GithubCommitResponse>("コミットURL").response::<ErrorBody>(502, "GitHubエラー"))
        .op(Op::get("/v1/agent/vps/read").summary("VPS上のファイルを読む").public().query::<VpsReadQuery>().ok::<VpsReadResponse>("内容").response::<ErrorBody>(400, "パス不正"))
        .op(Op::post("/v1/agent/vps/write").summary("VPS上へファイルを書く").public().body::<VpsWriteRequest>().ok::<OkBody>("書いた").response::<ErrorBody>(400, "パス不正"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn spec_is_lint_clean_and_covers_every_route() {
        let s = super::spec();
        assert!(s.lint().is_empty(), "{:?}", s.lint());
        let routes = [
            ("GET", "/"), ("GET", "/healthz"), ("POST", "/v1/generate"), ("POST", "/v1/generate/dxf"),
            ("POST", "/v1/drawings/upload"), ("GET", "/v1/drawings"), ("GET", "/v1/drawings/get"),
            ("POST", "/v1/drawings/analyze"), ("POST", "/v1/drawings/redesign"), ("POST", "/v1/drawings/merge"),
            ("POST", "/v1/agent/github/read"), ("POST", "/v1/agent/github/commit"),
            ("GET", "/v1/agent/vps/read"), ("POST", "/v1/agent/vps/write"),
        ];
        assert!(s.undocumented(routes).is_empty());
    }
}
