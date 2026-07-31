use avian2d::prelude::*;
use bevy::math::Vec2;
use bevy::prelude::{Local, MessageWriter, Res, ResMut, Time};
use bevy_egui::egui::{self, Align2, Color32, Mesh, Sense, Shape};
use bevy_egui::{EguiContexts, egui::PointerButton};

use crate::tools::ToolIcons;
use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::{egui_systems, update_changed};
use crate::ui::separator_custom::SeparatorCustom;
use crate::ui::{GravitySetting, RemoveTemporaryWindowsEvent, ToolboxState, WindowExt};

const DIRECTION_SELECTOR_SIZE: f32 = 48.0;

fn gravity_vector(settings: &GravitySetting) -> Vec2 {
    Vec2::from_angle(settings.direction) * settings.strength
}

fn direction_from_pointer(center: egui::Pos2, pointer: egui::Pos2) -> Option<f32> {
    let offset = pointer - center;
    (offset.length_sq() > f32::EPSILON).then(|| (-offset.y).atan2(offset.x))
}

fn direction_selector(ui: &mut egui::Ui, icons: &GuiIcons, direction: &mut f32) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(
        egui::Vec2::splat(DIRECTION_SELECTOR_SIZE),
        Sense::click_and_drag(),
    );
    let cursor = if response.dragged_by(PointerButton::Primary) {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    };
    response = response.on_hover_cursor(cursor);

    if response.is_pointer_button_down_on()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(new_direction) = direction_from_pointer(rect.center(), pointer)
    {
        let step = 5.0_f32.to_radians();
        *direction = (new_direction / step).round() * step;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
        ui.painter()
            .image(icons.direction_base, rect, uv, Color32::WHITE);

        let arrow_size = DIRECTION_SELECTOR_SIZE * 0.5;
        let arrow_rect = egui::Rect::from_min_max(
            egui::pos2(rect.center().x - arrow_size * 0.5, rect.center().y - arrow_size),
            egui::pos2(rect.center().x + arrow_size * 0.5, rect.center().y),
        );
        let mut arrow = Mesh::with_texture(icons.direction_arrow);
        arrow.add_rect_with_uv(arrow_rect, uv, Color32::WHITE);
        let rotation = egui::emath::Rot2::from_angle(std::f32::consts::FRAC_PI_2 - *direction);
        for vertex in &mut arrow.vertices {
            vertex.pos = rect.center() + rotation * (vertex.pos - rect.center());
        }
        ui.painter().add(Shape::mesh(arrow));
        ui.painter()
            .image(icons.direction_top, rect, uv, Color32::WHITE);
    }

    response
}

fn gravity_settings_ui(
    ui: &mut egui::Ui,
    icons: &GuiIcons,
    settings: &mut GravitySetting,
) -> bool {
    let mut changed = ui
        .add(
            egui::Slider::new(&mut settings.strength, 0.0..=20.0)
                .suffix(" m/s²")
                .text("Strength:")
                .custom(),
        )
        .changed();

    ui.horizontal(|ui| {
        let mut direction_degrees = settings.direction.to_degrees();
        if ui
            .add(
                egui::Slider::new(&mut direction_degrees, -180.0..=180.0)
                    .suffix("°")
                    .text("Direction:")
                    .custom(),
            )
            .changed()
        {
            settings.direction = direction_degrees.to_radians();
            changed = true;
        }
        if direction_selector(ui, icons, &mut settings.direction).changed() {
            changed = true;
        }
    });

    changed
}

