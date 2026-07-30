use crate::mouse_tracking::MainCamera;
use crate::objects::laser::LaserSettings;
use crate::objects::spring::{SpringEndHandle, SpringObject};
use crate::objects::tracer::TracerSettings;
use crate::objects::thruster::ThrusterSettings;
use crate::objects::{ColorComponent, MotorComponent};
use crate::tools::ToolIcons;
use crate::tools::add_object::{AttachmentLinks, despawn_attachment_links};
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow, TemporaryWindow, WindowSelectionTarget};
use crate::{CAMERA_Z, egui_systems};
use avian2d::prelude::*;
use bevy::camera::primitives::Aabb;
use bevy::math::Vec3Swizzles;
use bevy::prelude::ChildOf;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui::{Separator, pos2};
use bevy_egui::{EguiContexts, egui};
use std::time::Duration;

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
use crate::ui::windows::object::spring::SpringWindow;
use crate::ui::windows::object::tracer::TracerWindow;
use crate::ui::windows::object::thruster::ThrusterWindow;

use crate::ui::windows::object::velocities::VelocitiesWindow;

use crate::ui::windows::scene::background::BackgroundWindow;

use crate::ui::menu_item::MenuItem;
use crate::ui::windows::object::axle::AxleWindow;

egui_systems! {
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
        mut wnds: Query<(
            Entity,
            Option<&ChildOf>,
            Option<&WindowSelectionTarget>,
            &mut MenuWindow,
            &mut InitialPos,
        )>,
        is_temp: Query<Option<&TemporaryWindow>>,
        time: Res<Time>,
        mut egui_ctx: EguiContexts,
        icons: Res<GuiIcons>,
        tool_icons: Res<ToolIcons>,
        mut commands: Commands,
        entity_info: Query<(
            Option<&ColorComponent>,
            Option<&LinearVelocity>,
            Option<&CollisionLayers>,
            Option<&LaserSettings>,
            Option<&RigidBody>,
            Option<&MotorComponent>,
            Option<&SpringObject>,
            Option<&SpringEndHandle>,
            Option<&AttachmentLinks>,
            Option<&TracerSettings>,
            Option<&ThrusterSettings>,
        )>,
        mut cameras: Query<&mut Transform, With<MainCamera>>,
        mut zoom2scene: MessageWriter<ZoomToScene>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (wnd_id, entity, target, mut info_wnd, mut initial_pos) in wnds.iter_mut() {
            let targets = target
                .map(|target| target.iter().collect::<Vec<_>>())
                .unwrap_or_else(|| entity.map(ChildOf::parent).into_iter().collect());
            egui::Window::new("context menu")
                .default_size(egui::Vec2::ZERO)
                .resizable(false)
                .subwindow(wnd_id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    if let Some((_, id)) = info_wnd.selected_item {
                        if matches!(is_temp.get(id), Err(_) | Ok(None)) {
                            commands.entity(wnd_id).despawn();
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
                                        if let Some((_, id)) = info_wnd.selected_item {
                                            let _ = commands.get_entity(id).map(|mut ent| {
                                                ent.despawn();
                                            });
                                        }

                                        let new_wnd = commands.spawn((
                                            <$wnd as Default>::default(),
                                            WindowSelectionTarget::from_entities(targets.iter().copied()),
                                            InitialPos::initial(menu.rect.right_top())
                                        )).id();

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
                            ($text:literal, $icon:expr, $wnd:ty) => {
                                menu!(@ $text, Some($icon), $wnd);
                            };
                            ($text:literal, /, $wnd:ty) => {
                                menu!(@ $text, None, $wnd);
                            };
                        }

                    if !targets.is_empty() {
                        let has = |mut f: Box<dyn FnMut(&(
                            Option<&ColorComponent>,
                            Option<&LinearVelocity>,
                            Option<&CollisionLayers>,
                            Option<&LaserSettings>,
                            Option<&RigidBody>,
                            Option<&MotorComponent>,
                            Option<&SpringObject>,
                            Option<&SpringEndHandle>,
                            Option<&AttachmentLinks>,
                            Option<&TracerSettings>,
                            Option<&ThrusterSettings>,
                        )) -> bool>| {
                            targets.iter().copied().any(|id| {
                                entity_info.get(id).ok().is_some_and(|info| f(&info))
                            })
                        };

                        if item!("Erase", erase) {
                            for id in targets.iter().copied() {
                                if let Ok(info) = entity_info.get(id) {
                                    despawn_attachment_links(commands, info.8);
                                }
                                commands.entity(id).despawn();
                            }
                        }
                        if item!("Mirror", mirror) {}
                        if item!("Show plot", plot) {
                            commands.spawn((
                                PlotWindow::default(),
                                WindowSelectionTarget::from_entities(targets.iter().copied()),
                                InitialPos::persistent(pos2(100.0, 100.0)),
                            ));
                            commands.entity(wnd_id).despawn();
                        }
                        ui.add(Separator::default().horizontal());

                        menu!("Selection", /, SelectionWindow);
                        if has(Box::new(|info| info.0.is_some())) {
                            menu!("Appearance", color, AppearanceWindow);
                        }
                        //menu!("Text", text, TextWindow);
                        if has(Box::new(|info| info.4.is_some())) {
                            menu!("Material", material, MaterialWindow);
                        }
                        if has(Box::new(|info| info.1.is_some())) {
                            menu!("Velocities", velocity, VelocitiesWindow);
                        }
                        if has(Box::new(|info| info.6.is_some())) {
                            menu!("Springs", /, SpringWindow);
                        }
                        if has(Box::new(|info| info.5.is_some())) {
                            menu!("Axles", hinge, AxleWindow);
                        }
                        if has(Box::new(|info| info.9.is_some())) {
                            menu!("Tracers", tool_icons.egui_icon_tracer, TracerWindow);
                        }
                        if has(Box::new(|info| info.3.is_some())) {
                            menu!("Laser pens", lasermenu, LaserWindow);
                        }
                        if has(Box::new(|info| info.10.is_some())) {
                            menu!("Thrusters", tool_icons.egui_icon_thruster, ThrusterWindow);
                        }
                        menu!("Information", info, InformationWindow);
                        if has(Box::new(|info| info.2.is_some())) {
                            menu!("Collision layers", collisions, CollisionsWindow);
                        }
                        if has(Box::new(|info| info.4.is_some())) {
                            menu!("Geometry actions", /, GeometryActionsWindow);
                        }
                        menu!("Combine shapes", csg, CombineShapesWindow);
                        menu!("Controller", controller, ControllerWindow);
                        menu!("Script menu", /, ScriptMenuWindow);
                    } else {
                        if item!("Zoom to scene", zoom2scene) {
                            zoom2scene.write(ZoomToScene);
                        }
                        if item!("Default view") {
                            let mut camera = cameras.single_mut().unwrap();
                            camera.translation = Vec3::new(0.0, 2.0, CAMERA_Z);
                            let scale = 1.0 / 182.0; // todo: depends on window size
                            camera.scale = Vec3::new(scale, scale, 1.0);
                        }
                        menu!("Background", color, BackgroundWindow);
                    }
                });
        }
    }
}

#[derive(Message)]
struct ZoomToScene;

fn handle_zoom_to_scene(
    mut events: MessageReader<ZoomToScene>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    bboxes: Query<(&Position, &Aabb), Without<MainCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let prim = windows.single().unwrap();
    const FIT_MARGIN: f32 = 0.66;
    let win_size = Vec2::new(prim.width(), prim.height()) * FIT_MARGIN;

    let mut camera = cameras.single_mut().unwrap();

    for _ in events.read() {
        let bbox = bboxes
            .iter()
            .map(|(xform, bbox)| Rect::from_center_half_size(xform.0, bbox.half_extents.xy()))
            .fold(Rect::default(), |a, b| a.union(b));

        camera.translation = bbox.center().extend(CAMERA_Z);

        let scale = f32::max(bbox.width() / win_size.x, bbox.height() / win_size.y);
        camera.scale = Vec3::new(scale, scale, 1.0);
    }
}

type MenuId = &'static str;
