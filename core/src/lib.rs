use serde::{Deserialize, Serialize};
use risc0_zkvm::sha::Digest;

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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ship {
    pub ship_type: ShipType,
    pub position: (u8, u8),  // (x, y) coordinates of the ship's start position
    pub direction: Direction,
    pub hits: Vec<bool>,     // Mask to track hits on this ship
}

impl Ship {
    pub fn new(ship_type: ShipType, position: (u8, u8), direction: Direction) -> Self {
        let size = ship_type.size() as usize;
        Self {
            ship_type,
            position,
            direction,
            hits: vec![false; size],  // Initialize all positions as not hit
        }
    }

    pub fn is_sunk(&self) -> bool {
        self.hits.iter().all(|&hit| hit)
    }

    // Check if a given position hits this ship
    pub fn check_hit(&mut self, x: u8, y: u8) -> bool {
        let (ship_x, ship_y) = self.position;
        
        match self.direction {
            Direction::Horizontal => {
                if y != ship_y {
                    return false;
                }
                let offset = x.saturating_sub(ship_x) as usize;
                if offset < self.hits.len() {
                    self.hits[offset] = true;
                    return true;
                }
            }
            Direction::Vertical => {
                if x != ship_x {
                    return false;
                }
                let offset = y.saturating_sub(ship_y) as usize;
                if offset < self.hits.len() {
                    self.hits[offset] = true;
                    return true;
                }
            }
        }
        false
    }

    // Get all coordinates this ship occupies
    pub fn get_coordinates(&self) -> Vec<(u8, u8)> {
        let size = self.ship_type.size();
        let (x, y) = self.position;
        let mut coords = Vec::with_capacity(size as usize);

        match self.direction {
            Direction::Horizontal => {
                for offset in 0..size {
                    coords.push((x + offset, y));
                }
            }
            Direction::Vertical => {
                for offset in 0..size {
                    coords.push((x, y + offset));
                }
            }
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
    pub shot: (u8, u8),
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

    // Check if a ship can be placed at the given position
    pub fn can_place_ship(&self, ship_type: ShipType, pos: (u8, u8), direction: Direction) -> bool {
        let (x, y) = pos;
        let size = ship_type.size();

        // Check if ship would extend beyond board
        match direction {
            Direction::Horizontal => {
                if x as usize + size as usize > BOARD_SIZE {
                    return false;
                }
            }
            Direction::Vertical => {
                if y as usize + size as usize > BOARD_SIZE {
                    return false;
                }
            }
        }

        // Check if ship type is already placed
        if self.ships.iter().any(|ship| ship.ship_type == ship_type) {
            return false;
        }

        // Create temporary ship to check its coordinates
        let temp_ship = Ship::new(ship_type, pos, direction);
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

    // Place a ship at specific coordinates with given direction
    pub fn place_ship(&mut self, ship_type: ShipType, pos: (u8, u8), direction: Direction) -> bool {
        if self.can_place_ship(ship_type, pos, direction) {
            self.ships.push(Ship::new(ship_type, pos, direction));
            true
        } else {
            false
        }
    }

    // Validate the game state
    pub fn check(&self) -> bool {
        // Check all ships are within bounds and don't overlap
        for (i, ship_i) in self.ships.iter().enumerate() {
            let coords_i = ship_i.get_coordinates();
            
            // Check bounds
            if coords_i.iter().any(|&(x, y)| 
                x as usize >= BOARD_SIZE || y as usize >= BOARD_SIZE) {
                return false;
            }

            // Check ship type uniqueness and overlap
            for (j, ship_j) in self.ships.iter().enumerate().skip(i + 1) {
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
            found_types[ship.ship_type as usize] = true;
        }
        found_types.iter().all(|&present| present)
    }

    pub fn apply_shot(&mut self, x: u8, y: u8) -> Option<HitType> {
        if x as usize >= BOARD_SIZE || y as usize >= BOARD_SIZE {
            return None;
        }

        let cell = &mut self.grid[y as usize][x as usize];
        if *cell != CellState::Empty {
            return None;  // Already shot here
        }

        // Check if we hit any ships
        for ship in &mut self.ships {
            if ship.check_hit(x, y) {
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
        let mut positions: Vec<(u8, u8)> = (0..BOARD_SIZE as u8)
            .flat_map(|x| (0..BOARD_SIZE as u8).map(move |y| (x, y)))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub grid: [[CellState; BOARD_SIZE]; BOARD_SIZE],
    pub ships: Vec<Ship>,
}

impl Board {
    pub fn new() -> Self {
        Self {
            grid: [[CellState::Empty; BOARD_SIZE]; BOARD_SIZE],
            ships: Vec::with_capacity(NUM_SHIPS),
        }
    }

    // Validate if a ship can be placed at the given position
    pub fn can_place_ship(&self, ship_type: ShipType, pos: (u8, u8), direction: Direction) -> bool {
        let (x, y) = pos;
        let size = ship_type.size();

        // Check if ship would extend beyond board
        match direction {
            Direction::Horizontal => {
                if x as usize + size as usize > BOARD_SIZE {
                    return false;
                }
            }
            Direction::Vertical => {
                if y as usize + size as usize > BOARD_SIZE {
                    return false;
                }
            }
        }

        // Create temporary ship to check its coordinates
        let temp_ship = Ship::new(ship_type, pos, direction);
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

    pub fn place_ship(&mut self, ship_type: ShipType, pos: (u8, u8), direction: Direction) -> bool {
        if self.can_place_ship(ship_type, pos, direction) {
            self.ships.push(Ship::new(ship_type, pos, direction));
            true
        } else {
            false
        }
    }

    // Process a shot at the given coordinates
    pub fn shoot(&mut self, x: u8, y: u8) -> Option<CellState> {
        if x as usize >= BOARD_SIZE || y as usize >= BOARD_SIZE {
            return None;
        }

        let cell = &mut self.grid[y as usize][x as usize];
        if *cell != CellState::Empty {
            return None;  // Already shot here
        }

        // Check if we hit any ships
        for ship in &mut self.ships {
            if ship.check_hit(x, y) {
                *cell = CellState::Hit;
                return Some(CellState::Hit);
            }
        }

        *cell = CellState::Miss;
        Some(CellState::Miss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_board() {
        let state = GameState {
            ships: vec![
                Ship::new(ShipType::Carrier, (2, 3), Direction::Vertical),
                Ship::new(ShipType::Battleship, (3, 1), Direction::Horizontal),
                Ship::new(ShipType::Cruiser, (4, 7), Direction::Vertical),
                Ship::new(ShipType::Submarine, (7, 5), Direction::Horizontal),
                Ship::new(ShipType::Destroyer, (7, 7), Direction::Horizontal),
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
}