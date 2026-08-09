use bevy::prelude::{App, IntoScheduleConfigs, PreUpdate};

pub(crate) mod thyme;

pub(crate) fn add_systems(app: &mut App) {
    app.init_resource::<thyme::Console>()
        .insert_non_send(thyme::ScriptEngine::default())
        .add_systems(
            PreUpdate,
            thyme::evaluate_bindings.before(crate::ui::apply_ui_scale),
        );
}
