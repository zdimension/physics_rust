use std::collections::HashMap;

use crate::BORDER_THICKNESS;
use bevy::asset::AssetLoader;
use bevy::asset::LoadedAsset;
use bevy::prelude::*;
use bevy::reflect::TypeUuid;
use bevy_egui::egui::epaint::Hsva;
use bevy_prototype_lyon::draw::DrawMode;
use bevy_turborand::DelegatedRng;
use serde;
use serde::Deserialize;

#[derive(Debug, Deserialize, Copy, Clone)]
#[serde(default)]
pub struct ObjectAppearance {
    pub opaque_borders: bool,
    pub draw_circle_cakes: bool,
    pub ruler: bool,
    pub show_forces: bool,
    pub protractor: bool,
    pub show_momentums: bool,
    pub show_velocities: bool,
    pub borders: bool,
}

impl Default for ObjectAppearance {
    fn default() -> Self {
        Self {
            opaque_borders: true,
            draw_circle_cakes: true,
            ruler: false,
            show_forces: false,
            protractor: false,
            show_momentums: false,
            show_velocities: false,
            borders: true,
        }
    }
}

#[derive(Debug, Deserialize, Copy, Clone)]
pub struct HsvaRange(
    #[serde(deserialize_with = "deserialize_hsva")] Hsva,
    #[serde(deserialize_with = "deserialize_hsva")] Hsva,
);

fn deserialize_hsva<'a, D>(deserializer: D) -> Result<Hsva, D::Error>
where
    D: serde::Deserializer<'a>,
{
    use serde::de::Error;
    let (h, s, v, a) = <(f32, f32, f32, f32)>::deserialize(deserializer)?;
    let h = h / 360.0;
    if h < 0.0 || h > 1.0 || s < 0.0 || s > 1.0 || v < 0.0 || v > 1.0 || a < 0.0 || a > 1.0 {
        return Err(D::Error::custom("HSVA is invalid"));
    }
    Ok(Hsva::new(h, s, v, a))
}

fn deserialize_rgba<'a, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: serde::Deserializer<'a>,
{
    use serde::de::Error;
    let (r, g, b, a) = <(f32, f32, f32, f32)>::deserialize(deserializer)?;
    if r < 0.0 || r > 1.0 || g < 0.0 || g > 1.0 || b < 0.0 || b > 1.0 || a < 0.0 || a > 1.0 {
        return Err(D::Error::custom("RGBA is invalid"));
    }
    Ok(Color::rgba_linear(r, g, b, a))
}

fn f32_between(rng: &mut impl DelegatedRng, min: f32, max: f32) -> f32 {
    if min > max {
        return f32_between(rng, max, min);
    }
    let rnd = rng.f32(); // between 0 and 1
    min + (max - min) * rnd
}

pub trait ToRgba {
    fn to_rgba(&self) -> Color;
}

impl ToRgba for Hsva {
    fn to_rgba(&self) -> Color {
        let [r, g, b, a] = self.to_rgba_unmultiplied();
        Color::rgba_linear(r, g, b, a)
    }
}

impl HsvaRange {
    pub fn rand(&self, rng: &mut impl DelegatedRng) -> Color {
        self.rand_hsva(rng).to_rgba()
    }

    pub fn rand_hsva(&self, rng: &mut impl DelegatedRng) -> Hsva {
        let hr = f32_between(rng, self.0.h, self.1.h);
        let sr = f32_between(rng, self.0.s, self.1.s).sqrt();
        let vr = f32_between(rng, self.0.v, self.1.v).cbrt();
        let ar = f32_between(rng, self.0.a, self.1.a);
        Hsva::new(hr, sr, vr, ar)
    }
}

#[derive(Debug, Deserialize, Copy, Clone)]
#[serde(default)]
pub struct Palette {
    pub object_appearance: ObjectAppearance,
    pub draw_clouds: bool,
    #[serde(deserialize_with = "deserialize_rgba")]
    pub sky_color: Color,
    #[serde(deserialize_with = "deserialize_rgba")]
    pub selection_color: Color,
    pub color_range: HsvaRange,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            object_appearance: ObjectAppearance::default(),
            draw_clouds: true,
            sky_color: Color::rgba(0.44999999, 0.55000001, 1.0000000, 1.0000000),
            selection_color: Color::rgb(0.0, 0.0, 0.0),
            color_range: HsvaRange(
                Hsva::new(0.0, 0.0, 0.0, 1.0),
                Hsva::new(359.9, 1.0, 1.0, 1.0),
            ),
        }
    }
}

#[derive(Debug, Deserialize, TypeUuid)]
#[uuid = "005a11ae-18b1-4c47-9f2e-21827d204835"]
pub struct PaletteList(pub HashMap<String, Palette>);

#[derive(Default)]
pub struct PaletteLoader;

impl AssetLoader for PaletteLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<(), bevy::asset::Error>> {
        Box::pin(async move {
            let custom_asset = ron::de::from_bytes::<PaletteList>(bytes)?;
            load_context.set_default_asset(LoadedAsset::new(custom_asset));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}

impl Palette {
    fn get_color(&self, rng: &mut impl DelegatedRng) -> Color {
        self.color_range.rand(rng)
    }

    pub fn get_color_hsva(&self, rng: &mut impl DelegatedRng) -> Hsva {
        self.color_range.rand_hsva(rng)
    }

    pub fn get_color_hsva_opaque(&self, rng: &mut impl DelegatedRng) -> Hsva {
        Hsva {
            a: 1.0,
            ..self.get_color_hsva(rng)
        }
    }

    fn get_draw_mode(&self, rng: &mut impl DelegatedRng) -> DrawMode {
        let color = self.color_range.rand_hsva(rng);
        let darkened = Hsva {
            v: color.v * 0.5,
            ..color
        };
        DrawMode::Outlined {
            fill_mode: crate::make_fill(crate::hsva_to_rgba(color)),
            outline_mode: crate::make_stroke(crate::hsva_to_rgba(darkened), BORDER_THICKNESS),
        }
    }
}

#[derive(Resource)]
pub struct PaletteConfig {
    pub palettes: Handle<PaletteList>,
    pub current_palette: Palette,
}

impl FromWorld for PaletteConfig {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.get_resource_mut::<AssetServer>().unwrap();
        let palettes = asset_server.load("palettes.ron");
        Self {
            palettes,
            current_palette: Palette::default(),
        }
    }
}
