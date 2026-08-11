use avian2d::prelude::*;
use bevy::prelude::{Local, MessageWriter, Res, ResMut, Time};
use bevy_egui::egui::{
    self, Align2, Color32, Mesh, Popup, PopupCloseBehavior, RectAlign, Sense, SetOpenCommand,
    Shape,
};
use bevy_egui::{EguiContexts, egui::PointerButton};

use crate::tools::ToolIcons;
use crate::grid::GridSettings;
use crate::objects::air::AirSettings;
use crate::objects::gravity::GravitySetting;
use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::{egui_systems, update_changed};
use crate::ui::separator_custom::SeparatorCustom;
use crate::ui::{RemoveTemporaryWindowsEvent, ToolboxState, WindowExt, bool_checkbox, image_radio};

const DIRECTION_SELECTOR_SIZE: f32 = 48.0;
const SIM_SPEED_HOVER_DELAY: f32 = 0.5;

fn long_hovered(
    ctx: &egui::Context,
    response: &egui::Response,
    hover_start: &mut Option<f64>,
) -> bool {
    if !response.hovered() {
        *hover_start = None;
        return false;
    }

    let now = ctx.input(|input| input.time);
    let elapsed = (now - *hover_start.get_or_insert(now)) as f32;
    if elapsed < SIM_SPEED_HOVER_DELAY {
        ctx.request_repaint_after_secs(SIM_SPEED_HOVER_DELAY - elapsed);
        false
    } else {
        true
    }
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

fn gravity_settings_ui(ui: &mut egui::Ui, icons: &GuiIcons, settings: &mut GravitySetting) {
    ui.add(
        egui::Slider::new(&mut settings.strength, 0.0..=20.0)
            .suffix(" m/s²")
            .text("Strength:")
            .custom(),
    );

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
        }
        direction_selector(ui, icons, &mut settings.direction);
    });
}

fn air_settings_ui(ui: &mut egui::Ui, icons: &GuiIcons, settings: &mut AirSettings) {
    ui.add(
        egui::Slider::new(&mut settings.multiplier, 0.0..=100.0)
            .logarithmic(true)
            .smallest_positive(0.01)
            .text("Multiplier:")
            .custom(),
    );
    ui.add(
        egui::Slider::new(&mut settings.linear_term, 0.0..=10.0)
            .logarithmic(true)
            .smallest_positive(0.0001)
            .suffix(" N/(m²/s)")
            .text("Linear term:")
            .custom(),
    );
    ui.add(
        egui::Slider::new(&mut settings.quadratic_term, 0.0..=1.0)
            .logarithmic(true)
            .smallest_positive(0.0001)
            .suffix(" N/(m³/s²)")
            .text("Quadratic term:")
            .custom(),
    );

    ui.separator();

    ui.add(
        egui::Slider::new(&mut settings.wind_speed, 0.0..=50.0)
            .suffix(" m/s²")
            .text("Wind speed:")
            .custom(),
    );
    ui.horizontal(|ui| {
        let mut direction_degrees = settings.wind_direction.to_degrees();
        if ui
            .add(
                egui::Slider::new(&mut direction_degrees, -180.0..=180.0)
                    .suffix("°")
                    .text("Wind angle:")
                    .custom(),
            )
            .changed()
        {
            settings.wind_direction = direction_degrees.to_radians();
        }
        direction_selector(ui, icons, &mut settings.wind_direction);
    });
}

fn grid_settings_ui(ui: &mut egui::Ui, icons: &GuiIcons, settings: &mut GridSettings) {
    ui.horizontal(|ui| {
        ui.label("Number of axes:");
        if image_radio(ui, icons, settings.axes == 2, "2") {
            settings.axes = 2;
        }
        if image_radio(ui, icons, settings.axes == 3, "3") {
            settings.axes = 3;
        }
    });
    ui.add(
        egui::Slider::new(&mut settings.base, 2..=100)
            .logarithmic(true)
            .text("Grid base:")
            .custom(),
    );
    bool_checkbox(ui, icons, &mut settings.snap, "Snap to grid");
}

