use crate::ui::SwitcherWindow;
use crate::ui_commands::UiCommand;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;
use tracing::{error, info};

/// Handles UI commands and dispatches them to the SwitcherWindow
pub fn handle_ui_commands(
    switcher: Rc<RefCell<SwitcherWindow>>,
    mut ui_rx: mpsc::UnboundedReceiver<UiCommand>,
) {
    info!("UI command handler started");

    // Use glib to handle commands on the GTK main thread
    glib::spawn_future_local(async move {
        while let Some(command) = ui_rx.recv().await {
            info!("Received UI command: {:?}", command);

            match command {
                UiCommand::Show {
                    windows,
                    initial_index,
                } => {
                    info!("Showing UI with {} windows, index {}", windows.len(), initial_index);
                    switcher.borrow_mut().show(windows, initial_index);
                    info!("UI shown");
                }
                UiCommand::CycleNext => {
                    info!("Cycling to next window");
                    switcher.borrow_mut().cycle_next();
                }
                UiCommand::CyclePrev => {
                    info!("Cycling to previous window");
                    switcher.borrow_mut().cycle_prev();
                }
                UiCommand::Hide => {
                    info!("Hiding UI");
                    switcher.borrow().close();
                }
            }
        }

        error!("UI command handler stopped - channel closed!");
    });
}
