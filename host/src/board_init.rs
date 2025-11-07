// Helpers for interactively initializing a player's board.
//
// This file provides a small, self-contained `PlayerBoard` type and
// interactive helpers that prompt a human user (stdin) for placement of
// the canonical five ships. It does not depend on the `core` crate so it
// can be used independently; the types are intentionally similar so they
// are easy to map to `GameState` if you later wire them together.
//
// Usage (from a host binary):
//    let board = board_init::prompt_place_ships("Player 1");
//
use std::io::{self, Write};

pub const BOARD_SIZE: usize = 10;

/// Standard ship types and sizes used by the interactive helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipType {
    Carrier,    // size 5
    Battleship, // size 4
    Cruiser,    // size 3
    Submarine,  // size 3
    Destroyer,  // size 2
}

impl ShipType {
    pub fn all() -> &'static [ShipType; 5] {
        &[ShipType::Carrier, ShipType::Battleship, ShipType::Cruiser, ShipType::Submarine, ShipType::Destroyer]
    }
    pub fn size(&self) -> usize {
        match self {
            ShipType::Carrier => 5,
            ShipType::Battleship => 4,
            ShipType::Cruiser => 3,
            ShipType::Submarine => 3,
            ShipType::Destroyer => 2,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            ShipType::Carrier => "Carrier",
            ShipType::Battleship => "Battleship",
            ShipType::Cruiser => "Cruiser",
            ShipType::Submarine => "Submarine",
            ShipType::Destroyer => "Destroyer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone)]
pub struct Ship {
    pub ship_type: ShipType,
    pub position: Position,
    pub direction: Direction,
    /// hits per segment (false = intact, true = hit)
    pub hits: Vec<bool>,
}

impl Ship {
    pub fn new(ship_type: ShipType, pos: Position, dir: Direction) -> Self {
        let size = ship_type.size();
        Self { ship_type, position: pos, direction: dir, hits: vec![false; size] }
    }

    pub fn coords(&self) -> Vec<Position> {
        let mut out = Vec::with_capacity(self.ship_type.size());
        for i in 0..self.ship_type.size() {
            match self.direction {
                Direction::Horizontal => out.push(Position { x: self.position.x + i, y: self.position.y }),
                Direction::Vertical => out.push(Position { x: self.position.x, y: self.position.y + i }),
            }
        }
        out
    }

    pub fn is_sunk(&self) -> bool {
        self.hits.iter().all(|&h| h)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    Empty,
    Miss,
    Hit,
    Ship,
}

#[derive(Debug, Clone)]
pub struct PlayerBoard {
    pub ships: Vec<Ship>,
    pub grid: [[CellState; BOARD_SIZE]; BOARD_SIZE],
}

impl PlayerBoard {
    pub fn new_empty() -> Self {
        Self { ships: Vec::new(), grid: [[CellState::Empty; BOARD_SIZE]; BOARD_SIZE] }
    }

    /// Check whether a candidate ship placement would be valid (in-bounds,
    /// non-overlapping). This performs no mutation.
    pub fn can_place(&self, ship_type: ShipType, pos: Position, dir: Direction) -> bool {
        // check bounds
        match dir {
            Direction::Horizontal => {
                if pos.x + ship_type.size() > BOARD_SIZE { return false; }
                if pos.y >= BOARD_SIZE { return false; }
            }
            Direction::Vertical => {
                if pos.y + ship_type.size() > BOARD_SIZE { return false; }
                if pos.x >= BOARD_SIZE { return false; }
            }
        }
        // check overlap with existing ships
        let cand = Ship::new(ship_type, pos, dir);
        let cand_coords = cand.coords();
        for s in &self.ships {
            for c in cand_coords.iter() {
                if s.coords().iter().any(|sc| sc.x == c.x && sc.y == c.y) {
                    return false;
                }
            }
        }
        true
    }

    /// Place a ship assuming `can_place` returned true. Mutates board.
    pub fn place_ship(&mut self, ship_type: ShipType, pos: Position, dir: Direction) {
        let ship = Ship::new(ship_type, pos, dir);
        for p in ship.coords().iter() {
            self.grid[p.y][p.x] = CellState::Ship;
        }
        self.ships.push(ship);
    }

    /// Apply a shot to the board and return whether it was a hit/miss and
    /// which ship (if any) was affected.
    pub fn apply_shot(&mut self, pos: Position) -> Option<(bool, Option<ShipType>)> {
        if pos.x >= BOARD_SIZE || pos.y >= BOARD_SIZE { return None; }
        match self.grid[pos.y][pos.x] {
            CellState::Empty => {
                self.grid[pos.y][pos.x] = CellState::Miss;
                Some((false, None))
            }
            CellState::Miss | CellState::Hit => Some((false, None)), // already shot here
            CellState::Ship => {
                // find which ship
                for ship in &mut self.ships {
                    for (idx, c) in ship.coords().iter().enumerate() {
                        if c.x == pos.x && c.y == pos.y {
                            ship.hits[idx] = true;
                            self.grid[pos.y][pos.x] = CellState::Hit;
                            let sunk = ship.is_sunk();
                            return Some((true, Some(ship.ship_type)));
                        }
                    }
                }
                // should not happen, but treat as miss
                self.grid[pos.y][pos.x] = CellState::Miss;
                Some((false, None))
            }
        }
    }

    pub fn all_sunk(&self) -> bool {
        self.ships.iter().all(|s| s.is_sunk())
    }
}

/// Prompt the user (stdin) to place ships for a single player. The function
/// returns a filled `PlayerBoard`. Input format is line-based; for each ship
/// it will prompt for: "x y dir" where x and y are integers (0..9) and dir
/// is H or V.
pub fn prompt_place_ships(player_name: &str) -> PlayerBoard {
    let mut board = PlayerBoard::new_empty();
    println!("{}: place your ships on a {}x{} board.", player_name, BOARD_SIZE, BOARD_SIZE);
    println!("Coordinates are 0-based: x in [0..{}], y in [0..{}].", BOARD_SIZE-1, BOARD_SIZE-1);

    for &st in ShipType::all().iter() {
        loop {
            print!("Place {} (size {}) as: x y H/V: ", st.name(), st.size());
            io::stdout().flush().ok();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                println!("Failed to read input, try again.");
                continue;
            }
            let parts: Vec<_> = input.trim().split_whitespace().collect();
            if parts.len() != 3 {
                println!("Expected three tokens: x y H/V");
                continue;
            }
            let x = match parts[0].parse::<usize>() {
                Ok(v) => v,
                Err(_) => { println!("Invalid x"); continue; }
            };
            let y = match parts[1].parse::<usize>() {
                Ok(v) => v,
                Err(_) => { println!("Invalid y"); continue; }
            };
            let dir = match parts[2].to_uppercase().as_str() {
                "H" => Direction::Horizontal,
                "V" => Direction::Vertical,
                _ => { println!("Invalid direction, use H or V"); continue; }
            };
            let pos = Position { x, y };
            if !board.can_place(st, pos, dir) {
                println!("Invalid placement (out of bounds or overlapping). Try again.");
                continue;
            }
            board.place_ship(st, pos, dir);
            break;
        }
    }

    println!("{}: placement complete.\n", player_name);
    board
}
