include!("d-dense32.rs");

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dense_inline_escape_boundaries_and_decreasing_wide_values() {
        let distances = [
            0,
            1,
            DISTANCE_MASK - 1,
            DISTANCE_MASK,
            DISTANCE_MASK + 1,
            u64::from(u32::MAX),
            u64::MAX - 1,
        ];
        let mut got = Distances::new(distances.len());
        for (node, &distance) in distances.iter().enumerate() {
            assert!(got.improve(node as u32, distance));
            assert_eq!(got.get(node as u32), distance);
            assert!(!got.improve(node as u32, distance));
            assert!(!got.improve(node as u32, u64::MAX));
        }
        for node in 0..distances.len() {
            assert_eq!(got.improve(node as u32, 0), distances[node] != 0);
            assert_eq!(got.get(node as u32), 0);
        }
        got.clear();
        for node in 0..distances.len() {
            assert_eq!(got.get(node as u32), u64::MAX);
        }
    }
    #[test]
    fn dense_real_epoch_wrap_does_not_resurrect_old_values() {
        let mut got = Distances::new(3);
        got.improve(0, 7);
        got.improve(1, DISTANCE_MASK + 3);
        for _ in 0..u16::MAX {
            got.clear();
            assert_eq!(got.get(0), u64::MAX);
            assert_eq!(got.get(1), u64::MAX);
        }
        assert_eq!(got.epoch, 1);
        assert!(got.improve(0, 11));
        assert!(got.improve(1, DISTANCE_MASK + 1));
        assert_eq!(got.get(0), 11);
        assert_eq!(got.get(1), DISTANCE_MASK + 1);
    }
    #[test]
    fn dense_random_traces_match_full_width_map() {
        let mut state = 53u64;
        let mut got = Distances::new(1024);
        let mut reference = std::collections::HashMap::new();
        for i in 0..50_000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let node = (state >> 32) as u32 & 1023;
            let distance = if i % 3 == 0 {
                state & 0xffff
            } else {
                state >> 1
            };
            let previous = reference.get(&node).copied().unwrap_or(u64::MAX);
            assert_eq!(got.improve(node, distance), distance < previous);
            if distance < previous {
                reference.insert(node, distance);
            }
            assert_eq!(got.get(node), reference[&node]);
            if i % 5000 == 4999 {
                got.clear();
                reference.clear();
            }
        }
    }
}
