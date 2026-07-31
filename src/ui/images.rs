use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_egui::egui::TextureId;
use bevy_egui::{EguiTextureHandle, EguiUserTextures};

pub(crate) fn transparent_image() -> Image {
    Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

pub(crate) fn load_egui_image(
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
    egui_textures: &mut EguiUserTextures,
    path: &'static str,
) -> (Handle<Image>, Handle<Image>, TextureId) {
    let source = asset_server.load(path);
    let derived = images.add(transparent_image());
    let texture = egui_textures.add_image(EguiTextureHandle::Strong(derived.clone()));
    (source, derived, texture)
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
            egui_images: Vec<(Handle<Image>, Handle<Image>)>,
        }

        impl FromWorld for $type {
            fn from_world(world: &mut World) -> Self {
                let unsafe_world = world.as_unsafe_world_cell();
                let asset_server = unsafe { unsafe_world.get_resource::<AssetServer>().unwrap() };
                let mut images = unsafe { unsafe_world.get_resource_mut::<Assets<Image>>().unwrap() };
                let mut egui_textures = unsafe { unsafe_world.get_resource_mut::<EguiUserTextures>().unwrap() };
                let mut egui_images = Vec::new();
                Self {
                    $(
                        $name: {
                            let (source, derived, texture) = load_egui_image(
                                asset_server,
                                &mut images,
                                &mut egui_textures,
                                icon_set!(@ path $root, $name $(=> $file)?),
                            );
                            egui_images.push((source, derived));
                            texture
                        },
                    )*
                    egui_images,
                }
            }
        }

        impl $type {
            pub(crate) fn egui_image_for_source(
                &self,
                image_id: AssetId<Image>,
            ) -> Option<&Handle<Image>> {
                self.egui_images
                    .iter()
                    .find_map(|(source, derived)| (source.id() == image_id).then_some(derived))
            }
        }
    }
}

macro_rules! image_set {
    ($type:ident, $root:literal, [$($name:ident),*$(,)?]) => {
        #[derive(Resource)]
        pub struct $type {
            $(
                pub $name: Handle<Image>,
            )*
        }

        impl FromWorld for $type {
            fn from_world(world: &mut World) -> Self {
                let asset_server = world.resource::<AssetServer>();
                Self {
                    $(
                        $name: asset_server.load(concat!($root, stringify!($name), ".png")),
                    )*
                }
            }
        }

        impl $type {
            pub(crate) fn contains_image(&self, image_id: AssetId<Image>) -> bool {
                [$(self.$name.id(),)*].contains(&image_id)
            }
        }
    }
}

icon_set!(
    GuiIcons,
    "gui/",
    [
        arrow_down,
        arrow_left,
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
        force_arrow,
        rotate_origo,
        spring,
        spring_attachment,
        thruster_inner,
        thruster_outer,
        thruster_thrust,
        tracer
    ]
);
