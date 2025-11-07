use eframe::{egui, App};
use core::{GameState, Position, ShipType, Direction, CellState, HitType, BOARD_SIZE};

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "ZK Battleship (Simple GUI)",
        native_options,
        Box::new(|_cc| Box::new(BattleshipGui::default())),
    );
}

struct BattleshipGui {
    local: GameState,
    opponent: GameState,
    opponent_view: GameState,
    logs: Vec<String>,
    placing: Option<ShipType>,
    placing_dir: Direction,
    started: bool,
}

impl Default for BattleshipGui {
    fn default() -> Self {
        let local = GameState::new([0u8; 16]);
        let opponent = GameState::new([0u8; 16]);
        Self { local, opponent, opponent_view: GameState::new([0u8;16]), logs: vec!["Welcome to ZK Battleship (GUI)".to_string()], placing: None, placing_dir: Direction::Horizontal, started: false }
    }
}

impl App for BattleshipGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.heading("ZK Battleship - Simple GUI");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label("Your Board (click to place when a ship is selected)");
                    if let Some(pos) = draw_board(ui, &self.local, true) {
                        // on click: if placing ship, attempt to place
                        if let Some(st) = self.placing {
                            let ok = self.local.place_ship(st, pos, self.placing_dir);
                            if ok { self.logs.push(format!("Placed {:?} at {},{}", st, pos.x, pos.y)); self.placing = None; }
                            else { self.logs.push(format!("Failed to place {:?} at {},{}", st, pos.x, pos.y)); }
                        }
                    }
                });

                ui.vertical(|ui| {
                    ui.label("Opponent View (click to shoot)");
                    if let Some(pos) = draw_board(ui, &self.opponent_view, false) {
                        if !self.started { self.logs.push("Game not started".to_string()); }
                        else {
                            // prevent duplicate shots
                            if self.opponent_view.grid[pos.y as usize][pos.x as usize] != CellState::Empty {
                                self.logs.push(format!("Already shot at {},{}", pos.x, pos.y));
                            } else {
                                // apply shot to opponent authoritative state
                                if let Some(hit) = self.opponent.apply_shot(pos) {
                                    match hit {
                                        HitType::Miss => {
                                            self.opponent_view.grid[pos.y as usize][pos.x as usize] = CellState::Miss;
                                            self.logs.push(format!("Miss at {},{}", pos.x, pos.y));
                                            // opponent turn simulated
                                            simulate_opponent_turn(self);
                                        }
                                        HitType::Hit => {
                                            self.opponent_view.grid[pos.y as usize][pos.x as usize] = CellState::Hit;
                                            self.logs.push(format!("Hit at {},{}", pos.x, pos.y));
                                        }
                                        HitType::Sunk(st) => {
                                            self.opponent_view.grid[pos.y as usize][pos.x as usize] = CellState::Hit;
                                            self.logs.push(format!("Sunk {:?} at {},{}", st, pos.x, pos.y));
                                            if self.opponent.ships.iter().all(|s| s.is_sunk()) {
                                                self.logs.push("You win!".to_string());
                                                self.started = false;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                });

                ui.vertical(|ui| {
                    ui.label("Controls");
                    ui.horizontal(|ui| {
                        if ui.button("Start Game (deterministic opponent)").clicked() {
                            // place opponent ships deterministically
                            place_opponent_deterministic(&mut self.opponent);
                            self.opponent_view = GameState::new([0u8;16]);
                            self.started = true;
                            self.logs.push("Game started".to_string());
                        }
                        if ui.button("Reset").clicked() {
                            self.local = GameState::new([0u8;16]);
                            self.opponent = GameState::new([0u8;16]);
                            self.opponent_view = GameState::new([0u8;16]);
                            self.logs.clear();
                            self.logs.push("Reset".to_string());
                            self.started = false;
                        }
                    });

                    ui.separator();
                    ui.label("Select ship to place:");
                    ui.horizontal(|ui| {
                        for st in [ShipType::Carrier, ShipType::Battleship, ShipType::Cruiser, ShipType::Submarine, ShipType::Destroyer] {
                            let name = format!("{:?}", st);
                            if ui.button(name).clicked() { self.placing = Some(st); }
                        }
                    });
                    if ui.button(format!("Orientation: {:?}", self.placing_dir)).clicked() {
                        self.placing_dir = match self.placing_dir { Direction::Horizontal => Direction::Vertical, _ => Direction::Horizontal };
                    }

                    ui.separator();
                    ui.label("Logs:");
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        for l in &self.logs { ui.label(l); }
                    });
                });
            });
        });
    }
}

fn draw_board(ui: &mut egui::Ui, board: &GameState, reveal_ships: bool) -> Option<Position> {
    let cell = egui::Vec2::splat(28.0);
    let mut clicked: Option<Position> = None;
    egui::Grid::new("grid").spacing([2.0,2.0]).show(ui, |ui| {
        for y in 0..(BOARD_SIZE as usize) {
            for x in 0..(BOARD_SIZE as usize) {
                let mut label = ".".to_string();
                let ch = board.grid[y][x];
                if ch == CellState::Miss { label = "o".to_string(); }
                if ch == CellState::Hit { label = "X".to_string(); }
                if reveal_ships {
                    // reveal ships
                    if board.ships.iter().any(|s| s.get_coordinates().contains(&Position::new(x as u32, y as u32))) {
                        label = "S".to_string();
                    }
                }
                let button = egui::Button::new(label).min_size(cell);
                if ui.add(button).clicked() {
                    clicked = Some(Position::new(x as u32, y as u32));
                }
            }
            ui.end_row();
        }
    });
    clicked
}

fn place_opponent_deterministic(op: &mut GameState) {
    use core::ShipType;
    use core::Direction;
    op.place_ship(ShipType::Carrier, Position::new(0,0), Direction::Vertical);
    op.place_ship(ShipType::Battleship, Position::new(2,0), Direction::Vertical);
    op.place_ship(ShipType::Cruiser, Position::new(4,0), Direction::Vertical);
    op.place_ship(ShipType::Submarine, Position::new(6,0), Direction::Vertical);
    op.place_ship(ShipType::Destroyer, Position::new(8,0), Direction::Vertical);
}

fn simulate_opponent_turn(gui: &mut BattleshipGui) {
    // naive opponent: shoot first empty cell
    for y in 0..(BOARD_SIZE as usize) {
        for x in 0..(BOARD_SIZE as usize) {
            if gui.local.grid[y][x] == CellState::Empty {
                let p = Position::new(x as u32, y as u32);
                if let Some(hit) = gui.local.apply_shot(p) {
                    match hit {
                        HitType::Miss => {
                            gui.logs.push(format!("Opponent missed at {},{}", x, y));
                            return;
                        }
                        HitType::Hit => { gui.logs.push(format!("Opponent hit at {},{}", x, y)); continue; }
                        HitType::Sunk(st) => { gui.logs.push(format!("Opponent sunk {:?} at {},{}", st, x, y)); if gui.local.ships.iter().all(|s| s.is_sunk()) { gui.logs.push("Opponent wins".to_string()); gui.started = false; } return; }
                    }
                }
            }
        }
    }
}
