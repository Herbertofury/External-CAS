//! Complete search display groups, with cooperative cancellation at work boundaries.
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GalleryGroup {
    pub(crate) items: Vec<usize>,
}

/// Retain first-seen design order and stable swatch ordering within every design.
/// Cancellation never returns a partial set. One group's stable sort is indivisible.
pub(crate) fn group_search_rows<K: Ord>(
    rows: &[usize],
    cancel: &AtomicBool,
    design: impl Fn(usize) -> usize,
    order: impl Fn(usize) -> K,
) -> Option<Vec<GalleryGroup>> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    let mut positions = HashMap::new();
    let mut groups = Vec::<GalleryGroup>::new();
    for (ordinal, &index) in rows.iter().enumerate() {
        if ordinal % 256 == 0 && cancel.load(Ordering::Relaxed) {
            return None;
        }
        let next = groups.len();
        let group = *positions.entry(design(index)).or_insert(next);
        if group == next {
            groups.push(GalleryGroup { items: Vec::new() });
        }
        groups[group].items.push(index);
    }
    for group in &mut groups {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        group.items.sort_by_key(|&index| order(index));
    }
    (!cancel.load(Ordering::Relaxed)).then_some(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_all_items_first_seen_design_order_and_stable_swatch_ties() {
        let cancel = AtomicBool::new(false);
        let design = [3, 8, 8, 3, 5, 3];
        let order = [(3, 9), (1, 6), (0, 7), (2, 3), (0, 0), (2, 3)];
        let rows = [0, 1, 2, 5, 3, 4];
        let groups = group_search_rows(&rows, &cancel, |i| design[i], |i| order[i]).unwrap();
        assert_eq!(
            groups.into_iter().map(|g| g.items).collect::<Vec<_>>(),
            vec![vec![5, 3, 0], vec![2, 1], vec![4]]
        );
    }

    #[test]
    fn already_cancelled_search_never_reads_or_returns_even_empty_results() {
        let cancel = AtomicBool::new(true);
        assert!(group_search_rows(&[], &cancel, |_| unreachable!(), |_| ()).is_none());
        assert!(group_search_rows(&[4], &cancel, |_| unreachable!(), |_| ()).is_none());
    }

    #[test]
    fn cancelling_a_broad_search_stops_before_sorting_or_returning_partial_results() {
        let cancel = AtomicBool::new(false);
        let rows = (0..4096).collect::<Vec<_>>();
        let visited = std::cell::Cell::new(0);
        let result = group_search_rows(
            &rows,
            &cancel,
            |i| {
                visited.set(visited.get() + 1);
                if i == 100 {
                    cancel.store(true, Ordering::Relaxed);
                }
                i % 17
            },
            |_| -> usize { panic!("cancelled search must not sort") },
        );
        assert!(result.is_none());
        assert_eq!(visited.get(), 256);
    }

    #[test]
    fn cancellation_within_sort_discards_completed_group_and_skips_the_next() {
        let cancel = AtomicBool::new(false);
        let result = group_search_rows(&[1, 0, 3, 2], &cancel, |i| i / 2, |i| {
            assert!(i < 2, "obsolete search must not start sorting another design");
            cancel.store(true, Ordering::Relaxed);
            i
        });
        assert!(result.is_none());
    }
}
