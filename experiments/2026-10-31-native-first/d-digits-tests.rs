include!("d-digits.rs");

#[cfg(test)]
mod tests {
    use super::*;
    fn check(value: u32, prefix: usize, suffix: usize, separator: u8) {
        let mut input = vec![b'X'; prefix];
        let decimal = value.to_string();
        input.extend_from_slice(decimal.as_bytes());
        input.push(separator);
        // Adjacent numeric content must not leak through the wide load.
        input.extend(std::iter::repeat_n(b'9', suffix));
        let mut cursor = prefix;
        assert_eq!(number(&input, &mut cursor), value);
        assert_eq!(cursor, prefix + decimal.len() + 1);
    }
    #[test]
    fn decimal_widths_alignment_and_chunk_tails() {
        let mut values = vec![0, 1, u32::MAX];
        for exponent in 1..=9 {
            let power = 10u32.pow(exponent);
            values.extend([power - 1, power, power + 1]);
        }
        for value in values {
            for prefix in 0..17 {
                for suffix in 0..12 {
                    check(value, prefix, suffix, b' ');
                    check(value, prefix, suffix, b'\n');
                }
            }
        }
    }
    #[test]
    fn arbitrary_uint32_fields_and_adjacent_fields() {
        let mut state = 97u64;
        for i in 0..20_000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let value = (state >> 32) as u32;
            check(value, i % 17, i % 12, if i % 2 == 0 { b' ' } else { b'\n' });
        }
        let fields = [0, 9, 10, 9999999, 10000000, u32::MAX, 7];
        let input = fields
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(" ")
            + "\n";
        let mut cursor = 0;
        for value in fields {
            assert_eq!(number(input.as_bytes(), &mut cursor), value);
        }
        assert_eq!(cursor, input.len());
    }
}
