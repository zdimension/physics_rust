use crate::mouse::select;
use crate::mouse::select::SelectEvent;
use crate::tools::drag::{DragObject, DragState, DragTarget};
use crate::tools::pan::PanState;
use crate::tools::r#move::MoveState;
use crate::tools::rotate::RotateState;
use crate::tools::ToolEnum;
use crate::ui::UiState;
use crate::ui::images::AppIcons;
use crate::{CustomForce, InvTransformPoint, UsedMouseButton};
use bevy::math::Vec2;
use bevy::prelude::{ChildOf, Commands, Entity, GlobalTransform, Message, MessageReader, MessageWriter, Query, Res, ResMut, Transform, With, Without};
use crate::mouse_tracking::{MainCamera, MousePosWorld};
use crate::objects::spring::{self, SpringEnd, SpringPlacementState};
use crate::objects::ColorComponent;
use crate::palette::PaletteConfig;
use crate::rng::RngComponent;
use crate::tools::add_object::DepthSorter;
use avian2d::{math::*, prelude::*};

#[derive(Message)]
pub struct MouseLongOrMovedWriteback {
    event: MouseLongOrMoved,
}

impl From<MouseLongOrMoved> for MouseLongOrMovedWriteback {
    fn from(event: MouseLongOrMoved) -> Self {
        Self { event }
    }
}

pub fn mouse_long_or_moved_writeback(
    mut read: MessageReader<MouseLongOrMovedWriteback>,
    mut write: MessageWriter<MouseLongOrMoved>,
) {
    for event in read.read() {
        write.write(event.event);
    }
}

pub fn mouse_long_or_moved(
    mut events: MessageReader<MouseLongOrMoved>,
    mut ev_writeback: MessageWriter<MouseLongOrMovedWriteback>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mut ui_state: ResMut<UiState>,
    query: Query<(&GlobalTransform, &Position, &Rotation, Option<&RigidBody>), Without<MainCamera>>,
    mut commands: Commands,
    mut select_mouse: MessageWriter<SelectEvent>,
    mouse_pos: Res<MousePosWorld>,
    spatial_query: SpatialQuery,
    images: Res<AppIcons>,
    palette: Res<PaletteConfig>,
    mut rng: Query<&mut RngComponent>,
    mut z: ResMut<DepthSorter>,
    draw_objects: Query<Entity, With<crate::DrawObject>>,
) {
    use crate::tools::ToolEnum::*;
    use crate::UsedMouseButton;
    use bevy::log::info;
    use bevy::math::Vec3Swizzles;
    use avian2d::{math::*, prelude::*};
    for MouseLongOrMoved(hover_tool, pos, button) in events.read() {
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

        let scene = ui_state.scene;
        let ui_button = match button {
            UsedMouseButton::Left => &mut ui_state.mouse_left,
            UsedMouseButton::Right => &mut ui_state.mouse_right,
        };

        match hover_tool {
            Pan(None) => {
                info!("panning");
                *ui_button = Some(Pan(Some(PanState {
                    orig_camera_pos: cameras.single_mut().unwrap().translation.xy(),
                })));
            }
            Zoom(None) => {
                todo!()
            }
            _ => {
                let under_mouse =
                    select::find_under_mouse(&spatial_query, clickpos, Default::default(), |ent| {
                        let (transform, _, _, _) = query.get(ent).unwrap();
                        transform.translation_vec3a().z
                    })
                    .next();

                if matches!(
                    hover_tool,
                    Move(None) | Rotate(None) | Drag(None) | Fix(()) | Hinge(()) | Tracer(())
                ) {
                    select_mouse.write(SelectEvent {
                        entity: under_mouse,
                        open_menu: false,
                    });
                }

                match (hover_tool, under_mouse, selected_entity.map(|s| s.entity)) {
                    (Spring(None), _, _) => {
                        let start_body = select::find_under_mouse(
                            &spatial_query,
                            clickpos,
                            crate::tools::add_object::query_only_real(),
                            |ent| {
                                query
                                    .get(ent)
                                    .map(|(transform, _, _, _)| transform.translation_vec3a().z)
                                    .unwrap_or(f32::NEG_INFINITY)
                            },
                        )
                        .find(|ent| query.get(*ent).is_ok_and(|(_, _, _, body)| body.is_some()));
                        let start = if let Some(entity) = start_body {
                            let (transform, _, _, _) = query.get(entity).unwrap();
                            SpringEnd::from_body(entity, transform, clickpos)
                        } else {
                            SpringEnd::sky(clickpos)
                        };
                        let camera = cameras.single_mut().unwrap();
                        let unit_size = spring::unit_size_for_camera(&camera);
                        let preview = spring::spawn_spring(
                            &mut commands,
                            scene,
                            &images,
                            ColorComponent(
                                palette
                                    .current_palette
                                    .get_color_hsva_opaque(&mut *rng.single_mut().unwrap()),
                            ),
                            start,
                            SpringEnd::sky(curpos),
                            unit_size,
                            &mut *z,
                            true,
                        );
                        *ui_button = Some(Spring(Some(SpringPlacementState { preview, start })));
                    }
                    (Drag(None), Some(ent), _) => {
                        info!("start drag {:?}", ent);
                        let grab_local_point = query.get(ent).unwrap().0.to_local(curpos);
                        let drag_entity = commands
                            .spawn((
                                DragObject,
                                CustomForce::default(),
                                DragTarget {
                                    entity: ent,
                                    grab_local_point,
                                    mouse_pos: curpos,
                                },
                            ))
                            .insert(ChildOf(ent))
                            .id();
                        *ui_button = Some(Drag(Some(DragState {
                            entity: ent,
                            grab_local_point,
                            drag_entity,
                        })));
                    }
                    (Rotate(None), Some(under), _) => {
                        let (_, _, rot, body) = query.get(under).unwrap();
                        info!("start rotate {:?}", under);
                        *ui_button = Some(Rotate(Some(RotateState {
                            orig_obj_rot: rot.as_radians(),
                            overlay_ent: spawn_draw_object(&mut commands, &draw_objects),
                            scale: cameras.single_mut().unwrap().scale.x,
                        })));
                        if body.is_some() {
                            commands.entity(under).insert(RigidBody::Static);
                        }
                    }
                    (Rotate(None) | Move(None), None, _) => {
                        ev_writeback.write(MouseLongOrMoved(Pan(None), clickpos, *button).into());
                    }
                    (_, Some(under), Some(sel)) if under == sel => {
                        let (_, pos, _, body) = query.get(under).unwrap();
                        *ui_button = Some(Move(Some(MoveState {
                            obj_delta: pos.0 - curpos,
                        })));
                        if body.is_some() {
                            commands.entity(under).insert(RigidBody::Static);
                        }
                    }
                    (Box(None), _, _) => {
                        *ui_button = Some(Box(Some(spawn_draw_object(&mut commands, &draw_objects))));
                    }
                    (Circle(None), _, _) => {
                        *ui_button = Some(Circle(Some(spawn_draw_object(&mut commands, &draw_objects))));
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

#[derive(Copy, Clone, Message)]
pub struct MouseLongOrMoved(pub ToolEnum, pub Vec2, pub UsedMouseButton);

fn spawn_draw_object(
    commands: &mut Commands,
    draw_objects: &Query<Entity, With<crate::DrawObject>>,
) -> Entity {
    for draw_ent in draw_objects.iter() {
        if let Ok(mut draw_ent) = commands.get_entity(draw_ent) {
            draw_ent.despawn();
        }
    }
    commands.spawn(crate::DrawObject).id()
}
