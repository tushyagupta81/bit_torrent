use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

use crate::bencode::MetaInfo;
use crate::engine::{events::UiEvent, spawn_engine};
use crate::tui::app_state::process_event;
use crate::tui::{app_state::AppState, ui::run as tui_engine};

pub async fn run_tui(info: MetaInfo) -> anyhow::Result<()> {
    // 1. Create shared AppState
    let piece_count = info.info.pieces.len().div_ceil(20); // same as engine
    let app_state = Arc::new(RwLock::new(AppState::new(piece_count)));

    // 2. Create channel for UI events
    let (ui_tx, mut ui_rx) = mpsc::channel::<UiEvent>(256);

    let info_arc = Arc::new(info.clone());
    tokio::spawn({
        // let app_state = app_state.clone();
        let ui_sender = ui_tx.clone();
        async move {
            if let Err(e) = spawn_engine(info_arc, ui_sender).await {
                eprintln!("Engine error: {e}");
            }
        }
    });

    let app_state_clone = app_state.clone();
    tokio::spawn(async move {
        process_event(app_state_clone, &mut ui_rx).await;
    });

    // 4. Run TUI (blocking in main task)
    tui_engine(app_state.clone(), info)?;

    Ok(())
}
