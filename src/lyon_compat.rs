use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
pub use bevy_prototype_lyon::entity::Shape;
use bevy_prototype_lyon::geometry::Geometry;
use bevy_prototype_lyon::prelude::tess::path::{Path, math::point, path::Builder};

pub use bevy_prototype_lyon::prelude::{
    FillOptions, RectangleOrigin, ShapePlugin, StrokeOptions, shapes,
};

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
    pub options: StrokeOptions,
    pub color: Color,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            options: StrokeOptions::default(),
            color: Color::BLACK,
        }
    }
}

impl From<Fill> for bevy_prototype_lyon::draw::Fill {
    fn from(fill: Fill) -> Self {
        Self {
            options: fill.options,
            color: fill.color,
        }
    }
}

impl From<Stroke> for bevy_prototype_lyon::draw::Stroke {
    fn from(stroke: Stroke) -> Self {
        Self {
            options: stroke.options,
            color: stroke.color,
        }
    }
}

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

pub fn sync_draw_components(
    mut shapes: Query<
        (&mut Shape, Option<&Fill>, Option<&Stroke>),
        Or<(Changed<Fill>, Changed<Stroke>)>,
    >,
) {
    for (mut shape, fill, stroke) in shapes.iter_mut() {
        if let Some(fill) = fill {
            shape.fill = Some((*fill).into());
        }
        if let Some(stroke) = stroke {
            shape.stroke = Some((*stroke).into());
        }
    }
}

pub fn sanitize_empty_shape_meshes(
    mut meshes: ResMut<Assets<Mesh>>,
    shapes: Query<&Mesh2d, Changed<Shape>>,
) {
    for mesh in &shapes {
        let Some(mut mesh_asset) = meshes.get_mut(&mesh.0) else {
            continue;
        };
        if mesh_asset.get_vertex_buffer_size() == 0 {
            *mesh_asset = transparent_placeholder_mesh();
        }
    }
}

fn transparent_placeholder_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0, 0.0, 0.0, 0.0]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}
