use bevy::prelude::App;

pub(crate) mod thyme;

pub(crate) fn add_systems(app: &mut App) {
    app.insert_non_send(thyme::Console::default());
}
