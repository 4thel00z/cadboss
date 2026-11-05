//! The copy popup's targets and the key that picks each.

use crossterm::event::KeyCode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum YankTarget {
    Text,
    Layer,
    Handle,
    Dxf,
    Hex,
    Query,
    Command,
}

pub const TARGETS: [YankTarget; 7] = [
    YankTarget::Text,
    YankTarget::Layer,
    YankTarget::Handle,
    YankTarget::Dxf,
    YankTarget::Hex,
    YankTarget::Query,
    YankTarget::Command,
];

impl YankTarget {
    pub fn key(self) -> char {
        match self {
            YankTarget::Text => 't',
            YankTarget::Layer => 'l',
            YankTarget::Handle => 'h',
            YankTarget::Dxf => 'd',
            YankTarget::Hex => 'x',
            YankTarget::Query => 'q',
            YankTarget::Command => 'c',
        }
    }

    pub fn from_key(code: KeyCode) -> Option<YankTarget> {
        let KeyCode::Char(c) = code else {
            return None;
        };
        TARGETS.into_iter().find(|target| target.key() == c)
    }

    pub fn name(self) -> &'static str {
        match self {
            YankTarget::Text => "text",
            YankTarget::Layer => "layer",
            YankTarget::Handle => "handle",
            YankTarget::Dxf => "DXF",
            YankTarget::Hex => "hex",
            YankTarget::Query => "jq query",
            YankTarget::Command => "shell command",
        }
    }
}
