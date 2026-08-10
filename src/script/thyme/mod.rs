use std::{collections::VecDeque, fmt::Write};

use bevy::{ecs::world::World, prelude::Resource};

pub(crate) mod events;
mod native;
pub(crate) mod scene;

pub(crate) use events::PendingEvents;
pub(crate) use native::{SceneProperty, ScriptEngine};

#[derive(Default, Resource)]
pub(crate) struct Console {
    pub(crate) open: bool,
    pub(crate) input: String,
    pub(crate) output: String,
    pending: VecDeque<String>,
    history: Vec<String>,
    history_index: Option<usize>,
    history_draft: String,
}

impl Console {
    pub(crate) fn submit(&mut self) {
        let source = std::mem::take(&mut self.input);
        if !source.trim().is_empty() {
            self.history.push(source.clone());
            self.history_index = None;
            self.history_draft.clear();
            self.pending.push_back(source);
        }
    }

    pub(crate) fn history_up(&mut self) -> bool {
        let Some(index) = self
            .history_index
            .map(|index| index.saturating_sub(1))
            .or_else(|| self.history.len().checked_sub(1))
        else {
            return false;
        };
        if self.history_index.is_none() {
            self.history_draft.clone_from(&self.input);
        }
        self.history_index = Some(index);
        self.input.clone_from(&self.history[index]);
        true
    }

    pub(crate) fn history_down(&mut self) -> bool {
        let Some(index) = self.history_index else {
            return false;
        };
        if index + 1 < self.history.len() {
            self.history_index = Some(index + 1);
            self.input.clone_from(&self.history[index + 1]);
        } else {
            self.history_index = None;
            self.input.clone_from(&self.history_draft);
        }
        true
    }

    fn push_line(&mut self, line: impl std::fmt::Display) {
        if !self.output.is_empty() {
            self.output.push('\n');
        }
        write!(self.output, "{line}").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::Console;

    #[test]
    fn console_history_preserves_and_restores_the_current_draft() {
        let mut console = Console::default();
        for input in ["first", "second"] {
            console.input = input.into();
            console.submit();
        }
        console.input = "draft".into();

        assert!(console.history_up());
        assert_eq!(console.input, "second");
        assert!(console.history_up());
        assert_eq!(console.input, "first");
        assert!(console.history_down());
        assert_eq!(console.input, "second");
        assert!(console.history_down());
        assert_eq!(console.input, "draft");
    }
}

pub(crate) fn execute_console(world: &mut World) {
    let source = world.resource_mut::<Console>().pending.pop_front();
    let Some(source) = source else { return };
    let mut engine = world
        .remove_non_send::<ScriptEngine>()
        .expect("Thyme engine");

    world
        .resource_mut::<Console>()
        .push_line(format_args!("> {}", source.trim()));

    let result = engine.eval(world, source.trim());
    world.insert_non_send(engine);

    let console = &mut *world.resource_mut::<Console>();
    match result {
        Ok(value) => {
            if value != thyme::Value::Void {
                console.push_line(value);
            }
        }
        Err(error) => console.push_line(format_args!("ERROR: {error}")),
    }
}

pub(crate) fn evaluate_bindings(world: &mut World) {
    let mut engine = world
        .remove_non_send::<ScriptEngine>()
        .expect("Thyme engine");
    let errors = engine.evaluate_bindings(world);
    world.insert_non_send(engine);
    for error in errors {
        world
            .resource_mut::<Console>()
            .push_line(format_args!("ERROR: {error}"));
    }
}
