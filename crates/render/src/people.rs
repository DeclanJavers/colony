//! People: the rigged villager (assets/models/villager.glb, from tools/blender/villager.py) with
//! its animation clips. Each body part is bound to one bone, so skinning is one joint per vertex.
//! Clips are sampled and blended on the CPU each frame; the GPU skins with a storage buffer of
//! joint matrices. Skin, hair, eye and clothing colours come from each person's DNA.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use std::collections::BTreeMap;
use std::path::Path;

pub const MAX_PEOPLE: usize = 128;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PersonVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub colour: [f32; 3],
    /// 0 fixed colour, 1 skin, 2 hair, 3 clothes, 4 eyes
    pub material: f32,
    /// Which part: 0 body, 1 short hair, 2 long hair, 3 beard, 4 skirt
    pub part: f32,
    pub joint: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PersonInstance {
    pub model: [[f32; 4]; 4],
    pub skin: [f32; 4],
    pub hair: [f32; 4],
    pub clothes: [f32; 4],
    pub eyes: [f32; 4],
    /// x: visible parts (bit mask), y: first joint in the joint buffer, z: 1 if selected
    pub extra: [f32; 4],
}

#[derive(Clone, Copy)]
struct Trs {
    t: Vec3,
    r: Quat,
    s: Vec3,
}

impl Trs {
    fn mat(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.s, self.r, self.t)
    }
    fn blend(&self, o: &Trs, w: f32) -> Trs {
        Trs { t: self.t.lerp(o.t, w), r: self.r.slerp(o.r, w), s: self.s.lerp(o.s, w) }
    }
}

enum Path3 {
    T(Vec<Vec3>),
    R(Vec<Quat>),
    S(Vec<Vec3>),
}

struct Channel {
    node: usize,
    times: Vec<f32>,
    values: Path3,
}

pub struct Clip {
    pub duration: f32,
    channels: Vec<Channel>,
}

pub struct Rig {
    parent: Vec<Option<usize>>,
    rest: Vec<Trs>,
    /// Nodes in an order where parents come first.
    order: Vec<usize>,
    joints: Vec<usize>,
    ibm: Vec<Mat4>,
    pub clips: BTreeMap<String, Clip>,
    pub vertices: Vec<PersonVertex>,
    pub indices: Vec<u32>,
    /// Joint index of the right hand (for carried things).
    pub hand: usize,
    /// Node index of the left foot (for working out walking speed).
    foot: usize,
}

fn find(times: &[f32], t: f32) -> (usize, usize, f32) {
    if times.len() < 2 || t <= times[0] {
        return (0, 0, 0.0);
    }
    let i = times.partition_point(|&x| x <= t).min(times.len() - 1);
    let (a, b) = (i - 1, i);
    let w = ((t - times[a]) / (times[b] - times[a]).max(1e-6)).clamp(0.0, 1.0);
    (a, b, w)
}

