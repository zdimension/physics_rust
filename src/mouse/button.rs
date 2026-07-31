use crate::mouse_tracking::{MainCamera, MousePos, MousePosWorld};
use avian2d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::log::info;
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use std::time::Duration;

use pan::PanState;

use crate::mouse::r#move::MouseLongOrMoved;
use crate::mouse::select::{
    SelectEnclosedEvent, SelectUnderMouseEvent, SelectionConfig, SelectionMode,
    collider_under_point,
};
use crate::objects::spring::{FinishSpringEvent, UpdateSpringPreviewEvent};
use crate::tools::add_object::{
    AddAxleEvent, AddObjectEvent, AttachmentKind, PlaceAttachmentEvent,
};
use crate::tools::r#move::MoveEvent;
use crate::tools::pan;
use crate::tools::pan::PanEvent;
use crate::tools::plane::plane_outward_normal;
use crate::tools::rotate::{RotateEvent, rotation_delta};
use crate::tools::zoom::ZoomEvent;
use crate::ui::selection_overlay::{Overlay, OverlayState};
use crate::ui::{PointerToolState, Selected, ToolboxState};
//use crate::Despawn;
use crate::CustomForceDespawn;
use crate::UnfreezeEntityEvent;
use crate::UsedMouseButton;
use crate::tools::drag::DragEvent;

#[derive(SystemParam)]
pub struct ToolInteractionState<'w, 's> {
    pointer: ResMut<'w, PointerToolState>,
    toolbox: Res<'w, ToolboxState>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    selected: Query<'w, 's, Entity, With<Selected>>,
}

#[derive(SystemParam)]
pub struct AttachmentMoveCommit<'w, 's> {
    attachments: Query<'w, 's, (), With<AttachmentKind>>,
    place_attachment: MessageWriter<'w, PlaceAttachmentEvent>,
}

#[derive(SystemParam)]
pub struct LaserClickTargets<'w, 's> {
    rigid_bodies: Query<'w, 's, (), With<RigidBody>>,
    colliders: Query<
        'w,
        's,
        (Entity, &'static Collider, &'static GlobalTransform),
        Without<ColliderDisabled>,
    >,
}

