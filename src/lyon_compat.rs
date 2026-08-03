use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::color::Alpha;
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError, VertexFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{
    AlphaMode2d, Material2d, Material2dKey, Material2dPlugin, MeshMaterial2d,
};
use bevy_prototype_lyon::geometry::Geometry;
use bevy_prototype_lyon::prelude::tess::path::{Path, math::point, path::Builder};
use bevy_prototype_lyon::prelude::tess::{
    BuffersBuilder, FillTessellator, FillVertex, FillVertexConstructor, StrokeOptions,
    StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers, math::Point,
};
use std::collections::{HashMap, HashSet};

pub use bevy_prototype_lyon::prelude::{FillOptions, RectangleOrigin, shapes};

const SHAPE_SHADER: &str = "shaders/screen_space_shape.wgsl";
const SHAPE_MATERIAL_HANDLE: Handle<ScreenSpaceShapeMaterial> =
    bevy::asset::uuid_handle!("bb8709ee-52b6-4a94-9149-a69c776c9675");
const ATTRIBUTE_SCREEN_OFFSET: MeshVertexAttribute =
    MeshVertexAttribute::new("ScreenOffset", 1_847_362_941, VertexFormat::Float32x2);
const MIN_STROKE_TOPOLOGY_WIDTH: f32 = 1.0e-9;
const STROKE_TOPOLOGY_WIDTH_RATIO: f32 = 1.0e-4;
const BOUNDARY_VERTEX_MERGE_RATIO: f32 = 1.0e-7;
const BOUNDARY_VERTEX_MERGE_ULPS: f32 = 2.0;
const MAX_BOUNDARY_ARTIFACT_VERTICES: usize = 8;
const BOUNDARY_ARTIFACT_AREA_RATIO: f32 = 1.0e-2;
// Preserve ordinary square corners while preventing acute joins from turning
// a fixed-width screen-space stroke into an arbitrarily long miter spike.
const MAX_SCREEN_SPACE_JOIN_SCALE: f32 = std::f32::consts::SQRT_2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StrokeAlignment {
    /// Straddles the source path equally on both sides.
    #[default]
    Center,
    /// Occupies the filled region for closed paths, or the path's positive
    /// (left-hand) side when no filled boundary can be derived.
    Inward,
}

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Fill {
    pub options: FillOptions,
    pub color: Color,
}

impl Default for Fill {
    fn default() -> Self {
        Self {
            options: FillOptions::default(),
            color: Color::WHITE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Stroke {
    pub color: Color,
    pub width_px: f32,
    pub alignment: StrokeAlignment,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            width_px: 1.0,
            alignment: StrokeAlignment::Center,
        }
    }
}

#[derive(Component, Default, Clone)]
#[require(
    Mesh2d,
    MeshMaterial2d<ScreenSpaceShapeMaterial> = screen_space_shape_material(),
    Transform,
    Visibility
)]
pub struct Shape {
    pub path: Path,
}

fn screen_space_shape_material() -> MeshMaterial2d<ScreenSpaceShapeMaterial> {
    MeshMaterial2d(SHAPE_MATERIAL_HANDLE)
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct ScreenSpaceShapeMaterial {}

impl Material2d for ScreenSpaceShapeMaterial {
    fn vertex_shader() -> ShaderRef {
        SHAPE_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHAPE_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(1),
            ATTRIBUTE_SCREEN_OFFSET.at_shader_location(2),
        ])?];
        Ok(())
    }
}

pub struct ShapePlugin;

#[derive(Resource, Deref, DerefMut)]
struct ShapeFillTessellator(FillTessellator);

#[derive(Resource, Deref, DerefMut)]
struct ShapeStrokeTessellator(StrokeTessellator);

impl Plugin for ShapePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<ScreenSpaceShapeMaterial>::default())
            .insert_resource(ShapeFillTessellator(FillTessellator::new()))
            .insert_resource(ShapeStrokeTessellator(StrokeTessellator::new()))
            .configure_sets(
                PostUpdate,
                BuildShapes
                    .after(bevy::transform::TransformSystems::Propagate)
                    .before(bevy::asset::AssetEventSystems),
            )
            .add_systems(PostUpdate, build_shape_meshes.in_set(BuildShapes));

        let _ = app
            .world_mut()
            .resource_mut::<Assets<ScreenSpaceShapeMaterial>>()
            .insert(&SHAPE_MATERIAL_HANDLE, ScreenSpaceShapeMaterial {});
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub struct BuildShapes;

#[derive(Bundle, Default)]
pub struct ShapeBundle {
    shape: Shape,
    transform: Transform,
    visibility: Visibility,
}

impl ShapeBundle {
    pub fn new(path: Path, transform: Transform, visibility: Visibility) -> Self {
        let mut shape = Shape::default();
        shape.path = path;
        Self {
            shape,
            transform,
            visibility,
        }
    }
}

pub struct GeometryBuilder {
    builder: Builder,
}

impl GeometryBuilder {
    pub fn new() -> Self {
        Self {
            builder: Builder::new(),
        }
    }

