pub(crate) mod add_object;
pub(crate) mod drag;
pub(crate) mod r#move;
pub(crate) mod pan;
pub(crate) mod rotate;
pub(crate) mod zoom;

use paste::paste;

macro_rules! tools_enum {
    ($($pic:ident => $name:ident($data:ty)),*$(,)?) => {
        #[derive(Debug, Clone)]
        pub enum ToolEnum {
            $($name($data)),*
        }

        paste! {
            #[derive(Resource)]
            pub struct ToolIcons {
                $(
                    pub [<icon_ $pic>]: Handle<Image>,
                    pub [<egui_icon_ $pic>]: TextureId,
                    [<egui_image_ $pic>]: Handle<Image>
                ),*
            }

            impl FromWorld for ToolIcons {
                fn from_world(world: &mut World) -> Self {
                    let unsafe_world = world.as_unsafe_world_cell();
                    let mut egui_ctx = unsafe { unsafe_world.get_resource_mut::<EguiUserTextures>().unwrap() };
                    let asset_server = unsafe { unsafe_world.get_resource::<AssetServer>().unwrap() };
                    let mut images = unsafe { unsafe_world.get_resource_mut::<Assets<Image>>().unwrap() };
                    $(
                        let [<loaded_ $pic>] = crate::ui::images::load_egui_image(
                            asset_server,
                            &mut images,
                            &mut egui_ctx,
                            concat!("tools/", stringify!($pic), ".png"),
                        );
                    )*
                    Self {
                        $(
                            [<icon_ $pic>]: [<loaded_ $pic>].0,
                            [<egui_image_ $pic>]: [<loaded_ $pic>].1,
                            [<egui_icon_ $pic>]: [<loaded_ $pic>].2
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

                pub fn name(&self) -> &'static str {
                    match self {
                        $(
                            Self::$name(_) => stringify!($name)
                        ),*
                    }
                }
            }

            impl ToolIcons {
                pub(crate) fn contains_image(&self, image_id: AssetId<Image>) -> bool {
                    [$(self.[<icon_ $pic>].id(),)*].contains(&image_id)
                }

                pub(crate) fn egui_image_for_source(
                    &self,
                    image_id: AssetId<Image>,
                ) -> Option<&Handle<Image>> {
                    $(
                        if self.[<icon_ $pic>].id() == image_id {
                            return Some(&self.[<egui_image_ $pic>]);
                        }
                    )*
                    None
                }
            }
        }
    }
}

use crate::objects::spring::SpringPlacementState;
use crate::tools::drag::DragState;
use crate::tools::r#move::MoveState;
use crate::tools::pan::PanState;
use crate::tools::rotate::RotateState;
use crate::tools::zoom::ZoomState;
use bevy::prelude::*;
use bevy_egui::{EguiUserTextures, egui::TextureId};

tools_enum! {
    move => Move(Option<MoveState>),
    drag => Drag(Option<DragState>),
    rotate => Rotate(Option<RotateState>),

    box => Box(Option<Entity>),
    circle => Circle(Option<Entity>),

    spring => Spring(Option<SpringPlacementState>),
    fixjoint => Fix(()),
    hinge => Axle(()),
    tracer => Tracer(()),
    laserpen => Laser(()),
    thruster => Thruster(()),

    zoom => Zoom(Option<ZoomState>),
    pan => Pan(Option<PanState>),
}
