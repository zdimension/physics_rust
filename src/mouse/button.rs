use bevy::input::Input;
use bevy::log::info;
use bevy::prelude::{Commands, DespawnRecursiveExt, Entity, EventWriter, MouseButton, Query, Res, ResMut, Time, Transform, With};
use bevy::utils::Duration;
use bevy_egui::EguiContexts;
use bevy_mouse_tracking_plugin::{MousePos, MousePosWorld};
use bevy_xpbd_2d::{math::*, prelude::*};

use pan::PanState;

use crate::mouse::r#move::MouseLongOrMoved;
use crate::mouse::select::SelectUnderMouseEvent;
use crate::tools::add_object::{AddHingeEvent, AddObjectEvent};
use crate::tools::pan;
use crate::tools::pan::PanEvent;
use crate::tools::r#move::MoveEvent;
use crate::tools::rotate::RotateEvent;
use crate::ui::selection_overlay::{Overlay, OverlayState};
use crate::ui::{EntitySelection, UiState};
//use crate::Despawn;
use crate::tools::drag::{DragEvent, DragObject};
use crate::{CustomForceDespawn, ToRot};
use crate::UnfreezeEntityEvent;
use crate::UsedMouseButton;

pub fn left_release(
    mouse_button_input: Res<Input<MouseButton>>,
    mut commands: Commands,
    screen_pos: Res<MousePos>,
    mut ui_state: ResMut<UiState>,
    mouse_pos: Res<MousePosWorld>,
    mut add_obj: EventWriter<AddObjectEvent>,
    mut unfreeze: EventWriter<UnfreezeEntityEvent>,
    mut select_mouse: EventWriter<SelectUnderMouseEvent>,
    mut overlay: ResMut<OverlayState>,
    drag: Query<(Entity), With<DragObject>>,
) {
    use crate::tools::ToolEnum::*;
    use bevy::math::Vec3Swizzles;
    let screen_pos = **screen_pos;
    let pos = mouse_pos.xy();

    let ui_state = &mut *ui_state;
    for (button, state_pos, state_button, sel_ev) in [
        (
            UsedMouseButton::Left,
            &mut ui_state.mouse_left_pos,
            &mut ui_state.mouse_left,
            SelectUnderMouseEvent {
                pos,
                open_menu: false,
            },
        ),
        (
            UsedMouseButton::Right,
            &mut ui_state.mouse_right_pos,
            &mut ui_state.mouse_right,
            SelectUnderMouseEvent {
                pos,
                open_menu: true,
            },
        ),
    ] {
        'thing: {
            let pressed = mouse_button_input.pressed(button.into());
            if pressed {
                break 'thing;
            }
            let Some((_at, click_pos, click_pos_screen)) = *state_pos else { break 'thing; };
            let selected = state_button.take();
            info!("resetting state");
            *state_pos = None;
            let Some(tool) = selected else { break 'thing };
            // remove selection overlays
            if ui_state.mouse_button == Some(button) {
                ui_state.mouse_button = None;
            }
            *overlay = OverlayState { draw_ent: None };
            match tool {
                Box(Some(ent)) => {
                    commands.entity(ent).despawn_recursive();
                }
                Circle(Some(ent)) => {
                    commands.entity(ent).despawn_recursive();
                }
                Rotate(Some(state)) => {
                    commands
                        .entity(state.overlay_ent)
                        .despawn_recursive();
                }
                Drag(Some(state)) => {
                    commands.entity(state.drag_entity).insert(CustomForceDespawn);
                }
                _ => {}
            }
            match tool {
                Move(Some(_)) | Rotate(Some(_)) => {
                    if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                        unfreeze.send(UnfreezeEntityEvent { entity });
                    }
                }
                Box(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                    add_obj.send(AddObjectEvent::Box {
                        pos: click_pos,
                        size: pos - click_pos,
                    });
                    *state_button = Some(Box(None));
                }
                Circle(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                    add_obj.send(AddObjectEvent::Circle {
                        center: click_pos,
                        radius: (pos - click_pos).length(),
                    });
                    *state_button = Some(Circle(None));
                }
                Spring(Some(_)) => {
                    todo!()
                }
                Thruster(_) => {
                    todo!()
                }
                Fix(()) => {
                    add_obj.send(AddObjectEvent::Fix(pos));
                }
                Hinge(()) => {
                    add_obj.send(AddObjectEvent::Hinge(AddHingeEvent::Mouse(pos)));
                }
                Laser(()) => {
                    add_obj.send(AddObjectEvent::Laser(pos));
                }
                Tracer(()) => {
                    todo!()
                }
                Pan(Some(_)) | Zoom(Some(_)) | Drag(Some(_)) => {
                    //
                }
                _ => {
                    info!("selecting under mouse");
                    select_mouse.send(sel_ev);
                }
            }
        }
    }
}

