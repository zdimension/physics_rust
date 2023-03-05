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
                    pub [<icon_ $pic>]: Handle<Image>
                ),*
            }

            impl FromWorld for ToolIcons {
                fn from_world(world: &mut World) -> Self {
                    let asset_server = world.get_resource_mut::<AssetServer>().unwrap();
                    Self {
                        $(
                            [<icon_ $pic>]: asset_server.load(concat!("tools/", stringify!($pic), ".png"))
                        ),*
                    }
                }
            }

            impl ToolEnum {
                pub fn icon(&self, icons: impl AsRef<ToolIcons>) -> Handle<Image> {
                    let icons = icons.as_ref();
                    match self {
                        $(
                            Self::$name(_) => icons.[<icon_ $pic>].clone()
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

tools_enum! {
    move => Move(Option<MoveState>),
    drag => Drag(Option<DragState>),
    rotate => Rotate(Option<RotateState>),
    box => Box(Option<Entity>),
    circle => Circle(Option<Entity>),
    spring => Spring(Option<()>),
    thruster => Thruster(Option<()>),
    fixjoint => Fix(()),
    hinge => Hinge(()),
    laserpen => Laser(()),
    tracer => Tracer(()),
    pan => Pan(Option<PanState>),
    zoom => Zoom(Option<()>),
}

impl ToolEnum {
    pub(crate) fn is_same(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}
