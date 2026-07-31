use crate::config::AppConfig;
use crate::lyon_compat::GeometryBuilder;
use crate::lyon_compat::RectangleOrigin;
use crate::lyon_compat::shapes;
use crate::lyon_compat::{Fill, Shape, ShapeBundle, Stroke};
use crate::mouse_tracking::{MainCamera, MousePosWorld};
use crate::ui::Selected;
use crate::{BORDER_THICKNESS, make_fill, make_stroke};
use avian2d::parry::shape::TypedShape;
use avian2d::prelude::Collider;
use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use crate::FOREGROUND_Z;
use crate::tools::rotate::ROTATE_HELPER_RADIUS;

// Object layers are one unit apart, leaving this above object details but below the next object.
const SELECTION_OVERLAY_Z: f32 = 0.5;
const SELECTION_COLOR: Color = Color::WHITE;

#[derive(Copy, Clone, PartialEq)]
pub enum Overlay {
    Rectangle(Vec2),
    Circle(f32),
    Plane(Vec2, f32),
    Rotate(f32, f32, f32, Vec2),
}

#[derive(Resource, Default)]
pub struct OverlayState {
    pub draw_ent: Option<(Entity, Overlay, Vec2)>,
}

#[derive(Component)]
pub(crate) struct SelectionHighlight;

#[derive(Component)]
pub(crate) struct RotationOriginIcon {
    pub(crate) origin_angle: f32,
}

#[derive(Component)]
pub(crate) struct PlaneNormalIcon;

pub fn sync_selection_highlights(
    mut commands: Commands,
    selected: Query<Entity, With<Selected>>,
    mut highlight_parents: Query<(Entity, &ChildOf), With<SelectionHighlight>>,
    mut highlights: Query<(&mut Shape, &mut Transform, &mut Stroke), With<SelectionHighlight>>,
    colliders: Query<Ref<Collider>, Without<SelectionHighlight>>,
    added_selected: Query<(), Added<Selected>>,
    changed_selected_colliders: Query<(), (With<Selected>, Changed<Collider>)>,
    mut removed_selected: RemovedComponents<Selected>,
    mut removed_colliders: RemovedComponents<Collider>,
    app_config: Res<AppConfig>,
) {
    let selection_removed = removed_selected.read().next().is_some();
    let collider_removed = removed_colliders.read().next().is_some();
    if added_selected.is_empty()
        && changed_selected_colliders.is_empty()
        && !selection_removed
        && !collider_removed
        && !app_config.is_changed()
    {
        return;
    }

    let mut highlight_by_target = HashMap::new();
    for (highlight, parent) in &mut highlight_parents {
        let target = parent.parent();
        if selected.contains(target) && colliders.contains(target) {
            highlight_by_target.insert(target, highlight);
        } else {
            commands.entity(highlight).despawn();
        }
    }

    for target in &selected {
        let Ok(collider) = colliders.get(target) else {
            continue;
        };
        let highlight = highlight_by_target.get(&target).copied();
        if highlight.is_some() && !collider.is_changed() && !app_config.is_changed() {
            continue;
        }
        upsert_selection_highlight(
            target,
            highlight,
            collider_path(&collider),
            SELECTION_OVERLAY_Z,
            BORDER_THICKNESS * app_config.ui_scale_factor(),
            &mut commands,
            &mut highlights,
        );
    }
}

