use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Deterministic per-turn RNG: same scenario + turn number always resolves the
/// same way, which is what makes an event log replayable.
pub fn turn_rng(simulation_run_id: i64, turn: u32) -> ChaCha8Rng {
    let seed = (simulation_run_id as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (turn as u64);
    ChaCha8Rng::seed_from_u64(seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn same_run_and_turn_produces_same_sequence() {
        let mut a = turn_rng(42, 3);
        let mut b = turn_rng(42, 3);
        let vals_a: Vec<u32> = (0..5).map(|_| a.gen()).collect();
        let vals_b: Vec<u32> = (0..5).map(|_| b.gen()).collect();
        assert_eq!(vals_a, vals_b);
    }

    #[test]
    fn different_turns_produce_different_sequences() {
        let mut a = turn_rng(42, 3);
        let mut b = turn_rng(42, 4);
        let val_a: u32 = a.gen();
        let val_b: u32 = b.gen();
        assert_ne!(val_a, val_b);
    }
}
