//! 敷地(土地)情報。簡易的な多角形(矩形含む)として表現する。
//!
//! 座標系はメートル単位のローカル平面座標(x, y)を想定する。GIS的な
//! 測地系(緯度経度)からの変換は本クレートのスコープ外——不動産検索
//! システム側で既にローカル座標へ変換済みのデータを渡す前提。

use thiserror::Error;

/// 敷地の境界を表す多角形(反時計回り・時計回りいずれでも面積計算は
/// 絶対値を取るため問題ない)。最低3頂点が必要。
#[derive(Debug, Clone, PartialEq)]
pub struct Site {
    /// 敷地境界の頂点列(メートル単位、ローカル平面座標)。
    pub vertices: Vec<(f64, f64)>,
}

#[derive(Debug, Error, PartialEq)]
pub enum SiteError {
    #[error("site polygon must have at least 3 vertices, got {0}")]
    TooFewVertices(usize),
    #[error("site polygon has zero or near-zero area (degenerate/self-intersecting input?)")]
    DegeneratePolygon,
}

impl Site {
    /// 矩形敷地の簡易コンストラクタ(幅width_m x 奥行きdepth_mの矩形、
    /// 原点を左下角とする)。
    pub fn rectangle(width_m: f64, depth_m: f64) -> Result<Self, SiteError> {
        Site::polygon(vec![(0.0, 0.0), (width_m, 0.0), (width_m, depth_m), (0.0, depth_m)])
    }

    /// 任意多角形敷地のコンストラクタ。
    pub fn polygon(vertices: Vec<(f64, f64)>) -> Result<Self, SiteError> {
        if vertices.len() < 3 {
            return Err(SiteError::TooFewVertices(vertices.len()));
        }
        let site = Site { vertices };
        if site.area_m2() < 1e-6 {
            return Err(SiteError::DegeneratePolygon);
        }
        Ok(site)
    }

    /// 敷地面積(平方メートル)。シューレース公式(靴紐公式)による、
    /// 一般的な単純多角形の標準的な面積計算。
    pub fn area_m2(&self) -> f64 {
        let n = self.vertices.len();
        let mut sum = 0.0;
        for i in 0..n {
            let (x0, y0) = self.vertices[i];
            let (x1, y1) = self.vertices[(i + 1) % n];
            sum += x0 * y1 - x1 * y0;
        }
        (sum / 2.0).abs()
    }

    /// 軸並行境界ボックス((min_x, min_y), (max_x, max_y))。
    /// 直方体ボリューム生成(MVP)で敷地の外接矩形を使う際に用いる。
    pub fn bounding_box(&self) -> ((f64, f64), (f64, f64)) {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        for &(x, y) in &self.vertices {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        ((min_x, min_y), (max_x, max_y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_area_matches_width_times_depth() {
        let site = Site::rectangle(10.0, 10.0).unwrap();
        assert!((site.area_m2() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn arbitrary_quad_area_via_shoelace() {
        // 台形: (0,0),(4,0),(4,2),(0,3) の面積は台形公式で (3+2)/2*4=10
        let site = Site::polygon(vec![(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 3.0)]).unwrap();
        assert!((site.area_m2() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_too_few_vertices() {
        assert_eq!(Site::polygon(vec![(0.0, 0.0), (1.0, 0.0)]), Err(SiteError::TooFewVertices(2)));
    }

    #[test]
    fn rejects_degenerate_polygon() {
        // 全頂点が同一直線上(面積0)
        let err = Site::polygon(vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)]).unwrap_err();
        assert_eq!(err, SiteError::DegeneratePolygon);
    }

    #[test]
    fn bounding_box_of_rectangle() {
        let site = Site::rectangle(10.0, 5.0).unwrap();
        assert_eq!(site.bounding_box(), ((0.0, 0.0), (10.0, 5.0)));
    }
}
