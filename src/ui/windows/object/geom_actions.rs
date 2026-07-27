use crate::egui_systems;
use crate::tools::add_object::{AddAxleEvent, AddObjectEvent};
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow, WindowSelectionTarget, window_target_entities};
use avian2d::prelude::RigidBody;
use bevy::prelude::{ChildOf, Commands, Component, Entity, MessageWriter, Query, Res, With};
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::{EguiContexts, egui};

egui_systems!(GeometryActionsWindow::show);

#[derive(Default, Component)]
pub struct GeometryActionsWindow;

impl GeometryActionsWindow {
    pub fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<GeometryActionsWindow>,
        >,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        mut add_obj: MessageWriter<AddObjectEvent>,
        gui_icons: Res<GuiIcons>,
        rigid_bodies: Query<(), With<RigidBody>>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let bodies = window_target_entities(target, parent)
                .into_iter()
                .filter(|entity| rigid_bodies.contains(*entity))
                .collect::<Vec<_>>();
            if bodies.is_empty() {
                commands.entity(id).despawn();
                continue;
            }
            egui::Window::new("Geom actions")
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    if ui
                        .add(egui::Button::image_and_text(
                            SizedTexture::new(gui_icons.hinge, [16.0, 16.0]),
                            "Add center axle",
                        ))
                        .clicked()
                    {
                        for entity in bodies.iter().copied() {
                            add_obj.write(AddObjectEvent::Axle(AddAxleEvent::AddCenter(entity)));
                        }
                    }
                });
        }
    }
}
