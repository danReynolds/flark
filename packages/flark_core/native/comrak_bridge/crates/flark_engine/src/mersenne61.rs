//! Exact arithmetic for commitment lanes over the Mersenne prime 2^61 - 1.

pub(crate) const MODULUS: u64 = (1_u64 << 61) - 1;

pub(crate) const fn add_mod(left: u64, right: u64) -> u64 {
    reduce(left as u128 + right as u128)
}

pub(crate) const fn multiply_mod(left: u64, right: u64) -> u64 {
    reduce(left as u128 * right as u128)
}

const fn reduce(value: u128) -> u64 {
    // Three folds cover the full u128 input range. For p = 2^61 - 1,
    // 2^61 is congruent to 1 mod p, so each fold is exactly equivalent to a
    // remainder operation without invoking software u128 division on ARM.
    let modulus = MODULUS as u128;
    let first = (value & modulus) + (value >> 61);
    let second = (first & modulus) + (first >> 61);
    let third = (second & modulus) + (second >> 61);
    let reduced = third as u64;
    if reduced >= MODULUS {
        reduced - MODULUS
    } else {
        reduced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(value: u128) -> u64 {
        (value % MODULUS as u128) as u64
    }

    #[test]
    fn folded_arithmetic_matches_u128_remainder() {
        let boundaries = [
            0,
            1,
            2,
            3,
            MODULUS / 2,
            MODULUS - 3,
            MODULUS - 2,
            MODULUS - 1,
            u64::MAX,
        ];
        for left in boundaries {
            for right in boundaries {
                assert_eq!(
                    add_mod(left, right),
                    reference(left as u128 + right as u128)
                );
                assert_eq!(
                    multiply_mod(left, right),
                    reference(left as u128 * right as u128),
                );
            }
        }

        let mut state = 0x6a09_e667_f3bc_c909_u64;
        for _ in 0..100_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let left = state;
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let right = state;
            assert_eq!(
                add_mod(left, right),
                reference(left as u128 + right as u128)
            );
            assert_eq!(
                multiply_mod(left, right),
                reference(left as u128 * right as u128),
            );
        }
    }
}
