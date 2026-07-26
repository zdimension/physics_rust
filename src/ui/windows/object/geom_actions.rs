use crate::tools::add_object::{AddAxleEvent, AddObjectEvent};
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow};
use bevy::prelude::{info, Commands, Component, Entity, MessageWriter, ChildOf, Query, Res, With};
use bevy_egui::{egui, EguiContexts};
use bevy_egui::egui::load::SizedTexture;
use crate::egui_systems;

egui_systems!(GeometryActionsWindow::show);

#[derive(Default, Component)]
pub struct GeometryActionsWindow;

impl GeometryActionsWindow {
    pub fn show(
        mut wnds: Query<(Entity, &ChildOf, &mut InitialPos), With<GeometryActionsWindow>>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        mut add_obj: MessageWriter<AddObjectEvent>,
        gui_icons: Res<GuiIcons>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            egui::Window::new("Geom actions")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    if ui
                        .add(egui::Button::image_and_text(
                            SizedTexture::new(gui_icons.hinge,
                            [16.0, 16.0]),
                            "Add center axle",
                        ))
                        .clicked()
                    {
                        info!("Add center axle {:?}", parent.parent());
                        add_obj.write(AddObjectEvent::Axle(AddAxleEvent::AddCenter(
                            parent.parent(),
                        )));
                    }
                });
        }
    }
}
