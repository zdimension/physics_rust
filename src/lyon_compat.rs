use bevy::prelude::*;
use bevy_prototype_lyon::geometry::Geometry;
pub use bevy_prototype_lyon::entity::Shape;
use bevy_prototype_lyon::prelude::tess::path::{path::Builder, Path};

pub use bevy_prototype_lyon::prelude::{shapes, FillOptions, RectangleOrigin, ShapePlugin, StrokeOptions};

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
    mut shapes: Query<(&mut Shape, Option<&Fill>, Option<&Stroke>), Or<(Changed<Fill>, Changed<Stroke>)>>,
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