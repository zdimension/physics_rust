use bevy::prelude::*;

use crate::systems;

systems! {
    mod menu,
    mod object,
    mod scene,
    mod scene_actions,
    mod toolbar,
    mod toolbox,
    mod menubar,
    mod options,
}