    pub fn add(mut self, geometry: &impl Geometry<Builder>) -> Self {
        geometry.add_geometry(&mut self.builder);
        self
    }

    pub fn begin(mut self, at: Vec2) -> Self {
        self.builder.begin(point(at.x, at.y));
        self
    }

    pub fn line_to(mut self, to: Vec2) -> Self {
        self.builder.line_to(point(to.x, to.y));
        self
    }

    pub fn end(mut self, close: bool) -> Self {
        self.builder.end(close);
        self
    }

    pub fn close(mut self) -> Self {
        self.builder.close();
        self
    }

    pub fn build(self) -> Path {
        self.builder.build()
    }

    pub fn build_as(geometry: &impl Geometry<Builder>) -> Path {
        let mut builder = Builder::new();
        geometry.add_geometry(&mut builder);
        builder.build()
    }
}

#[derive(Clone, Copy)]
struct ShapeVertex {
    position: [f32; 2],
    color: [f32; 4],
    screen_offset: [f32; 2],
}

type ShapeVertexBuffers = VertexBuffers<ShapeVertex, u32>;
type FillVertexBuffers = VertexBuffers<Point, u32>;

struct PointVertexBuilder;

impl FillVertexConstructor<Point> for PointVertexBuilder {
    fn new_vertex(&mut self, vertex: FillVertex<'_>) -> Point {
        vertex.position()
    }
}

struct StrokeVertexBuilder {
    color: Color,
    pixel_width: f32,
    alignment: StrokeAlignment,
}

impl StrokeVertexConstructor<ShapeVertex> for StrokeVertexBuilder {
    fn new_vertex(&mut self, vertex: StrokeVertex<'_, '_>) -> ShapeVertex {
        let point = vertex.position_on_path();
        let screen_offset = stroke_screen_offset(
            vertex.normal(),
            vertex.side().is_positive(),
            self.pixel_width,
            self.alignment,
        );
        ShapeVertex {
            position: [point.x, point.y],
            color: self.color.to_linear().to_f32_array(),
            screen_offset: [screen_offset.x, screen_offset.y],
        }
    }
}

fn stroke_screen_offset(
    mut normal: bevy_prototype_lyon::prelude::tess::math::Vector,
    positive_side: bool,
    pixel_width: f32,
    alignment: StrokeAlignment,
) -> bevy_prototype_lyon::prelude::tess::math::Vector {
    // Lyon's `normal` is a miter extrusion vector, not a unit normal. Its
    // length can become enormous at acute joins. That is appropriate when
    // tessellating in object space, but here the shader interprets the vector
    // length as physical pixels, so cap it before applying the pixel width.
    let length_squared = normal.square_length();
    let maximum_length_squared = MAX_SCREEN_SPACE_JOIN_SCALE.powi(2);
    if length_squared > maximum_length_squared {
        normal *= MAX_SCREEN_SPACE_JOIN_SCALE / length_squared.sqrt();
    }

    match alignment {
        StrokeAlignment::Center => normal * (pixel_width * 0.5),
        StrokeAlignment::Inward if positive_side => normal * pixel_width,
        StrokeAlignment::Inward => bevy_prototype_lyon::prelude::tess::math::vector(0.0, 0.0),
    }
}

fn tessellate_fill(
    tessellator: &mut FillTessellator,
    path: &Path,
    options: FillOptions,
) -> FillVertexBuffers {
    let mut buffers = FillVertexBuffers::new();
    if let Err(error) = tessellator.tessellate_path(
        path,
        &options,
        &mut BuffersBuilder::new(&mut buffers, PointVertexBuilder),
    ) {
        error!("FillTessellator error: {error:?}");
    }
    buffers
}

fn append_fill(buffers: &mut ShapeVertexBuffers, fill: &FillVertexBuffers, color: Color) {
    let first_vertex = buffers.vertices.len() as u32;
    let color = color.to_linear().to_f32_array();
    buffers
        .indices
        .extend(fill.indices.iter().map(|index| first_vertex + index));
    buffers
        .vertices
        .extend(fill.vertices.iter().map(|position| ShapeVertex {
            position: [position.x, position.y],
            color,
            screen_offset: [0.0; 2],
        }));
}

#[derive(Clone, Copy)]
struct BoundaryEdge {
    from: u32,
    to: u32,
}

/// Tessellates a path using the standard fill rule and returns its occupied
/// region as closed contours, each directed with filled material on its left.
pub(crate) fn filled_path_boundary_contours(path: &Path) -> Option<Vec<Vec<Vec2>>> {
    let fill = tessellate_fill(&mut FillTessellator::new(), path, FillOptions::default());
    filled_boundary_contours(&fill)
}

fn filled_boundary_path(fill: &FillVertexBuffers) -> Option<Path> {
    let mut contours = filled_boundary_contours(fill)?;
    remove_small_foldover_contours(&mut contours);
    let mut builder = GeometryBuilder::new();
    for contour in contours {
        builder = builder.begin(contour[0]);
        for point in contour.iter().skip(1).copied() {
            builder = builder.line_to(point);
        }
        builder = builder.close();
    }
    Some(builder.build())
}

