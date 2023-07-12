use crate::mouse::select;
use crate::mouse::select::SelectEvent;
use crate::tools::drag::DragState;
use crate::tools::pan::PanState;
use crate::tools::r#move::MoveState;
use crate::tools::rotate::RotateState;
use crate::tools::ToolEnum;
use crate::ui::UiState;
use crate::UsedMouseButton;
use bevy::math::Vec2;
use bevy::prelude::{
    Commands, Event, EventReader, EventWriter, Query, Res, ResMut, Transform, With, Without,
};
use bevy_mouse_tracking_plugin::{MainCamera, MousePosWorld};
use bevy_rapier2d::dynamics::RigidBody;
use bevy_rapier2d::plugin::RapierContext;

#[derive(Event)]
pub struct MouseLongOrMovedWriteback {
    event: MouseLongOrMoved,
}

impl From<MouseLongOrMoved> for MouseLongOrMovedWriteback {
    fn from(event: MouseLongOrMoved) -> Self {
        Self { event }
    }
}

pub fn mouse_long_or_moved_writeback(
    mut read: EventReader<MouseLongOrMovedWriteback>,
    mut write: EventWriter<MouseLongOrMoved>,
) {
    for event in read.iter() {
        write.send(event.event);
    }
}

pub fn mouse_long_or_moved(
    mut events: EventReader<MouseLongOrMoved>,
    mut ev_writeback: EventWriter<MouseLongOrMovedWriteback>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mut ui_state: ResMut<UiState>,
    mut query: Query<(&mut Transform, Option<&mut RigidBody>), Without<MainCamera>>,
    mut commands: Commands,
    rapier: Res<RapierContext>,
    mut select_mouse: EventWriter<SelectEvent>,
    mouse_pos: Res<MousePosWorld>,
) {
    use crate::tools::ToolEnum::*;
    use crate::{DrawObject, UsedMouseButton};
    use bevy::log::info;
    use bevy::math::Vec3Swizzles;
    use bevy_rapier2d::pipeline::QueryFilter;
    for MouseLongOrMoved(hover_tool, pos, button) in events.iter() {
        let clickpos = *pos;
        let curpos = mouse_pos.xy();
        info!("long or moved!");

        let selected_entity = ui_state.selected_entity;

        /*let (ui_button, other_button) = match button {
            UsedMouseButton::Left => (&ui_state.mouse_left, &ui_state.mouse_right),
            UsedMouseButton::Right => (&ui_state.mouse_right, &ui_state.mouse_left)
        };

        if Some(button) == ui_state.mouse_button.as_ref() && other_button.is_some() {
            continue;
        }*/
        // todo: is this really needed?

        let ui_button = match button {
            UsedMouseButton::Left => &mut ui_state.mouse_left,
            UsedMouseButton::Right => &mut ui_state.mouse_right,
        };

        match hover_tool {
            Pan(None) => {
                info!("panning");
                *ui_button = Some(Pan(Some(PanState {
                    orig_camera_pos: cameras.single_mut().translation.xy(),
                })));
            }
            Zoom(None) => {
                todo!()
            }
            _ => {
                let under_mouse =
                    select::find_under_mouse(&rapier, clickpos, QueryFilter::default(), |ent| {
                        let (transform, _) = query.get(ent).unwrap();
                        transform.translation.z
                    })
                    .next();

                if matches!(
                    hover_tool,
                    Move(None) | Rotate(None) | Fix(()) | Hinge(()) | Tracer(())
                ) {
                    select_mouse.send(SelectEvent {
                        entity: under_mouse,
                        open_menu: false,
                    });
                }

                match (hover_tool, under_mouse, selected_entity.map(|s| s.entity)) {
                    (Spring(None), _, _) => todo!(),
                    (Drag(None), Some(ent), _) => {
                        *ui_button = Some(Drag(Some(DragState {
                            entity: ent,
                            orig_obj_pos: curpos - query.get_mut(ent).unwrap().0.translation.xy(),
                        })));
                    }
                    (Rotate(None), Some(under), _) => {
                        let (transform, body) = query.get_mut(under).unwrap();
                        info!("start rotate {:?}", under);
                        *ui_button = Some(Rotate(Some(RotateState {
                            orig_obj_rot: transform.rotation,
                            overlay_ent: commands.spawn(DrawObject).id(),
                            scale: cameras.single_mut().scale.x,
                        })));
                        if let Some(mut body) = body {
                            *body = RigidBody::Fixed;
                        }
                    }
                    (Rotate(None) | Move(None), None, _) => {
                        ev_writeback.send(MouseLongOrMoved(Pan(None), clickpos, *button).into());
                    }
                    (_, Some(under), Some(sel)) if under == sel => {
                        let (transform, body) = query.get_mut(under).unwrap();
                        *ui_button = Some(Move(Some(MoveState {
                            obj_delta: transform.translation.xy() - curpos,
                        })));
                        if let Some(mut body) = body {
                            *body = RigidBody::KinematicPositionBased;
                        }
                    }
                    (Box(None), _, _) => {
                        *ui_button = Some(Box(Some(commands.spawn(DrawObject).id())));
                    }
                    (Circle(None), _, _) => {
                        *ui_button = Some(Circle(Some(commands.spawn(DrawObject).id())));
                    }
                    (tool, _, _) => {
                        dbg!(tool);
                        //todo!()
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Event)]
pub struct MouseLongOrMoved(pub ToolEnum, pub Vec2, pub UsedMouseButton);
