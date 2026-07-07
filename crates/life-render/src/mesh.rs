#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex { pub pos: [f32; 3], pub uv: [f32; 2] }

/// Torus with `major`/`minor` radii, `nu`×`nv` segments. UV spans [0,1]² over the surface.
pub fn torus(major: f32, minor: f32, nu: u32, nv: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut verts = Vec::new();
    for i in 0..=nu {
        let u = i as f32 / nu as f32 * std::f32::consts::TAU;
        for j in 0..=nv {
            let v = j as f32 / nv as f32 * std::f32::consts::TAU;
            let x = (major + minor * v.cos()) * u.cos();
            let y = minor * v.sin();
            let z = (major + minor * v.cos()) * u.sin();
            verts.push(Vertex { pos: [x, y, z], uv: [i as f32 / nu as f32, j as f32 / nv as f32] });
        }
    }
    let mut idx = Vec::new();
    let stride = nv + 1;
    for i in 0..nu {
        for j in 0..nv {
            let a = i * stride + j;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (verts, idx)
}

/// Flat quad in the XY plane, UV [0,1]².
pub fn quad() -> (Vec<Vertex>, Vec<u32>) {
    let v = vec![
        Vertex { pos: [-1.5, -1.5, 0.0], uv: [0.0, 1.0] },
        Vertex { pos: [ 1.5, -1.5, 0.0], uv: [1.0, 1.0] },
        Vertex { pos: [ 1.5,  1.5, 0.0], uv: [1.0, 0.0] },
        Vertex { pos: [-1.5,  1.5, 0.0], uv: [0.0, 0.0] },
    ];
    (v, vec![0, 1, 2, 0, 2, 3])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn torus_index_count_matches_grid() {
        let (v, idx) = torus(2.0, 0.8, 16, 8);
        assert_eq!(v.len(), (17 * 9) as usize);
        assert_eq!(idx.len(), (16 * 8 * 6) as usize);
        assert!(idx.iter().all(|&i| (i as usize) < v.len()));
    }
}