/// A centered tooth profile can fold across an acute corner and Lyon then
/// represents the fold as a tiny detached triangle or quad. The fill is too
/// small to notice, but outlining that component at a fixed pixel width makes
/// it look like a bright shard. Suppress only these simple, negligible
/// positive-area components from the visual border; holes and detailed small
/// components are retained.
fn remove_small_foldover_contours(contours: &mut Vec<Vec<Vec2>>) {
    let largest_positive_area = contours
        .iter()
        .map(|contour| contour_signed_area(contour))
        .filter(|area| *area > 0.0)
        .fold(0.0, f32::max);
    let maximum_artifact_area = largest_positive_area * BOUNDARY_ARTIFACT_AREA_RATIO;
    contours.retain(|contour| {
        let area = contour_signed_area(contour);
        area <= 0.0
            || contour.len() > MAX_BOUNDARY_ARTIFACT_VERTICES
            || area >= maximum_artifact_area
    });
}

fn contour_signed_area(contour: &[Vec2]) -> f32 {
    contour
        .iter()
        .zip(contour.iter().cycle().skip(1))
        .map(|(from, to)| from.perp_dot(*to))
        .sum::<f32>()
        * 0.5
}

/// Builds the actual boundary of a filled region from its triangles. Triangle
/// edges are directed with material on their left; shared internal edges cancel.
fn filled_boundary_contours(fill: &FillVertexBuffers) -> Option<Vec<Vec<Vec2>>> {
    let (points, canonical_indices, merge_distance) = canonical_fill_vertices(&fill.vertices);
    let mut triangle_edges = Vec::new();

    for triangle in fill.indices.chunks_exact(3) {
        let mut triangle = [
            canonical_indices[triangle[0] as usize],
            canonical_indices[triangle[1] as usize],
            canonical_indices[triangle[2] as usize],
        ];
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
            continue;
        }
        let a = point_vec(points[triangle[0] as usize]);
        let b = point_vec(points[triangle[1] as usize]);
        let c = point_vec(points[triangle[2] as usize]);
        let signed_area_twice = (b - a).perp_dot(c - a);
        if signed_area_twice.abs() <= f32::EPSILON {
            continue;
        }
        if signed_area_twice < 0.0 {
            triangle.swap(1, 2);
        }

        for (from, to) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            triangle_edges.push((from, to));
        }
    }

    // Cancel shared triangle edges. The remaining directed edges form a
    // balanced graph with filled material on their left.
    let mut unsplit_balances = HashMap::<(u32, u32), i32>::new();
    for (from, to) in triangle_edges {
        let key = if from < to { (from, to) } else { (to, from) };
        *unsplit_balances.entry(key).or_default() += if from < to { 1 } else { -1 };
    }
    let unmatched_edges = unsplit_balances
        .into_iter()
        .filter_map(|((from, to), balance)| match balance.cmp(&0) {
            std::cmp::Ordering::Greater => Some((from, to)),
            std::cmp::Ordering::Less => Some((to, from)),
            std::cmp::Ordering::Equal => None,
        })
        .collect::<Vec<_>>();
    let unmatched_boundary_edges = unmatched_edges
        .iter()
        .map(|(from, to)| BoundaryEdge {
            from: *from,
            to: *to,
        })
        .collect::<Vec<_>>();
    let mut split_balances = HashMap::<(u32, u32), i32>::new();
    for (from, to) in split_edges_at_vertices(&points, &unmatched_edges, merge_distance) {
        let key = if from < to { (from, to) } else { (to, from) };
        *split_balances.entry(key).or_default() += if from < to { 1 } else { -1 };
    }
    let split_boundary_edges = split_balances
        .into_iter()
        .filter_map(|((from, to), balance)| match balance.cmp(&0) {
            std::cmp::Ordering::Greater => Some(BoundaryEdge { from, to }),
            std::cmp::Ordering::Less => Some(BoundaryEdge { from: to, to: from }),
            std::cmp::Ordering::Equal => None,
        })
        .collect::<Vec<_>>();

    // T-junction splitting removes internal seams in ordinary contours and
    // holes. Near dense self-intersections, however, approximate collinearity
    // can split the wrong edge and unbalance the graph, which discards the
    // main outline. Use the repaired topology only when it is still balanced.
    let mut boundary_edges = if boundary_edges_are_balanced(&split_boundary_edges) {
        split_boundary_edges
    } else {
        unmatched_boundary_edges
    };
    boundary_edges.sort_unstable_by_key(|edge| (edge.from, edge.to));
    trace_boundary_contours(&points, &boundary_edges)
}

