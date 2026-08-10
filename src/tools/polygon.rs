use avian2d::parry::shape::{Compound, SharedShape, TriMesh};
use avian2d::prelude::{Collider, Rotation};
use bevy::prelude::*;
use bevy_prototype_lyon::prelude::tess::{
    FillOptions, FillTessellator,
    geometry_builder::{VertexBuffers, simple_builder},
    math::Point,
    path::Path,
};

use crate::lyon_compat::GeometryBuilder;

pub const MIN_PREVIEW_AREA_PX: f32 = 100.0;
pub const FREEHAND_SAMPLE_DISTANCE_PX: f32 = 2.0;

#[derive(Debug, Clone)]
pub struct PolygonPlacementState {
    pub overlay_ent: Entity,
    pub origin: Vec2,
    pub points: Vec<Vec2>,
}

impl PolygonPlacementState {
    pub fn new(overlay_ent: Entity, origin: Vec2) -> Self {
        Self {
            overlay_ent,
            origin,
            points: vec![Vec2::ZERO],
        }
    }

    pub fn push_world_point(&mut self, point: Vec2, minimum_distance: f32) -> bool {
        let point = point - self.origin;
        if self
            .points
            .last()
            .is_some_and(|last| last.distance_squared(point) < minimum_distance.powi(2))
        {
            return false;
        }
        self.points.push(point);
        true
    }

    pub fn screen_area(&self, camera_scale: f32) -> f32 {
        tessellate_polygon(&self.points)
            .map_or(0.0, |geometry| geometry.area / camera_scale.powi(2))
    }
}

pub struct PolygonGeometry {
    pub vertices: Vec<Vec2>,
    pub triangles: Vec<[u32; 3]>,
    pub area: f32,
}

impl PolygonGeometry {
    pub fn collider(&self) -> Collider {
        if let Ok(mesh) = TriMesh::new(self.vertices.clone(), self.triangles.clone())
            && let Some(compound) = Compound::decompose_trimesh(&mesh)
        {
            return Collider::from(SharedShape::new(compound));
        }

        Collider::compound(
            self.triangles
                .iter()
                .map(|triangle| {
                    (
                        Vec2::ZERO,
                        Rotation::IDENTITY,
                        Collider::triangle(
                            self.vertices[triangle[0] as usize],
                            self.vertices[triangle[1] as usize],
                            self.vertices[triangle[2] as usize],
                        ),
                    )
                })
                .collect(),
        )
    }
}

pub fn polygon_path(points: &[Vec2], closed: bool) -> Path {
    let Some(first) = points.first().copied() else {
        return GeometryBuilder::new().build();
    };
    let builder = points
        .iter()
        .skip(1)
        .copied()
        .fold(GeometryBuilder::new().begin(first), |builder, point| {
            builder.line_to(point)
        });
    if closed {
        builder.close().build()
    } else {
        builder.end(false).build()
    }
}

pub fn surfaces_path(surfaces: &[Vec<Vec2>]) -> Path {
    surfaces
        .iter()
        .filter_map(|surface| surface.split_first())
        .fold(GeometryBuilder::new(), |builder, (&first, rest)| {
            rest.iter()
                .copied()
                .fold(builder.begin(first), |builder, point| {
                    builder.line_to(point)
                })
                .close()
        })
        .build()
}

pub fn tessellate_polygon(points: &[Vec2]) -> Option<PolygonGeometry> {
    if points.len() < 3 {
        return None;
    }

    let path = polygon_path(points, true);
    tessellate_path(&path)
}

pub fn tessellate_path(path: &Path) -> Option<PolygonGeometry> {
    let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
    FillTessellator::new()
        .tessellate_path(
            path,
            &FillOptions::default(),
            &mut simple_builder(&mut buffers),
        )
        .ok()?;

    let vertices = buffers
        .vertices
        .into_iter()
        .map(|point| Vec2::new(point.x, point.y))
        .collect::<Vec<_>>();
    let triangles = buffers
        .indices
        .chunks_exact(3)
        .map(|triangle| {
            let mut triangle = [
                u32::from(triangle[0]),
                u32::from(triangle[1]),
                u32::from(triangle[2]),
            ];
            if (vertices[triangle[1] as usize] - vertices[triangle[0] as usize])
                .perp_dot(vertices[triangle[2] as usize] - vertices[triangle[0] as usize])
                < 0.0
            {
                triangle.swap(1, 2);
            }
            triangle
        })
        .filter(|triangle| {
            triangle_area(
                vertices[triangle[0] as usize],
                vertices[triangle[1] as usize],
                vertices[triangle[2] as usize],
            ) > f32::EPSILON
        })
        .collect::<Vec<_>>();
    let area: f32 = triangles
        .iter()
        .map(|triangle| {
            triangle_area(
                vertices[triangle[0] as usize],
                vertices[triangle[1] as usize],
                vertices[triangle[2] as usize],
            )
        })
        .sum();

    (!triangles.is_empty() && area.is_finite() && area > 0.0).then_some(PolygonGeometry {
        vertices,
        triangles,
        area,
    })
}

fn triangle_area(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    ((b - a).perp_dot(c - a) * 0.5).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_triangle_tessellates_to_its_expected_area() {
        let points = [Vec2::ZERO, Vec2::X, Vec2::ONE];

        assert!((tessellate_polygon(&points).unwrap().area - 0.5).abs() < 1.0e-5);
    }

    #[test]
    fn figure_eight_tessellates_both_enclosed_lobes() {
        let points = [
            Vec2::new(-2.0, 0.0),
            Vec2::new(-1.0, 1.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(-1.0, -1.0),
            Vec2::new(-2.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(0.0, 0.0),
        ];

        let geometry = tessellate_polygon(&points).unwrap();

        assert!((geometry.area - 4.0).abs() < 1.0e-4);
        assert!(!geometry.triangles.is_empty());
    }

    #[test]
    fn later_surfaces_cut_holes_from_the_first() {
        let path = surfaces_path(&[
            vec![
                Vec2::new(-2.0, -2.0),
                Vec2::new(2.0, -2.0),
                Vec2::new(2.0, 2.0),
                Vec2::new(-2.0, 2.0),
            ],
            vec![
                Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(-1.0, 1.0),
            ],
        ]);

        assert!((tessellate_path(&path).unwrap().area - 12.0).abs() < 1.0e-4);
    }

    #[test]
    fn repeated_sample_positions_are_ignored() {
        let mut state = PolygonPlacementState::new(Entity::PLACEHOLDER, Vec2::new(5.0, 3.0));

        assert!(!state.push_world_point(Vec2::new(5.0, 3.0), 0.1));
        assert!(state.push_world_point(Vec2::new(6.0, 3.0), 0.1));
        assert_eq!(state.points, [Vec2::ZERO, Vec2::X]);
    }

    #[test]
    fn collider_merges_adjacent_triangles_into_convex_parts() {
        let geometry = tessellate_polygon(&[
            Vec2::ZERO,
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 2.0),
        ])
        .unwrap();
        let triangle_count = geometry.triangles.len();
        let collider = geometry.collider();
        let avian2d::parry::shape::TypedShape::Compound(compound) =
            collider.shape().as_typed_shape()
        else {
            panic!("a freeform polygon should produce a compound collider");
        };

        assert!(
            compound.shapes().len() < triangle_count,
            "{} convex parts from {triangle_count} triangles; vertices={:?}; triangles={:?}",
            compound.shapes().len(),
            geometry.vertices,
            geometry.triangles,
        );
    }
}
