use crate::tools::add_object::{AddHingeEvent, AddObjectEvent};
use crate::ui::images::GuiIcons;
use crate::ui::{BevyIdThing, InitialPos, Subwindow};
use bevy::prelude::{info, Commands, Component, Entity, EventWriter, Parent, Query, Res, With};
use bevy_egui::{egui, EguiContexts};
use bevy_rapier2d::na::DimAdd;
use crate::systems;

systems!(GeometryActionsWindow::show);

#[derive(Default, Component)]
pub struct GeometryActionsWindow;

impl GeometryActionsWindow {
    pub fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<GeometryActionsWindow>>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        mut add_obj: EventWriter<AddObjectEvent>,
        gui_icons: Res<GuiIcons>,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            egui::Window::new("Geom actions")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    if ui
                        .add(egui::Button::image_and_text(
                            gui_icons.hinge,
                            [16.0, 16.0],
                            "Add center axle",
                        ))
                        .clicked()
                    {
                        info!("Add center axle {:?}", parent.get());
                        add_obj.send(AddObjectEvent::Hinge(AddHingeEvent::AddCenter(
                            parent.get(),
                        )));
                    }
                });
        }
    }
}