fn canonical_fill_vertices(vertices: &[Point]) -> (Vec<Point>, Vec<u32>, f32) {
    let minimum = vertices
        .iter()
        .fold(Vec2::splat(f32::INFINITY), |bounds, point| {
            bounds.min(point_vec(*point))
        });
    let maximum = vertices
        .iter()
        .fold(Vec2::splat(f32::NEG_INFINITY), |bounds, point| {
            bounds.max(point_vec(*point))
        });
    let extent = (maximum - minimum).max_element();
    let coordinate_scale = minimum.abs().max(maximum.abs()).max_element();
    let merge_distance = (extent * BOUNDARY_VERTEX_MERGE_RATIO)
        .max(coordinate_scale * f32::EPSILON * BOUNDARY_VERTEX_MERGE_ULPS)
        .max(f32::MIN_POSITIVE);
    let mut points_by_cell = HashMap::<(i64, i64), Vec<u32>>::new();
    let mut points = Vec::new();
    let canonical_indices = vertices
        .iter()
        .map(|vertex| {
            let normalized_x = if vertex.x == 0.0 { 0.0 } else { vertex.x };
            let normalized_y = if vertex.y == 0.0 { 0.0 } else { vertex.y };
            let vertex = point(normalized_x, normalized_y);
            let cell = (
                (normalized_x / merge_distance).floor() as i64,
                (normalized_y / merge_distance).floor() as i64,
            );
            for cell_y in cell.1 - 1..=cell.1 + 1 {
                for cell_x in cell.0 - 1..=cell.0 + 1 {
                    if let Some(index) = points_by_cell
                        .get(&(cell_x, cell_y))
                        .into_iter()
                        .flatten()
                        .copied()
                        .find(|index| {
                            point_vec(points[*index as usize]).distance_squared(point_vec(vertex))
                                <= merge_distance.powi(2)
                        })
                    {
                        return index;
                    }
                }
            }

            let index = points.len() as u32;
            points.push(vertex);
            points_by_cell.entry(cell).or_default().push(index);
            index
        })
        .collect();
    (points, canonical_indices, merge_distance)
}

fn boundary_edges_are_balanced(edges: &[BoundaryEdge]) -> bool {
    let mut balances = HashMap::<u32, i32>::new();
    for edge in edges {
        *balances.entry(edge.from).or_default() += 1;
        *balances.entry(edge.to).or_default() -= 1;
    }
    balances.into_values().all(|balance| balance == 0)
}

/// Lyon can emit a T-junction where one triangle has a long edge and its
/// neighbor has several collinear edge segments. Split the long edge at all
/// such vertices so shared material edges can cancel segment-for-segment.
fn split_edges_at_vertices(
    points: &[Point],
    edges: &[(u32, u32)],
    tolerance: f32,
) -> Vec<(u32, u32)> {
    let mut by_x = points
        .iter()
        .enumerate()
        .map(|(index, point)| (point.x, index as u32))
        .collect::<Vec<_>>();
    let mut by_y = points
        .iter()
        .enumerate()
        .map(|(index, point)| (point.y, index as u32))
        .collect::<Vec<_>>();
    by_x.sort_unstable_by(|left, right| left.0.total_cmp(&right.0));
    by_y.sort_unstable_by(|left, right| left.0.total_cmp(&right.0));

    let mut split = Vec::new();
    for &(from, to) in edges {
        let a = point_vec(points[from as usize]);
        let b = point_vec(points[to as usize]);
        let edge = b - a;
        let length_squared = edge.length_squared();
        if length_squared <= f32::EPSILON {
            continue;
        }
        let length = length_squared.sqrt();
        let sorted = if edge.x.abs() >= edge.y.abs() {
            &by_x
        } else {
            &by_y
        };
        let (start_coordinate, end_coordinate) = if edge.x.abs() >= edge.y.abs() {
            (a.x, b.x)
        } else {
            (a.y, b.y)
        };
        let lower = start_coordinate.min(end_coordinate) - tolerance;
        let upper = start_coordinate.max(end_coordinate) + tolerance;
        let first = sorted.partition_point(|(coordinate, _)| *coordinate < lower);
        let last = sorted.partition_point(|(coordinate, _)| *coordinate <= upper);
        let mut vertices = sorted[first..last]
            .iter()
            .filter_map(|(_, index)| {
                let point = point_vec(points[*index as usize]);
                let along = (point - a).dot(edge) / length_squared;
                ((-f32::EPSILON..=1.0 + f32::EPSILON).contains(&along)
                    && edge.perp_dot(point - a).abs() <= tolerance * length)
                    .then_some((along.clamp(0.0, 1.0), *index))
            })
            .collect::<Vec<_>>();
        vertices.push((0.0, from));
        vertices.push((1.0, to));
        vertices.sort_unstable_by(|left, right| left.0.total_cmp(&right.0));
        vertices.dedup_by_key(|(_, index)| *index);
        split.extend(vertices.windows(2).filter_map(|vertices| {
            let from = vertices[0].1;
            let to = vertices[1].1;
            (from != to).then_some((from, to))
        }));
    }
    split
}

