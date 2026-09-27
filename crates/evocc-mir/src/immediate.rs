//! Encodability of existing AArch64 immediate instruction forms.

use crate::Width;

/// A logical immediate is a repeated power-of-two element with one nonempty,
/// nonfull cyclic run of ones. Inspect that representation without folding
/// zero/all-ones operations or changing the semantic operand width.
pub(crate) fn logical_mask(value: u64, width: Width) -> bool {
    if value & !width.mask() != 0 || value == 0 || value == width.mask() {
        return false;
    }
    for size in [2, 4, 8, 16, 32, 64] {
        if size > width.bits() {
            break;
        }
        let mask = u64::MAX >> (64 - size);
        let element = value & mask;
        // Two transitions around the ring mean exactly one run of each bit.
        let rotated = ((element << 1) & mask) | (element >> (size - 1));
        if (element ^ rotated).count_ones() != 2 {
            continue;
        }
        let mut repeated = element;
        let mut filled = size;
        while filled < width.bits() {
            repeated |= repeated << filled;
            filled *= 2;
        }
        if repeated == value {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_rotated_run_and_random_nonencoding_matches_the_recognizer() {
        for (width, expected_count) in [(Width::W, 1302), (Width::X, 5334)] {
            let mut encodings = BTreeSet::new();
            // Independent construction: place each bit of every run and repeat
            // it, rather than using the recognizer's transition test/doubling.
            for size in [2, 4, 8, 16, 32, 64] {
                if size > width.bits() {
                    break;
                }
                for ones in 1..size {
                    for start in 0..size {
                        let mut value = 0_u64;
                        for repeat in 0..width.bits() / size {
                            for bit in 0..ones {
                                value |= 1_u64 << (repeat * size + (start + bit) % size);
                            }
                        }
                        assert!(logical_mask(value, width), "{width:?}: {value:x}");
                        encodings.insert(value);
                    }
                }
            }
            assert_eq!(encodings.len(), expected_count);
            let mut random = 0x9182_7364_5546_3728_u64;
            for _ in 0..65536 {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                let value = random & width.mask();
                assert_eq!(logical_mask(value, width), encodings.contains(&value));
            }
            assert!(!logical_mask(0, width));
            assert!(!logical_mask(width.mask(), width));
        }
        assert!(!logical_mask(1_u64 << 32, Width::W));
        assert!(!logical_mask(0xffff_ffff_0000_0001, Width::W));
    }
}