pub fn left_release(
    mouse_button_input: Res<ButtonInput<MouseButton>>,
    mut commands: Commands,
    screen_pos: Res<MousePos>,
    mut tool_state: ToolInteractionState,
    mouse_pos: Res<MousePosWorld>,
    mut add_obj: MessageWriter<AddObjectEvent>,
    mut unfreeze: MessageWriter<UnfreezeEntityEvent>,
    mut select_mouse: MessageWriter<SelectUnderMouseEvent>,
    mut select_enclosed: MessageWriter<SelectEnclosedEvent>,
    mut ev_spring_finish: MessageWriter<FinishSpringEvent>,
    mut overlay: ResMut<OverlayState>,
    selection_config: Res<SelectionConfig>,
    laser_click_targets: LaserClickTargets,
    cameras: Query<&Transform, With<MainCamera>>,
    mut ev_zoom: MessageWriter<ZoomEvent>,
    mut attachment_move_commit: AttachmentMoveCommit,
) {
    use crate::tools::ToolEnum::*;
    use bevy::math::Vec3Swizzles;
    let screen_pos = **screen_pos;
    let pos = mouse_pos.xy();
    let selected_entities = tool_state.selected.iter().collect::<Vec<_>>();
    let selection_mode = if tool_state.keys.pressed(KeyCode::ControlLeft)
        || tool_state.keys.pressed(KeyCode::ControlRight)
    {
        SelectionMode::Toggle
    } else {
        SelectionMode::Replace
    };

    let pointer_state = &mut *tool_state.pointer;
    let mut rebase_active_zoom = false;
    for (button, state_pos, state_button, sel_ev) in [
        (
            UsedMouseButton::Left,
            &mut pointer_state.mouse_left_pos,
            &mut pointer_state.mouse_left,
            SelectUnderMouseEvent {
                pos,
                mode: selection_mode,
                open_menu: false,
            },
        ),
        (
            UsedMouseButton::Right,
            &mut pointer_state.mouse_right_pos,
            &mut pointer_state.mouse_right,
            SelectUnderMouseEvent {
                pos,
                mode: selection_mode,
                open_menu: true,
            },
        ),
    ] {
        'thing: {
            let pressed = mouse_button_input.pressed(button.into());
            if pressed {
                break 'thing;
            }
            let Some((_at, click_pos, click_pos_screen)) = *state_pos else {
                break 'thing;
            };
            let selected = state_button.take();
            info!("resetting state");
            *state_pos = None;
            let Some(tool) = selected else { break 'thing };
            // remove selection overlays
            if pointer_state.mouse_button == Some(button) {
                pointer_state.mouse_button = None;
            }
            *overlay = OverlayState { draw_ent: None };
            match &tool {
                Box(Some(ent)) => {
                    commands.entity(*ent).despawn();
                }
                Circle(Some(ent)) => {
                    commands.entity(*ent).despawn();
                }
                Plane(Some(state)) => {
                    commands.entity(state.overlay_ent).despawn();
                }
                Rotate(Some(state)) => {
                    commands.entity(state.overlay_ent).despawn();
                }
                Drag(Some(state)) => {
                    commands
                        .entity(state.drag_entity)
                        .insert(CustomForceDespawn);
                }
                _ => {}
            }
            match tool {
                Move(Some(state)) => {
                    for entity in selected_entities.iter().copied() {
                        if attachment_move_commit.attachments.contains(entity) {
                            attachment_move_commit
                                .place_attachment
                                .write(PlaceAttachmentEvent {
                                    entity,
                                    pos: pos + state.primary_delta,
                                });
                        } else if laser_click_targets.rigid_bodies.contains(entity) {
                            unfreeze.write(UnfreezeEntityEvent { entity });
                        }
                    }
                }
                Rotate(Some(_)) => {
                    for entity in selected_entities.iter().copied() {
                        if laser_click_targets.rigid_bodies.contains(entity) {
                            unfreeze.write(UnfreezeEntityEvent { entity });
                        }
                    }
                }
                Box(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                    if selection_config.select_by_encircling {
                        select_enclosed.write(SelectEnclosedEvent {
                            start: click_pos,
                            end: pos,
                            mode: selection_mode,
                            open_menu: false,
                            fallback_add_object: Some(AddObjectEvent::Box {
                                pos: click_pos,
                                size: pos - click_pos,
                            }),
                        });
                    } else {
                        add_obj.write(AddObjectEvent::Box {
                            pos: click_pos,
                            size: pos - click_pos,
                        });
                    }
                    *state_button = Some(Box(None));
                }
                Circle(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                    add_obj.write(AddObjectEvent::Circle {
                        center: click_pos,
                        radius: (pos - click_pos).length(),
                    });
                    *state_button = Some(Circle(None));
                }
                Plane(Some(state)) => {
                    add_obj.write(AddObjectEvent::Plane {
                        point: click_pos,
                        outward_normal: plane_outward_normal(click_pos, pos, state.scale),
                        color: state.color,
                    });
                    *state_button = Some(Plane(None));
                }
                Plane(None) => {}
                Spring(Some(state)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                    ev_spring_finish.write(FinishSpringEvent {
                        state,
                        end_pos: pos,
                    });
                    *state_button = Some(Spring(None));
                }
                Spring(Some(state)) => {
                    commands.entity(state.preview).despawn();
                }
                Thruster(()) => {
                    let under_mouse = collider_under_point(pos, &laser_click_targets.colliders);
                    match under_mouse {
                        Some(entity) if laser_click_targets.rigid_bodies.contains(entity) => {
                            add_obj.write(AddObjectEvent::Thruster(pos));
                        }
                        Some(_) => {
                            select_mouse.write(sel_ev);
                        }
                        None => {}
                    }
                }
                Fix(()) => {
                    add_obj.write(AddObjectEvent::Fix(pos));
                }
                Axle(()) => {
                    add_obj.write(AddObjectEvent::Axle(AddAxleEvent::Mouse(pos)));
                }
                Laser(()) => {
                    let under_mouse = collider_under_point(pos, &laser_click_targets.colliders);
                    let clicked_physical_object =
                        under_mouse.map(|entity| laser_click_targets.rigid_bodies.contains(entity));
                    if laser_click_should_place(clicked_physical_object) {
                        add_obj.write(AddObjectEvent::Laser(pos));
                    } else {
                        select_mouse.write(sel_ev);
                    }
                }
                Tracer(()) => {
                    add_obj.write(AddObjectEvent::Tracer(pos));
                }
                Pan(Some(_)) => {
                    rebase_active_zoom = true;
                }
                Zoom(Some(_)) | Drag(Some(_)) => {
                    //
                }
                _ => {
                    info!("selecting under mouse");
                    select_mouse.write(sel_ev);
                }
            }
        }
    }

    if rebase_active_zoom && mouse_button_input.pressed(MouseButton::Left) {
        if let Some(Zoom(Some(state))) = &mut pointer_state.mouse_left {
            if let Ok(camera) = cameras.single() {
                state.orig_camera_pos = camera.translation.xy();
                state.orig_camera_scale = camera.scale.x;
                state.click_pos_screen = screen_pos;
                ev_zoom.write(ZoomEvent {
                    state: *state,
                    mouse_pos_screen: screen_pos,
                });
            }
        }
    }
}