fn trace_boundary_contours(points: &[Point], edges: &[BoundaryEdge]) -> Option<Vec<Vec<Vec2>>> {
    let mut incident = HashMap::<u32, (Vec<usize>, Vec<usize>)>::new();
    for (index, edge) in edges.iter().enumerate() {
        incident.entry(edge.to).or_default().0.push(index);
        incident.entry(edge.from).or_default().1.push(index);
    }

    // Pair incoming and outgoing half-edges bijectively at every vertex. A
    // per-edge "closest clockwise" lookup can choose the same outgoing edge
    // for multiple incoming edges at a non-manifold junction; subsequent
    // walks then consume that edge and leave the main outline open. Preserving
    // circular order while choosing the lowest-cost rotation gives the usual
    // clockwise successor rule and guarantees disjoint cycles.
    let mut successors = vec![None; edges.len()];
    for (vertex, (mut incoming, mut outgoing)) in incident {
        let origin = point_vec(points[vertex as usize]);
        incoming.sort_unstable_by(|left, right| {
            let left_direction = point_vec(points[edges[*left].from as usize]) - origin;
            let right_direction = point_vec(points[edges[*right].from as usize]) - origin;
            left_direction
                .y
                .atan2(left_direction.x)
                .total_cmp(&right_direction.y.atan2(right_direction.x))
                .then_with(|| left.cmp(right))
        });
        outgoing.sort_unstable_by(|left, right| {
            let left_direction = point_vec(points[edges[*left].to as usize]) - origin;
            let right_direction = point_vec(points[edges[*right].to as usize]) - origin;
            left_direction
                .y
                .atan2(left_direction.x)
                .total_cmp(&right_direction.y.atan2(right_direction.x))
                .then_with(|| left.cmp(right))
        });

        if incoming.len() == outgoing.len() && !incoming.is_empty() {
            let count = incoming.len();
            let best_shift = (0..count)
                .min_by(|left_shift, right_shift| {
                    let pairing_cost = |shift: usize| {
                        incoming
                            .iter()
                            .enumerate()
                            .map(|(index, incoming_edge)| {
                                let outgoing_edge = outgoing[(index + shift) % count];
                                let reverse_incoming =
                                    point_vec(points[edges[*incoming_edge].from as usize]) - origin;
                                let outgoing_direction =
                                    point_vec(points[edges[outgoing_edge].to as usize]) - origin;
                                clockwise_angle(reverse_incoming, outgoing_direction)
                            })
                            .sum::<f32>()
                    };
                    pairing_cost(*left_shift).total_cmp(&pairing_cost(*right_shift))
                })
                .unwrap();
            for (index, incoming_edge) in incoming.into_iter().enumerate() {
                successors[incoming_edge] = Some(outgoing[(index + best_shift) % count]);
            }
        } else {
            debug!(
                "Unbalanced filled-boundary vertex {vertex}: {} incoming, {} outgoing",
                incoming.len(),
                outgoing.len()
            );
            let mut remaining = outgoing;
            for incoming_edge in incoming {
                let reverse_incoming =
                    point_vec(points[edges[incoming_edge].from as usize]) - origin;
                let Some((remaining_index, _)) =
                    remaining
                        .iter()
                        .enumerate()
                        .min_by(|(_, left), (_, right)| {
                            let left_direction =
                                point_vec(points[edges[**left].to as usize]) - origin;
                            let right_direction =
                                point_vec(points[edges[**right].to as usize]) - origin;
                            clockwise_angle(reverse_incoming, left_direction)
                                .total_cmp(&clockwise_angle(reverse_incoming, right_direction))
                        })
                else {
                    break;
                };
                successors[incoming_edge] = Some(remaining.swap_remove(remaining_index));
            }
        }
    }

    let mut used = vec![false; edges.len()];
    let mut contours = Vec::new();
    let mut discarded_open_contours = 0;
    for first_edge in 0..edges.len() {
        if used[first_edge] {
            continue;
        }
        let start = edges[first_edge].from;
        let mut current_edge = first_edge;
        let mut contour = vec![point_vec(points[start as usize])];
        let mut closed = false;

        for _ in 0..=edges.len() {
            if used[current_edge] {
                break;
            }
            used[current_edge] = true;
            let edge = edges[current_edge];
            if edge.to == start {
                closed = true;
                break;
            }
            contour.push(point_vec(points[edge.to as usize]));
            let Some(next_edge) = successors[current_edge] else {
                break;
            };
            current_edge = next_edge;
        }

        if closed && contour.len() >= 3 {
            contours.push(contour);
        } else {
            discarded_open_contours += 1;
        }
    }

    if discarded_open_contours > 0 {
        debug!("Discarded {discarded_open_contours} open filled-boundary edge walks");
    }
    (!contours.is_empty()).then_some(contours)
}

fn clockwise_angle(from: Vec2, to: Vec2) -> f32 {
    (-from.perp_dot(to).atan2(from.dot(to))).rem_euclid(std::f32::consts::TAU)
}

fn point_vec(point: Point) -> Vec2 {
    Vec2::new(point.x, point.y)
}

