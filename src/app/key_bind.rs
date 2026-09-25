// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::key::Named;
use cosmic::widget::menu::KeyBind;
use cosmic::widget::menu::key_bind::Modifier;

use crate::app::Action;

pub fn key_binds() -> HashMap<KeyBind, Action> {
    let mut key_binds = HashMap::new();

    macro_rules! bind {
        ([$($modifier:ident),* $(,)?], $key:expr, $action:ident) => {{
            key_binds.insert(
                KeyBind {
                    modifiers: vec![$(Modifier::$modifier),*],
                    key: $key,
                },
                Action::$action,
            );
        }};
    }
    bind!([Ctrl], Key::Character("n".into()), NewBackup);
    bind!([Ctrl], Key::Character("b".into()), BackUpNow);
    bind!([Ctrl], Key::Character("w".into()), WindowClose);
    bind!([Ctrl, Shift], Key::Character("n".into()), WindowNew);
    bind!([Ctrl], Key::Character(",".into()), Settings);
    bind!([Ctrl], Key::Character("i".into()), About);
    bind!([], Key::Named(Named::F1), Help);

    key_binds
}
