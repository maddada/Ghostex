// C1 repo-restructure split: home for main.rs content extracted out of the
// monolithic file. Wave 1 populated `helpers` (stateless free fns); wave 2
// added `window` (window entities) and `element` (gpui Element impls); wave 3
// adds `actions`, `hotkeys`, `ffi`, `consts`, and `model` (Region A types and
// sub-models). Later waves add the god-object module itself.
pub(crate) mod actions;
pub(crate) mod consts;
pub(crate) mod element;
pub(crate) mod ffi;
pub(crate) mod helpers;
pub(crate) mod hotkeys;
pub(crate) mod model;
pub(crate) mod window;