fn build_shape_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fill_tessellator: ResMut<ShapeFillTessellator>,
    mut stroke_tessellator: ResMut<ShapeStrokeTessellator>,
    changed_shapes: Query<Entity, Or<(Changed<Shape>, Changed<Fill>, Changed<Stroke>)>>,
    mut removed_shapes: RemovedComponents<Shape>,
    mut removed_fills: RemovedComponents<Fill>,
    mut removed_strokes: RemovedComponents<Stroke>,
    mut shapes: Query<(&Shape, Option<&Fill>, Option<&Stroke>, &mut Mesh2d)>,
    mut meshes_without_shapes: Query<&mut Mesh2d, Without<Shape>>,
) {
    for entity in removed_shapes.read() {
        let Ok(mut mesh) = meshes_without_shapes.get_mut(entity) else {
            continue;
        };
        let placeholder = transparent_placeholder_mesh();
        if let Some(mut existing_mesh) = meshes.get_mut(&mesh.0) {
            *existing_mesh = placeholder;
        } else {
            mesh.0 = meshes.add(placeholder);
        }
        commands.entity(entity).remove::<NoFrustumCulling>();
    }

    let entities = changed_shapes
        .iter()
        .chain(removed_fills.read())
        .chain(removed_strokes.read())
        .collect::<HashSet<_>>();

    for entity in entities {
        let Ok((shape, fill, stroke, mut mesh)) = shapes.get_mut(entity) else {
            continue;
        };
        let mut buffers = ShapeVertexBuffers::new();
        let visible_fill = fill.filter(|fill| fill.color.alpha() > 0.0);
        let visible_stroke =
            stroke.filter(|stroke| stroke.width_px > 0.0 && stroke.color.alpha() > 0.0);
        let needs_filled_boundary =
            visible_stroke.is_some_and(|stroke| stroke.alignment == StrokeAlignment::Inward);
        let fill_geometry = (visible_fill.is_some() || needs_filled_boundary).then(|| {
            tessellate_fill(
                &mut fill_tessellator,
                &shape.path,
                fill.map_or_else(FillOptions::default, |fill| fill.options),
            )
        });

        if let (Some(fill), Some(fill_geometry)) = (visible_fill, fill_geometry.as_ref()) {
            append_fill(&mut buffers, fill_geometry, fill.color);
        }

        let inward_boundary = visible_stroke
            .filter(|stroke| stroke.alignment == StrokeAlignment::Inward)
            .and_then(|_| fill_geometry.as_ref())
            .and_then(filled_boundary_path);
        let stroke_path = visible_stroke.map(|_| inward_boundary.as_ref().unwrap_or(&shape.path));

        if let (Some(stroke), Some(stroke_path)) = (visible_stroke, stroke_path) {
            // The actual width is applied in screen pixels by the vertex shader.
            // A tiny path-relative tessellation width keeps Lyon's join topology
            // stable without tying the mesh to camera zoom.
            let options = StrokeOptions::default()
                .with_tolerance(crate::STROKE_TOLERANCE)
                .with_line_width(stroke_topology_width(stroke_path));
            if let Err(error) = stroke_tessellator.tessellate_path(
                stroke_path,
                &options,
                &mut BuffersBuilder::new(
                    &mut buffers,
                    StrokeVertexBuilder {
                        color: stroke.color,
                        pixel_width: stroke.width_px,
                        alignment: stroke.alignment,
                    },
                ),
            ) {
                error!("StrokeTessellator error: {error:?}");
            }
        }

        let can_escape_cpu_bounds = visible_stroke.is_some_and(|stroke| {
            stroke.alignment == StrokeAlignment::Center || inward_boundary.is_none()
        });
        if can_escape_cpu_bounds {
            commands.entity(entity).insert(NoFrustumCulling);
        } else {
            commands.entity(entity).remove::<NoFrustumCulling>();
        }

        let new_mesh = shape_mesh(&buffers);
        if let Some(mut existing_mesh) = meshes.get_mut(&mesh.0) {
            *existing_mesh = new_mesh;
        } else {
            mesh.0 = meshes.add(new_mesh);
        }
    }
}

