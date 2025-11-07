// core: Battleship game logic designed to be deterministic and ZK-friendly.
//
// This module implements a compact Battleship game state and operations
// that are stable for serialization and commitments. Keep `GameState` as
// the canonical authority for placement and shots. The implementation
// favors fixed-size representations (u8 bitmasks, u32 positions) to
// reduce nondeterminism inside ZK guests.

use serde::{Deserialize, Serialize};
use risc0_zkvm::sha::Digest;
use risc0_zkvm::sha::Sha256;

#[cfg(feature = "rand")]
use {
    rand::{distributions::{Distribution, Standard}, Rng},
    rand::seq::SliceRandom,
};

/// Board dimensions. Fixed-size board simplifies reasoning and
/// serialization across prover/verifier.
pub const BOARD_SIZE: usize = 10;

/// Number of distinct ship types used in the canonical setup.
pub const NUM_SHIPS: usize = 5;

/// Canonical ship sizes (in reading order): Carrier, Battleship,
/// Cruiser, Submarine, Destroyer.
pub const SHIP_SIZES: [u8; NUM_SHIPS] = [5, 4, 3, 3, 2];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    Horizontal,
    Vertical,
}

// Direction is intentionally a small enum so it serializes compactly and
// can be used in Position/ship arithmetic without allocations.

// ============================================================================
// Position: a safe, fixed-size coordinate type
//
// Use u32 for x/y to avoid accidental underflow/overflow during arithmetic
// while keeping serialization deterministic. All board-bounds checks use
// `in_bounds()` and are enforced by placement/shot logic.
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

    /// Check whether the provided absolute board `shot` hits this ship.
    ///
    /// If the shot hits a segment within the ship, set the corresponding
    /// bit in `self.hits` and return `true`. Otherwise return `false`.
    ///
    /// Important: this function performs local ship arithmetic and does
    /// not consult the board grid. Callers should ensure coordinate
    /// bounds as needed.
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

    // Note on `pepper` (ZK consideration):
    // - `pepper` is included inside the serialized `GameState` used for
    //   commitments. If the pepper must remain secret, the prover must
    //   not reveal it in outputs; if the pepper is public, it may be
    //   published alongside commitments. Keep prover and verifier logic
    //   consistent about pepper handling.

    /// Check whether a ship of `ship_type` can be placed at `pos` facing
    /// `direction`. Checks include:
    ///  - start and end within board bounds
    ///  - that a ship of the same type isn't already placed
    ///  - no coordinate overlap with existing ships
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

    /// Attempt to place a ship; returns true on success. Delegates to
    /// `can_place_ship` for validation and mutates `self.ships` on success.
    pub fn place_ship(&mut self, ship_type: ShipType, pos: impl Into<Position>, direction: Direction) -> bool {
        let pos: Position = pos.into();
        if self.can_place_ship(ship_type, pos, direction) {
            self.ships.push(Ship::new(ship_type, pos, direction));
            true
        } else {
            false
        }
    }

    /// Place multiple ships in order. This is intentionally non-atomic;
    /// previously successful placements are retained even if later
    /// placements fail. Returns true only if all placements succeed.
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
    #[cfg(feature = "rand")]
    /// Try to place all ships randomly using the provided RNG. On failure
    /// clears `self.ships` and returns false.
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

    /// Run a full consistency check on the game state:
    /// - all ships within bounds
    /// - no overlaps
    /// - exactly one of each ship type present
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

    /// Apply a shot at `shot` and update `self.grid` and any hit ship.
    ///
    /// Returns:
    /// - `Some(HitType::Hit)` if a ship segment was hit (but not sunk)
    /// - `Some(HitType::Sunk(ship_type))` if the shot sank a ship
    /// - `Some(HitType::Miss)` if in-bounds and no ship was hit
    /// - `None` for out-of-bounds shots or if the cell was already shot
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

    // -------------------- additional end-to-end tests --------------------

    #[test]
    fn test_sink_ship() {
        // Place a carrier horizontally at (0,0)
        let mut state = GameState::new([0; 16]);
        assert!(state.place_ship(ShipType::Carrier, Position::new(0, 0), Direction::Horizontal));

        // Fire at each segment; the final shot should report Sunk
        for i in 0..5 {
            let pos = Position::new(i, 0);
            let res = state.apply_shot(pos);
            if i < 4 {
                assert_eq!(res, Some(HitType::Hit));
            } else {
                // last segment -> sunk
                assert_eq!(res, Some(HitType::Sunk(ShipType::Carrier)));
            }
        }

        // Verify ship recorded as sunk
        let ship = state.ships.iter().find(|s| s.ship_type == ShipType::Carrier).unwrap();
        assert!(ship.is_sunk());
    }

    #[test]
    fn test_repeated_shot_idempotent() {
        let mut state = GameState::new([0; 16]);

        // No ships: first shot is a miss, second identical shot should return None
        let res1 = state.apply_shot(Position::new(1, 1));
        assert_eq!(res1, Some(HitType::Miss));

        // Second shot at same cell -> already shot -> None per API
        let res2 = state.apply_shot(Position::new(1, 1));
        assert_eq!(res2, None);
    }

    #[test]
    fn test_commit_equality() {
        // Two identical states should produce identical commits; modifying one
        // should change the commit.
        let mut s1 = GameState::new([0; 16]);
        s1.place_ship(ShipType::Destroyer, Position::new(2, 2), Direction::Horizontal);

        let mut s2 = s1.clone();

        let c1 = s1.commit();
        let c2 = s2.commit();
        assert_eq!(c1, c2);

        // Mutate s2 (change pepper) and ensure commit changes.
        s2.pepper[0] = 1;
        let c3 = s2.commit();
        assert_ne!(c1, c3);
    }
}