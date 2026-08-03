use std::f32::consts::FRAC_PI_2;

use avian2d::prelude::*;
use bevy::prelude::*;

use crate::lyon_compat::{GeometryBuilder, Shape, ShapeBundle, shapes};
use crate::mouse_tracking::MainCamera;
use crate::objects::ColorComponent;
use crate::objects::phy_obj::PhysicalProperties;
use crate::update_from::UpdateFrom;
use crate::{BORDER_WIDTH_PX, make_fill, make_stroke};

#[derive(Component, Copy, Clone, Debug, Default)]
pub struct PlaneObject;

#[derive(Component)]
pub(crate) struct PlaneVisual;

#[derive(Component)]
pub(crate) struct PlaneBoundary;

pub(crate) fn spawn_plane(
    commands: &mut Commands,
    scene: Entity,
    point: Vec2,
    outward_normal: Vec2,
    color: bevy_egui::egui::ecolor::Hsva,
) -> Entity {
    let outward_normal = outward_normal.normalize_or_zero();
    let outward_normal = if outward_normal == Vec2::ZERO {
        Vec2::Y
    } else {
        outward_normal
    };
    let angle = outward_normal.to_angle() - FRAC_PI_2;
    let entity = commands
        .spawn((
            PlaneObject,
            RigidBody::Static,
            Collider::half_space(Vec2::Y),
            PhysicalProperties::default().with_collision_layers(CollisionLayers::ALL),
            Position(point),
            Rotation::radians(angle),
            ChildOf(scene),
        ))
        .id();

    insert_plane_visual(commands, entity, point, angle, color);
    entity
}

pub(crate) fn insert_plane_preview(
    commands: &mut Commands,
    entity: Entity,
    point: Vec2,
    color: bevy_egui::egui::ecolor::Hsva,
) {
    insert_plane_visual(commands, entity, point, 0.0, color);
}

fn insert_plane_visual(
    commands: &mut Commands,
    entity: Entity,
    point: Vec2,
    angle: f32,
    color: bevy_egui::egui::ecolor::Hsva,
) {
    let empty_path = GeometryBuilder::new().build();
    commands.entity(entity).insert((
        PlaneVisual,
        ShapeBundle::new(
            empty_path.clone(),
            Transform::from_translation(point.extend(0.0))
                .with_rotation(Quat::from_rotation_z(angle)),
            Visibility::Inherited,
        ),
        ColorComponent(color),
        UpdateFrom::<ColorComponent>::This,
        make_fill(Color::WHITE),
    ));

    commands.entity(entity).with_children(|parent| {
        parent.spawn((
            PlaneBoundary,
            ShapeBundle::new(
                empty_path,
                Transform::from_translation(Vec3::Z * 0.25),
                Visibility::Inherited,
            ),
            make_stroke(Color::WHITE, BORDER_WIDTH_PX),
            UpdateFrom::<ColorComponent>::entity(entity),
        ));
    });
}

pub(crate) fn update_plane_visuals(
    cameras: Query<&Transform, With<MainCamera>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut planes: Query<(Entity, &Transform, &mut Shape), With<PlaneVisual>>,
    mut boundaries: Query<(&ChildOf, &mut Shape), (With<PlaneBoundary>, Without<PlaneVisual>)>,
) {
    let (Ok(camera), Ok(window)) = (cameras.single(), windows.single()) else {
        return;
    };
    let view_radius =
        0.5 * Vec2::new(window.width(), window.height()).length() * camera.scale.x.abs();
    let camera_pos = camera.translation.truncate();

    for (plane_entity, transform, mut fill_shape) in &mut planes {
        let angle = transform.rotation.to_euler(EulerRot::XYZ).2;
        let camera_local = Rot2::radians(-angle) * (camera_pos - transform.translation.truncate());
        let border_margin = camera.scale.x.abs() * BORDER_WIDTH_PX;
        let half_width = camera_local.x.abs() + view_radius + border_margin;
        let depth = (-camera_local.y).max(0.0) + view_radius + border_margin;
        fill_shape.path = GeometryBuilder::build_as(&shapes::Polygon {
            points: vec![
                Vec2::new(-half_width, 0.0),
                Vec2::new(half_width, 0.0),
                Vec2::new(half_width, -depth),
                Vec2::new(-half_width, -depth),
            ],
            closed: true,
        });

        for (parent, mut boundary_shape) in &mut boundaries {
            if parent.parent() != plane_entity {
                continue;
            }
            boundary_shape.path = plane_boundary_path(half_width);
        }
    }
}

fn plane_boundary_path(half_width: f32) -> bevy_prototype_lyon::prelude::tess::path::Path {
    GeometryBuilder::new()
        .begin(Vec2::new(-half_width, 0.0))
        .line_to(Vec2::new(half_width, 0.0))
        .end(false)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::window::PrimaryWindow;

    #[test]
    fn open_plane_boundary_path_is_ended_before_building() {
        let _ = plane_boundary_path(100.0);
    }

    #[test]
    fn plane_visual_system_builds_fill_and_boundary_without_panicking() {
        let mut world = World::new();
        world.spawn((MainCamera, Transform::default()));
        world.spawn((PrimaryWindow, Window::default()));
        let plane = world
            .spawn((PlaneVisual, Transform::default(), Shape::default()))
            .id();
        world.spawn((PlaneBoundary, ChildOf(plane), Shape::default()));

        world.run_system_once(update_plane_visuals).unwrap();
    }
}
