use crate::ui::{InitialPos, Subwindow};
use crate::GuiIcons;
use bevy::hierarchy::{BuildChildren, DespawnRecursiveExt, Parent};
use bevy::prelude::*;
use bevy_egui::egui::{pos2, vec2, Separator};
use bevy_egui::{egui, EguiContext};
use std::time::Duration;

use crate::ui::windows::appearance::AppearanceWindow;
use crate::ui::windows::collisions::CollisionsWindow;
use crate::ui::windows::combine_shapes::CombineShapesWindow;
use crate::ui::windows::controller::ControllerWindow;
use crate::ui::windows::geom_actions::GeometryActionsWindow;
use crate::ui::windows::information::InformationWindow;
use crate::ui::windows::material::MaterialWindow;
use crate::ui::windows::script::ScriptMenuWindow;
use crate::ui::windows::selection::SelectionWindow;
use crate::ui::windows::text::TextWindow;
use crate::ui::windows::velocities::VelocitiesWindow;

use crate::ui::menu_item::MenuItem;
use crate::ui::windows::plot::PlotWindow;

#[derive(Default, Component)]
pub struct MenuWindow {
    hovered_item: Option<(MenuId, Duration)>,
    selected_item: Option<MenuId>,
    our_child_window: Option<Entity>,
}

impl MenuWindow {
    pub(crate) fn show(
        mut wnds: Query<(Entity, Option<&Parent>, &mut MenuWindow, &mut InitialPos)>,
        time: Res<Time>,
        mut egui_ctx: ResMut<EguiContext>,
        icons: Res<GuiIcons>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, entity, mut info_wnd, mut initial_pos) in wnds.iter_mut() {
            let entity = entity.map(Parent::get);
            egui::Window::new("context menu")
                .default_size(vec2(0.0, 0.0))
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    macro_rules! item {
                            (@ $text:literal, $icon:expr) => {
                                ui.add(MenuItem::button($icon, $text.to_string())).clicked()
                            };
                            ($text:literal, $icon:ident) => {
                                item!(@ $text, Some(icons.$icon))
                            };
                            ($text:literal) => {
                                item!(@ $text, None)
                            };
                        }
                    macro_rules! menu {
                            (@ $text: literal, $icon: expr, $wnd:ty) => {
                                let our_id = $text;
                                let us_selected = matches!(info_wnd.selected_item, Some(id) if id == our_id);
                                let menu = ui.add(MenuItem::menu($icon, $text.to_string(), icons.arrow_right).selected(us_selected));

                                if !us_selected {
                                    let selected = match info_wnd.hovered_item {
                                        Some((id, at)) if id == our_id && (time.elapsed() - at) > Duration::from_millis(500) => true,
                                        _ => menu.clicked()
                                    };

                                    if selected {
                                        info!("clicked: {}", $text);
                                        info_wnd.selected_item = Some(our_id);

                                        if let Some(id) = info_wnd.our_child_window {
                                            commands.entity(id).despawn_recursive();
                                        }

                                        info!("rect: {:?}", menu.rect);

                                        let new_wnd = commands.spawn((
                                            <$wnd as Default>::default(),
                                            InitialPos::initial(menu.rect.right_top())
                                        )).id();

                                        if let Some(id) = entity {
                                            commands.entity(id).push_children(&[new_wnd]);
                                        }

                                        info_wnd.our_child_window = Some(new_wnd);
                                    }
                                }

                                let us = matches!(info_wnd.hovered_item, Some((id, _)) if id == our_id);
                                if menu.hovered() && !us { // we're hovering but someone else was
                                    info_wnd.hovered_item = Some((our_id, time.elapsed())); // we're the new hoverer
                                } else if !menu.hovered() && us { // not hovering and we were
                                    info_wnd.hovered_item = None; // now we're not
                                }
                            };
                            ($text:literal, $icon:ident, $wnd:ty) => {
                                menu!(@ $text, Some(icons.$icon), $wnd);
                            };
                            ($text:literal, /, $wnd:ty) => {
                                menu!(@ $text, None, $wnd);
                            };
                        }

                    match entity {
                        Some(id) => {
                            if item!("Erase", erase) {
                                commands.entity(id).despawn_recursive();
                            }
                            if item!("Mirror", mirror) {}
                            if item!("Show plot", plot) {
                                commands.entity(id).with_children(|parent| {
                                    parent.spawn((PlotWindow::default(), InitialPos::persistent(pos2(100.0, 100.0))));
                                });
                            }
                            ui.add(Separator::default().horizontal());

                            menu!("Selection", /, SelectionWindow);
                            menu!("Appearance", color, AppearanceWindow);
                            menu!("Text", text, TextWindow);
                            menu!("Material", material, MaterialWindow);
                            menu!("Velocities", velocity, VelocitiesWindow);
                            menu!("Information", info, InformationWindow);
                            menu!("Collision layers", collisions, CollisionsWindow);
                            menu!("Geometry actions", /, GeometryActionsWindow);
                            menu!("Combine shapes", csg, CombineShapesWindow);
                            menu!("Controller", controller, ControllerWindow);
                            menu!("Script menu", /, ScriptMenuWindow);
                        }
                        None => {
                            if item!("Zoom to scene", zoom2scene) {}
                            if item!("Default view") {}
                            if item!("Background", color) {}
                        }
                    }
                });
        }
    }
}

type MenuId = &'static str;
