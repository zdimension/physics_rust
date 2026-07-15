pub(crate) mod add_object;
pub(crate) mod drag;
pub(crate) mod r#move;
pub(crate) mod pan;
pub(crate) mod rotate;

use paste::paste;

macro_rules! tools_enum {
    ($($pic:ident => $name:ident($data:ty)),*$(,)?) => {
        #[derive(Debug, Copy, Clone)]
        pub enum ToolEnum {
            $($name($data)),*
        }

        paste! {
            #[derive(Resource)]
            pub struct ToolIcons {
                $(
                    pub [<icon_ $pic>]: Handle<Image>,
                    pub [<egui_icon_ $pic>]: TextureId
                ),*
            }

            impl FromWorld for ToolIcons {
                fn from_world(world: &mut World) -> Self {
                    let unsafe_world = world.as_unsafe_world_cell();
                    let mut egui_ctx = unsafe { unsafe_world.get_resource_mut::<EguiUserTextures>().unwrap() };
                    let asset_server = unsafe { unsafe_world.get_resource::<AssetServer>().unwrap() };
                    Self {
                        $(
                            [<icon_ $pic>]: asset_server.load(concat!("tools/", stringify!($pic), ".png")),
                            [<egui_icon_ $pic>]: {
                                let handle = asset_server.load(concat!("tools/", stringify!($pic), ".png"));
                                egui_ctx.add_image(EguiTextureHandle::Strong(handle))
                            }
                        ),*
                    }
                }
            }

            impl ToolEnum {
                pub fn is_same(&self, other: &Self) -> bool {
                    std::mem::discriminant(self) == std::mem::discriminant(other)
                }

                pub fn icon(&self, icons: impl AsRef<ToolIcons>) -> Handle<Image> {
                    let icons = icons.as_ref();
                    match self {
                        $(
                            Self::$name(_) => icons.[<icon_ $pic>].clone()
                        ),*
                    }
                }

                pub fn egui_icon(&self, icons: impl AsRef<ToolIcons>) -> TextureId {
                    let icons = icons.as_ref();
                    match self {
                        $(
                            Self::$name(_) => icons.[<egui_icon_ $pic>]
                        ),*
                    }
                }
            }
        }
    }
}

use crate::tools::drag::DragState;
use crate::tools::pan::PanState;
use crate::tools::r#move::MoveState;
use crate::tools::rotate::RotateState;
use bevy::prelude::*;
use bevy_egui::{egui::TextureId, EguiTextureHandle, EguiUserTextures};

tools_enum! {
    move => Move(Option<MoveState>),
    drag => Drag(Option<DragState>),
    rotate => Rotate(Option<RotateState>),
    box => Box(Option<Entity>),
    circle => Circle(Option<Entity>),
    spring => Spring(Option<Entity>),
    fixjoint => Fix(()),
    hinge => Hinge(()),
    tracer => Tracer(()),
    laserpen => Laser(()),
    thruster => Thruster(()),
    zoom => Zoom(Option<Entity>),
    pan => Pan(Option<PanState>),
}