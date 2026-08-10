use bevy::{
    input::InputSystems,
    prelude::{App, IntoScheduleConfigs, Last, PreUpdate},
};

pub(crate) mod thyme;

pub(crate) fn add_systems(app: &mut App) {
    app.init_resource::<thyme::Console>()
        .init_resource::<thyme::PendingEvents>()
        .init_resource::<thyme::scene::PendingScene>()
        .insert_non_send(thyme::ScriptEngine::default())
        .add_systems(
            PreUpdate,
            thyme::evaluate_bindings.before(crate::ui::apply_ui_scale),
        )
        .add_systems(
            PreUpdate,
            thyme::events::collect_keyboard.after(InputSystems),
        )
        .add_systems(
            PreUpdate,
            thyme::events::dispatch
                .after(thyme::events::collect_keyboard)
                .after(thyme::evaluate_bindings)
                .after(crate::mouse::button::left_release),
        )
        .add_systems(Last, thyme::scene::load_pending);
}
