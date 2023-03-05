use crate::mouse::r#move::MouseLongOrMoved;
use crate::mouse::select::SelectUnderMouseEvent;
use crate::tools::add_object::AddObjectEvent;
use crate::tools::pan;
use crate::tools::pan::PanEvent;
use crate::tools::r#move::MoveEvent;
use crate::tools::rotate::RotateEvent;
use crate::ui::selection_overlay::{Overlay, OverlayState};
use crate::ui::{ContextMenuEvent, EntitySelection, UiState};
use crate::UnfreezeEntityEvent;
use crate::UsedMouseButton;
use crate::Despawn;
use bevy::input::Input;
use bevy::log::info;
use bevy::prelude::{
    Commands, EventWriter, MouseButton, Query, Res, ResMut, Time, Transform, Windows,
};
use bevy::utils::Duration;
use bevy_egui::EguiContext;
use bevy_mouse_tracking_plugin::{MousePos, MousePosWorld};
use pan::PanState;

pub fn left_release(
    mouse_button_input: Res<Input<MouseButton>>,
    mut commands: Commands,
    screen_pos: Res<MousePos>,
    mut ui_state: ResMut<UiState>,
    mouse_pos: Res<MousePosWorld>,
    mut add_obj: EventWriter<AddObjectEvent>,
    mut unfreeze: EventWriter<UnfreezeEntityEvent>,
    mut select_mouse: EventWriter<SelectUnderMouseEvent>,
    _context_menu: EventWriter<ContextMenuEvent>,
    mut overlay: ResMut<OverlayState>,
    _windows: Res<Windows>,
) {
    use crate::tools::ToolEnum::*;
    use bevy::math::Vec3Swizzles;
    let screen_pos = **screen_pos;
    let pos = mouse_pos.xy();

    macro_rules! process_button {
        ($button: expr, $state_pos:expr, $state_button:expr, $click_act:expr) => {
            'thing: {
                let pressed = mouse_button_input.pressed($button.into());
                if pressed {
                    break 'thing;
                }
                let Some((_at, click_pos, click_pos_screen)) = $state_pos else { break 'thing; };
                let selected = std::mem::replace(&mut $state_button, None);
                info!("resetting state");
                $state_pos = None;
                let Some(tool) = selected else { break 'thing };
                // remove selection overlays
                if ui_state.mouse_button == Some($button) {
                    ui_state.mouse_button = None;
                }
                *overlay = OverlayState { draw_ent: None };
                match tool {
                    Box(Some(ent)) => {
                        commands.entity(ent).insert(Despawn::Single);
                    }
                    Circle(Some(ent)) => {
                        commands.entity(ent).insert(Despawn::Single);
                    }
                    Rotate(Some(state)) => {
                        commands.entity(state.overlay_ent).insert(Despawn::Recursive);
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
                    }
                    Circle(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                        add_obj.send(AddObjectEvent::Circle {
                            center: click_pos,
                            radius: (pos - click_pos).length(),
                        });
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
                        add_obj.send(AddObjectEvent::Hinge(pos));
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
                    _ => $click_act,
                }
            }
        };
    }

    process_button!(
        UsedMouseButton::Left,
        ui_state.mouse_left_pos,
        ui_state.mouse_left,
        {
            info!("selecting under mouse");
            select_mouse.send(SelectUnderMouseEvent {
                pos,
                open_menu: false,
            });
        }
    );
    process_button!(
        UsedMouseButton::Right,
        ui_state.mouse_right_pos,
        ui_state.mouse_right,
        {
            info!("selecting under mouse");
            select_mouse.send(SelectUnderMouseEvent {
                pos,
                open_menu: true,
            });
        }
    );
}

pub fn left_pressed(
    mouse_button_input: Res<Input<MouseButton>>,
    mut ui_state: ResMut<UiState>,
    mouse_pos: Res<MousePosWorld>,
    screen_pos: Res<MousePos>,
    mut egui_ctx: ResMut<EguiContext>,
    mut ev_long_or_moved: EventWriter<MouseLongOrMoved>,
    mut ev_pan: EventWriter<PanEvent>,
    mut ev_move: EventWriter<MoveEvent>,
    mut ev_rotate: EventWriter<RotateEvent>,
    mut overlay: ResMut<OverlayState>,
    time: Res<Time>,
    xform: Query<&Transform>,
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

    macro_rules! process_button {
        ($button:expr, $tool:expr, $state_pos:expr, $state_button:expr) => {
            'thing: {
                let button = $button;
                let tool = $tool;
                let pressed = mouse_button_input.pressed(button.into());

                if !pressed {
                    break 'thing;
                }
                if let Some((at, click_pos, click_pos_screen)) = $state_pos {
                    match $state_button {
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
                                $state_pos = None;
                                $state_button = None;
                            }
                        }
                        Some(Rotate(Some(state))) => {
                            if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                                ev_rotate.send(RotateEvent {
                                    entity,
                                    orig_obj_rot: state.orig_obj_rot,
                                    click_pos,
                                    mouse_pos: pos,
                                    scale: state.scale
                                });
                                let xf = xform.get(entity).expect("Missing xform");
                                *overlay = OverlayState {
                                    draw_ent: Some((
                                        state.overlay_ent,
                                        Overlay::Rotate(2.0 * xf.rotation.z.asin(), state.scale, state.orig_obj_rot.z, click_pos),
                                        xf.translation.xy(),
                                    )),
                                };
                            } else {
                                $state_pos = None;
                                $state_button = None;
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
                            let long_press = time.elapsed() - at > Duration::from_millis(200);
                            let moved = (click_pos - pos).length() > 0.0;
                            let long_or_moved = long_press || moved;
                            if long_or_moved {
                                info!("sending long/moved (button was {:?})", $state_button);
                                ev_long_or_moved.send(MouseLongOrMoved(tool, click_pos, $button));
                            }
                        }
                    }
                } else if mouse_button_input.just_pressed(button.into())
                    && !egui_ctx.ctx_mut().is_using_pointer()
                    && !egui_ctx.ctx_mut().is_pointer_over_area()
                {
                    info!("button pressed ({:?})", button);
                    $state_button = Some(tool);
                    $state_pos = Some((time.elapsed(), pos, screen_pos));
                    if ui_state.mouse_button == None {
                        ui_state.mouse_button = Some(button);
                    }
                }
            }
        };
    }

    process_button!(
        UsedMouseButton::Left,
        match ui_state.mouse_right {
            Some(_x) => Pan(None),
            None => ui_state.toolbox_selected,
        },
        ui_state.mouse_left_pos,
        ui_state.mouse_left
    );
    process_button!(
        UsedMouseButton::Right,
        match ui_state.mouse_left {
            Some(_x) => Pan(None),
            None => Rotate(None),
        },
        ui_state.mouse_right_pos,
        ui_state.mouse_right
    );
}
