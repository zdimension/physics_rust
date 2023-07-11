use bevy::prelude::*;
use bevy_egui::egui::TextureId;
use bevy_egui::{EguiUserTextures};

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
    ($type:ident, $root:literal, [$($name:ident),*$(,)?]) => {
        #[derive(Resource, Copy, Clone)]
        pub struct $type {
            $(
                pub $name: TextureId,
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
                            let egui_id = egui_ctx.add_image(handle);
                            egui_id
                        },
                    )*
                }
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
                            let egui_id = egui_ctx.add_image(handle.clone());
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
    [hinge_background, hinge_balls, hinge_inner, laserpen]
);
