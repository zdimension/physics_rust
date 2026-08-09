use avian2d::parry::shape::{Shape as ParryShape, TypedShape};
use avian2d::prelude::{Collider, Rotation};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use std::f32::consts::{FRAC_PI_2, PI};

use crate::config::AppConfig;
use crate::mouse_tracking::MainCamera;
use crate::palette::PaletteConfig;

const MIN_MINOR_SPACING_PX: f32 = 35.0;
const MAX_MINOR_SPACING_PX: f32 = 135.0;
const GRID_LINE_THICKNESS_PX: f32 = 1.5;

#[derive(Resource, Clone, Copy, Debug)]
pub struct GridSettings {
    pub enabled: bool,
    pub axes: u32,
    pub base: u32,
    pub snap: bool,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            axes: 2,
            base: 4,
            snap: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLayout {
    pub minor_step: f32,
    pub major_step: f32,
    minor_visible: bool,
    axes: u32,
    base: i32,
}

impl GridSettings {
    pub fn layout(self, camera_scale: f32) -> Option<GridLayout> {
        if !self.enabled || !camera_scale.is_finite() || camera_scale <= 0.0 {
            return None;
        }

        let axes = self.axes.max(2);
        let line_spacing_factor = (PI / axes as f32).sin();
        let base = self.base.clamp(2, 100);
        let base_f32 = base as f32;
        let largest_minor_step = camera_scale * MAX_MINOR_SPACING_PX / line_spacing_factor;
        let exponent = (largest_minor_step.ln() / base_f32.ln()).floor();
        let minor_step = base_f32.powf(exponent);
        let major_step = minor_step * base_f32;
        let minor_visible = minor_step * line_spacing_factor / camera_scale >= MIN_MINOR_SPACING_PX;
        (minor_step.is_finite() && minor_step > 0.0).then_some(GridLayout {
            minor_step,
            major_step,
            minor_visible,
            axes,
            base: base as i32,
        })
    }

    pub fn snap_point(self, point: Vec2, camera_scale: f32) -> Vec2 {
        if !self.snap {
            return point;
        }
        self.layout(camera_scale)
            .map_or(point, |layout| layout.snap_point(point))
    }
}

impl GridLayout {
    fn visible_step(self) -> f32 {
        if self.minor_visible {
            self.minor_step
        } else {
            self.major_step
        }
    }

    fn line_families(self) -> (Vec<Vec2>, f32) {
        let visible_step = self.visible_step();
        let angle_step = PI / self.axes as f32;
        let normals = (0..self.axes)
            .map(|axis| {
                let angle = FRAC_PI_2 + axis as f32 * angle_step;
                Vec2::new(clean_trig(angle.cos()), clean_trig(angle.sin()))
            })
            .collect();
        (normals, visible_step * angle_step.sin())
    }

    pub fn snap_point(self, point: Vec2) -> Vec2 {
        let (normals, line_step) = self.line_families();
        let mut best = None;
        for (i, &a) in normals.iter().enumerate() {
            for &b in &normals[i + 1..] {
                for a_offset in adjacent_line_offsets(a.dot(point), line_step) {
                    for b_offset in adjacent_line_offsets(b.dot(point), line_step) {
                        if let Some(correction) = line_intersection(a, b, a_offset, b_offset) {
                            if best.is_none_or(|current: Vec2| {
                                correction.length_squared() < current.length_squared()
                            }) {
                                best = Some(correction);
                            }
                        }
                    }
                }
            }
        }
        point + best.unwrap_or(Vec2::ZERO)
    }

