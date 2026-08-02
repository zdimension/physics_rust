use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::egui_systems;
use crate::mouse_tracking::MainCamera;

egui_systems!(draw_scale_bar);

const TARGET_LENGTH_PX: f64 = 100.0;
const MIN_LENGTH_PX: f32 = 30.0;
const MAX_LENGTH_PX: f32 = 330.0;
const LINE_THICKNESS: f32 = 1.0;
const END_HEIGHT: f32 = 14.0;
const SCREEN_MARGIN: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScaleBar {
    exponent: i32,
    world_length: f64,
    screen_length: f32,
}

fn scale_bar(camera_scale: f32) -> Option<ScaleBar> {
    let meters_per_pixel = f64::from(camera_scale.abs());
    if !meters_per_pixel.is_finite() || meters_per_pixel <= 0.0 {
        return None;
    }

    let exponent = (meters_per_pixel * TARGET_LENGTH_PX).log10().round() as i32;
    let world_length = 10.0_f64.powi(exponent);
    let screen_length = (world_length / meters_per_pixel) as f32;
    if !screen_length.is_finite()
        || !(MIN_LENGTH_PX..=MAX_LENGTH_PX).contains(&screen_length)
    {
        return None;
    }

    Some(ScaleBar {
        exponent,
        world_length,
        screen_length,
    })
}

fn scale_label(scale: ScaleBar) -> String {
    match scale.exponent {
        0..=6 => format!("{:.0} m", scale.world_length),
        -6..=-1 => format!(
            "{:.*} m",
            scale.exponent.unsigned_abs() as usize,
            scale.world_length
        ),
        exponent => format!("1e{exponent} m"),
    }
}

fn draw_scale_bar(
    mut egui_contexts: EguiContexts,
    cameras: Query<&Transform, With<MainCamera>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let Some(scale) = scale_bar(camera.scale.x) else {
        return;
    };
    let Ok(ctx) = egui_contexts.ctx_mut() else {
        return;
    };

    egui::Area::new(egui::Id::new("world scale"))
        .anchor(
            egui::Align2::RIGHT_BOTTOM,
            egui::vec2(-SCREEN_MARGIN, -SCREEN_MARGIN),
        )
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                ui.label(scale_label(scale));

                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(scale.screen_length, END_HEIGHT),
                    egui::Sense::hover(),
                );
                let stroke = egui::Stroke::new(LINE_THICKNESS, ui.visuals().text_color());
                let left = egui::pos2(rect.left(), rect.center().y);
                let right = egui::pos2(rect.right(), rect.center().y);
                let top_left = egui::pos2(rect.left(), rect.top());
                let top_right = egui::pos2(rect.right(), rect.top());
                let bottom_left = egui::pos2(rect.left(), rect.bottom());
                let bottom_right = egui::pos2(rect.right(), rect.bottom());
                ui.painter().line_segment([left, right], stroke);
                ui.painter()
                    .line_segment([top_left, bottom_left], stroke);
                ui.painter()
                    .line_segment([top_right, bottom_right], stroke);
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_scale_displays_one_meter_as_one_hundred_pixels() {
        let scale = scale_bar(0.01).unwrap();

        assert_eq!(scale.exponent, 0);
        assert_eq!(scale.world_length, 1.0);
        assert!((scale.screen_length - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scale_length_stays_in_requested_bounds_across_zoom_levels() {
        for exponent in -12..=12 {
            for multiplier in [1.0, 1.7, 3.0, 5.5, 9.9] {
                let camera_scale = multiplier * 10.0_f32.powi(exponent);
                let scale = scale_bar(camera_scale).unwrap();

                assert!(scale.world_length.log10().fract().abs() < f64::EPSILON);
                assert!(scale.screen_length >= MIN_LENGTH_PX);
                assert!(scale.screen_length <= MAX_LENGTH_PX);
            }
        }
    }

    #[test]
    fn labels_use_meter_values_for_normal_zoom_levels() {
        let labels = [-2, -1, 0, 1, 2]
            .map(|exponent| {
                scale_label(ScaleBar {
                    exponent,
                    world_length: 10.0_f64.powi(exponent),
                    screen_length: 100.0,
                })
            });

        assert_eq!(labels, ["0.01 m", "0.1 m", "1 m", "10 m", "100 m"]);
    }
}