fn upsert_selection_highlight(
    target: Entity,
    highlight: Option<Entity>,
    path: bevy_prototype_lyon::prelude::tess::path::Path,
    local_z: f32,
    stroke_width: f32,
    commands: &mut Commands,
    highlights: &mut Query<(&mut Shape, &mut Transform, &mut Stroke), With<SelectionHighlight>>,
) {
    if let Some(highlight) = highlight
        && let Ok((mut shape, mut transform, mut stroke)) = highlights.get_mut(highlight)
    {
        shape.path = path;
        transform.translation = Vec3::Z * local_z;
        stroke.options.line_width = stroke_width;
        return;
    }

    commands.entity(target).with_children(|parent| {
        parent.spawn((
            SelectionHighlight,
            ShapeBundle::new(
                path,
                Transform::from_translation(Vec3::Z * local_z),
                Visibility::Inherited,
            ),
            make_fill(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            make_stroke(SELECTION_COLOR, stroke_width),
        ));
    });
}

fn collider_path(collider: &Collider) -> bevy_prototype_lyon::prelude::tess::path::Path {
    match collider.shape().as_typed_shape() {
        TypedShape::Ball(ball) => GeometryBuilder::build_as(&shapes::Circle {
            radius: ball.radius,
            ..Default::default()
        }),
        TypedShape::Cuboid(cuboid) => GeometryBuilder::build_as(&shapes::Rectangle {
            extents: Vec2::new(cuboid.half_extents.x, cuboid.half_extents.y) * 2.0,
            ..Default::default()
        }),
        TypedShape::ConvexPolygon(polygon) => GeometryBuilder::build_as(&shapes::Polygon {
            points: polygon
                .points()
                .iter()
                .map(|point| Vec2::new(point.x, point.y))
                .collect(),
            closed: true,
        }),
        TypedShape::HalfSpace(_) => GeometryBuilder::new()
            .begin(Vec2::new(-100_000.0, 0.0))
            .line_to(Vec2::new(100_000.0, 0.0))
            .end(false)
            .build(),
        _ => {
            let aabb = collider.shape().compute_local_aabb();
            GeometryBuilder::new()
                .begin(Vec2::new(aabb.mins.x, aabb.mins.y))
                .line_to(Vec2::new(aabb.maxs.x, aabb.mins.y))
                .line_to(Vec2::new(aabb.maxs.x, aabb.maxs.y))
                .line_to(Vec2::new(aabb.mins.x, aabb.maxs.y))
                .close()
                .build()
        }
    }
}

pub fn process_draw_overlay(
    cameras: Query<&Transform, With<MainCamera>>,
    mut overlay: ResMut<OverlayState>,
    mut commands: Commands,
    mouse: Res<MousePosWorld>,
    mut gizmos: Gizmos,
    app_config: Res<AppConfig>,
    mut overlay_queries: ParamSet<(
        Query<
            (
                &mut Shape,
                &mut Transform,
                Option<&mut Fill>,
                Option<&mut Stroke>,
            ),
            (With<crate::DrawObject>, Without<MainCamera>),
        >,
        Query<
            (
                &ChildOf,
                Option<&RotationOriginIcon>,
                Option<&PlaneNormalIcon>,
                &mut Transform,
            ),
            (Without<crate::DrawObject>, Without<MainCamera>),
        >,
    )>,
    mut last_overlay: Local<Option<(Entity, Overlay, Vec2, f32, i32)>>,
    mut active_overlay: Local<Option<Entity>>,
) {
    let Some((draw_ent, shape, pos)) = overlay.draw_ent else {
        *last_overlay = None;
        if let Some(active_overlay) = active_overlay.take() {
            clear_overlay_shape(active_overlay, &mut overlay_queries.p0());
        }
        return;
    };

    let Ok(camera) = cameras.single() else {
        return;
    };
    let current = (draw_ent, shape, pos, camera.scale.x, app_config.ui_scale);
    if last_overlay.as_ref() == Some(&current) {
        return;
    }

    if commands.get_entity(draw_ent).is_err() {
        overlay.draw_ent = None;
        *last_overlay = None;
        if *active_overlay == Some(draw_ent) {
            *active_overlay = None;
        }
        return;
    }

    if active_overlay.is_some_and(|active_overlay| active_overlay != draw_ent) {
        clear_overlay_shape(active_overlay.unwrap(), &mut overlay_queries.p0());
    }
    *active_overlay = Some(draw_ent);

    if let Overlay::Rotate(current_rot, scale, original_rot, click) = shape {
        {
            let mut rotation_origin_icons = overlay_queries.p1();
            for (parent, icon, _, mut transform) in &mut rotation_origin_icons {
                if parent.parent() == draw_ent
                    && let Some(icon) = icon
                {
                    transform.rotation = Quat::from_rotation_z(
                        icon.origin_angle + rotation_overlay_delta(current_rot, original_rot),
                    );
                }
            }
        }
        let path = draw_rotate_overlay(
            &mut gizmos,
            pos,
            current_rot,
            scale,
            original_rot,
            click,
            mouse.xy(),
        );
        let pink = Color::srgba_u8(255, 64, 255, 127);
        {
            let mut root_shapes = overlay_queries.p0();
            upsert_overlay_shape(
                draw_ent,
                pos,
                path,
                crate::make_fill(pink),
                crate::make_stroke(pink, 0.0),
                &mut commands,
                &mut root_shapes,
            );
        }
        *last_overlay = None;
        return;
    }

    if let Overlay::Plane(normal, scale) = shape {
        let plane_angle = normal.to_angle() - std::f32::consts::FRAC_PI_2;
        {
            let mut roots = overlay_queries.p0();
            if let Ok((_, mut transform, _, _)) = roots.get_mut(draw_ent) {
                transform.rotation = Quat::from_rotation_z(plane_angle);
            }
        }
        {
            let mut icons = overlay_queries.p1();
            for (parent, _, plane_icon, mut transform) in &mut icons {
                if parent.parent() == draw_ent && plane_icon.is_some() {
                    transform.translation =
                        (Vec2::Y * scale * ROTATE_HELPER_RADIUS * 0.5).extend(FOREGROUND_Z);
                    transform.rotation = Quat::IDENTITY;
                }
            }
        }
        gizmos
            .circle_2d(
                Isometry2d::new(pos, Rot2::IDENTITY),
                scale * ROTATE_HELPER_RADIUS,
                Color::srgba(1.0, 1.0, 1.0, 0.68),
            )
            .resolution(64);
        *last_overlay = None;
        return;
    }

    let builder = GeometryBuilder::new();
    let (thickness, color, path) = match shape {
        Overlay::Rectangle(size) => (
            5.0,
            Color::WHITE,
            builder
                .add(&shapes::Rectangle {
                    extents: size,
                    origin: RectangleOrigin::BottomLeft,
                    radii: None,
                })
                .build(),
        ),
        Overlay::Circle(radius) => (
            5.0,
            Color::WHITE,
            builder
                .add(&shapes::Circle {
                    radius,
                    ..Default::default()
                })
                .build(),
        ),
        Overlay::Plane(..) => unreachable!(),
        Overlay::Rotate(..) => unreachable!(),
    };

    {
        let mut root_shapes = overlay_queries.p0();
        upsert_overlay_shape(
            draw_ent,
            pos,
            path,
            crate::make_fill(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            crate::make_stroke(
                color,
                thickness * camera.scale.x * app_config.ui_scale_factor(),
            ),
            &mut commands,
            &mut root_shapes,
        );
    }

    *last_overlay = Some(current);
}

fn rotation_overlay_delta(current_rotation: f32, original_rotation: f32) -> f32 {
    current_rotation - original_rotation
}

fn clear_overlay_shape(
    draw_ent: Entity,
    root_shapes: &mut Query<
        (
            &mut Shape,
            &mut Transform,
            Option<&mut Fill>,
            Option<&mut Stroke>,
        ),
        (With<crate::DrawObject>, Without<MainCamera>),
    >,
) {
    let Ok((mut root_shape, _, root_fill, root_stroke)) = root_shapes.get_mut(draw_ent) else {
        return;
    };
    root_shape.path = GeometryBuilder::new().build();
    if let Some(mut root_fill) = root_fill {
        root_fill.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
    }
    if let Some(mut root_stroke) = root_stroke {
        root_stroke.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
    }
}

fn upsert_overlay_shape(
    draw_ent: Entity,
    pos: Vec2,
    path: bevy_prototype_lyon::prelude::tess::path::Path,
    fill: Fill,
    stroke: Stroke,
    commands: &mut Commands,
    root_shapes: &mut Query<
        (
            &mut Shape,
            &mut Transform,
            Option<&mut Fill>,
            Option<&mut Stroke>,
        ),
        (With<crate::DrawObject>, Without<MainCamera>),
    >,
) {
    match root_shapes.get_mut(draw_ent) {
        Ok((mut root_shape, mut root_transform, root_fill, root_stroke)) => {
            root_shape.path = path;
            root_transform.translation = pos.extend(FOREGROUND_Z);
            if let Some(mut root_fill) = root_fill {
                *root_fill = fill;
            } else {
                commands.entity(draw_ent).insert(fill);
            }
            if let Some(mut root_stroke) = root_stroke {
                *root_stroke = stroke;
            } else {
                commands.entity(draw_ent).insert(stroke);
            }
        }
        Err(_) => {
            commands.entity(draw_ent).insert((
                ShapeBundle::new(
                    path,
                    Transform::from_translation(pos.extend(FOREGROUND_Z)),
                    Visibility::Inherited,
                ),
                fill,
                stroke,
            ));
        }
    }
}

fn draw_rotate_overlay(
    gizmos: &mut Gizmos,
    center: Vec2,
    current_rot: f32,
    scale: f32,
    original_rot: f32,
    click: Vec2,
    mouse: Vec2,
) -> bevy_prototype_lyon::prelude::tess::path::Path {
    const RESOLUTION: u32 = 96;
    let helper_radius = scale * ROTATE_HELPER_RADIUS;

    gizmos
        .circle_2d(
            Isometry2d::new(center, Rot2::IDENTITY),
            helper_radius,
            Color::srgba(1.0, 1.0, 1.0, 0.68),
        )
        .resolution(64);
    draw_absolute_rotation_arc(gizmos, center, helper_radius, scale, current_rot);

    let start = click - center;
    let radius = mouse.distance(center);
    if start.length_squared() <= f32::EPSILON || radius <= f32::EPSILON {
        return GeometryBuilder::new().build();
    }

    let start_angle = start.y.atan2(start.x);
    let delta_angle = (current_rot - original_rot).clamp(-TAU, TAU);
    if delta_angle.abs() <= f32::EPSILON {
        return GeometryBuilder::new().build();
    }

    let segments = ((delta_angle.abs() / TAU) * RESOLUTION as f32)
        .ceil()
        .max(1.0) as u32;
    let arc_points = (0..=segments).map(|i| {
        let progress = i as f32 / segments as f32;
        let angle = start_angle + delta_angle * progress;
        Vec2::from_angle(angle) * radius
    });

    arc_points
        .fold(
            GeometryBuilder::new().begin(Vec2::ZERO),
            |builder, point| builder.line_to(point),
        )
        .close()
        .build()
}

fn draw_absolute_rotation_arc(
    gizmos: &mut Gizmos,
    center: Vec2,
    helper_radius: f32,
    camera_scale: f32,
    current_rot: f32,
) {
    const RESOLUTION: u32 = 96;
    let angle = {
        let normalized = current_rot.rem_euclid(TAU);
        if normalized > PI {
            normalized - TAU
        } else {
            normalized
        }
    };
    if angle.abs() <= f32::EPSILON {
        return;
    }

    let segments = ((angle.abs() / TAU) * RESOLUTION as f32).ceil().max(1.0) as u32;
    let color = Color::srgba(1.0, 1.0, 1.0, 0.82);

    for radius in [
        helper_radius + 2.0 * camera_scale,
        helper_radius + 4.0 * camera_scale,
    ] {
        let points = (0..=segments).map(|i| {
            let progress = i as f32 / segments as f32;
            let angle = angle * progress;
            (center + Vec2::from_angle(angle) * radius).extend(FOREGROUND_Z)
        });
        gizmos.linestrip(points, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_origin_follows_the_interaction_delta() {
        let initial_origin_angle = 35.0_f32.to_radians();
        let interaction_start = -20.0_f32.to_radians();
        let interaction_current = 40.0_f32.to_radians();

        let angle =
            initial_origin_angle + rotation_overlay_delta(interaction_current, interaction_start);

        assert!((angle - 95.0_f32.to_radians()).abs() < 1.0e-6);
    }
}
