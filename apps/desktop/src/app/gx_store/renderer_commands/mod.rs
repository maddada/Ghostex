//! The CLI's renderer commands, answered by the desktop's own store socket: `answer.rs` takes each
//! command the local gx-client handed over and writes its answer back, `perform.rs` performs the
//! verb gx-core validated (`ghostex_gx_core::plan_renderer_command`).

mod answer;
mod perform;
