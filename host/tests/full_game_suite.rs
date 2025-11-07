use core::{GameState, ShipType, Direction, Position, HitType, BOARD_SIZE};
use host::visualize::display_board_str;
use rand::{SeedableRng, rngs::StdRng, Rng};

// Comprehensive test suite merging prior e2e tests and adding randomized
// full-game simulations and edge cases.

#[test]
fn test_basic_hit_then_sunk_behavior() {
    let mut p2 = GameState::new([0;16]);
    p2.place_ship(ShipType::Destroyer, Position::new(0,0), Direction::Horizontal);

    // miss
    assert_eq!(p2.apply_shot(Position::new(5,5)), Some(HitType::Miss));

    // hit then sink
    assert_eq!(p2.apply_shot(Position::new(0,0)), Some(HitType::Hit));
    assert_eq!(p2.apply_shot(Position::new(1,0)), Some(HitType::Sunk(ShipType::Destroyer)));
}

#[test]
fn test_miss_and_repeated_shot_behavior() {
    let mut p = GameState::new([0;16]);
    let res1 = p.apply_shot(Position::new(9,9));
    assert_eq!(res1, Some(HitType::Miss));
    // second shot at same cell -> None (already shot)
    let res2 = p.apply_shot(Position::new(9,9));
    assert_eq!(res2, None);
}

#[test]
fn test_visualization_hidden_and_revealed() {
    let mut p = GameState::new([0;16]);
    p.place_ship(ShipType::Carrier, Position::new(0,0), Direction::Horizontal);

    let hidden = display_board_str(&p, false);
    let revealed = display_board_str(&p, true);

    assert!(!hidden.contains('S'), "Hidden board leaked ship markers");
    assert!(revealed.contains('S'));
}

#[test]
fn test_randomized_full_game_simulations() {
    // Run several seeded simulations to increase confidence
    for seed in 0_u64..10_u64 {
        let mut rng = StdRng::seed_from_u64(seed);

        let mut p1 = GameState::new([0;16]);
        let mut p2 = GameState::new([0;16]);

        // try to place randomly; allow a few attempts in case of failure
        let mut ok = false;
        for _ in 0..10 {
            let mut r1 = StdRng::seed_from_u64(rng.gen());
            let mut r2 = StdRng::seed_from_u64(rng.gen());
            p1.ships.clear();
            p2.ships.clear();
            if p1.place_ships_randomly(&mut r1) && p2.place_ships_randomly(&mut r2) {
                ok = true;
                break;
            }
        }
        assert!(ok, "random placement failed repeatedly for seed {}", seed);

        // Simulate naive scanning players as in full_game_sim
        let mut turn = 0usize;
        let mut p1_idx = 0usize;
        let mut p2_idx = 0usize;
        let board_size = BOARD_SIZE as usize;
        let total = board_size * board_size;
        let mut moves = 0usize;
        while moves < 2000 {
            moves += 1;
            if turn == 0 {
                // p1 target
                // find next unseen cell
                let mut found = false;
                while p1_idx < total {
                    let x = (p1_idx) % board_size;
                    let y = (p1_idx) / board_size;
                    p1_idx += 1;
                    match p2.grid[y][x] {
                        core::CellState::Empty => {
                            found = true;
                            match p2.apply_shot(Position::new(x as u32, y as u32)) {
                                Some(HitType::Miss) => { turn = 1; break; }
                                Some(HitType::Hit) => { break; }
                                Some(HitType::Sunk(_)) => { turn = 1; break; }
                                None => continue,
                            }
                        }
                        _ => continue,
                    }
                }
                if !found { turn = 1; }
                if p2.ships.iter().all(|s| s.is_sunk()) { break; }
            } else {
                let mut found = false;
                while p2_idx < total {
                    let x = (p2_idx) % board_size;
                    let y = (p2_idx) / board_size;
                    p2_idx += 1;
                    match p1.grid[y][x] {
                        core::CellState::Empty => {
                            found = true;
                            match p1.apply_shot(Position::new(x as u32, y as u32)) {
                                Some(HitType::Miss) => { turn = 0; break; }
                                Some(HitType::Hit) => { break; }
                                Some(HitType::Sunk(_)) => { turn = 0; break; }
                                None => continue,
                            }
                        }
                        _ => continue,
                    }
                }
                if !found { turn = 0; }
                if p1.ships.iter().all(|s| s.is_sunk()) { break; }
            }
        }

        // ensure one side lost
        let p1_all = p1.ships.iter().all(|s| s.is_sunk());
        let p2_all = p2.ships.iter().all(|s| s.is_sunk());
        assert!(p1_all ^ p2_all, "Exactly one player's fleet should be sunk (seed {})", seed);
    }
}

