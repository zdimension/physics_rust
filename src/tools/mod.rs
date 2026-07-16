pub(crate) mod add_object;
pub(crate) mod drag;
pub(crate) mod r#move;
pub(crate) mod pan;
pub(crate) mod rotate;

use std::collections::HashSet;

use crate::ui::images::GuiIcons;
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

                pub fn name(&self) -> &'static str {
                    match self {
                        $(
                            Self::$name(_) => stringify!($name)
                        ),*
                    }
                }
            }

            impl ToolIcons {
                fn contains_image(&self, image_id: AssetId<Image>) -> bool {
                    [
                        $(
                            self.[<icon_ $pic>].id(),
                        )*
                    ]
                    .contains(&image_id)
                }
            }
        }
    }
}

use crate::tools::drag::DragState;
use crate::tools::pan::PanState;
use crate::tools::r#move::MoveState;
use crate::tools::rotate::RotateState;
use crate::objects::spring::SpringPlacementState;
use bevy::prelude::*;
use bevy_egui::{egui::TextureId, EguiTextureHandle, EguiUserTextures};



tools_enum! {
    move => Move(Option<MoveState>),
    drag => Drag(Option<DragState>),
    rotate => Rotate(Option<RotateState>),

    box => Box(Option<Entity>),
    circle => Circle(Option<Entity>),
    
    spring => Spring(Option<SpringPlacementState>),
    fixjoint => Fix(()),
    hinge => Hinge(()),
    tracer => Tracer(()),
    laserpen => Laser(()),
    thruster => Thruster(()),

    zoom => Zoom(Option<Entity>),
    pan => Pan(Option<PanState>),
}

#[derive(Resource, Default)]
pub(crate) struct EguiImageAlphaState {
    self_modified: HashSet<AssetId<Image>>,
}

pub(crate) fn premultiply_egui_image_alpha(
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
    mut alpha_state: ResMut<EguiImageAlphaState>,
) {
    for event in image_events.read() {
        let (image_id, is_modified) = match event {
            AssetEvent::LoadedWithDependencies { id } => (*id, false),
            AssetEvent::Modified { id } => (*id, true),
            _ => continue,
        };
        if !tool_icons.contains_image(image_id) && !gui_icons.contains_image(image_id) {
            continue;
        }
        if is_modified && alpha_state.self_modified.remove(&image_id) {
            continue;
        }

        let Some(mut image) = images.get_mut(image_id) else {
            continue;
        };
        if premultiply_image_alpha(&mut image) {
            alpha_state.self_modified.insert(image_id);
        }
    }
}

fn premultiply_image_alpha(image: &mut Image) -> bool {
    let Some(data) = image.data.as_mut() else {
        return false;
    };
    if data.len() % 4 != 0 {
        return false;
    }

    premultiply_rgba(data);
    true
}

fn premultiply_rgba(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let alpha = pixel[3] as u16;
        for component in &mut pixel[..3] {
            *component = (*component as u16 * alpha / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::premultiply_rgba;

    #[test]
    fn premultiplies_straight_alpha_pixels() {
        let mut pixels = [255, 255, 255, 0, 255, 255, 255, 128, 255, 255, 255, 255];

        premultiply_rgba(&mut pixels);

        assert_eq!(pixels, [0, 0, 0, 0, 128, 128, 128, 128, 255, 255, 255, 255]);
    }
}
