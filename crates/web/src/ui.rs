//! ui/README.md has one line on each piece.

pub(crate) mod action;
pub(crate) mod auto_toggle;
pub(crate) mod button;
pub(crate) mod chips;
pub(crate) mod debounced;
pub(crate) mod dialog;
pub(crate) mod dropdown;
pub(crate) mod file_pick;
pub(crate) mod find;
pub(crate) mod floating;
pub(crate) mod kept;
pub(crate) mod load;
pub(crate) mod loadable;
pub(crate) mod logo;
pub(crate) mod menu;
pub(crate) mod name_field;
pub(crate) mod observe;
pub(crate) mod pager;
pub(crate) mod pane;
pub(crate) mod pathbar;
pub(crate) mod pick;
pub(crate) mod picker;
pub(crate) mod pinned_table;
pub(crate) mod poll;
pub(crate) mod popbox;
pub(crate) mod popup;
pub(crate) mod renamable;
pub(crate) mod search;
pub(crate) mod segmented;
pub(crate) mod tabs;
pub(crate) mod tag;
pub(crate) mod turn_tools;
pub(crate) mod xterm;

stylance::import_style!(#[allow(dead_code, unreachable_pub)] pub style, "ui.module.scss");