    pub fn snap_translation(self, bodies: &[SnapBody<'_>]) -> Vec2 {
        let (normals, line_step) = self.line_families();
        let mut residuals = vec![Vec::new(); normals.len()];

        for (family, normal) in normals.iter().copied().enumerate() {
            for body in bodies {
                let local_normal = body.rotation.inverse() * normal;
                let center_offset = local_normal.dot(body.center_of_mass);
                add_residual(
                    &mut residuals[family],
                    normal.dot(body.position) + center_offset,
                    line_step,
                );

                let direction = avian2d::parry::math::Vector::new(local_normal.x, local_normal.y);
                for offset in support_offsets(body.collider.shape_scaled().as_ref(), direction) {
                    add_residual(
                        &mut residuals[family],
                        normal.dot(body.position) + offset,
                        line_step,
                    );
                }
            }
        }

        let mut best = None;
        for first in 0..normals.len() {
            for second in (first + 1)..normals.len() {
                let a = normals[first];
                let b = normals[second];
                for &a_residual in &residuals[first] {
                    for &b_residual in &residuals[second] {
                        let Some(correction) = line_intersection(a, b, a_residual, b_residual)
                        else {
                            continue;
                        };
                        if best.is_none_or(|current: Vec2| {
                            correction.length_squared() < current.length_squared()
                        }) {
                            best = Some(correction);
                        }
                    }
                }
            }
        }
        best.unwrap_or(Vec2::ZERO)
    }
}

fn clean_trig(value: f32) -> f32 {
    if value.abs() < 1.0e-6 { 0.0 } else { value }
}

fn adjacent_line_offsets(projection: f32, step: f32) -> [f32; 2] {
    let lower = (projection / step).floor() * step - projection;
    [lower, lower + step]
}

fn line_intersection(a: Vec2, b: Vec2, a_offset: f32, b_offset: f32) -> Option<Vec2> {
    let determinant = a.perp_dot(b);
    (determinant.abs() > f32::EPSILON).then(|| {
        Vec2::new(
            (a_offset * b.y - a.y * b_offset) / determinant,
            (a.x * b_offset - a_offset * b.x) / determinant,
        )
    })
}

fn support_offsets(shape: &dyn ParryShape, direction: avian2d::parry::math::Vector) -> Vec<f32> {
    if let Some(support_map) = shape.as_support_map() {
        let max = support_map.local_support_point(direction).dot(direction);
        let min = support_map.local_support_point(-direction).dot(direction);
        return vec![min, max];
    }

    let TypedShape::Compound(compound) = shape.as_typed_shape() else {
        return Vec::new();
    };
    compound
        .shapes()
        .iter()
        .flat_map(|(pose, child)| {
            let support_map = child.as_support_map()?;
            Some([
                support_map.support_point(pose, -direction).dot(direction),
                support_map.support_point(pose, direction).dot(direction),
            ])
        })
        .flatten()
        .collect()
}

fn add_residual(residuals: &mut Vec<f32>, projected_position: f32, line_step: f32) {
    residuals.push((projected_position / line_step).round() * line_step - projected_position);
}

pub struct SnapBody<'a> {
    pub position: Vec2,
    pub rotation: Rotation,
    pub center_of_mass: Vec2,
    pub collider: &'a Collider,
}

pub fn draw_grid(
    mut contexts: EguiContexts,
    settings: Res<GridSettings>,
    app_config: Res<AppConfig>,
    palette: Res<PaletteConfig>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform, &Transform), With<MainCamera>>,
) {
    let Ok((camera, camera_global, camera_transform)) = cameras.single() else {
        return;
    };
    let Some(layout) = settings.layout(camera_transform.scale.x.abs()) else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let viewport_corners = [
        Vec2::ZERO,
        Vec2::new(window.width(), 0.0),
        Vec2::new(window.width(), window.height()),
        Vec2::new(0.0, window.height()),
    ];
    let Some(world_corners) = viewport_corners
        .map(|point| camera.viewport_to_world_2d(camera_global, point).ok())
        .into_iter()
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };

    let ui_scale = app_config.ui_scale.max(f32::EPSILON);
    let sky = palette.current_palette.sky_color.to_srgba();
    let luminance = 0.2126 * sky.red + 0.7152 * sky.green + 0.0722 * sky.blue;
    let line_color = if luminance > 0.55 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("world grid"),
    ));
    let (normals, line_step) = layout.line_families();
    let reach = world_corners
        .iter()
        .map(|corner| corner.length())
        .fold(0.0_f32, f32::max)
        + line_step * 2.0;

    for normal in normals {
        let direction = Vec2::new(-normal.y, normal.x);
        let (min_projection, max_projection) =
            world_corners
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), point| {
                    let projection = normal.dot(*point);
                    (min.min(projection), max.max(projection))
                });
        let first = (min_projection / line_step).floor() as i32 - 1;
        let last = (max_projection / line_step).ceil() as i32 + 1;
        for index in first..=last {
            let center = normal * (index as f32 * line_step);
            let world_start = center - direction * reach;
            let world_end = center + direction * reach;
            let (Ok(screen_start), Ok(screen_end)) = (
                camera.world_to_viewport(camera_global, world_start.extend(0.0)),
                camera.world_to_viewport(camera_global, world_end.extend(0.0)),
            ) else {
                continue;
            };
            let alpha = if index == 0 {
                150
            } else if !layout.minor_visible || index.rem_euclid(layout.base) == 0 {
                90
            } else {
                45
            };
            painter.line_segment(
                [
                    egui::pos2(screen_start.x / ui_scale, screen_start.y / ui_scale),
                    egui::pos2(screen_end.x / ui_scale, screen_end.y / ui_scale),
                ],
                egui::Stroke::new(
                    GRID_LINE_THICKNESS_PX / ui_scale,
                    line_color.gamma_multiply(alpha as f32 / 255.0),
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_grid_has_one_meter_minor_steps_at_default_zoom() {
        let mut settings = GridSettings::default();
        settings.enabled = true;
        let layout = settings.layout(0.01).unwrap();

        assert_eq!(layout.major_step, 4.0);
        assert_eq!(layout.minor_step, 1.0);
        assert!(layout.minor_visible);
    }

    #[test]
    fn zooming_in_promotes_the_old_minor_level_to_major() {
        let mut settings = GridSettings::default();
        settings.enabled = true;

        let before = settings.layout(0.01).unwrap();
        let after = settings.layout(0.005).unwrap();

        assert_eq!(before.minor_step, after.major_step);
        assert_eq!(after.minor_step, before.minor_step / settings.base as f32);
        assert!((after.minor_step / 0.005 - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn overly_dense_subdivisions_are_hidden_and_not_used_for_snapping() {
        let settings = GridSettings {
            enabled: true,
            axes: 2,
            base: 100,
            snap: true,
        };
        let layout = settings.layout(0.001).unwrap();

        assert!(!layout.minor_visible);
        assert_eq!(layout.visible_step(), layout.major_step);
        assert_eq!(layout.snap_point(Vec2::splat(0.6)), Vec2::ONE);
    }

    #[test]
    fn every_rendered_minor_level_stays_in_the_target_screen_range() {
        for axes in [2, 3, 5, 12] {
            for base in [2, 3, 4, 10, 100] {
                let settings = GridSettings {
                    enabled: true,
                    axes,
                    base,
                    snap: true,
                };
                for exponent in -8..=8 {
                    for multiplier in [1.0, 1.7, 5.0] {
                        let camera_scale = multiplier * 10.0_f32.powi(exponent);
                        let layout = settings.layout(camera_scale).unwrap();
                        if layout.minor_visible {
                            let pixels =
                                layout.minor_step * (PI / axes as f32).sin() / camera_scale;
                            assert!(pixels >= MIN_MINOR_SPACING_PX - 1.0e-3);
                            assert!(pixels <= MAX_MINOR_SPACING_PX + 1.0e-3);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn rectangular_points_snap_to_the_visible_minor_lattice() {
        let layout = GridLayout {
            minor_step: 0.25,
            major_step: 1.0,
            minor_visible: true,
            axes: 2,
            base: 4,
        };

        assert_eq!(
            layout.snap_point(Vec2::new(0.13, 0.61)),
            Vec2::new(0.25, 0.5)
        );
    }

    #[test]
    fn triangular_points_snap_to_an_equilateral_lattice() {
        let layout = GridLayout {
            minor_step: 1.0,
            major_step: 4.0,
            minor_visible: true,
            axes: 3,
            base: 4,
        };

        let snapped = layout.snap_point(Vec2::new(0.48, 0.82));
        assert!(snapped.distance(Vec2::new(0.5, 3.0_f32.sqrt() * 0.5)) < 1.0e-6);
    }

    #[test]
    fn arbitrary_axis_counts_draw_and_snap_generically() {
        for axes in 2..=12 {
            let layout = GridSettings {
                enabled: true,
                axes,
                base: 4,
                snap: true,
            }
            .layout(0.01)
            .unwrap();
            let (normals, line_step) = layout.line_families();
            assert_eq!(normals.len(), axes as usize);

            let snapped = layout.snap_point(Vec2::new(0.37, 0.61));
            assert!(
                normals
                    .iter()
                    .filter(|normal| {
                        let line = normal.dot(snapped) / line_step;
                        (line - line.round()).abs() < 1.0e-4
                    })
                    .count()
                    >= 2
            );
        }
    }

    #[test]
    fn circle_can_snap_by_its_center_or_tangent() {
        let layout = GridLayout {
            minor_step: 1.0,
            major_step: 4.0,
            minor_visible: true,
            axes: 2,
            base: 4,
        };
        let collider = Collider::circle(0.2);
        let body = SnapBody {
            position: Vec2::new(0.31, 0.44),
            rotation: Rotation::IDENTITY,
            center_of_mass: Vec2::ZERO,
            collider: &collider,
        };

        let correction = layout.snap_translation(&[body]);
        let snapped = Vec2::new(0.31, 0.44) + correction;
        let x_feature_is_snapped = [snapped.x, snapped.x - 0.2, snapped.x + 0.2]
            .into_iter()
            .any(|value| (value - value.round()).abs() < 1.0e-5);
        let y_feature_is_snapped = [snapped.y, snapped.y - 0.2, snapped.y + 0.2]
            .into_iter()
            .any(|value| (value - value.round()).abs() < 1.0e-5);

        assert!(x_feature_is_snapped && y_feature_is_snapped);

        let already_snapped = SnapBody {
            position: snapped,
            rotation: Rotation::IDENTITY,
            center_of_mass: Vec2::ZERO,
            collider: &collider,
        };
        assert!(layout.snap_translation(&[already_snapped]).length_squared() < 1.0e-10);
    }

    #[test]
    fn compound_shapes_offer_each_convex_part_for_edge_snapping() {
        let collider = Collider::compound(vec![
            (
                Vec2::new(-1.0, 0.0),
                Rotation::IDENTITY,
                Collider::rectangle(1.0, 1.0),
            ),
            (
                Vec2::new(2.0, 0.0),
                Rotation::IDENTITY,
                Collider::rectangle(1.0, 1.0),
            ),
        ]);
        let mut offsets = support_offsets(
            collider.shape_scaled().as_ref(),
            avian2d::parry::math::Vector::X,
        );
        offsets.sort_by(f32::total_cmp);

        assert_eq!(offsets, [-1.5, -0.5, 1.5, 2.5]);
    }
}
