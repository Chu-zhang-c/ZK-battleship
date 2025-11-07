use eframe::{egui, App};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::mpsc;
use core::{GameState, Position, BOARD_SIZE, CellState};

/// Commands sent from the UI thread to the core/network thread.
#[derive(Clone, Debug)]
pub enum UiCommand {
    Shoot(Position),
    // Additional commands (Connect/Host/PlaceShips) can be added later.
}

/// Events sent from the core/network thread back to the UI.
#[derive(Clone, Debug)]
pub enum UiEvent {
    LocalStateUpdated(GameState),
    OpponentViewUpdated(GameState),
    Log(String),
    GameOver(Option<String>),
}

pub struct BattleshipApp {
    rx: Receiver<UiEvent>,
    tx: Sender<UiCommand>,
    local: GameState,
    opp_view: GameState,
    logs: Vec<String>,
}

impl BattleshipApp {
    pub fn new(rx: Receiver<UiEvent>, tx: Sender<UiCommand>) -> Self {
        let empty = GameState::new([0u8; 16]);
        Self { rx, tx, local: empty.clone(), opp_view: empty, logs: Vec::new() }
    }
}

impl App for BattleshipApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain any incoming events
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                UiEvent::LocalStateUpdated(gs) => self.local = gs,
                UiEvent::OpponentViewUpdated(gs) => self.opp_view = gs,
                UiEvent::Log(s) => { self.logs.push(s); if self.logs.len() > 300 { self.logs.remove(0); } }
                UiEvent::GameOver(w) => { self.logs.push(format!("Game over: {:?}", w)); }
            }
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("ZK Battleship");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Quit").clicked() {
                        std::process::exit(0);
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Your Board");
                    draw_board_ui(ui, &self.local, true, &self.tx);
                });
                ui.vertical(|ui| {
                    ui.label("Opponent View (click to shoot)");
                    draw_board_ui(ui, &self.opp_view, false, &self.tx);
                });
            });

            ui.separator();
            ui.label("Logs:");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for l in &self.logs {
                    ui.label(l);
                }
            });
        });
    }

    // eframe::App does not require a `name()` method; keep the struct minimal.
}

fn draw_board_ui(ui: &mut egui::Ui, board: &GameState, show_ships: bool, tx: &Sender<UiCommand>) {
    use egui::RichText;

    let cell_size = egui::Vec2::splat(30.0);

    egui::Grid::new("board_grid").spacing([4.0,4.0]).show(ui, |ui| {
        for y in 0..(BOARD_SIZE as usize) {
            for x in 0..(BOARD_SIZE as usize) {
                let label = match board.grid[y][x] {
                    CellState::Empty => ".",
                    CellState::Miss => "o",
                    CellState::Hit => "X",
                };

                // If ships should be visible (your board), show them as 'S'
                let display = if show_ships {
                    if board.ships.iter().any(|s| s.get_coordinates().contains(&core::Position::new(x as u32, y as u32))) {
                        "S"
                    } else { label }
                } else { label };

                let btn = egui::Button::new(RichText::new(display)).min_size(cell_size);
                if ui.add(btn).clicked() && !show_ships {
                    // send a Shoot command to core
                    let _ = tx.send(UiCommand::Shoot(core::Position::new(x as u32, y as u32)));
                }
            }
            ui.end_row();
        }
    });
}

// Helper to create a simple pair of channels for embedding the UI; used in `main`.
pub fn make_channels() -> (Sender<UiCommand>, Receiver<UiEvent>, Sender<UiEvent>, Receiver<UiCommand>) {
    let (tx_ui_cmd, rx_core_cmd) = mpsc::channel::<UiCommand>();
    let (tx_core_evt, rx_ui_evt) = mpsc::channel::<UiEvent>();
    (tx_ui_cmd, rx_ui_evt, tx_core_evt, rx_core_cmd)
}
