include!("d-compact-map.rs");

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_map_keeps_exact_escape_values_and_wide_to_small_improvements() {
        let limit = u64::from(u32::MAX);
        let mut got = Search::new();
        for (node, distance) in [limit - 1, limit, limit + 1, u64::MAX - 1]
            .into_iter()
            .enumerate()
        {
            assert!(got.improve(node as u32, distance));
            assert_eq!(got.get(node as u32), distance);
            assert!(!got.improve(node as u32, distance));
            assert!(!got.improve(node as u32, u64::MAX));
            assert!(got.improve(node as u32, 2));
            assert_eq!(got.get(node as u32), 2);
            assert!(!got.improve(node as u32, limit + 2));
        }
        assert!(!got.improve(u32::MAX, u64::MAX));
        assert_eq!(got.get(u32::MAX), u64::MAX);
        assert!(got.improve(u32::MAX, limit));
        assert_eq!(got.get(u32::MAX), limit);
        got.start(17);
        assert_eq!(got.get(17), 0);
        for node in [0, 1, 2, 3, u32::MAX] {
            assert_eq!(got.get(node), u64::MAX);
        }
    }
    #[test]
    fn compact_map_random_traces_match_full_width_map() {
        let mut state = 127u64;
        let mut got = Search::new();
        let mut reference = HashMap::new();
        for i in 0..50_000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let node = (state >> 32) as u32 & 4095;
            let distance = if i % 3 == 0 {
                state & 0xffff_ffff
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
                got.start(node);
                reference.clear();
                reference.insert(node, 0);
            }
        }
    }
}