pub fn left_pressed(
    mouse_button_input: Res<Input<MouseButton>>,
    mut ui_state: ResMut<UiState>,
    mouse_pos: Res<MousePosWorld>,
    screen_pos: Res<MousePos>,
    mut egui_ctx: EguiContexts,
    mut ev_long_or_moved: EventWriter<MouseLongOrMoved>,
    mut ev_pan: EventWriter<PanEvent>,
    mut ev_move: EventWriter<MoveEvent>,
    mut ev_rotate: EventWriter<RotateEvent>,
    mut ev_drag: EventWriter<DragEvent>,
    mut overlay: ResMut<OverlayState>,
    time: Res<Time>,
    xform: Query<(&Rotation, &Position)>,
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

    let ui_state = &mut *ui_state; // https://bevy-cheatbook.github.io/pitfalls/split-borrows.html
    let left_tool_if_right = match ui_state.mouse_right {
        Some(_) => Pan(None),
        None => ui_state.toolbox_selected,
    };
    let right_tool_if_left = match ui_state.mouse_left {
        Some(_) => Pan(None),
        None => Rotate(None),
    };
    for (button, tool, state_pos, state_button) in [
        (
            UsedMouseButton::Left,
            left_tool_if_right,
            &mut ui_state.mouse_left_pos,
            &mut ui_state.mouse_left,
        ),
        (
            UsedMouseButton::Right,
            right_tool_if_left,
            &mut ui_state.mouse_right_pos,
            &mut ui_state.mouse_right,
        ),
    ] {
        'thing: {
            let pressed = mouse_button_input.pressed(button.into());

            if !pressed {
                break 'thing;
            }
            if let Some((at, click_pos, click_pos_screen)) = *state_pos {
                match *state_button {
                    Some(Pan(Some(PanState { orig_camera_pos }))) => {
                        ev_pan.send(PanEvent {
                            orig_camera_pos,
                            delta: click_pos_screen - screen_pos,
                        });
                    }
                    Some(Move(Some(state))) => {
                        if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                            ev_move.send(MoveEvent {
                                entity,
                                pos: pos + state.obj_delta,
                            });
                        } else {
                            info!("move target disappeared, resetting");
                            *state_pos = None;
                            *state_button = None;
                        }
                    }
                    Some(Rotate(Some(state))) => {
                        if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                            ev_rotate.send(RotateEvent {
                                entity,
                                orig_obj_rot: state.orig_obj_rot,
                                click_pos,
                                mouse_pos: pos,
                                scale: state.scale,
                            });
                            let (rot, pos) = xform.get(entity).expect("Missing xform");
                            *overlay = OverlayState {
                                draw_ent: Some((
                                    state.overlay_ent,
                                    Overlay::Rotate(
                                        rot.as_radians(),
                                        state.scale,
                                        state.orig_obj_rot,
                                        click_pos,
                                    ),
                                    pos.0,
                                )),
                            };
                        } else {
                            info!("rotate target disappeared, resetting");
                            *state_pos = None;
                            *state_button = None;
                        }
                    }
                    Some(Drag(Some(state))) => {
                        if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                            ev_drag.send(DragEvent {
                                state: state,
                                mouse_pos: pos,
                            });
                        } else {
                            info!("drag target disappeared, resetting");
                            *state_pos = None;
                            *state_button = None;
                        }
                    }
                    Some(Box(Some(draw_ent))) => {
                        *overlay = OverlayState {
                            draw_ent: Some((
                                draw_ent,
                                Overlay::Rectangle(pos - click_pos),
                                click_pos,
                            )),
                        };
                    }
                    Some(Circle(Some(draw_ent))) => {
                        *overlay = OverlayState {
                            draw_ent: Some((
                                draw_ent,
                                Overlay::Circle((pos - click_pos).length()),
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
                            ev_long_or_moved.send(MouseLongOrMoved(tool, click_pos, button));
                        }
                    }
                }
            } else if mouse_button_input.just_pressed(button.into())
                && !egui_ctx.ctx_mut().is_using_pointer()
                && !egui_ctx.ctx_mut().is_pointer_over_area()
            {
                info!("button pressed ({:?})", button);
                *state_button = Some(tool);
                *state_pos = Some((time.elapsed(), pos, screen_pos));
                if ui_state.mouse_button.is_none() {
                    ui_state.mouse_button = Some(button);
                }
            }
        }
    }
}
