//! A command ticket source: a program the user points a workspace at, which gxserver runs like
//! `gh` to learn the source's name, icon and id format (`info`), list its tickets (`list`), read
//! one (`detail`) and create one (`create`). Its icon is rebuilt from an allowlist (`icon.rs`).

mod icon;

pub(crate) use icon::*;
