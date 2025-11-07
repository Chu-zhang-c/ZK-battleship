use serde::{Deserialize, Serialize};
use risc0_zkvm::sha::Digest;
use risc0_zkvm::sha::Sha256;

#[cfg(feature = "rand")]
use {
    rand::{distributions::{Distribution, Standard}, Rng},
    rand::seq::SliceRandom,
};

// Constants for the game
pub const BOARD_SIZE: usize = 10;
pub const NUM_SHIPS: usize = 5;

// Ship sizes (Carrier: 5, Battleship: 4, Cruiser: 3, Submarine: 3, Destroyer: 2)
pub const SHIP_SIZES: [u8; NUM_SHIPS] = [5, 4, 3, 3, 2];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    Horizontal,
    Vertical,
}

// ============================================================================
// Position type (safer than raw integers)
// ============================================================================
#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub struct Position {
    pub x: u32,
    pub y: u32,
}

impl Position {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    pub fn step(self, dir: Direction, dist: u32) -> Self {
        match dir {
            Direction::Vertical => Self { x: self.x, y: self.y + dist },
            Direction::Horizontal => Self { x: self.x + dist, y: self.y },
        }
    }

    pub fn in_bounds(&self) -> bool {
        self.x < BOARD_SIZE as u32 && self.y < BOARD_SIZE as u32
    }
}

