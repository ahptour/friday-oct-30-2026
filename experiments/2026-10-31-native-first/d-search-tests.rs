include!("d-search.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn random(state: &mut u64) -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *state
    }
    #[test]
    fn queue_matches_standard_heap_across_width_boundaries() {
        let mut state = 19;
        let mut got = Queue::new();
        let mut expected = BinaryHeap::new();
        for i in 0..60_000 {
            if random(&mut state) & 3 != 0 || expected.is_empty() {
                let value = random(&mut state);
                let distance = match i % 4 {
                    0 => value & 1023,
                    1 => u64::from(u32::MAX) + (value & 7),
                    2 => value >> 1,
                    _ => 0,
                };
                let entry = Reverse((distance, random(&mut state) as u32));
                expected.push(entry);
                got.push(entry);
            } else {
                assert_eq!(got.pop(), expected.pop());
            }
            assert_eq!(got.peek(), expected.peek().copied());
            assert_eq!(got.len(), expected.len());
            if i % 10_000 == 9999 {
                got.clear();
                expected.clear();
            }
        }
        while !expected.is_empty() {
            assert_eq!(got.pop(), expected.pop());
        }
        assert_eq!(got.pop(), None);
    }
    #[test]
    fn labels_match_map_across_growth_reuse_and_epoch_wrap() {
        let mut state = 41;
        let mut got = Distances::new();
        let mut expected: HashMap<u32, u64> = HashMap::new();
        for round in 0..4 {
            if round == 3 {
                got.epoch = u32::MAX;
            }
            got.clear();
            expected.clear();
            for i in 0..30_000 {
                let node = if i % 7 == 0 {
                    u32::MAX
                } else {
                    (random(&mut state) >> 32) as u32 % 4096
                };
                let distance = random(&mut state) >> 1;
                let old = expected.get(&node).copied().unwrap_or(u64::MAX);
                assert_eq!(got.improve(node, distance), distance < old);
                if distance < old {
                    expected.insert(node, distance);
                }
                assert_eq!(got.get(node), expected[&node]);
                let absent_or_present = random(&mut state) as u32;
                assert_eq!(
                    got.get(absent_or_present),
                    expected
                        .get(&absent_or_present)
                        .copied()
                        .unwrap_or(u64::MAX)
                );
            }
            for (&node, &distance) in &expected {
                assert_eq!(got.get(node), distance);
            }
        }
    }
}
