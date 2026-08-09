use avian2d::prelude::{Physics, PhysicsTime};
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyCode, KeyboardInput},
    },
    math::Vec2,
    prelude::{Entity, MessageReader, ResMut, Resource, Time, World},
};

use super::{Console, ScriptEngine};

enum InputEvent {
    Click {
        entity: Entity,
        pos: Vec2,
    },
    Key {
        pressed: bool,
        code: String,
        character: Option<String>,
    },
}

#[derive(Default, Resource)]
pub(crate) struct PendingEvents(Vec<InputEvent>);

impl PendingEvents {
    pub(crate) fn click(&mut self, entity: Entity, pos: Vec2) {
        self.0.push(InputEvent::Click { entity, pos });
    }
}

pub(crate) fn collect_keyboard(
    mut input: MessageReader<KeyboardInput>,
    mut pending: ResMut<PendingEvents>,
) {
    pending.0.extend(input.read().map(|input| InputEvent::Key {
        pressed: input.state == ButtonState::Pressed,
        code: key_code(input),
        character: key_char(input),
    }));
}

pub(crate) fn dispatch(world: &mut World) {
    let events = std::mem::take(&mut world.resource_mut::<PendingEvents>().0);
    if world.resource::<Time<Physics>>().is_paused() {
        return;
    }
    let mut engine = world
        .remove_non_send::<ScriptEngine>()
        .expect("Thyme engine");
    let mut errors = Vec::new();
    for event in events {
        match event {
            InputEvent::Click { entity, pos } if world.get_entity(entity).is_ok() => {
                errors.extend(engine.dispatch_click(world, entity, pos));
            }
            InputEvent::Key {
                pressed,
                code,
                character,
            } => errors.extend(engine.dispatch_key(world, pressed, &code, character.as_deref())),
            _ => {}
        }
    }
    world.insert_non_send(engine);
    for error in errors {
        world
            .resource_mut::<Console>()
            .push_line(format_args!("ERROR: {error}"));
    }
}

fn key_char(input: &KeyboardInput) -> Option<String> {
    let text = input.text.as_deref().or_else(|| match &input.logical_key {
        Key::Character(text) => Some(text.as_str()),
        _ => None,
    })?;
    (text.chars().count() == 1).then(|| text.to_owned())
}

