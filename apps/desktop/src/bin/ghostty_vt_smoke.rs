/*
CDXC:GPUILibghosttyVt 2026-07-03:
Smoke/demo binary for the libghostty-vt Rust wrapper — the P1a deliverable
proving parse → render-state → row readback end to end, and demonstrating the
two-level dirty contract (update never clears; the caller clears global and
per-row layers independently). This is intentionally a printed walkthrough,
not a test harness; keep it a single small file. Run with:

    cargo run --bin ghostty-vt-smoke
*/

// This smoke/demo binary `#[path]`-includes shared modules (ghostty_vt, terminal_model,
// terminal_element, shared_settings, support_logs, ...) but only exercises a slice of
// them, so most of their items are legitimately unused *here*. The allow is scoped to
// this demo crate root so the real app binary keeps full dead-code coverage.
#![allow(dead_code)]

#[path = "../ghostty_vt.rs"]
mod ghostty_vt;

use ghostty_vt::{VtDirty, VtRenderState, VtTerminal};

fn dump_rows(label: &str, state: &mut VtRenderState) -> Result<(), ghostty_vt::VtError> {
    let (cols, rows) = state.size()?;
    println!(
        "{label}: global dirty = {:?}, size = {cols}x{rows}, cursor = {:?}",
        state.dirty()?,
        state.cursor_viewport()?
    );
    let mut iter = state.rows()?;
    let mut y = 0;
    while let Some(mut row) = iter.next_row() {
        println!(
            "  row {y} [{}] {:?}",
            if row.is_dirty()? { "dirty" } else { "clean" },
            row.text()?
        );
        y += 1;
    }
    Ok(())
}

fn clear_all_dirty(state: &mut VtRenderState) -> Result<(), ghostty_vt::VtError> {
    // The caller owns BOTH dirty layers: clear every row flag, then the
    // global flag. Neither clear affects the other layer.
    let mut iter = state.rows()?;
    while let Some(mut row) = iter.next_row() {
        row.clear_dirty()?;
    }
    state.clear_dirty()
}

fn main() -> Result<(), ghostty_vt::VtError> {
    let mut terminal = VtTerminal::new(20, 4, 1000)?;
    let mut state = VtRenderState::new()?;

    // 1. Plain text, SGR colors (palette green + bold, truecolor orange),
    //    and a second line.
    terminal.feed(b"plain \x1b[1;32mGREEN\x1b[0m\r\n\x1b[38;2;255;128;0morange\x1b[0m text");
    state.update(&mut terminal)?;
    dump_rows("after feed", &mut state)?;

    // Show SGR readback for row 0 cells 6..11 ("GREEN") and row 1 cell 0.
    let mut iter = state.rows()?;
    if let Some(mut row) = iter.next_row() {
        let mut cells = row.cells()?;
        let mut x = 0;
        while let Some(cell) = cells.next_cell() {
            if x == 6 {
                println!(
                    "  row 0 cell 6: fg = {:?}, bold = {}",
                    cell.fg_color()?,
                    cell.style()?.bold
                );
            }
            x += 1;
        }
    }
    if let Some(mut row) = iter.next_row() {
        let mut cells = row.cells()?;
        if let Some(cell) = cells.next_cell() {
            println!(
                "  row 1 cell 0: fg = {:?}, bold = {}",
                cell.fg_color()?,
                cell.style()?.bold
            );
        }
    }
    drop(iter);
    clear_all_dirty(&mut state)?;

    // 2. Update with no new terminal input: everything must read clean,
    //    because update only raises dirty state and we just cleared it.
    state.update(&mut terminal)?;
    dump_rows("after no-op update (dirty cleared by caller)", &mut state)?;
    assert_eq!(state.dirty()?, VtDirty::Clean);

    // 3. Cursor movement: jump home and overwrite one character. Only the
    //    touched row should be dirty.
    terminal.feed(b"\x1b[1;1HX");
    state.update(&mut terminal)?;
    dump_rows("after cursor home + overwrite 'X'", &mut state)?;
    clear_all_dirty(&mut state)?;

    // 4. Resize narrower: the primary screen reflows and the whole frame
    //    reads dirty again.
    terminal.resize(10, 4, 8, 16)?;
    state.update(&mut terminal)?;
    dump_rows("after resize to 10x4", &mut state)?;

    println!("smoke complete");
    Ok(())
}
