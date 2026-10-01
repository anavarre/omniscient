//! The visible list as flat, keyed rows, and the minimal edit between two
//! of them.
//!
//! Every page renders as one list of rows (section headers, tasks,
//! sub-tasks, "show more", projects…). Each row carries a stable `key` and
//! its full display data as JSON; the JSON doubles as the fingerprint, so
//! a row is re-sent to QML exactly when something it shows changed. The
//! Qt list model applies the ops below as fine-grained insert/remove/move/
//! change notifications, which keeps scroll position, selection and running
//! animations intact across edits.

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: String,
    pub json: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Remove `count` rows starting at `at`.
    Remove { at: usize, count: usize },
    /// Insert `rows` so the first lands at `at`.
    Insert { at: usize, rows: Vec<Row> },
    /// Move one row from `from` to `to` (`to < from`: rows only move up).
    Move { from: usize, to: usize },
    /// Row `at` keeps its key but shows new data.
    Change { at: usize, row: Row },
    /// Too much changed to be worth animating: replace everything.
    Reset { rows: Vec<Row> },
}

/// Ops that turn `old` into `new`. Applying them in order (see `apply`)
/// reproduces `new` exactly; keys must be unique within each list.
pub fn diff(old: &[Row], new: &[Row]) -> Vec<Op> {
    use std::collections::{HashMap, HashSet};
    let keep: HashSet<&str> = new.iter().map(|r| r.key.as_str()).collect();
    let mut ops = vec![];
    let mut cur: Vec<Row> = old.to_vec();

    // 1. Drop rows that are gone, as contiguous runs from the bottom up.
    let mut i = cur.len();
    while i > 0 {
        i -= 1;
        if keep.contains(cur[i].key.as_str()) {
            continue;
        }
        let end = i + 1;
        while i > 0 && !keep.contains(cur[i - 1].key.as_str()) {
            i -= 1;
        }
        ops.push(Op::Remove { at: i, count: end - i });
        cur.drain(i..end);
    }

    // 2. Walk the target order. Rows before the cursor are final; a known
    //    row moves up to the cursor, unknown ones are inserted there
    //    (consecutive inserts batch into one op).
    let positions = |cur: &[Row]| -> HashMap<String, usize> { cur.iter().enumerate().map(|(i, r)| (r.key.clone(), i)).collect() };
    let mut index = positions(&cur);
    let mut batch: Option<(usize, Vec<Row>)> = None;
    let flush = |batch: &mut Option<(usize, Vec<Row>)>, cur: &mut Vec<Row>, ops: &mut Vec<Op>| -> bool {
        let Some((at, rows)) = batch.take() else { return false };
        cur.splice(at..at, rows.iter().cloned());
        ops.push(Op::Insert { at, rows });
        true
    };
    for (i, want) in new.iter().enumerate() {
        if !index.contains_key(&want.key) {
            batch.get_or_insert_with(|| (i, vec![])).1.push(want.clone());
            continue;
        }
        if flush(&mut batch, &mut cur, &mut ops) {
            index = positions(&cur);
        }
        let j = index[&want.key];
        if j != i {
            let row = cur.remove(j);
            cur.insert(i, row);
            ops.push(Op::Move { from: j, to: i });
            index = positions(&cur);
        }
        if cur[i].json != want.json {
            cur[i].json.clone_from(&want.json);
            ops.push(Op::Change { at: i, row: want.clone() });
        }
    }
    flush(&mut batch, &mut cur, &mut ops);

    // Regrouping reshuffles everything; one reset beats hundreds of moves.
    let structural = ops.iter().filter(|o| !matches!(o, Op::Change { .. })).count();
    if structural > 48 && structural * 2 > new.len() {
        return vec![Op::Reset { rows: new.to_vec() }];
    }
    ops
}

/// Applies ops to a row list — what the Qt model does, minus notifications.
pub fn apply(rows: &mut Vec<Row>, ops: &[Op]) {
    for op in ops {
        match op {
            Op::Remove { at, count } => {
                rows.drain(*at..*at + *count);
            }
            Op::Insert { at, rows: new } => {
                rows.splice(*at..*at, new.iter().cloned());
            }
            Op::Move { from, to } => {
                let r = rows.remove(*from);
                rows.insert(*to, r);
            }
            Op::Change { at, row } => rows[*at] = row.clone(),
            Op::Reset { rows: new } => *rows = new.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(spec: &str) -> Vec<Row> {
        // "a b c'" → keys a, b, c; a trailing ' marks changed data.
        spec.split_whitespace()
            .map(|s| Row { key: s.trim_end_matches('\'').into(), json: s.into() })
            .collect()
    }

    fn check(old: &str, new: &str) -> Vec<Op> {
        let (old, new) = (rows(old), rows(new));
        let ops = diff(&old, &new);
        let mut got = old.clone();
        apply(&mut got, &ops);
        assert_eq!(got, new, "ops {ops:?}");
        ops
    }

    #[test]
    fn identical_lists_need_nothing() {
        assert!(check("a b c", "a b c").is_empty());
        assert!(check("", "").is_empty());
    }

    #[test]
    fn one_changed_row_is_one_change() {
        assert_eq!(check("a b c", "a b' c"), vec![Op::Change { at: 1, row: Row { key: "b".into(), json: "b'".into() } }]);
    }

    #[test]
    fn inserts_batch_and_removes_coalesce() {
        assert_eq!(check("a d", "a b c d").len(), 1);
        assert_eq!(check("a b c d e", "a e").len(), 1);
        check("", "a b c");
        check("a b c", "");
    }

    #[test]
    fn moves_and_mixed_edits() {
        check("a b c d", "d a b c");
        check("a b c d", "b a d c");
        check("s1 a b s2 c", "s1 b s2 c a'");
        check("s1 a x b s2 c", "s0 y s1 b' a s2 z c");
        check("a b c", "c x b y a");
    }

    #[test]
    fn task_leaving_a_section_keeps_neighbours() {
        // Completing "b" moves it from Today into Completed.
        let ops = check("today a b c done x", "today a c done b' x");
        assert!(ops.iter().all(|o| !matches!(o, Op::Reset { .. })));
    }

    #[test]
    fn reshuffles_fall_back_to_reset() {
        let old: String = (0..100).map(|i| format!("k{i} ")).collect();
        let new: String = (0..100).rev().map(|i| format!("k{i} ")).collect();
        let ops = check(&old, &new);
        assert!(matches!(ops.as_slice(), [Op::Reset { .. }]));
    }

    #[test]
    fn randomised_round_trips() {
        // Deterministic LCG so failures reproduce.
        let mut seed: u64 = 42;
        let mut next = |n: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        for _ in 0..500 {
            let old: Vec<String> = (0..next(12)).map(|i| format!("k{i}")).collect();
            let mut new: Vec<String> = old.iter().filter(|_| next(4) != 0).cloned().collect();
            for _ in 0..next(4) {
                let at = next(new.len() as u64 + 1) as usize;
                new.insert(at, format!("n{}", next(1000)));
            }
            new.dedup();
            let mut seen = std::collections::HashSet::new();
            new.retain(|k| seen.insert(k.clone()));
            for _ in 0..next(3) {
                if new.len() > 1 {
                    let a = next(new.len() as u64) as usize;
                    let b = next(new.len() as u64) as usize;
                    new.swap(a, b);
                }
            }
            let new: Vec<String> = new.into_iter().map(|k| if next(5) == 0 { format!("{k}'") } else { k }).collect();
            check(&old.join(" "), &new.join(" "));
        }
    }
}