fn laser_click_should_place(clicked_physical_object: Option<bool>) -> bool {
    clicked_physical_object.unwrap_or(true)
}

pub fn left_pressed(
    mouse_button_input: Res<ButtonInput<MouseButton>>,
    mut tool_state: ToolInteractionState,
    mouse_pos: Res<MousePosWorld>,
    screen_pos: Res<MousePos>,
    mut egui_ctx: EguiContexts,
    mut ev_long_or_moved: MessageWriter<MouseLongOrMoved>,
    mut ev_pan: MessageWriter<PanEvent>,
    mut ev_move: MessageWriter<MoveEvent>,
    mut ev_rotate: MessageWriter<RotateEvent>,
    mut ev_zoom: MessageWriter<ZoomEvent>,
    mut ev_drag: MessageWriter<DragEvent>,
    mut ev_spring_preview: MessageWriter<UpdateSpringPreviewEvent>,
    mut overlay: ResMut<OverlayState>,
    time: Res<Time>,
) {
    let screen_pos = **screen_pos;

    use crate::tools::ToolEnum::*;
    use bevy::math::Vec3Swizzles;

    enum HandleStatus {
        Handled,
        HandledAndStop,
        NotHandled,
    }

    let pos = mouse_pos.xy();

    let selected_tool = tool_state.toolbox.toolbox_selected.clone();
    let pointer_state = &mut *tool_state.pointer; // https://bevy-cheatbook.github.io/pitfalls/split-borrows.html
    let left_tool_if_right = match pointer_state.mouse_right_pos {
        Some(_) => Pan(None),
        None => selected_tool,
    };
    let right_tool_if_left = match pointer_state.mouse_left_pos {
        Some(_) => Pan(None),
        None => Rotate(None),
    };
    for (button, tool, state_pos, state_button) in [
        (
            UsedMouseButton::Left,
            left_tool_if_right.clone(),
            &mut pointer_state.mouse_left_pos,
            &mut pointer_state.mouse_left,
        ),
        (
            UsedMouseButton::Right,
            right_tool_if_left.clone(),
            &mut pointer_state.mouse_right_pos,
            &mut pointer_state.mouse_right,
        ),
    ] {
        'thing: {
            let pressed = mouse_button_input.pressed(button.into());

            if !pressed {
                break 'thing;
            }
            if let Some((at, click_pos, click_pos_screen)) = *state_pos {
                match state_button.as_ref() {
                    Some(Pan(Some(PanState { orig_camera_pos }))) => {
                        ev_pan.write(PanEvent {
                            orig_camera_pos: *orig_camera_pos,
                            delta: click_pos_screen - screen_pos,
                        });
                    }
                    Some(Zoom(Some(state))) => {
                        ev_zoom.write(ZoomEvent {
                            state: *state,
                            mouse_pos_screen: screen_pos,
                        });
                    }
                    Some(Move(Some(state))) => {
                        if !state.targets.is_empty() {
                            for (entity, original_pos) in state.targets.iter().copied() {
                                ev_move.write(MoveEvent {
                                    entity,
                                    pos: original_pos + (pos - click_pos),
                                });
                            }
                        } else {
                            info!("move target disappeared, resetting");
                            *state_pos = None;
                            *state_button = None;
                        }
                    }
                    Some(Rotate(Some(state))) => {
                        if !state.targets.is_empty() {
                            let current_angle =
                                state.current_angle + rotation_delta(state, click_pos, pos);
                            ev_rotate.write(RotateEvent {
                                state: state.clone(),
                                click_pos,
                                mouse_pos: pos,
                            });
                            *overlay = OverlayState {
                                draw_ent: Some((
                                    state.overlay_ent,
                                    Overlay::Rotate(
                                        current_angle,
                                        state.scale,
                                        state.current_angle,
                                        click_pos,
                                    ),
                                    state.pivot,
                                )),
                            };
                        } else {
                            info!("rotate target disappeared, resetting");
                            *state_pos = None;
                            *state_button = None;
                        }
                    }
                    Some(Drag(Some(state))) => {
                        ev_drag.write(DragEvent {
                            state: *state,
                            mouse_pos: pos,
                        });
                    }
                    Some(Spring(Some(state))) => {
                        ev_spring_preview.write(UpdateSpringPreviewEvent {
                            preview: state.preview,
                            end_pos: pos,
                        });
                    }
                    Some(Box(Some(draw_ent))) => {
                        *overlay = OverlayState {
                            draw_ent: Some((
                                *draw_ent,
                                Overlay::Rectangle(pos - click_pos),
                                click_pos,
                            )),
                        };
                    }
                    Some(Circle(Some(draw_ent))) => {
                        *overlay = OverlayState {
                            draw_ent: Some((
                                *draw_ent,
                                Overlay::Circle((pos - click_pos).length()),
                                click_pos,
                            )),
                        };
                    }
                    Some(Plane(Some(state))) => {
                        *overlay = OverlayState {
                            draw_ent: Some((
                                state.overlay_ent,
                                Overlay::Plane(
                                    plane_outward_normal(click_pos, pos, state.scale),
                                    state.scale,
                                ),
                                click_pos,
                            )),
                        };
                    }
                    _ => {
                        info!("current_state: {:?}", *state_button);
                        let long_press = time.elapsed() - at > Duration::from_millis(200);
                        let moved = (click_pos - pos).length() > 0.0;
                        let long_or_moved = long_press || moved;
                        if long_or_moved {
                            info!("sending long/moved (button was {:?})", state_button);
                            ev_long_or_moved.write(MouseLongOrMoved(
                                tool,
                                click_pos,
                                click_pos_screen,
                                button,
                            ));
                        }
                    }
                }
            } else if mouse_button_input.just_pressed(button.into())
                && !egui_ctx
                    .ctx_mut()
                    .expect("primary egui context")
                    .egui_is_using_pointer()
                && !egui_ctx
                    .ctx_mut()
                    .expect("primary egui context")
                    .is_pointer_over_egui()
            {
                info!("button pressed ({:?})", button);
                *state_button = Some(tool);
                *state_pos = Some((time.elapsed(), pos, screen_pos));
                if pointer_state.mouse_button.is_none() {
                    pointer_state.mouse_button = Some(button);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::laser_click_should_place;

    #[test]
    fn laser_tool_places_only_on_sky_or_physical_objects() {
        assert!(laser_click_should_place(None));
        assert!(laser_click_should_place(Some(true)));
        assert!(!laser_click_should_place(Some(false)));
    }
}
