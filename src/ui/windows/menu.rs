use crate::objects::laser::LaserBundle;
use crate::objects::{ColorComponent, MotorComponent};
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow, TemporaryWindow};
use crate::{CAMERA_Z,  systems};
use bevy::hierarchy::{BuildChildren, Parent};
use bevy::prelude::*;
use bevy_egui::egui::{pos2, Separator};
use bevy_egui::{egui, EguiContexts};
use bevy_rapier2d::prelude::{CollisionGroups, RigidBody, Velocity};
use std::time::Duration;
use bevy::math::Vec3Swizzles;
use bevy::render::primitives::Aabb;
use bevy::window::PrimaryWindow;
use bevy_mouse_tracking_plugin::MainCamera;

use crate::ui::windows::object::appearance::AppearanceWindow;
use crate::ui::windows::object::collisions::CollisionsWindow;
use crate::ui::windows::object::combine_shapes::CombineShapesWindow;
use crate::ui::windows::object::controller::ControllerWindow;
use crate::ui::windows::object::geom_actions::GeometryActionsWindow;
use crate::ui::windows::object::information::InformationWindow;
use crate::ui::windows::object::laser::LaserWindow;
use crate::ui::windows::object::material::MaterialWindow;
use crate::ui::windows::object::plot::PlotWindow;
use crate::ui::windows::object::script::ScriptMenuWindow;
use crate::ui::windows::object::selection::SelectionWindow;

use crate::ui::windows::object::velocities::VelocitiesWindow;

use crate::ui::windows::scene::background::BackgroundWindow;

use crate::ui::menu_item::MenuItem;
use crate::ui::windows::object::hinge::HingeWindow;

systems! {
    MenuWindow::show,
    handle_zoom_to_scene,
    event ZoomToScene
}

#[derive(Default, Component)]
pub struct MenuWindow {
    hovered_item: Option<(MenuId, Duration)>,
    selected_item: Option<(MenuId, Entity)>,
}

impl MenuWindow {
    fn show(
        mut wnds: Query<(Entity, Option<&Parent>, &mut MenuWindow, &mut InitialPos)>,
        is_temp: Query<Option<&TemporaryWindow>>,
        time: Res<Time>,
        mut egui_ctx: EguiContexts,
        icons: Res<GuiIcons>,
        mut commands: Commands,
        entity_info: Query<(
            Option<&ColorComponent>,
            Option<&Velocity>,
            Option<&CollisionGroups>,
            Option<&LaserBundle>,
            Option<&RigidBody>,
            Option<&MotorComponent>,
        )>,
        mut cameras: Query<&mut Transform, With<MainCamera>>,
        mut zoom2scene: EventWriter<ZoomToScene>
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (wnd_id, entity, mut info_wnd, mut initial_pos) in wnds.iter_mut() {
            let entity = entity.map(Parent::get);
            egui::Window::new("context menu")
                .default_size(egui::Vec2::ZERO)
                .resizable(false)
                .subwindow(wnd_id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    if let Some((_, id)) = info_wnd.selected_item {
                        if matches!(is_temp.get(id), Err(_) | Ok(None)) {
                            commands.entity(wnd_id).despawn_recursive();
                        }
                    }

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
                                let us_selected = matches!(info_wnd.selected_item, Some((id, _)) if id == our_id);
                                let menu = ui.add(MenuItem::menu($icon, $text.to_string(), icons.arrow_right).selected(us_selected));

                                if !us_selected {
                                    let selected = match info_wnd.hovered_item {
                                        Some((id, at)) if id == our_id && (time.elapsed() - at) > Duration::from_millis(500) => true,
                                        _ => menu.clicked()
                                    };

                                    if selected {
                                        info!("clicked: {}", $text);

                                        if let Some((_, id)) = info_wnd.selected_item {
                                            commands.get_entity(id).map(|mut ent| _ = ent.despawn_recursive());
                                        }

                                        info!("rect: {:?}", menu.rect);

                                        let new_wnd = commands.spawn((
                                            <$wnd as Default>::default(),
                                            InitialPos::initial(menu.rect.right_top())
                                        )).id();

                                        info!("new wnd: {:?}", new_wnd);

                                        if let Some(id) = entity {
                                            commands.entity(id).push_children(&[new_wnd]);
                                        }

                                        info_wnd.selected_item = Some((our_id, new_wnd));
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
                            let info = entity_info.get(id).expect("Missing entity info");

                            if item!("Erase", erase) {
                                commands.entity(id).despawn_recursive();
                            }
                            if item!("Mirror", mirror) {}
                            if item!("Show plot", plot) {
                                commands.entity(id).with_children(|parent| {
                                    parent.spawn((PlotWindow::default(), InitialPos::persistent(pos2(100.0, 100.0))));
                                });
                                commands.entity(wnd_id).despawn_recursive();
                            }
                            ui.add(Separator::default().horizontal());

                            menu!("Selection", /, SelectionWindow);
                            if info.0.is_some() {
                                menu!("Appearance", color, AppearanceWindow);
                            }
                            //menu!("Text", text, TextWindow);
                            if info.4.is_some() {
                                menu!("Material", material, MaterialWindow);
                            }
                            if info.1.is_some() {
                                menu!("Velocities", velocity, VelocitiesWindow);
                            }
                            if info.5.is_some() {
                                menu!("Axles", hinge, HingeWindow);
                            }
                            if info.3.is_some() {
                                menu!("Laser pens", lasermenu, LaserWindow);
                            }
                            menu!("Information", info, InformationWindow);
                            if info.2.is_some() {
                                menu!("Collision layers", collisions, CollisionsWindow);
                            }
                            if info.4.is_some() {
                                menu!("Geometry actions", /, GeometryActionsWindow);
                            }
                            menu!("Combine shapes", csg, CombineShapesWindow);
                            menu!("Controller", controller, ControllerWindow);
                            menu!("Script menu", /, ScriptMenuWindow);
                        }
                        None => {
                            if item!("Zoom to scene", zoom2scene) {
                                zoom2scene.send(ZoomToScene);
                            }
                            if item!("Default view") {
                                let mut camera = cameras.single_mut();
                                camera.translation = Vec3::new(0.0, 2.0, CAMERA_Z);
                                let scale = 1.0 / 182.0; // todo: depends on window size
                                camera.scale = Vec3::new(scale, scale, 1.0);
                            }
                            menu!("Background", color, BackgroundWindow);
                        }
                    }
                });
        }
    }
}

#[derive(Event)]
struct ZoomToScene;

fn handle_zoom_to_scene(
    mut events: EventReader<ZoomToScene>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    bboxes: Query<(&Transform, &Aabb), Without<MainCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>
) {
    let prim = windows.get_single().unwrap();
    const FIT_MARGIN: f32 = 0.66;
    let win_size = Vec2::new(prim.width(), prim.height()) * FIT_MARGIN;

    let mut camera = cameras.single_mut();

    for _ in events.iter() {
        let bbox = bboxes
            .iter()
            .map(|(xform, bbox)| Rect::from_center_half_size(xform.translation.xy(), bbox.half_extents.xy()))
            .fold(Rect::default(), |a, b| a.union(b));

        camera.translation = bbox.center().extend(CAMERA_Z);

        let scale = f32::max(bbox.width() / win_size.x, bbox.height() / win_size.y);
        camera.scale = Vec3::new(scale, scale, 1.0);
    }
}

type MenuId = &'static str;