impl From<(u32, u32)> for Position {
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl From<(u8, u8)> for Position {
    fn from(value: (u8, u8)) -> Self {
        Self::new(value.0 as u32, value.1 as u32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShipType {
    Carrier,    // size 5
    Battleship, // size 4
    Cruiser,    // size 3
    Submarine,  // size 3
    Destroyer,  // size 2
}

impl ShipType {
    pub fn size(&self) -> u8 {
        match self {
            ShipType::Carrier => 5,
            ShipType::Battleship => 4,
            ShipType::Cruiser => 3,
            ShipType::Submarine => 3,
            ShipType::Destroyer => 2,
        }
    }
    /// Return a stable index for this ship type (0..NUM_SHIPS)
    pub fn index(&self) -> usize {
        match self {
            ShipType::Carrier => 0,
            ShipType::Battleship => 1,
            ShipType::Cruiser => 2,
            ShipType::Submarine => 3,
            ShipType::Destroyer => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Ship {
    pub ship_type: ShipType,
    pub position: Position,  // (x, y) coordinates of the ship's start position
    pub direction: Direction,
    /// Bitmask of hits; bit 0 = first segment, bit 1 = second, etc.
    /// Only the lowest `size` bits are used. Using a fixed-size u8 avoids
    /// dynamic allocation and makes serialization deterministic for ZK.
    pub hits: u8,
}

impl Ship {
    pub fn new(ship_type: ShipType, position: impl Into<Position>, direction: Direction) -> Self {
        Self { ship_type, position: position.into(), direction, hits: 0 }
    }

    pub fn is_sunk(&self) -> bool {
        let size = self.ship_type.size() as u8;
        let mask = if size >= 8 { 0xFFu8 } else { (1u8 << size) - 1 };
        (self.hits & mask) == mask
    }

    // Check if a given position hits this ship
    pub fn check_hit(&mut self, shot: Position) -> bool {
        let ship_x = self.position.x;
        let ship_y = self.position.y;

        match self.direction {
            Direction::Horizontal => {
                // Must be on the same row and not before the ship start
                if shot.y != ship_y || shot.x < ship_x {
                    return false;
                }
                let offset = (shot.x - ship_x) as usize;
                if offset < self.ship_type.size() as usize {
                    self.hits |= 1u8 << offset;
                    return true;
                }
            }
            Direction::Vertical => {
                // Must be on the same column and not before the ship start
                if shot.x != ship_x || shot.y < ship_y {
                    return false;
                }
                let offset = (shot.y - ship_y) as usize;
                if offset < self.ship_type.size() as usize {
                    self.hits |= 1u8 << offset;
                    return true;
                }
            }
        }
        false
    }

    // Get all coordinates this ship occupies
    pub fn get_coordinates(&self) -> Vec<Position> {
        let size = self.ship_type.size();
        let mut coords = Vec::with_capacity(size as usize);

        for offset in 0..size {
            coords.push(self.position.step(self.direction, offset as u32));
        }
        coords
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CellState {
    Empty,
    Miss,
    Hit,
}

// Zero-Knowledge Types
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Hash)]
pub enum HitType {
    Miss,
    Hit,
    Sunk(ShipType),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct RoundCommit {
    pub old_state: Digest,
    pub new_state: Digest,
    pub shot: Position,
    pub hit: HitType,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct GameState {
    pub ships: Vec<Ship>,
    pub pepper: [u8; 16],
    pub grid: [[CellState; BOARD_SIZE]; BOARD_SIZE],
}

impl GameState {
    pub fn new(pepper: [u8; 16]) -> Self {
        Self {
            ships: Vec::new(),
            pepper,
            grid: [[CellState::Empty; BOARD_SIZE]; BOARD_SIZE],
        }
    }

    // Note on `pepper`: this is included inside the `GameState` to allow
    // commitments to be randomized/blinded if desired. Decide the threat
    // model for your protocol:
    // - If `pepper` must be secret (keeps board randomness hidden), do NOT
    //   publish it alongside `RoundCommit` or prover outputs.
    // - If `pepper` is public, it offers replay-robustness but no secrecy.
    // Currently `pepper` is part of the committed state. Ensure you handle
    // it consistently between prover and verifier.

    // Check if a ship can be placed at specific coordinates
    pub fn can_place_ship(&self, ship_type: ShipType, pos: impl Into<Position>, direction: Direction) -> bool {
        let start: Position = pos.into();
        let size = ship_type.size();

        // Check start coordinates are within bounds
        if !start.in_bounds() {
            return false;
        }

        // Calculate and check end coordinates based on direction
        let end = start.step(direction, (size - 1) as u32);
        if !end.in_bounds() {
            return false;
        }

        // Check if this ship type is already placed
        if self.ships.iter().any(|ship| ship.ship_type == ship_type) {
            return false;
        }

        // Create temporary ship to check its coordinates
        let temp_ship = Ship::new(ship_type, start, direction);
        let new_coords = temp_ship.get_coordinates();

        // Check if any of the coordinates overlap with existing ships
        for existing_ship in &self.ships {
            let existing_coords = existing_ship.get_coordinates();
            for coord in &new_coords {
                if existing_coords.contains(coord) {
                    return false;
                }
            }
        }

        true
    }

    // Place a ship at specific coordinates
    pub fn place_ship(&mut self, ship_type: ShipType, pos: impl Into<Position>, direction: Direction) -> bool {
        let pos: Position = pos.into();
        if self.can_place_ship(ship_type, pos, direction) {
            self.ships.push(Ship::new(ship_type, pos, direction));
            true
        } else {
            false
        }
    }

    // Place multiple ships iteratively (non-atomic): attempts each placement
    // in order and keeps any successfully placed ships. Returns true if all
    // ships were placed, false if one or more failed.
    pub fn place_ships(&mut self, ships: Vec<(ShipType, Position, Direction)>) -> bool {
        let mut all_ok = true;
        for (ship_type, pos, direction) in ships {
            let ok = self.place_ship(ship_type, pos, direction);
            if !ok {
                all_ok = false;
            }
        }
        all_ok
    }

    #[cfg(feature = "rand")]
    pub fn place_ships_randomly<R: Rng + ?Sized>(&mut self, rng: &mut R) -> bool {
        let mut positions: Vec<Position> = (0..BOARD_SIZE as u32)
            .flat_map(|x| (0..BOARD_SIZE as u32).map(move |y| Position::new(x, y)))
            .collect();
        positions.shuffle(rng);

        self.ships.clear();
        
        for ship_type in [
            ShipType::Carrier,
            ShipType::Battleship,
            ShipType::Cruiser,
            ShipType::Submarine,
            ShipType::Destroyer,
        ] {
            let mut placed = false;
            for &pos in &positions {
                for dir in [Direction::Horizontal, Direction::Vertical] {
                    if self.place_ship(ship_type, pos, dir) {
                        placed = true;
                        break;
                    }
                }
                if placed {
                    break;
                }
            }
            if !placed {
                self.ships.clear();
                return false;
            }
        }

        true
    }

    pub fn check(&self) -> bool {
        // Check all ships are within bounds and don't overlap
        for (i, ship_i) in self.ships.iter().enumerate() {
            let coords_i = ship_i.get_coordinates();

            // Check bounds
            if coords_i.iter().any(|pos| !pos.in_bounds()) {
                return false;
            }

            // Check ship type uniqueness and overlap
            for (_j, ship_j) in self.ships.iter().enumerate().skip(i + 1) {
                if ship_i.ship_type == ship_j.ship_type {
                    return false;
                }

                let coords_j = ship_j.get_coordinates();
                if coords_i.iter().any(|coord| coords_j.contains(coord)) {
                    return false;
                }
            }
        }

        // Check if all ship types are present
        let mut found_types = [false; NUM_SHIPS];
        for ship in &self.ships {
            found_types[ship.ship_type.index()] = true;
        }
        found_types.iter().all(|&present| present)
    }

    pub fn apply_shot(&mut self, shot: impl Into<Position>) -> Option<HitType> {
        let shot: Position = shot.into();
        if !shot.in_bounds() {
            return None;
        }

        let cell = &mut self.grid[shot.y as usize][shot.x as usize];
        if *cell != CellState::Empty {
            return None; // Already shot here
        }

        // Check if we hit any ships
        for ship in &mut self.ships {
            if ship.check_hit(shot) {
                *cell = CellState::Hit;
                if ship.is_sunk() {
                    return Some(HitType::Sunk(ship.ship_type));
                }
                return Some(HitType::Hit);
            }
        }

        *cell = CellState::Miss;
        Some(HitType::Miss)
    }

    pub fn commit(&self) -> Digest {
        let bytes = bincode::serialize(self).expect("serialization should succeed");
        *risc0_zkvm::sha::Impl::hash_bytes(&bytes)
    }
}

#[cfg(feature = "rand")]
impl Distribution<GameState> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> GameState {
        let mut positions: Vec<Position> = (0..BOARD_SIZE as u32)
            .flat_map(|x| (0..BOARD_SIZE as u32).map(move |y| Position::new(x, y)))
            .collect();
        positions.shuffle(rng);

        let mut state = GameState::new(rng.gen());
        
        'outer: for ship_type in [
            ShipType::Carrier,
            ShipType::Battleship,
            ShipType::Cruiser,
            ShipType::Submarine,
            ShipType::Destroyer,
        ] {
            for &pos in &positions {
                for dir in [Direction::Horizontal, Direction::Vertical] {
                    let ship = Ship::new(ship_type, pos, dir);
                    if state.ships.len() < NUM_SHIPS && !state.ships.iter().any(|existing| {
                        existing.ship_type == ship.ship_type || 
                        ship.get_coordinates().iter().any(|coord| 
                            existing.get_coordinates().contains(coord))
                    }) {
                        state.ships.push(ship);
                        continue 'outer;
                    }
                }
            }
            panic!("Failed to place {:?}", ship_type);
        }

        assert!(state.check());
        state
    }
}

// `Board` removed. `GameState` is the canonical authoritative structure for
// placement, hits, and commitments (ZK). All logic should use `GameState`
// to avoid duplicated and potentially divergent rules.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_board() {
        let state = GameState {
            ships: vec![
                Ship::new(ShipType::Carrier, Position::new(2, 3), Direction::Vertical),
                Ship::new(ShipType::Battleship, Position::new(3, 1), Direction::Horizontal),
                Ship::new(ShipType::Cruiser, Position::new(4, 7), Direction::Vertical),
                Ship::new(ShipType::Submarine, Position::new(7, 5), Direction::Horizontal),
                Ship::new(ShipType::Destroyer, Position::new(7, 7), Direction::Horizontal),
            ],
            pepper: [0; 16],
            grid: [[CellState::Empty; BOARD_SIZE]; BOARD_SIZE],
        };
        assert!(state.check());
    }

    #[test]
    #[cfg(feature = "rand")]
    fn test_random_boards() {
        for _ in 0..100 {
            let state: GameState = rand::random();
            assert!(state.check());
        }
    }

    #[test]
    fn test_shot_before_start_not_hit() {
        let mut state = GameState {
            ships: vec![Ship::new(ShipType::Cruiser, Position::new(5, 5), Direction::Horizontal)],
            pepper: [0; 16],
            grid: [[CellState::Empty; BOARD_SIZE]; BOARD_SIZE],
        };

    // Shot before the ship's start should be a miss
    let res = state.apply_shot(Position::new(4, 5));
        assert_eq!(res, Some(HitType::Miss));
        // Ship's hit mask should remain zero
        assert_eq!(state.ships[0].hits, 0u8);
        assert_eq!(state.grid[5][4], CellState::Miss);
    }

    #[test]
    fn test_boundary_placement() {
        let state = GameState::new([0; 16]);
    // Carrier size 5 placed at x=5 horizontally should end at x=9 (valid)
    assert!(state.can_place_ship(ShipType::Carrier, Position::new(5, 9), Direction::Horizontal));
    // Carrier at x=6 would end at x=10 which is out of bounds
    assert!(!state.can_place_ship(ShipType::Carrier, Position::new(6, 9), Direction::Horizontal));
    }

    #[test]
    fn test_overlap_detection() {
        let mut state = GameState::new([0; 16]);
    assert!(state.place_ship(ShipType::Carrier, Position::new(0, 0), Direction::Horizontal));
    // Battleship overlapping carrier should fail placement
    assert!(!state.place_ship(ShipType::Battleship, Position::new(0, 0), Direction::Vertical));
    // Non-overlapping placement should succeed
    assert!(state.place_ship(ShipType::Battleship, Position::new(0, 1), Direction::Vertical));
    }
}