pub fn draw_bottom_toolbar(
    mut egui_ctx: EguiContexts,
    mut toolbox_state: ResMut<ToolboxState>,
    //mut rapier: ResMut<RapierConfiguration>,
    mut gravity_conf: Local<GravitySetting>,
    mut gravity_settings_open: Local<bool>,
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
    mut clear_tmp: MessageWriter<RemoveTemporaryWindowsEvent>,
    mut gravity: ResMut<Gravity>,
    mut physics: ResMut<Time<Physics>>,
) {
    let ctx = egui_ctx.ctx_mut().expect("primary egui context");
    let mut gravity_button_left = None;
    let toolbar = egui::Window::new("Tools2")
        .anchor(Align2::CENTER_BOTTOM, [0.0, -1.0])
        .title_bar(false)
        .resizable(false)
        .show_translucent(ctx, |ui| {
            ui.style_mut().spacing.item_spacing = egui::Vec2::new(3.0, 3.0);
            ui.horizontal(|ui| {
                let toolbox_state = &mut *toolbox_state;
                for def in toolbox_state.toolbox_bottom.iter() {
                    if ui
                        .add(
                            IconButton::new(def.egui_icon(&tool_icons), 32.0)
                                .selected(toolbox_state.toolbox_selected.is_same(def)),
                        )
                        .clicked()
                    {
                        toolbox_state.toolbox_selected = def.clone();
                        clear_tmp.write(RemoveTemporaryWindowsEvent);
                    }
                }

                ui.add(SeparatorCustom::default());

                let playpause = ui.add(IconButton::new(
                    if physics.is_paused() {
                        gui_icons.play
                    } else {
                        gui_icons.pause
                    },
                    32.0,
                ));

                if playpause.clicked() {
                    if physics.is_paused() {
                        physics.unpause();
                    } else {
                        physics.pause();
                    }
                }
                playpause.context_menu(|ui| {
                    update_changed!(ui, || physics.relative_speed() => |x| physics.set_relative_speed(x), 0.1..=10.0, |slider| {
                        slider.logarithmic(true).text("Simulation speed :")
                    });
                });

                ui.add(SeparatorCustom::default());

                let gravity_btn =
                    ui.add(IconButton::new(gui_icons.gravity, 32.0).selected(gravity_conf.enabled));
                gravity_button_left = Some(gravity_btn.rect.left());
                if gravity_btn.clicked() {
                    gravity_conf.enabled = !gravity_conf.enabled;
                    if gravity_conf.enabled {
                        gravity.0 = gravity_vector(&gravity_conf);
                    } else {
                        gravity.0 = Vec2::ZERO;
                    }
                }

                if gravity_btn.secondary_clicked() {
                    *gravity_settings_open = true;
                }
            })
        });

    if *gravity_settings_open
        && let (Some(toolbar), Some(button_left)) = (toolbar, gravity_button_left)
    {
        let mut open = true;
        let anchor = egui::pos2(button_left, toolbar.response.rect.top() - 1.0);
        egui::Window::new("Gravity")
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(anchor)
            .resizable(false)
            .open(&mut open)
            .show_translucent(ctx, |ui| {
                if gravity_settings_ui(ui, &gui_icons, &mut gravity_conf)
                    && gravity_conf.enabled
                {
                    gravity.0 = gravity_vector(&gravity_conf);
                }
            });
        *gravity_settings_open = open;
    }
}

egui_systems!(draw_bottom_toolbar);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downward_gravity_is_minus_ninety_degrees() {
        let settings = GravitySetting::default();

        assert!((settings.direction.to_degrees() + 90.0).abs() < f32::EPSILON);
        assert!((gravity_vector(&settings) - Vec2::new(0.0, -9.81)).length() < 1.0e-5);
    }

    #[test]
    fn pointer_direction_uses_mathematical_screen_angles() {
        let center = egui::pos2(50.0, 50.0);

        let down = direction_from_pointer(center, egui::pos2(50.0, 60.0)).unwrap();
        let right = direction_from_pointer(center, egui::pos2(60.0, 50.0)).unwrap();

        assert!((down.to_degrees() + 90.0).abs() < f32::EPSILON);
        assert!(right.abs() < f32::EPSILON);
    }

    #[test]
    fn compass_direction_snaps_to_five_degree_steps() {
        let step = 5.0_f32.to_radians();
        let raw_direction = 13.0_f32.to_radians();

        let snapped = (raw_direction / step).round() * step;

        assert!((snapped.to_degrees() - 15.0).abs() < 1.0e-5);
    }
}