fn stroke_topology_width(path: &Path) -> f32 {
    let mut minimum = bevy_prototype_lyon::prelude::tess::math::point(f32::INFINITY, f32::INFINITY);
    let mut maximum =
        bevy_prototype_lyon::prelude::tess::math::point(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for event in path.iter() {
        for point in [event.from(), event.to()] {
            minimum.x = minimum.x.min(point.x);
            minimum.y = minimum.y.min(point.y);
            maximum.x = maximum.x.max(point.x);
            maximum.y = maximum.y.max(point.y);
        }
    }
    let path_extent = (maximum.x - minimum.x).max(maximum.y - minimum.y).max(0.0);
    (path_extent * STROKE_TOPOLOGY_WIDTH_RATIO).max(MIN_STROKE_TOPOLOGY_WIDTH)
}

fn shape_mesh(buffers: &ShapeVertexBuffers) -> Mesh {
    if buffers.vertices.is_empty() {
        return transparent_placeholder_mesh();
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_indices(Indices::U32(buffers.indices.clone()));
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        buffers
            .vertices
            .iter()
            .map(|vertex| [vertex.position[0], vertex.position[1], 0.0])
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        buffers
            .vertices
            .iter()
            .map(|vertex| vertex.color)
            .collect::<Vec<_>>(),
    );
    mesh.insert_attribute(
        ATTRIBUTE_SCREEN_OFFSET,
        buffers
            .vertices
            .iter()
            .map(|vertex| vertex.screen_offset)
            .collect::<Vec<_>>(),
    );
    mesh
}

fn transparent_placeholder_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0, 0.0, 0.0, 0.0]; 3]);
    mesh.insert_attribute(ATTRIBUTE_SCREEN_OFFSET, vec![[0.0, 0.0]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::gear::gearify_path;
    use crate::tools::gear::{GearOutline, GearSettings};
    use bevy::ecs::system::RunSystemOnce;
    use bevy_prototype_lyon::prelude::tess::math::vector;

    #[test]
    fn inward_strokes_leave_the_boundary_side_fixed() {
        let normal = vector(0.6, -0.8);

        assert_eq!(
            stroke_screen_offset(normal, false, 5.0, StrokeAlignment::Inward),
            vector(0.0, 0.0)
        );
        assert_eq!(
            stroke_screen_offset(normal, true, 5.0, StrokeAlignment::Inward),
            normal * 5.0
        );
    }

    #[test]
    fn screen_space_strokes_cap_acute_miter_spikes() {
        let long_miter = vector(10.0, 0.0);
        let offset = stroke_screen_offset(long_miter, true, 5.0, StrokeAlignment::Inward);

        assert!((offset.x - MAX_SCREEN_SPACE_JOIN_SCALE * 5.0).abs() < 1.0e-5);
        assert_eq!(offset.y, 0.0);
    }

    #[test]
    fn empty_shapes_still_produce_an_allocatable_mesh() {
        let mesh = shape_mesh(&ShapeVertexBuffers::new());

        assert!(mesh.get_vertex_buffer_size() > 0);
        assert!(
            mesh.get_index_buffer_bytes()
                .is_some_and(|data| !data.is_empty())
        );
    }

    #[test]
    fn filled_boundary_orients_opposite_figure_eight_lobes_toward_material() {
        let path = GeometryBuilder::new()
            .begin(Vec2::ZERO)
            .line_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(-2.0, 0.0))
            .line_to(Vec2::new(-1.0, 1.0))
            .line_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, -1.0))
            .line_to(Vec2::new(2.0, 0.0))
            .line_to(Vec2::new(1.0, 1.0))
            .close()
            .build();
        let mut tessellator = FillTessellator::new();
        let fill = tessellate_fill(&mut tessellator, &path, FillOptions::default());
        let boundary = filled_boundary_path(&fill).expect("figure eight should have a boundary");
        let contour_areas = path_contour_areas(&boundary);

        assert_eq!(contour_areas.len(), 2);
        assert!(contour_areas.iter().all(|area| *area > 0.0));
    }

    #[test]
    fn filled_boundary_splits_a_crossing_bow_tie_into_material_facing_lobes() {
        let path = GeometryBuilder::new()
            .begin(Vec2::new(-2.0, -1.0))
            .line_to(Vec2::new(2.0, 1.0))
            .line_to(Vec2::new(-2.0, 1.0))
            .line_to(Vec2::new(2.0, -1.0))
            .close()
            .build();
        let mut tessellator = FillTessellator::new();
        let fill = tessellate_fill(&mut tessellator, &path, FillOptions::default());
        let boundary = filled_boundary_path(&fill).expect("bow tie should have a boundary");
        let contour_areas = path_contour_areas(&boundary);

        assert_eq!(contour_areas.len(), 2);
        assert!(contour_areas.iter().all(|area| *area > 0.0));
    }

    #[test]
    fn filled_boundary_keeps_gear_holes_facing_toward_material() {
        let outline = GearOutline::from_radius(
            2.0,
            GearSettings {
                internal: true,
                ..Default::default()
            },
        )
        .unwrap();
        let mut tessellator = FillTessellator::new();
        let fill = tessellate_fill(&mut tessellator, &outline.path(), FillOptions::default());
        let boundary = filled_boundary_path(&fill).expect("gear should have a boundary");
        let contour_areas = path_contour_areas(&boundary);

        assert_eq!(contour_areas.len(), 2);
        assert_eq!(contour_areas.iter().filter(|area| **area > 0.0).count(), 1);
        assert_eq!(contour_areas.iter().filter(|area| **area < 0.0).count(), 1);
    }

    #[test]
    fn gearified_figure_eight_strokes_have_bounded_screen_offsets() {
        let touching = GeometryBuilder::new()
            .begin(Vec2::ZERO)
            .line_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(-2.0, 0.0))
            .line_to(Vec2::new(-1.0, 1.0))
            .line_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, -1.0))
            .line_to(Vec2::new(2.0, 0.0))
            .line_to(Vec2::new(1.0, 1.0))
            .close()
            .build();
        let crossing = GeometryBuilder::new()
            .begin(Vec2::new(-2.0, -1.0))
            .line_to(Vec2::new(2.0, 1.0))
            .line_to(Vec2::new(-2.0, 1.0))
            .line_to(Vec2::new(2.0, -1.0))
            .close()
            .build();

        for source in [touching, crossing] {
            let gearified = gearify_path(&source, 0.2).unwrap();
            let mut fill_tessellator = FillTessellator::new();
            let fill = tessellate_fill(
                &mut fill_tessellator,
                &gearified,
                FillOptions::default().with_tolerance(crate::STROKE_TOLERANCE),
            );
            let boundary = filled_boundary_path(&fill).expect("gearified figure eight boundary");
            let contour_areas = path_contour_areas(&boundary);
            assert_eq!(contour_areas.len(), 2, "unexpected outlined tooth islands");
            assert!(contour_areas.iter().all(|area| *area > 1.0));
            let mut buffers = ShapeVertexBuffers::new();
            let options = StrokeOptions::default()
                .with_tolerance(crate::STROKE_TOLERANCE)
                .with_line_width(stroke_topology_width(&boundary));
            StrokeTessellator::new()
                .tessellate_path(
                    &boundary,
                    &options,
                    &mut BuffersBuilder::new(
                        &mut buffers,
                        StrokeVertexBuilder {
                            color: Color::WHITE,
                            pixel_width: 5.0,
                            alignment: StrokeAlignment::Inward,
                        },
                    ),
                )
                .unwrap();
            let maximum_offset = buffers
                .vertices
                .iter()
                .map(|vertex| Vec2::from(vertex.screen_offset).length())
                .fold(0.0, f32::max);

            assert!(
                maximum_offset <= 5.0 * std::f32::consts::SQRT_2 * 1.001,
                "screen-space join spike: {maximum_offset}px"
            );
        }
    }

    #[test]
    fn fill_and_stroke_components_drive_meshes_without_shape_state_copies() {
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(ShapeFillTessellator(FillTessellator::new()));
        world.insert_resource(ShapeStrokeTessellator(StrokeTessellator::new()));
        let entity = world
            .spawn((
                Shape {
                    path: GeometryBuilder::build_as(&shapes::Rectangle {
                        extents: Vec2::ONE,
                        ..Default::default()
                    }),
                },
                Fill::default(),
                Mesh2d::default(),
            ))
            .id();

        world.run_system_once(build_shape_meshes).unwrap();
        world.flush();
        let mesh_handle = world.entity(entity).get::<Mesh2d>().unwrap().0.clone();
        assert!(
            world
                .resource::<Assets<Mesh>>()
                .get(&mesh_handle)
                .unwrap()
                .count_vertices()
                > 3
        );

        world.entity_mut(entity).remove::<Fill>();
        world.run_system_once(build_shape_meshes).unwrap();
        world.flush();
        assert_eq!(
            world
                .resource::<Assets<Mesh>>()
                .get(&mesh_handle)
                .unwrap()
                .count_vertices(),
            3
        );

        world.entity_mut(entity).insert(Fill::default());
        world.run_system_once(build_shape_meshes).unwrap();
        world.flush();
        assert!(
            world
                .resource::<Assets<Mesh>>()
                .get(&mesh_handle)
                .unwrap()
                .count_vertices()
                > 3
        );

        world.entity_mut(entity).remove::<Shape>();
        world.run_system_once(build_shape_meshes).unwrap();
        world.flush();
        assert_eq!(
            world
                .resource::<Assets<Mesh>>()
                .get(&mesh_handle)
                .unwrap()
                .count_vertices(),
            3
        );
    }

    #[test]
    fn fully_transparent_shapes_emit_only_a_culled_placeholder() {
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(ShapeFillTessellator(FillTessellator::new()));
        world.insert_resource(ShapeStrokeTessellator(StrokeTessellator::new()));
        let entity = world
            .spawn((
                Shape {
                    path: GeometryBuilder::new()
                        .begin(Vec2::ZERO)
                        .line_to(Vec2::X)
                        .end(false)
                        .build(),
                },
                Fill {
                    color: Color::NONE,
                    ..Default::default()
                },
                Stroke {
                    color: Color::NONE,
                    width_px: 5.0,
                    alignment: StrokeAlignment::Center,
                },
                Mesh2d::default(),
            ))
            .id();

        world.run_system_once(build_shape_meshes).unwrap();
        world.flush();

        assert!(!world.entity(entity).contains::<NoFrustumCulling>());
        let mesh_handle = &world.entity(entity).get::<Mesh2d>().unwrap().0;
        assert_eq!(
            world
                .resource::<Assets<Mesh>>()
                .get(mesh_handle)
                .unwrap()
                .count_vertices(),
            3
        );
    }

    fn path_contour_areas(path: &Path) -> Vec<f32> {
        let mut contours = Vec::new();
        let mut current = Vec::new();
        for event in path.iter() {
            if current.is_empty() {
                current.push(point_vec(event.from()));
            }
            current.push(point_vec(event.to()));
            if matches!(
                event,
                bevy_prototype_lyon::prelude::tess::path::PathEvent::End { .. }
            ) {
                let area = current
                    .iter()
                    .copied()
                    .zip(current.iter().copied().cycle().skip(1))
                    .take(current.len())
                    .map(|(from, to)| from.perp_dot(to))
                    .sum::<f32>()
                    * 0.5;
                contours.push(area);
                current.clear();
            }
        }
        contours
    }
}
