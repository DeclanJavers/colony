//! Models from the Blender pipeline (assets/models/*.glb): each file becomes one mesh with its
//! material colours baked into the vertices, drawn once per frame for all its instances. Parts
//! named `leaves1` / `leaves2` take a per-instance colour (seasons, ripening grain); `flame`s glow;
//! `window`s glow when the instance says someone's home; roof materials (`roof_*`, except the
//! dark ridges) take snow.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::path::Path;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ModelVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// Linear RGB
    pub colour: [f32; 3],
    /// 0 fixed, 1 leaves, 2 lighter top leaves, 3 flame, 4 window, 5 roof
    pub flag: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Instance {
    pub model: [[f32; 4]; 4],
    pub tint: [f32; 4],
}

impl Instance {
    pub fn new(m: Mat4, tint: Option<[f32; 3]>) -> Self {
        let t = tint.map_or([0.0; 4], |c| [c[0], c[1], c[2], 1.0]);
        Instance { model: m.to_cols_array_2d(), tint: t }
    }

    /// With any flames hidden (a fire that's out).
    pub fn unlit(m: Mat4) -> Self {
        Instance { model: m.to_cols_array_2d(), tint: [0.0, 0.0, 0.0, 2.0] }
    }

    /// A building with its windows lit (someone's home and awake), and an optional tint for
    /// any `leaves` parts.
    pub fn lit(m: Mat4, tint: Option<[f32; 3]>) -> Self {
        let t = tint.unwrap_or([0.0; 3]);
        Instance { model: m.to_cols_array_2d(), tint: [t[0], t[1], t[2], if tint.is_some() { 4.0 } else { 3.0 }] }
    }
}

pub struct ModelMesh {
    pub name: String,
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    /// Height of the model's top, for culling margins.
    pub height: f32,
}

pub fn load(path: &Path) -> Result<ModelMesh, String> {
    let (doc, buffers, _) = gltf::import(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = ModelMesh {
        name: path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        vertices: Vec::new(),
        indices: Vec::new(),
        height: 0.0,
    };
    let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("no scene")?;
    for node in scene.nodes() {
        walk(&node, Mat4::IDENTITY, &buffers, &mut out);
    }
    if out.vertices.is_empty() {
        return Err(format!("{}: no meshes", path.display()));
    }
    Ok(out)
}

fn walk(node: &gltf::Node, parent: Mat4, buffers: &[gltf::buffer::Data], out: &mut ModelMesh) {
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent * local;
    if let Some(mesh) = node.mesh() {
        let name = node.name().or(mesh.name()).unwrap_or("");
        let node_flag = if name.starts_with("flame") {
            3.0
        } else if name.starts_with("leaves2") {
            2.0
        } else if name.starts_with("leaves1") {
            1.0
        } else if name.starts_with("window") {
            4.0
        } else {
            0.0
        };
        let normal_m = world.inverse().transpose();
        for prim in mesh.primitives() {
            let mat_name = prim.material().name().unwrap_or("");
            let flag = if node_flag == 0.0 && mat_name.starts_with("roof_") && !mat_name.ends_with("_dark") { 5.0 } else { node_flag };
            let reader = prim.reader(|b| Some(&buffers[b.index()]));
            let Some(positions) = reader.read_positions() else { continue };
            let positions: Vec<[f32; 3]> = positions.collect();
            let normals: Vec<[f32; 3]> = reader.read_normals().map(|n| n.collect()).unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
            let c = prim.material().pbr_metallic_roughness().base_color_factor();
            let base = out.vertices.len() as u32;
            for (p, n) in positions.iter().zip(&normals) {
                let wp = world.transform_point3(Vec3::from(*p));
                let wn = normal_m.transform_vector3(Vec3::from(*n)).normalize_or_zero();
                out.height = out.height.max(wp.y);
                out.vertices.push(ModelVertex { pos: wp.into(), normal: wn.into(), colour: [c[0], c[1], c[2]], flag });
            }
            match reader.read_indices() {
                Some(ix) => out.indices.extend(ix.into_u32().map(|i| base + i)),
                None => out.indices.extend((0..positions.len() as u32).map(|i| base + i)),
            }
        }
    }
    for child in node.children() {
        walk(&child, world, buffers, out);
    }
}
