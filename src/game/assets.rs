//! Asset loading: glTF models, animation graphs, fonts and the walkable-ground heightmap.

use bevy::animation::AnimationTargetId;
use bevy::asset::UntypedAssetId;
use bevy::gltf::{Gltf, GltfNode};
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use super::AppState;
use crate::sim::data::Gun;
use crate::sim::layout::Layout;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        // Fonts are embedded so the UI never depends on system fonts; inserted at build
        // time because the first `OnEnter` runs before startup commands apply.
        let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
        let display = fonts.add(Font::from_bytes(
            include_bytes!("../../assets/fonts/NotoSans-Bold.ttf").to_vec(),
        ));
        let mono = fonts.add(Font::from_bytes(
            include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf").to_vec(),
        ));
        app.insert_resource(Fonts { display, mono })
            .insert_resource(LayoutRes(Layout::maplewood()))
            .add_systems(Startup, start_loading)
            .add_systems(Update, finish_loading.run_if(in_state(AppState::Loading)));
    }
}

#[derive(Resource, Clone)]
pub struct Fonts {
    pub display: Handle<Font>,
    pub mono: Handle<Font>,
}

impl Fonts {
    pub fn display(&self, size: f32) -> TextFont {
        TextFont {
            font: self.display.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }

    pub fn mono(&self, size: f32) -> TextFont {
        TextFont {
            font: self.mono.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

#[derive(Resource, Clone)]
pub struct LayoutRes(pub Layout);

#[derive(Resource)]
pub struct Loading {
    pub granny: Handle<Gltf>,
    pub zombie: Handle<Gltf>,
    pub town: Handle<Gltf>,
    pub lamp: Handle<Gltf>,
    pub revolver: Handle<Gltf>,
    pub shotgun: Handle<Gltf>,
}

impl Loading {
    fn handles(&self) -> [UntypedAssetId; 6] {
        [
            self.granny.id().untyped(),
            self.zombie.id().untyped(),
            self.town.id().untyped(),
            self.lamp.id().untyped(),
            self.revolver.id().untyped(),
            self.shotgun.id().untyped(),
        ]
    }
}

/// Clip slots shared by Granny and the shamblers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clip {
    Idle,
    Walk,
    Attack,
    Die,
    /// Granny's gun layers: upper body only.
    AimRevolver,
    FireRevolver,
    AimShotgun,
    FireShotgun,
    /// Idle and walk restricted to the legs, under the gun layers.
    IdleLower,
    WalkLower,
}

impl Clip {
    pub const COUNT: usize = 10;
    pub const ALL: [Clip; Clip::COUNT] = [
        Clip::Idle,
        Clip::Walk,
        Clip::Attack,
        Clip::Die,
        Clip::AimRevolver,
        Clip::FireRevolver,
        Clip::AimShotgun,
        Clip::FireShotgun,
        Clip::IdleLower,
        Clip::WalkLower,
    ];

    pub fn aim(gun: Gun) -> Clip {
        match gun {
            Gun::Revolver => Clip::AimRevolver,
            Gun::Shotgun => Clip::AimShotgun,
        }
    }

    pub fn fire(gun: Gun) -> Clip {
        match gun {
            Gun::Revolver => Clip::FireRevolver,
            Gun::Shotgun => Clip::FireShotgun,
        }
    }

    /// Source clip name in Granny's glTF, and which body part it may drive.
    fn granny_source(self) -> (&'static str, Layer) {
        match self {
            Clip::Idle => ("Idle", Layer::Full),
            Clip::Walk => ("Walk", Layer::Full),
            Clip::Attack => ("Attack", Layer::Full),
            Clip::Die => ("Die", Layer::Full),
            Clip::AimRevolver => ("AimRevolver", Layer::Upper),
            Clip::FireRevolver => ("FireRevolver", Layer::Upper),
            Clip::AimShotgun => ("AimShotgun", Layer::Upper),
            Clip::FireShotgun => ("FireShotgun", Layer::Upper),
            Clip::IdleLower => ("Idle", Layer::Lower),
            Clip::WalkLower => ("Walk", Layer::Lower),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Full,
    Upper,
    Lower,
}

/// Mask groups: a set bit in a clip's mask stops it from driving that group.
const UPPER_GROUP: u32 = 0;
const LOWER_GROUP: u32 = 1;

/// Bones the gun layers own: everything from the spine up.
const UPPER_BONES: [&str; 13] = [
    "spine",
    "chest",
    "neck",
    "head",
    "glasses",
    "shoulder.L",
    "upper_arm.L",
    "forearm.L",
    "hand.L",
    "shoulder.R",
    "upper_arm.R",
    "forearm.R",
    "hand.R",
];

#[derive(Clone)]
pub struct ActorKit {
    pub scene: Handle<WorldAsset>,
    pub graph: Handle<AnimationGraph>,
    nodes: [Option<AnimationNodeIndex>; Clip::COUNT],
    durations: [f32; Clip::COUNT],
}

impl ActorKit {
    pub fn node(&self, clip: Clip) -> Option<AnimationNodeIndex> {
        self.nodes[clip as usize]
    }

    pub fn duration(&self, clip: Clip) -> f32 {
        self.durations[clip as usize]
    }
}

#[derive(Resource, Clone)]
pub struct GameAssets {
    pub granny: ActorKit,
    pub zombie: ActorKit,
    pub town: Handle<WorldAsset>,
    pub lamp: Handle<WorldAsset>,
    pub revolver: Handle<WorldAsset>,
    pub shotgun: Handle<WorldAsset>,
}

impl GameAssets {
    pub fn gun(&self, gun: Gun) -> Handle<WorldAsset> {
        match gun {
            Gun::Revolver => self.revolver.clone(),
            Gun::Shotgun => self.shotgun.clone(),
        }
    }
}

/// Every node's name path from its scene root, as Bevy builds animation target ids.
fn node_paths(gltf: &Gltf, nodes: &Assets<GltfNode>) -> Vec<(Vec<Name>, String)> {
    let children: std::collections::HashSet<AssetId<GltfNode>> = gltf
        .nodes
        .iter()
        .filter_map(|h| nodes.get(h))
        .flat_map(|n| n.children.iter().map(|c| c.id()))
        .collect();
    let mut out = Vec::new();
    let mut stack: Vec<(AssetId<GltfNode>, Vec<Name>)> = gltf
        .nodes
        .iter()
        .filter(|h| !children.contains(&h.id()))
        .map(|h| (h.id(), Vec::new()))
        .collect();
    while let Some((id, mut path)) = stack.pop() {
        let Some(node) = nodes.get(id) else { continue };
        path.push(Name::new(node.name.clone()));
        out.push((path.clone(), node.name.clone()));
        for c in &node.children {
            stack.push((c.id(), path.clone()));
        }
    }
    out
}

/// Granny's graph: full-body clips, plus gun layers masked to the upper body over
/// idle/walk masked to the legs, so she can aim and fire while walking.
fn granny_kit(
    gltf: &Gltf,
    nodes: &Assets<GltfNode>,
    clips: &Assets<AnimationClip>,
    graphs: &mut Assets<AnimationGraph>,
) -> ActorKit {
    let mut graph = AnimationGraph::new();
    let root = graph.root;
    let mut kit_nodes = [None; Clip::COUNT];
    let mut durations = [1.0; Clip::COUNT];
    for clip in Clip::ALL {
        let (name, layer) = clip.granny_source();
        let Some(handle) = gltf.named_animations.get(name).cloned() else {
            warn!("granny.glb has no {name} clip");
            continue;
        };
        durations[clip as usize] = clips.get(&handle).map(|c| c.duration()).unwrap_or(1.0);
        let mask = match layer {
            Layer::Full => 0,
            Layer::Upper => 1 << LOWER_GROUP,
            Layer::Lower => 1 << UPPER_GROUP,
        };
        kit_nodes[clip as usize] = Some(graph.add_clip_with_mask(handle, mask, 1.0, root));
    }
    for (path, name) in node_paths(gltf, nodes) {
        let group = if UPPER_BONES.contains(&name.as_str()) {
            UPPER_GROUP
        } else {
            LOWER_GROUP
        };
        graph.add_target_to_mask_group(AnimationTargetId::from_names(path.iter()), group);
    }
    ActorKit {
        scene: gltf.scenes[0].clone(),
        graph: graphs.add(graph),
        nodes: kit_nodes,
        durations,
    }
}

/// Walkable surface height sampled from the town mesh (sidewalks and lawns sit up to ~0.37 m).
#[derive(Resource, Clone)]
pub struct Ground {
    min: Vec2,
    cell: f32,
    size: usize,
    heights: Vec<f32>,
}

impl Ground {
    pub fn flat() -> Self {
        Self {
            min: Vec2::ZERO,
            cell: 1.0,
            size: 1,
            heights: vec![0.0],
        }
    }

    pub fn height(&self, p: Vec2) -> f32 {
        let local = (p - self.min) / self.cell;
        let (x, y) = (local.x.floor() as isize, local.y.floor() as isize);
        if x < 0 || y < 0 || x as usize >= self.size || y as usize >= self.size {
            return 0.0;
        }
        self.heights[y as usize * self.size + x as usize]
    }

    /// Rasterizes upward-facing, ground-level triangles into a max-height grid.
    fn from_triangles(
        triangles: impl Iterator<Item = [Vec3; 3]>,
        min: Vec2,
        extent: f32,
        cell: f32,
    ) -> Self {
        let size = (extent / cell).ceil() as usize;
        let mut heights = vec![f32::NEG_INFINITY; size * size];
        for [a, b, c] in triangles {
            let n = (b - a).cross(c - a);
            if n.length_squared() == 0.0 || n.normalize().y.abs() < 0.7 {
                continue;
            }
            if a.y.max(b.y).max(c.y) > 0.6 || a.y.min(b.y).min(c.y) < -0.5 {
                continue;
            }
            let (pa, pb, pc) = (a.xz(), b.xz(), c.xz());
            let lo = pa.min(pb).min(pc);
            let hi = pa.max(pb).max(pc);
            let x0 = (((lo.x - min.x) / cell).floor().max(0.0)) as usize;
            let y0 = (((lo.y - min.y) / cell).floor().max(0.0)) as usize;
            let x1 = ((((hi.x - min.x) / cell).ceil()) as usize).min(size);
            let y1 = ((((hi.y - min.y) / cell).ceil()) as usize).min(size);
            let area = (pb - pa).perp_dot(pc - pa);
            if area.abs() < 1e-9 {
                continue;
            }
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = min + Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * cell;
                    let w0 = (pb - p).perp_dot(pc - p) / area;
                    let w1 = (pc - p).perp_dot(pa - p) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                        continue;
                    }
                    let h = w0 * a.y + w1 * b.y + w2 * c.y;
                    let slot = &mut heights[y * size + x];
                    *slot = slot.max(h);
                }
            }
        }
        for h in &mut heights {
            if !h.is_finite() {
                *h = 0.0;
            }
        }
        Self {
            min,
            cell,
            size,
            heights,
        }
    }
}

fn start_loading(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(Loading {
        granny: server.load("models/granny.glb"),
        zombie: server.load("models/GrannyZombie_Animated.glb"),
        town: server.load("models/maplewood-town.glb"),
        lamp: server.load("models/streetlamp.glb"),
        revolver: server.load("models/revolver.glb"),
        shotgun: server.load("models/shotgun.glb"),
    });
}

/// Fraction of the model files that finished loading, for the loading screen.
pub fn loading_progress(loading: &Loading, server: &AssetServer) -> f32 {
    let done = loading
        .handles()
        .iter()
        .filter(|id| server.is_loaded_with_dependencies(**id))
        .count();
    done as f32 / loading.handles().len() as f32
}

#[allow(clippy::too_many_arguments)]
fn finish_loading(
    mut commands: Commands,
    loading: Res<Loading>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_nodes: Res<Assets<bevy::gltf::GltfNode>>,
    gltf_meshes: Res<Assets<bevy::gltf::GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
    clips: Res<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if loading_progress(&loading, &server) < 1.0 {
        return;
    }
    let (Some(granny), Some(zombie), Some(town), Some(lamp)) = (
        gltfs.get(&loading.granny),
        gltfs.get(&loading.zombie),
        gltfs.get(&loading.town),
        gltfs.get(&loading.lamp),
    ) else {
        return;
    };
    let kit = |gltf: &Gltf, names: [&str; 4], graphs: &mut Assets<AnimationGraph>| {
        let handles: Vec<Handle<AnimationClip>> = names
            .iter()
            .map(|n| {
                gltf.named_animations
                    .get(*n)
                    .cloned()
                    .unwrap_or_else(|| panic!("missing clip {n}"))
            })
            .collect();
        let mut durations = [1.0; Clip::COUNT];
        for (i, h) in handles.iter().enumerate() {
            durations[i] = clips.get(h).map(|c| c.duration()).unwrap_or(1.0);
        }
        let (graph, nodes) = AnimationGraph::from_clips(handles);
        let mut kit_nodes = [None; Clip::COUNT];
        for (i, n) in nodes.into_iter().enumerate() {
            kit_nodes[i] = Some(n);
        }
        ActorKit {
            scene: gltf.scenes[0].clone(),
            graph: graphs.add(graph),
            nodes: kit_nodes,
            durations,
        }
    };
    let (Some(revolver), Some(shotgun)) =
        (gltfs.get(&loading.revolver), gltfs.get(&loading.shotgun))
    else {
        return;
    };
    let assets = GameAssets {
        granny: granny_kit(granny, &gltf_nodes, &clips, &mut graphs),
        revolver: revolver.scenes[0].clone(),
        shotgun: shotgun.scenes[0].clone(),
        zombie: kit(
            zombie,
            [
                "Zombie_Reach_Idle",
                "Zombie_Walk",
                "Zombie_Attack",
                "Zombie_Die",
            ],
            &mut graphs,
        ),
        town: town.scenes[0].clone(),
        lamp: lamp.scenes[0].clone(),
    };

    let mut triangles: Vec<[Vec3; 3]> = Vec::new();
    for node in town.nodes.iter().filter_map(|h| gltf_nodes.get(h)) {
        let Some(mesh) = node.mesh.as_ref().and_then(|h| gltf_meshes.get(h)) else {
            continue;
        };
        let affine = node.transform.compute_affine();
        for prim in &mesh.primitives {
            let Some(m) = meshes.get(&prim.mesh) else {
                continue;
            };
            let Some(VertexAttributeValues::Float32x3(pos)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            let pos: Vec<Vec3> = pos
                .iter()
                .map(|p| affine.transform_point3(Vec3::from(*p)))
                .collect();
            let idx: Vec<usize> = match m.indices() {
                Some(Indices::U16(i)) => i.iter().map(|&v| v as usize).collect(),
                Some(Indices::U32(i)) => i.iter().map(|&v| v as usize).collect(),
                None => (0..pos.len()).collect(),
            };
            triangles.extend(
                idx.as_chunks::<3>()
                    .0
                    .iter()
                    .map(|t| [pos[t[0]], pos[t[1]], pos[t[2]]]),
            );
        }
    }
    let ground = if triangles.is_empty() {
        Ground::flat()
    } else {
        Ground::from_triangles(triangles.into_iter(), Vec2::splat(-82.0), 164.0, 0.25)
    };
    commands.insert_resource(ground);
    commands.insert_resource(assets);
    next.set(AppState::Title);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heightmap_keeps_highest_flat_surface_and_ignores_walls() {
        let floor = [
            Vec3::new(-10.0, 0.0, -10.0),
            Vec3::new(10.0, 0.0, -10.0),
            Vec3::new(-10.0, 0.0, 10.0),
        ];
        let curb = [
            Vec3::new(-10.0, 0.3, -10.0),
            Vec3::new(0.0, 0.3, -10.0),
            Vec3::new(-10.0, 0.3, 0.0),
        ];
        let wall = [
            Vec3::new(-10.0, 0.0, -10.0),
            Vec3::new(10.0, 0.0, -10.0),
            Vec3::new(-10.0, 0.5, -10.0),
        ];
        let g = Ground::from_triangles(
            [floor, curb, wall].into_iter(),
            Vec2::splat(-10.0),
            20.0,
            0.5,
        );
        assert!((g.height(Vec2::new(-8.0, -8.0)) - 0.3).abs() < 1e-4);
        assert!(g.height(Vec2::new(5.0, -8.0)).abs() < 1e-4);
        assert_eq!(g.height(Vec2::new(50.0, 50.0)), 0.0);
    }
}
