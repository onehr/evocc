//! C1: bounded candidates for the existing total GPR MovImm operation.
//! Tables choose which families exist; costs select, never justify equality.

use super::costs::{LOGICAL_BYTES, WIDE_BYTES};
use super::encodings::{MOV_WIDE_MAX, MOV_WIDE_SHIFT};
use super::patterns::{ConstantKind, CONSTANT_PATTERNS};
use crate::Width;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Zero { half: u16, shift: u32 },
    Ones { half: u16, shift: u32 },
    Keep { half: u16, shift: u32 },
    Logical(u64),
}

impl Part {
    fn bytes(self) -> u32 {
        match self {
            Self::Logical(_) => LOGICAL_BYTES,
            _ => WIDE_BYTES,
        }
    }
}

fn wide(value: u64, width: Width, ones: bool) -> Vec<Part> {
    let halves = u32::from(width.bits()) / MOV_WIDE_SHIFT;
    let fill = if ones { MOV_WIDE_MAX as u16 } else { 0 };
    let half = |index| (value >> (MOV_WIDE_SHIFT * index)) as u16;
    let first = (0..halves).find(|&i| half(i) != fill).unwrap_or(0);
    let mut result = vec![if ones {
        Part::Ones {
            half: !half(first),
            shift: MOV_WIDE_SHIFT * first,
        }
    } else {
        Part::Zero {
            half: half(first),
            shift: MOV_WIDE_SHIFT * first,
        }
    }];
    for index in 0..halves {
        if index != first && half(index) != fill {
            result.push(Part::Keep {
                half: half(index),
                shift: MOV_WIDE_SHIFT * index,
            });
        }
    }
    result
}

/// None means preserve the legacy sequence exactly, including ties. No new
/// scratch, memory, flags or allocation effects are introduced by a candidate.
pub fn select(value: u64, width: Width) -> Option<Vec<Part>> {
    let value = value & width.mask();
    let legacy_parts = 1
        + (1..u32::from(width.bits()) / MOV_WIDE_SHIFT)
            .filter(|i| (value >> (MOV_WIDE_SHIFT * i)) & MOV_WIDE_MAX != 0)
            .count() as u32;
    let mut best_bytes = legacy_parts * WIDE_BYTES;
    let mut best = None;
    for pattern in CONSTANT_PATTERNS {
        if !pattern.widths.contains(&u32::from(width.bits())) {
            continue;
        }
        let candidate = match pattern.kind {
            ConstantKind::MovzMovk => wide(value, width, false),
            ConstantKind::MovnMovk => wide(value, width, true),
            ConstantKind::OrrZero => {
                if !crate::immediate::logical_mask(value, width) {
                    continue;
                }
                vec![Part::Logical(value)]
            }
        };
        let bytes = candidate.iter().map(|p| p.bytes()).sum();
        if bytes < best_bytes {
            best_bytes = bytes;
            best = Some(candidate);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reconstruct destination bits, not the selection algorithm. Start with
    // dirty upper/lower bits to catch dependence on a previous destination.
    fn execute(parts: &[Part], width: Width) -> u64 {
        let mut destination = 0xa5a5_5a5a_dead_beef;
        for part in parts {
            destination = match *part {
                Part::Zero { half, shift } => u64::from(half) << shift,
                Part::Ones { half, shift } => !(u64::from(half) << shift),
                Part::Keep { half, shift } => {
                    (destination & !(0xffff_u64 << shift)) | (u64::from(half) << shift)
                }
                Part::Logical(value) => value,
            } & width.mask();
        }
        destination
    }

    fn check(value: u64, width: Width) {
        if let Some(parts) = select(value, width) {
            assert_eq!(execute(&parts, width), value & width.mask());
            for part in &parts {
                match part {
                    Part::Zero { shift, .. }
                    | Part::Ones { shift, .. }
                    | Part::Keep { shift, .. } => {
                        assert!(*shift < u32::from(width.bits()) && shift % 16 == 0)
                    }
                    Part::Logical(_) => assert_eq!(parts.len(), 1),
                }
            }
            let legacy_count = 1
                + (1..u32::from(width.bits()) / 16)
                    .filter(|i| (value >> (16 * i)) & 0xffff != 0)
                    .count();
            assert!(parts.len() < legacy_count);
        }
    }

    #[test]
    fn boundaries_every_halfword_and_fixed_random_values_reconstruct() {
        for width in [Width::W, Width::X] {
            for value in [
                0,
                1,
                u64::MAX,
                0xffff_ffff,
                0xffff_0000_ffff_0000,
                0x8000_0000_0000_0000,
                0x1234_5678_9abc_def0,
            ] {
                check(value, width);
            }
            for shift in (0..u32::from(width.bits())).step_by(16) {
                for half in 0..=u16::MAX {
                    let value = u64::from(half) << shift;
                    check(value, width);
                    check(!value, width);
                }
            }
            let mut state = 0xa197_7364_55ed_cafe_u64;
            for _ in 0..65536 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                check(state, width);
            }
        }
    }

    #[test]
    fn every_logical_encoding_reconstructs_and_legacy_ties_stay_unchanged() {
        for width in [Width::W, Width::X] {
            for size in [2, 4, 8, 16, 32, 64] {
                if size > u32::from(width.bits()) {
                    continue;
                }
                for count in 1..size {
                    for start in 0..size {
                        let mut value = 0;
                        for repeat in 0..u32::from(width.bits()) / size {
                            for bit in 0..count {
                                value |= 1_u64 << (repeat * size + (start + bit) % size);
                            }
                        }
                        check(value, width);
                    }
                }
            }
            for value in [0, 1, 0xffff, 0x1234_5678_9abc_def0] {
                assert_eq!(select(value, width), None);
            }
        }
    }
}