impl Rig {
    pub fn load(path: &Path) -> Result<Rig, String> {
        let (doc, buffers, _) = gltf::import(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let nodes: Vec<gltf::Node> = doc.nodes().collect();
        let mut parent = vec![None; nodes.len()];
        for n in &nodes {
            for c in n.children() {
                parent[c.index()] = Some(n.index());
            }
        }
        let rest: Vec<Trs> = nodes
            .iter()
            .map(|n| {
                let (t, r, s) = n.transform().decomposed();
                Trs { t: Vec3::from(t), r: Quat::from_array(r), s: Vec3::from(s) }
            })
            .collect();
        let mut order = Vec::new();
        fn visit(i: usize, nodes: &[gltf::Node], order: &mut Vec<usize>) {
            order.push(i);
            for c in nodes[i].children() {
                visit(c.index(), nodes, order);
            }
        }
        for (i, p) in parent.iter().enumerate() {
            if p.is_none() {
                visit(i, &nodes, &mut order);
            }
        }
        let skin = doc.skins().next().ok_or("villager has no skin")?;
        let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
        let reader = skin.reader(|b| Some(&buffers[b.index()]));
        let ibm: Vec<Mat4> = reader.read_inverse_bind_matrices().map(|m| m.map(|c| Mat4::from_cols_array_2d(&c)).collect()).unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
        let hand = joints.iter().position(|&j| nodes[j].name() == Some("RightHand")).unwrap_or(0);
        let foot = nodes.iter().position(|n| n.name() == Some("LeftFoot")).unwrap_or(0);

        let mut clips = BTreeMap::new();
        for anim in doc.animations() {
            let mut channels = Vec::new();
            let mut duration = 0.0f32;
            for ch in anim.channels() {
                let r = ch.reader(|b| Some(&buffers[b.index()]));
                let Some(times) = r.read_inputs() else { continue };
                let times: Vec<f32> = times.collect();
                duration = duration.max(times.last().copied().unwrap_or(0.0));
                let Some(out) = r.read_outputs() else { continue };
                let values = match out {
                    gltf::animation::util::ReadOutputs::Translations(v) => Path3::T(v.map(Vec3::from).collect()),
                    gltf::animation::util::ReadOutputs::Rotations(v) => Path3::R(v.into_f32().map(Quat::from_array).collect()),
                    gltf::animation::util::ReadOutputs::Scales(v) => Path3::S(v.map(Vec3::from).collect()),
                    _ => continue,
                };
                channels.push(Channel { node: ch.target().node().index(), times, values });
            }
            clips.insert(anim.name().unwrap_or("clip").to_string(), Clip { duration: duration.max(0.01), channels });
        }

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for n in &nodes {
            let Some(mesh) = n.mesh() else { continue };
            let part = match n.name().unwrap_or("") {
                "hair_short" => 1.0,
                "hair_long" => 2.0,
                "beard" => 3.0,
                "skirt" => 4.0,
                _ => 0.0,
            };
            for prim in mesh.primitives() {
                let r = prim.reader(|b| Some(&buffers[b.index()]));
                let Some(pos) = r.read_positions() else { continue };
                let pos: Vec<[f32; 3]> = pos.collect();
                let normals: Vec<[f32; 3]> = r.read_normals().map(|v| v.collect()).unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; pos.len()]);
                let js: Vec<[u16; 4]> = r.read_joints(0).map(|j| j.into_u16().collect()).unwrap_or_else(|| vec![[0; 4]; pos.len()]);
                let ws: Vec<[f32; 4]> = r.read_weights(0).map(|w| w.into_f32().collect()).unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 0.0]; pos.len()]);
                let m = prim.material();
                let c = m.pbr_metallic_roughness().base_color_factor();
                let material = match m.name().unwrap_or("") {
                    "skin" => 1.0,
                    "hair" => 2.0,
                    "shirt" => 3.0,
                    "eye" => 4.0,
                    _ => 0.0,
                };
                let base = vertices.len() as u32;
                for k in 0..pos.len() {
                    let w = ws[k];
                    let mut best = 0;
                    for q in 1..4 {
                        if w[q] > w[best] {
                            best = q;
                        }
                    }
                    vertices.push(PersonVertex { pos: pos[k], normal: normals[k], colour: [c[0], c[1], c[2]], material, part, joint: js[k][best] as u32 });
                }
                match r.read_indices() {
                    Some(ix) => indices.extend(ix.into_u32().map(|i| base + i)),
                    None => indices.extend((0..pos.len() as u32).map(|i| base + i)),
                }
            }
        }
        Ok(Rig { parent, rest, order, joints, ibm, clips, vertices, indices, hand, foot })
    }

    fn sample(&self, clip: &Clip, t: f32, pose: &mut [Trs]) {
        let t = t.rem_euclid(clip.duration);
        for ch in &clip.channels {
            let (a, b, w) = find(&ch.times, t);
            let p = &mut pose[ch.node];
            match &ch.values {
                Path3::T(v) => p.t = v[a].lerp(v[b], w),
                Path3::R(v) => p.r = v[a].slerp(v[b], w),
                Path3::S(v) => p.s = v[a].lerp(v[b], w),
            }
        }
    }

    /// Joint matrices for a pose blended from two clips (`w` = weight of `b`), plus the right
    /// hand's transform in model space.
    pub fn pose(&self, a: (&str, f32), b: Option<(&str, f32, f32)>, out: &mut Vec<Mat4>) -> Mat4 {
        let mut pa = self.rest.clone();
        if let Some(c) = self.clips.get(a.0) {
            self.sample(c, a.1, &mut pa);
        }
        if let Some((name, t, w)) = b
            && let Some(c) = self.clips.get(name)
        {
            let mut pb = self.rest.clone();
            self.sample(c, t, &mut pb);
            for (x, y) in pa.iter_mut().zip(&pb) {
                *x = x.blend(y, w);
            }
        }
        let mut global = vec![Mat4::IDENTITY; pa.len()];
        for &i in &self.order {
            global[i] = match self.parent[i] {
                Some(p) => global[p] * pa[i].mat(),
                None => pa[i].mat(),
            };
        }
        for (j, &node) in self.joints.iter().enumerate() {
            out.push(global[node] * self.ibm[j]);
        }
        global[self.joints[self.hand]]
    }

    /// Ground speed (m/s, model scale 1) at which a clip's feet don't slide: the left foot's
    /// total backward travel over the clip, covered while it's planted (about 60% of the time).
    pub fn natural_speed(&self, clip: &str) -> f32 {
        let Some(c) = self.clips.get(clip) else { return 1.0 };
        let samples = 240;
        let mut back = 0.0;
        let mut last: Option<f32> = None;
        for k in 0..=samples {
            let mut pose = self.rest.clone();
            self.sample(c, c.duration * k as f32 / samples as f32, &mut pose);
            let mut global = vec![Mat4::IDENTITY; pose.len()];
            for &i in &self.order {
                global[i] = match self.parent[i] {
                    Some(p) => global[p] * pose[i].mat(),
                    None => pose[i].mat(),
                };
            }
            let z = global[self.foot].w_axis.z;
            if let Some(l) = last {
                back += (l - z).max(0.0);
            }
            last = Some(z);
        }
        (back / (0.6 * c.duration)).max(0.1)
    }

    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    /// Clips that play once and hold their last frame.
    pub fn holds(clip: &str) -> bool {
        matches!(clip, "lie_down" | "sit")
    }
}

