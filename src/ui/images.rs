use std::collections::HashSet;

use bevy::prelude::*;
use bevy_egui::egui::TextureId;
use bevy_egui::{EguiTextureHandle, EguiUserTextures};

pub struct LoadedImage {
    bevy: Handle<Image>,
    egui: TextureId,
}

impl LoadedImage {
    pub fn clone(&self) -> Handle<Image> {
        self.bevy.clone()
    }
}

macro_rules! icon_set {
    (@ path $root:literal, $name:ident) => {
        concat!($root, stringify!($name), ".png")
    };
    (@ path $root:literal, $name:ident => $file:literal) => {
        concat!($root, $file)
    };
    ($type:ident, $root:literal, [$($name:ident $(=> $file:literal)?),*$(,)?]) => {
        #[derive(Resource)]
        pub struct $type {
            $(
                pub $name: TextureId,
            )*
            image_ids: HashSet<AssetId<Image>>,
        }

            impl FromWorld for $type {
            fn from_world(world: &mut World) -> Self {
                let unsafe_world = world.as_unsafe_world_cell();
                let mut egui_ctx = unsafe { unsafe_world.get_resource_mut::<EguiUserTextures>().unwrap() };
                let asset_server = unsafe { unsafe_world.get_resource::<AssetServer>().unwrap() };
                let mut image_ids = HashSet::new();
                Self {
                    $(
                        $name: {
                            let handle = asset_server.load(icon_set!(@ path $root, $name $(=> $file)?));
                            image_ids.insert(handle.id());
                            let egui_id = egui_ctx.add_image(EguiTextureHandle::Strong(handle));
                            egui_id
                        },
                    )*
                    image_ids,
                }
            }
        }

        impl $type {
            pub(crate) fn contains_image(&self, image_id: AssetId<Image>) -> bool {
                self.image_ids.contains(&image_id)
            }
        }
    }
}

macro_rules! image_set {
    ($type:ident, $root:literal, [$($name:ident),*$(,)?]) => {
        #[derive(Resource)]
        pub struct $type {
            $(
                pub $name: LoadedImage,
            )*
        }

        impl FromWorld for $type {
            fn from_world(world: &mut World) -> Self {
                let unsafe_world = world.as_unsafe_world_cell();
                let mut egui_ctx = unsafe { unsafe_world.get_resource_mut::<EguiUserTextures>().unwrap() };
                let asset_server = unsafe { unsafe_world.get_resource::<AssetServer>().unwrap() };
                Self {
                    $(
                        $name: {
                            let handle = asset_server.load(concat!($root, stringify!($name), ".png"));
                            let egui_id = egui_ctx.add_image(EguiTextureHandle::Strong(handle.clone()));
                            LoadedImage {
                                bevy: handle,
                                egui: egui_id,
                            }
                        },
                    )*
                }
            }
        }
    }
}

icon_set!(
    GuiIcons,
    "gui/",
    [
        arrow_down,
        arrow_right,
        arrow_up,
        checkbox_off => "checkbox-off.png",
        checkbox_on => "checkbox-on.png",
        checkbox_unknown => "checkbox-unknown.png",
        collisions,
        color,
        controller,
        csg,
        erase,
        gravity,
        hinge,
        info,
        lasermenu,
        material,
        mirror,
        new,
        open,
        options,
        pause,
        play,
        plot,
        plot_clear,
        save,
        text,
        velocity,
        zoom2scene
    ]
);

image_set!(
    AppIcons,
    "app/",
    [
        fixjoint_inner,
        fixjoint_outer,
        hinge_background,
        hinge_balls,
        hinge_inner,
        hinge_motor,
        hinge_motor_ccw,
        hinge_motor_cw,
        laserpen,
        spring,
        spring_attachment,
        tracer
    ]
);