fn key_code(input: &KeyboardInput) -> String {
    use KeyCode::*;

    let numpad = match input.key_code {
        Numpad0 => "[0]",
        Numpad1 => "[1]",
        Numpad2 => "[2]",
        Numpad3 => "[3]",
        Numpad4 => "[4]",
        Numpad5 => "[5]",
        Numpad6 => "[6]",
        Numpad7 => "[7]",
        Numpad8 => "[8]",
        Numpad9 => "[9]",
        NumpadDecimal => "[.]",
        NumpadDivide => "[/]",
        NumpadMultiply => "[*]",
        NumpadSubtract => "[-]",
        NumpadAdd => "[+]",
        NumpadEqual => "equals",
        NumpadEnter => "enter",
        _ => "",
    };
    if !numpad.is_empty() {
        return numpad.into();
    }
    if let Key::Character(text) = &input.logical_key
        && text.chars().count() == 1
    {
        return text.to_lowercase();
    }

    match input.key_code {
        Backquote => "`",
        Backslash | IntlBackslash => "\\",
        BracketLeft => "[",
        BracketRight => "]",
        Comma => ",",
        Digit0 => "0",
        Digit1 => "1",
        Digit2 => "2",
        Digit3 => "3",
        Digit4 => "4",
        Digit5 => "5",
        Digit6 => "6",
        Digit7 => "7",
        Digit8 => "8",
        Digit9 => "9",
        Equal => "=",
        KeyA => "a",
        KeyB => "b",
        KeyC => "c",
        KeyD => "d",
        KeyE => "e",
        KeyF => "f",
        KeyG => "g",
        KeyH => "h",
        KeyI => "i",
        KeyJ => "j",
        KeyK => "k",
        KeyL => "l",
        KeyM => "m",
        KeyN => "n",
        KeyO => "o",
        KeyP => "p",
        KeyQ => "q",
        KeyR => "r",
        KeyS => "s",
        KeyT => "t",
        KeyU => "u",
        KeyV => "v",
        KeyW => "w",
        KeyX => "x",
        KeyY => "y",
        KeyZ => "z",
        Minus => "-",
        Period => ".",
        Quote => "'",
        Semicolon => ";",
        Slash => "/",
        Space => " ",
        Escape => "escape",
        Tab => "tab",
        Backspace => "backspace",
        Pause => "pause",
        Delete => "delete",
        Enter => "enter",
        ArrowUp => "up",
        ArrowDown => "down",
        ArrowRight => "right",
        ArrowLeft => "left",
        Insert => "insert",
        Home => "home",
        End => "end",
        PageUp => "page_up",
        PageDown => "page_down",
        F1 => "f1",
        F2 => "f2",
        F3 => "f3",
        F4 => "f4",
        F5 => "f5",
        F6 => "f6",
        F7 => "f7",
        F8 => "f8",
        F9 => "f9",
        F10 => "f10",
        F11 => "f11",
        F12 => "f12",
        NumLock => "numlock",
        CapsLock => "caps_lock",
        ScrollLock => "scroll_lock",
        AltRight => "alt_graph",
        Help => "help",
        PrintScreen => "print_screen",
        ContextMenu => "menu",
        Power => "power",
        ControlLeft | ControlRight => "ctrl",
        ShiftLeft | ShiftRight => "shift",
        AltLeft => "alt",
        SuperLeft | SuperRight | Meta => "meta",
        _ => "unknown",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian2d::prelude::Position;
    use bevy::input::keyboard::Key;
    use bevy::prelude::{GlobalTransform, Transform};

    fn input(key_code: KeyCode, logical_key: Key, text: Option<&str>) -> KeyboardInput {
        KeyboardInput {
            key_code,
            logical_key,
            state: ButtonState::Pressed,
            text: text.map(Into::into),
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }

    #[test]
    fn key_names_follow_thyme_spelling_and_keep_typed_characters() {
        let letter = input(KeyCode::KeyQ, Key::Character("A".into()), Some("A"));
        assert_eq!(key_code(&letter), "a");
        assert_eq!(key_char(&letter).as_deref(), Some("A"));

        let numpad = input(KeyCode::Numpad2, Key::Character("2".into()), Some("2"));
        assert_eq!(key_code(&numpad), "[2]");

        let arrow = input(KeyCode::ArrowLeft, Key::ArrowLeft, None);
        assert_eq!(key_code(&arrow), "left");
        assert_eq!(key_char(&arrow), None);
    }

    #[test]
    fn input_events_are_discarded_while_physics_is_paused() {
        let mut world = World::new();
        world.insert_resource(PendingEvents::default());
        world.insert_resource(Console::default());
        let mut time = Time::<Physics>::default();
        time.pause();
        world.insert_resource(time);
        let entity = world
            .spawn((
                Position::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let mut engine = ScriptEngine::default();
        engine
            .set_selection_property(
                &mut world,
                &[entity],
                "onClick",
                "(e) => { e.this.pos = e.pos }",
            )
            .unwrap();
        world.insert_non_send(engine);

        world
            .resource_mut::<PendingEvents>()
            .click(entity, Vec2::new(2.0, 3.0));
        dispatch(&mut world);
        assert_eq!(world.get::<Position>(entity).unwrap().0, Vec2::ZERO);

        world.resource_mut::<Time<Physics>>().unpause();
        world
            .resource_mut::<PendingEvents>()
            .click(entity, Vec2::new(2.0, 3.0));
        dispatch(&mut world);
        assert_eq!(
            world.get::<Position>(entity).unwrap().0,
            Vec2::new(2.0, 3.0)
        );
    }
}
