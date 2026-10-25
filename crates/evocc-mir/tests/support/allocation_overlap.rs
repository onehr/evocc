//! Exact segment-overlap queries for the long, sparse intervals in regexec.c.

use super::{overlaps, Interval};
use std::cell::Cell;

thread_local! {
    static PROBES: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn observe_probe() {
    PROBES.with(|count| count.set(count.get() + 1));
}

fn interval(segments: Vec<(usize, usize)>) -> Interval<()> {
    Interval {
        value: 0,
        class: (),
        start: segments.first().map_or(0, |p| p.0),
        end: segments.last().map_or(0, |p| p.1),
        segments,
        spans_call: false,
        spans_second_return: false,
    }
}

#[test]
fn a_short_query_does_not_scan_a_long_disjoint_prefix() {
    let count = 4096usize;
    let long = interval((0..count).map(|i| (4 * i, 4 * i + 1)).collect());
    let short = interval(vec![(4 * count - 6, 4 * count - 6)]);
    for (left, right) in [(&long, &short), (&short, &long)] {
        PROBES.with(|value| value.set(0));
        assert!(!overlaps(left, right));
        assert!(
            PROBES.with(Cell::get) <= 2 * (count.ilog2() as usize + 4),
            "a one-segment query must skip the long prefix, probes={}",
            PROBES.with(Cell::get)
        );
    }
}

#[test]
fn similarly_sized_disjoint_lists_keep_a_linear_probe_bound() {
    let count = 1024;
    let left = interval((0..count).map(|i| (4 * i, 4 * i)).collect());
    let right = interval((0..count).map(|i| (4 * i + 2, 4 * i + 2)).collect());
    PROBES.with(|value| value.set(0));
    assert!(!overlaps(&left, &right));
    assert!(PROBES.with(Cell::get) <= 4 * count + 4);
}

fn from_points(mask: u32) -> Interval<()> {
    let mut segments: Vec<(usize, usize)> = vec![];
    for at in 0..6 {
        if mask & (1 << at) != 0 {
            if let Some(last) = segments.last_mut().filter(|last| last.1 + 1 == at) {
                last.1 = at;
            } else {
                segments.push((at, at));
            }
        }
    }
    interval(segments)
}

#[test]
fn overlap_matches_all_pairs_of_small_point_sets() {
    for left in 0..64 {
        for right in 0..64 {
            assert_eq!(
                overlaps(&from_points(left), &from_points(right)),
                left & right != 0,
                "point sets {left:#x}, {right:#x}"
            );
        }
    }
}

#[test]
fn inclusive_endpoints_and_maximum_addresses_are_exact() {
    let max = usize::MAX;
    let ranges = interval(vec![(max - 4, max - 2), (max, max)]);
    assert!(!overlaps(&ranges, &interval(vec![(max - 1, max - 1)])));
    assert!(overlaps(&ranges, &interval(vec![(max - 2, max - 2)])));
    assert!(overlaps(&ranges, &interval(vec![(max, max)])));
}