#[test]
fn test_edge_cases_placement_and_shots() {
    let state = GameState::new([0;16]);
    // boundary placement valid
    assert!(state.can_place_ship(ShipType::Carrier, Position::new(5,9), Direction::Horizontal));
    assert!(!state.can_place_ship(ShipType::Carrier, Position::new(6,9), Direction::Horizontal));

    // out of bounds shot
    let mut s = GameState::new([0;16]);
    assert_eq!(s.apply_shot(Position::new(100, 100)), None);
}

#[test]
fn test_place_ships_iterative_partial_success() {
    let mut state = GameState::new([0;16]);
    // First placement valid
    let placements = vec![
        (ShipType::Carrier, Position::new(0,0), Direction::Horizontal),
        // Overlaps carrier -> should fail, but carrier should remain placed
        (ShipType::Battleship, Position::new(0,0), Direction::Vertical),
    ];
    let all_ok = state.place_ships(placements);
    assert!(!all_ok, "Expected partial failure due to overlap");
    // Carrier should be present
    assert!(state.ships.iter().any(|s| s.ship_type == ShipType::Carrier));
    // Battleship should not be present
    assert!(!state.ships.iter().any(|s| s.ship_type == ShipType::Battleship));
}

#[test]
fn test_can_place_rejects_duplicate_ship_type() {
    let mut state = GameState::new([0;16]);
    assert!(state.place_ship(ShipType::Cruiser, Position::new(0,0), Direction::Horizontal));
    // same type again should be rejected
    assert!(!state.can_place_ship(ShipType::Cruiser, Position::new(2,2), Direction::Vertical));
}

#[test]
fn test_ship_hit_mask_and_sinking() {
    let mut state = GameState::new([0;16]);
    state.place_ship(ShipType::Cruiser, Position::new(4,4), Direction::Horizontal);
    // hit middle segment
    assert_eq!(state.apply_shot(Position::new(5,4)), Some(HitType::Hit));
    // ensure hits bitmask non-zero and not sunk
    assert!(state.ships[0].hits != 0);
    assert!(!state.ships[0].is_sunk());
    // hit remaining segments
    assert_eq!(state.apply_shot(Position::new(4,4)), Some(HitType::Hit));
    assert_eq!(state.apply_shot(Position::new(6,4)), Some(HitType::Sunk(ShipType::Cruiser)));
    assert!(state.ships[0].is_sunk());
}

#[test]
fn test_commit_consistency_and_mutation() {
    let mut s1 = GameState::new([0;16]);
    s1.place_ship(ShipType::Destroyer, Position::new(2,2), Direction::Horizontal);
    let mut s2 = s1.clone();
    // commits equal
    assert_eq!(s1.commit(), s2.commit());
    // mutate s2's pepper -> commit should differ
    s2.pepper[0] = 1;
    assert_ne!(s1.commit(), s2.commit());
    // mutate s1 by applying a shot -> commit differs
    let _ = s1.apply_shot(Position::new(2,2));
    assert_ne!(s1.commit(), s2.commit());
}

#[test]
fn test_place_ships_randomly_and_check() {
    let mut s = GameState::new([0;16]);
    let mut rng = StdRng::seed_from_u64(42);
    let ok = s.place_ships_randomly(&mut rng);
    assert!(ok, "random placement failed");
    assert!(s.check(), "state.check() should be true after random placement");
}

