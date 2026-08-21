//! 3Dメッシュ(頂点+インデックス)の簡易表現と、直方体(ボックス)メッシュ
//! 生成。MVPスコープの通り、複雑な曲面等は扱わず直方体・直方体の組み
//! 合わせのみを対象とする。

/// 1頂点(位置+RGBA色、0.0〜1.0)。座標系はメートル単位のワールド座標
/// (x=東西, y=高さ, z=南北)を想定する——レンダラー(`cg-cad-render`)側で
/// NDC座標系へ変換する。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

/// 三角形メッシュ(頂点列+インデックス列、3個ずつで1三角形)。
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn new() -> Self {
        Mesh::default()
    }

    /// 2つのメッシュを結合する(インデックスは自動でオフセットする)。
    pub fn merge(&mut self, other: &Mesh) {
        let offset = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + offset));
    }

    /// 軸並行の直方体(ボックス)メッシュを生成する。
    /// `min`/`max`はワールド座標のコーナー2点、`color`は全頂点に適用する
    /// 単色(このMVPでは面ごとの異なる着色は行わない——段階3の着色
    /// フェーズで拡張余地として残す)。
    pub fn axis_aligned_box(min: [f32; 3], max: [f32; 3], color: [f32; 4]) -> Mesh {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;

        // 8コーナー。
        let corners = [
            [x0, y0, z0], // 0
            [x1, y0, z0], // 1
            [x1, y1, z0], // 2
            [x0, y1, z0], // 3
            [x0, y0, z1], // 4
            [x1, y0, z1], // 5
            [x1, y1, z1], // 6
            [x0, y1, z1], // 7
        ];
        let vertices: Vec<MeshVertex> = corners.iter().map(|&p| MeshVertex { position: p, color }).collect();

        // 6面 x 2三角形 = 12三角形。法線の向き(表裏)は今回未使用
        // (フラットカラーの塗りつぶしのみ、深度テストで正しく描画される)。
        #[rustfmt::skip]
        let indices: Vec<u32> = vec![
            // -z面(front)
            0, 1, 2,  0, 2, 3,
            // +z面(back)
            5, 4, 7,  5, 7, 6,
            // -x面(left)
            4, 0, 3,  4, 3, 7,
            // +x面(right)
            1, 5, 6,  1, 6, 2,
            // -y面(bottom)
            4, 5, 1,  4, 1, 0,
            // +y面(top)
            3, 2, 6,  3, 6, 7,
        ];

        Mesh { vertices, indices }
    }

    /// 三角形数(インデックス数/3)。
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_aligned_box_has_8_vertices_and_12_triangles() {
        let m = Mesh::axis_aligned_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(m.vertices.len(), 8);
        assert_eq!(m.triangle_count(), 12);
        for i in &m.indices {
            assert!((*i as usize) < m.vertices.len());
        }
    }

    #[test]
    fn merge_offsets_indices_correctly() {
        let mut a = Mesh::axis_aligned_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 0.0, 1.0]);
        let b = Mesh::axis_aligned_box([2.0, 0.0, 0.0], [3.0, 1.0, 1.0], [0.0, 1.0, 0.0, 1.0]);
        let a_vertex_count_before = a.vertices.len();
        a.merge(&b);
        assert_eq!(a.vertices.len(), a_vertex_count_before + b.vertices.len());
        assert_eq!(a.triangle_count(), 24);
        for i in &a.indices {
            assert!((*i as usize) < a.vertices.len());
        }
    }
}