pub fn draw_bottom_toolbar(
    mut egui_ctx: EguiContexts,
    mut toolbox_state: ResMut<ToolboxState>,
    //mut rapier: ResMut<RapierConfiguration>,
    mut gravity_conf: ResMut<GravitySetting>,
    mut gravity_settings_open: Local<bool>,
    mut air_settings_open: Local<bool>,
    mut grid_settings_open: Local<bool>,
    mut playpause_hover_start: Local<Option<f64>>,
    mut air_settings: ResMut<AirSettings>,
    mut grid_settings: ResMut<GridSettings>,
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
    mut clear_tmp: MessageWriter<RemoveTemporaryWindowsEvent>,
    mut physics: ResMut<Time<Physics>>,
) {
    let ctx = egui_ctx.ctx_mut().expect("primary egui context");
    let mut gravity_button_left = None;
    let mut air_button_left = None;
    let mut grid_button_left = None;
    let mut playpause_response = None;
    let toolbar = egui::Window::new("Tools2")
        .anchor(Align2::CENTER_BOTTOM, [0.0, -1.0])
        .title_bar(false)
        .auto_sized()
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
                playpause_response = Some(playpause);

                ui.add(SeparatorCustom::default());

                let gravity_btn = ui.add(
                    IconButton::new(gui_icons.gravity, 32.0)
                        .overlay(gui_icons.more_options)
                        .selected(gravity_conf.enabled),
                );
                gravity_button_left = Some(gravity_btn.rect.left());
                if gravity_btn.clicked() {
                    gravity_conf.enabled = !gravity_conf.enabled;
                }

                if gravity_btn.secondary_clicked() {
                    *gravity_settings_open = true;
                }

                let air_btn = ui.add(
                    IconButton::new(gui_icons.air, 32.0)
                        .overlay(gui_icons.more_options)
                        .selected(air_settings.enabled),
                );
                air_button_left = Some(air_btn.rect.left());
                if air_btn.clicked() {
                    air_settings.enabled = !air_settings.enabled;
                }
                if air_btn.secondary_clicked() {
                    *air_settings_open = true;
                }

                let grid_btn = ui.add(
                    IconButton::new(gui_icons.grid, 32.0)
                        .overlay(gui_icons.more_options)
                        .selected(grid_settings.enabled),
                );
                grid_button_left = Some(grid_btn.rect.left());
                if grid_btn.clicked() {
                    grid_settings.enabled = !grid_settings.enabled;
                }
                if grid_btn.secondary_clicked() {
                    *grid_settings_open = true;
                }
            })
        });

    if let (Some(toolbar), Some(playpause)) = (toolbar.as_ref(), playpause_response.as_ref()) {
        let popup_id = playpause.id.with("simulation speed");
        let popup_open = Popup::is_id_open(ctx, popup_id);
        let long_hover = if popup_open {
            *playpause_hover_start = None;
            false
        } else {
            long_hovered(ctx, playpause, &mut playpause_hover_start)
        };
        let requested_open = playpause.secondary_clicked()
            || long_hover;
        let anchor = egui::Rect::from_pos(egui::pos2(
            playpause.rect.center().x,
            toolbar.response.rect.top(),
        ));

        Popup::from_response(playpause)
            .id(popup_id)
            .anchor(anchor)
            .align(RectAlign::TOP)
            .align_alternatives(&[])
            .gap(1.0)
            .open_memory(
                requested_open.then_some(SetOpenCommand::Bool(true)),
            )
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                update_changed!(ui, || physics.relative_speed() => |x| physics.set_relative_speed(x), 0.1..=10.0, |slider| {
                    slider
                        .logarithmic(true)
                        .text("Simulation speed :")
                        .custom()
                });
            });
    }

    if *gravity_settings_open
        && let (Some(toolbar), Some(button_left)) = (toolbar.as_ref(), gravity_button_left)
    {
        let mut open = true;
        let anchor = egui::pos2(button_left, toolbar.response.rect.top() - 1.0);
        egui::Window::new("Gravity")
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(anchor)
            .auto_sized()
            .open(&mut open)
            .show_translucent(ctx, |ui| {
                gravity_settings_ui(ui, &gui_icons, &mut gravity_conf);
            });
        *gravity_settings_open = open;
    }

    if *air_settings_open
        && let (Some(toolbar), Some(button_left)) = (toolbar.as_ref(), air_button_left)
    {
        let mut open = true;
        let anchor = egui::pos2(button_left, toolbar.response.rect.top() - 1.0);
        egui::Window::new("Air")
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(anchor)
            .resizable(false)
            .open(&mut open)
            .show_translucent(ctx, |ui| {
                air_settings_ui(ui, &gui_icons, &mut air_settings);
            });
        *air_settings_open = open;
    }

    if *grid_settings_open
        && let (Some(toolbar), Some(button_left)) = (toolbar.as_ref(), grid_button_left)
    {
        let mut open = true;
        let anchor = egui::pos2(button_left, toolbar.response.rect.top() - 1.0);
        egui::Window::new("Grid")
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(anchor)
            .auto_sized()
            .open(&mut open)
            .show_translucent(ctx, |ui| {
                grid_settings_ui(ui, &gui_icons, &mut grid_settings);
            });
        *grid_settings_open = open;
    }
}

egui_systems!(draw_bottom_toolbar);

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    #[test]
    fn downward_gravity_is_minus_ninety_degrees() {
        let settings = GravitySetting::default();

        assert!((settings.direction.to_degrees() + 90.0).abs() < f32::EPSILON);
        assert!((settings.vector() - Vec2::new(0.0, -9.81)).length() < 1.0e-5);
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
