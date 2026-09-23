// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use cosmic::Element;
use cosmic::widget::RcElementWrapper;
use cosmic::widget::menu::{self, ItemHeight, ItemWidth, KeyBind};

use crate::app::{Action, Message};
use crate::fl;

pub fn menu_bar<'a>(key_binds: &HashMap<KeyBind, Action>) -> Element<'a, Message> {
    menu::bar(vec![
        menu::Tree::with_children(
            RcElementWrapper::new(Element::from(menu::root(fl!("file")))),
            menu::items(
                key_binds,
                vec![
                    menu::Item::Button(fl!("menu-new-backup"), None, Action::NewBackup),
                    menu::Item::Button(fl!("back-up-now"), None, Action::BackUpNow),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("new-window"), None, Action::WindowNew),
                    menu::Item::Button(fl!("quit"), None, Action::WindowClose),
                ],
            ),
        ),
        menu::Tree::with_children(
            RcElementWrapper::new(Element::from(menu::root(fl!("view")))),
            menu::items(
                key_binds,
                vec![
                    menu::Item::Button(fl!("menu-settings"), None, Action::Settings),
                    menu::Item::Divider,
                    menu::Item::Button(fl!("menu-about"), None, Action::About),
                ],
            ),
        ),
    ])
    .item_height(ItemHeight::Dynamic(40))
    .item_width(ItemWidth::Uniform(240))
    .spacing(4.0)
    .into()
}