/// One person to draw this frame (built by the game from the sim).
#[derive(Clone, Debug)]
pub struct PersonDraw {
    pub key: u64,
    pub pos: Vec3,
    pub facing: f32,
    pub height: f32,
    pub build: f32,
    /// Overall size (children are smaller; 1 for grown people).
    pub size: f32,
    pub skin: [f32; 3],
    pub hair: [f32; 3],
    pub clothes: [f32; 3],
    pub eyes: [f32; 3],
    /// Bits: 1 short hair, 2 long hair, 3 beard, 4 skirt (the body always shows).
    pub parts: u32,
    pub clip: &'static str,
    /// How fast to play the clip (walking follows the real speed over the ground, so feet don't slide).
    pub rate: f32,
    /// Model to show in the right hand (a node or prop model name), if carrying.
    pub carry: Option<&'static str>,
    pub selected: bool,
    /// A coloured ring under them (mood and needs lenses).
    pub ring: Option<[f32; 3]>,
}

/// Per-person animation state kept by the renderer (for smooth clip changes).
#[derive(Clone)]
pub struct AnimState {
    pub clip: &'static str,
    pub since: f32,
    pub prev: &'static str,
    /// How far into the current and previous clips (seconds of clip time).
    pub phase: f32,
    pub prev_phase: f32,
    pub last: f32,
}
