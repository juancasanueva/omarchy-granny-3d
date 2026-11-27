//! Asset loading: glTF models, animation graphs, fonts and the walkable-ground heightmap.

use bevy::animation::AnimationTargetId;
use bevy::asset::{LoadState, RecursiveDependencyLoadState, UntypedAssetId};
use bevy::gltf::{Gltf, GltfNode};
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use super::AppState;
use crate::sim::data::{Gun, Weapon};
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
    pub bat: Handle<Gltf>,
    pub revolver: Handle<Gltf>,
    pub shotgun: Handle<Gltf>,
}

impl Loading {
    fn handles(&self) -> [UntypedAssetId; 7] {
        [
            self.granny.id().untyped(),
            self.zombie.id().untyped(),
            self.town.id().untyped(),
            self.lamp.id().untyped(),
            self.bat.id().untyped(),
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
    /// Granny's weapon holds and shots: upper body only.
    AimBat,
    AimRevolver,
    FireRevolver,
    AimShotgun,
    FireShotgun,
    /// Idle and walk restricted to the legs, under the gun layers.
    IdleLower,
    WalkLower,
}

impl Clip {
    pub const COUNT: usize = 11;
    pub const ALL: [Clip; Clip::COUNT] = [
        Clip::Idle,
        Clip::Walk,
        Clip::Attack,
        Clip::Die,
        Clip::AimBat,
        Clip::AimRevolver,
        Clip::FireRevolver,
        Clip::AimShotgun,
        Clip::FireShotgun,
        Clip::IdleLower,
        Clip::WalkLower,
    ];

    /// The upper-body hold for a weapon.
    pub fn aim(weapon: Weapon) -> Clip {
        match weapon {
            Weapon::Bat => Clip::AimBat,
            Weapon::Revolver => Clip::AimRevolver,
            Weapon::Shotgun => Clip::AimShotgun,
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
            Clip::AimBat => ("AimBat", Layer::Upper),
            Clip::AimRevolver => ("AimRevolver", Layer::Upper),
            Clip::FireRevolver => ("FireRevolver", Layer::Upper),
            Clip::AimShotgun => ("AimShotgun", Layer::Upper),
            Clip::FireShotgun => ("FireShotgun", Layer::Upper),
            Clip::IdleLower => ("Idle", Layer::Lower),
            Clip::WalkLower => ("Walk", Layer::Lower),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub bat: Handle<WorldAsset>,
    pub revolver: Handle<WorldAsset>,
    pub shotgun: Handle<WorldAsset>,
}

impl GameAssets {
    /// The model Granny holds for a weapon.
    pub fn weapon(&self, weapon: Weapon) -> Handle<WorldAsset> {
        match weapon {
            Weapon::Bat => self.bat.clone(),
            Weapon::Revolver => self.revolver.clone(),
            Weapon::Shotgun => self.shotgun.clone(),
        }
    }
}

/// Every node's name path from its scene root, as Bevy builds animation target ids.
fn node_paths(gltf: &Gltf, nodes: &Assets<GltfNode>) -> Vec<(Vec<String>, String)> {
    let index: std::collections::HashMap<AssetId<GltfNode>, usize> = gltf
        .nodes
        .iter()
        .enumerate()
        .map(|(i, h)| (h.id(), i))
        .collect();
    let tree: Vec<(String, Vec<usize>)> = gltf
        .nodes
        .iter()
        .map(|h| match nodes.get(h) {
            Some(n) => (
                n.name.clone(),
                n.children
                    .iter()
                    .filter_map(|c| index.get(&c.id()).copied())
                    .collect(),
            ),
            None => (String::new(), Vec::new()),
        })
        .collect();
    name_paths(&tree)
}

/// Name paths for a node tree given as `(name, child indices)`, walking down from the roots.
fn name_paths(tree: &[(String, Vec<usize>)]) -> Vec<(Vec<String>, String)> {
    let mut is_child = vec![false; tree.len()];
    for (_, kids) in tree {
        for &k in kids {
            if let Some(flag) = is_child.get_mut(k) {
                *flag = true;
            }
        }
    }
    let mut out = Vec::new();
    let mut stack: Vec<(usize, Vec<String>)> = (0..tree.len())
        .filter(|&i| !is_child[i])
        .map(|i| (i, Vec::new()))
        .collect();
    while let Some((i, mut path)) = stack.pop() {
        let (name, kids) = &tree[i];
        path.push(name.clone());
        out.push((path.clone(), name.clone()));
        for &k in kids {
            if k < tree.len() {
                stack.push((k, path.clone()));
            }
        }
    }
    out
}

/// Which mask group a bone belongs to.
fn mask_group(bone: &str) -> u32 {
    if UPPER_BONES.contains(&bone) {
        UPPER_GROUP
    } else {
        LOWER_GROUP
    }
}

/// The groups a layer must not drive.
fn layer_mask(layer: Layer) -> AnimationMask {
    match layer {
        Layer::Full => 0,
        Layer::Upper => 1 << LOWER_GROUP,
        Layer::Lower => 1 << UPPER_GROUP,
    }
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
        kit_nodes[clip as usize] =
            Some(graph.add_clip_with_mask(handle, layer_mask(layer), 1.0, root));
    }
    for (path, name) in node_paths(gltf, nodes) {
        let names: Vec<Name> = path.into_iter().map(Name::new).collect();
        graph.add_target_to_mask_group(
            AnimationTargetId::from_names(names.iter()),
            mask_group(&name),
        );
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

const GRANNY_GLB: &str = "models/granny.glb";
const ZOMBIE_GLB: &str = "models/GrannyZombie_Animated.glb";
const TOWN_GLB: &str = "models/maplewood-town.glb";
const LAMP_GLB: &str = "models/streetlamp.glb";
const BAT_GLB: &str = "models/bat.glb";
const REVOLVER_GLB: &str = "models/revolver.glb";
const SHOTGUN_GLB: &str = "models/shotgun.glb";

fn start_loading(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(Loading {
        granny: server.load(GRANNY_GLB),
        zombie: server.load(ZOMBIE_GLB),
        town: server.load(TOWN_GLB),
        lamp: server.load(LAMP_GLB),
        bat: server.load(BAT_GLB),
        revolver: server.load(REVOLVER_GLB),
        shotgun: server.load(SHOTGUN_GLB),
    });
}

/// Models that could not be loaded, as "path: error" lines for the log and loading screen.
#[derive(Resource, Debug, Clone, Default)]
pub struct LoadFailures(pub Vec<String>);

/// "path: error" for every model whose own load or dependency load failed.
pub fn failure_report(states: &[(&str, Option<String>)]) -> Vec<String> {
    states
        .iter()
        .filter_map(|(path, error)| error.as_ref().map(|e| format!("{path}: {e}")))
        .collect()
}

fn loading_failures(loading: &Loading, server: &AssetServer) -> Vec<String> {
    // Each failure is named by the asset server's own path for that handle.
    let states: Vec<(String, Option<String>)> = loading
        .handles()
        .into_iter()
        .map(|id| {
            let path = server
                .get_path(id)
                .map(|p| p.to_string())
                .unwrap_or_else(|| format!("{id:?}"));
            let error = match (
                server.load_state(id),
                server.recursive_dependency_load_state(id),
            ) {
                (LoadState::Failed(e), _) => Some(e.to_string()),
                (_, RecursiveDependencyLoadState::Failed(e)) => Some(e.to_string()),
                _ => None,
            };
            (path, error)
        })
        .collect();
    let borrowed: Vec<(&str, Option<String>)> = states
        .iter()
        .map(|(p, e)| (p.as_str(), e.clone()))
        .collect();
    failure_report(&borrowed)
}

/// Failures in `current` that were not reported yet; remembers them in `reported`.
fn newly_failed(reported: &mut Vec<String>, current: &[String]) -> Vec<String> {
    let fresh: Vec<String> = current
        .iter()
        .filter(|f| !reported.contains(f))
        .cloned()
        .collect();
    reported.extend(fresh.iter().cloned());
    fresh
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
    mut reported: Local<Vec<String>>,
) {
    // Keep scanning while loading: a dependency can fail on a later frame than another model.
    let fresh = newly_failed(&mut reported, &loading_failures(&loading, &server));
    if !fresh.is_empty() {
        for line in &fresh {
            error!("could not load {line}");
        }
        commands.insert_resource(LoadFailures(reported.clone()));
    }
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
    let (Some(bat), Some(revolver), Some(shotgun)) = (
        gltfs.get(&loading.bat),
        gltfs.get(&loading.revolver),
        gltfs.get(&loading.shotgun),
    ) else {
        return;
    };
    let assets = GameAssets {
        granny: granny_kit(granny, &gltf_nodes, &clips, &mut graphs),
        bat: bat.scenes[0].clone(),
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
    fn failures_found_on_later_frames_are_still_reported() {
        let mut reported = Vec::new();
        let first = vec!["models/shotgun.glb: missing".to_string()];
        assert_eq!(newly_failed(&mut reported, &first), first);
        assert!(
            newly_failed(&mut reported, &first).is_empty(),
            "each failure is logged once"
        );
        let later = vec![
            "models/shotgun.glb: missing".to_string(),
            "models/granny.glb: bad texture".to_string(),
        ];
        assert_eq!(
            newly_failed(&mut reported, &later),
            vec!["models/granny.glb: bad texture".to_string()]
        );
        assert_eq!(reported.len(), 2);
    }

    #[test]
    fn failed_models_are_named_with_their_error() {
        let report = failure_report(&[
            ("models/granny.glb", None),
            (
                "models/shotgun.glb",
                Some("Path not found: assets/models/shotgun.glb".into()),
            ),
        ]);
        assert_eq!(
            report,
            vec!["models/shotgun.glb: Path not found: assets/models/shotgun.glb".to_string()]
        );
        assert!(failure_report(&[("models/granny.glb", None)]).is_empty());
    }

    /// Granny's node tree as `(name, children)`, read from the shipped `granny.glb`.
    /// A shipped model's glTF JSON and its binary chunk.
    fn glb(name: &str) -> (serde_json::Value, Vec<u8>) {
        let path = format!("{}/assets/models/{name}", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json = serde_json::from_slice(&bytes[20..20 + len]).unwrap();
        let bin = bytes[20 + len + 8..].to_vec();
        (json, bin)
    }

    fn granny_tree() -> Vec<(String, Vec<usize>)> {
        let (json, _) = glb("granny.glb");
        json["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                let name = n["name"].as_str().unwrap_or_default().to_string();
                let kids = n["children"]
                    .as_array()
                    .map(|c| c.iter().map(|i| i.as_u64().unwrap() as usize).collect())
                    .unwrap_or_default();
                (name, kids)
            })
            .collect()
    }

    /// First keyframe of `bone`'s rotation in the named clip.
    fn first_rotation(json: &serde_json::Value, bin: &[u8], clip: &str, bone: &str) -> [f32; 4] {
        let nodes = json["nodes"].as_array().unwrap();
        let anim = json["animations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == clip)
            .unwrap();
        let channel = anim["channels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["target"]["path"] == "rotation"
                    && nodes[c["target"]["node"].as_u64().unwrap() as usize]["name"] == bone
            })
            .unwrap_or_else(|| panic!("{clip} has no {bone} rotation"));
        let sampler = &anim["samplers"][channel["sampler"].as_u64().unwrap() as usize];
        let accessor = &json["accessors"][sampler["output"].as_u64().unwrap() as usize];
        let view = &json["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
        let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize
            + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
        std::array::from_fn(|i| {
            f32::from_le_bytes(bin[offset + 4 * i..offset + 4 * i + 4].try_into().unwrap())
        })
    }

    #[test]
    fn every_clip_the_game_plays_is_in_granny_glb() {
        let (json, _) = glb("granny.glb");
        let names: Vec<&str> = json["animations"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|a| a["name"].as_str())
            .collect();
        for clip in Clip::ALL {
            let (name, _) = clip.granny_source();
            assert!(
                names.contains(&name),
                "granny.glb is missing the {name} clip"
            );
        }
    }

    #[test]
    fn weapon_holds_are_real_poses_not_idle() {
        let (json, bin) = glb("granny.glb");
        let idle = first_rotation(&json, &bin, "Idle", "hand.R");
        for hold in ["AimBat", "AimRevolver", "AimShotgun"] {
            let q = first_rotation(&json, &bin, hold, "hand.R");
            let dot: f32 = q.iter().zip(idle).map(|(a, b)| a * b).sum();
            assert!(dot.abs() < 0.99, "{hold} holds the hand like Idle");
        }
    }

    #[test]
    fn bat_is_bat_sized_with_its_handle_in_the_hand() {
        let (json, _) = glb("bat.glb");
        let pos = &json["accessors"][json["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
            .as_u64()
            .unwrap() as usize];
        let min: Vec<f64> = pos["min"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let max: Vec<f64> = pos["max"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let size: f64 = (0..3)
            .map(|i| (max[i] - min[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        assert!((0.55..0.8).contains(&size), "bat length {size}");
        // The bat is exported in the hand bone's frame, so the palm (near the origin) is inside its bounds.
        let palm = [0.0, 0.06, 0.0];
        for i in 0..3 {
            assert!(
                min[i] - 0.05 <= palm[i] && palm[i] <= max[i] + 0.05,
                "handle not at the palm on axis {i}"
            );
        }
    }

    #[test]
    fn guns_carry_a_muzzle_point() {
        for gun in ["revolver.glb", "shotgun.glb"] {
            let (json, _) = glb(gun);
            let has_muzzle = json["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["name"] == "Muzzle");
            assert!(has_muzzle, "{gun} has no Muzzle node");
        }
    }

    #[test]
    fn every_upper_bone_exists_in_granny_with_its_full_path() {
        let paths = name_paths(&granny_tree());
        for bone in UPPER_BONES {
            assert!(
                paths.iter().any(|(_, name)| name == bone),
                "{bone} missing from granny.glb"
            );
        }
        let hand = paths.iter().find(|(_, n)| n == "hand.R").unwrap();
        assert_eq!(
            hand.0,
            [
                "GrannyRig",
                "root",
                "hips",
                "spine",
                "chest",
                "shoulder.R",
                "upper_arm.R",
                "forearm.R",
                "hand.R"
            ]
        );
    }

    #[test]
    fn arms_and_torso_are_upper_and_legs_are_lower() {
        assert_eq!(mask_group("hand.R"), UPPER_GROUP);
        assert_eq!(mask_group("chest"), UPPER_GROUP);
        for leg in ["root", "hips", "thigh.L", "shin.R", "foot.L"] {
            assert_eq!(mask_group(leg), LOWER_GROUP, "{leg}");
        }
        let groups: std::collections::HashSet<u32> = name_paths(&granny_tree())
            .iter()
            .map(|(_, n)| mask_group(n))
            .collect();
        assert!(groups.contains(&UPPER_GROUP) && groups.contains(&LOWER_GROUP));
    }

    #[test]
    fn gun_layers_skip_the_legs_and_leg_layers_skip_the_upper_body() {
        assert_eq!(layer_mask(Layer::Full), 0);
        assert_eq!(layer_mask(Layer::Upper), 1 << LOWER_GROUP);
        assert_eq!(layer_mask(Layer::Lower), 1 << UPPER_GROUP);
        assert_eq!(Clip::AimBat.granny_source().1, Layer::Upper);
        assert_eq!(Clip::AimRevolver.granny_source().1, Layer::Upper);
        assert_eq!(Clip::FireShotgun.granny_source().1, Layer::Upper);
        assert_eq!(Clip::WalkLower.granny_source().1, Layer::Lower);
        assert_eq!(Clip::Attack.granny_source().1, Layer::Full);
    }

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
