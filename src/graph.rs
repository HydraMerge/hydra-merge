//! Lanes of the commit graph, as in the HydraMerge commit list. Pure, no egui.
//!
//! Commits come newest first. Each lane waits for one commit hash; a commit takes the lane that
//! waits for it, hands it to its first parent, and opens or joins lanes for other parents.

/// How one commit row is drawn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRow {
    /// Lane of the commit's dot.
    pub lane: usize,
    /// Lanes that run straight through the row (another branch passes by).
    pub through: Vec<usize>,
    /// Lanes that end at the dot from above (children of this commit).
    pub from_above: Vec<usize>,
    /// Lanes that leave the dot downwards (its parents).
    pub to_below: Vec<usize>,
    /// Lanes in use in this row, for the column width.
    pub width: usize,
}

/// One row per commit. `parents[i]` are the parent hashes of `hashes[i]`.
pub fn layout(hashes: &[&str], parents: &[Vec<&str>]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<&str>> = Vec::new();
    hashes
        .iter()
        .zip(parents)
        .map(|(hash, parents)| row(&mut lanes, hash, parents))
        .collect()
}

fn row<'a>(lanes: &mut Vec<Option<&'a str>>, hash: &'a str, parents: &[&'a str]) -> GraphRow {
    let waiting: Vec<usize> = lanes
        .iter()
        .enumerate()
        .filter(|(_, want)| **want == Some(hash))
        .map(|(i, _)| i)
        .collect();
    let lane = waiting.first().copied().unwrap_or_else(|| free_lane(lanes));
    let through: Vec<usize> = lanes
        .iter()
        .enumerate()
        .filter(|(i, want)| want.is_some() && !waiting.contains(i))
        .map(|(i, _)| i)
        .collect();
    for i in &waiting {
        lanes[*i] = None;
    }
    let to_below = assign_parents(lanes, lane, parents);
    trim(lanes);
    let width = lanes
        .len()
        .max(lane + 1)
        .max(through.iter().max().map_or(0, |m| m + 1));
    GraphRow {
        lane,
        through,
        from_above: waiting,
        to_below,
        width,
    }
}

/// The first parent continues the commit's lane; others reuse a lane already waiting for them.
fn assign_parents<'a>(
    lanes: &mut Vec<Option<&'a str>>,
    lane: usize,
    parents: &[&'a str],
) -> Vec<usize> {
    let mut to_below = Vec::new();
    for (n, parent) in parents.iter().enumerate() {
        // The first parent always keeps the commit's lane, even when another lane waits for the
        // same commit: both lanes then join at its dot, which keeps a branch's line straight.
        let existing = lanes.iter().position(|want| *want == Some(*parent));
        let target = match existing {
            _ if n == 0 => {
                ensure_len(lanes, lane + 1);
                lanes[lane] = Some(parent);
                lane
            }
            Some(at) => at,
            None => {
                let at = free_lane(lanes);
                lanes[at] = Some(parent);
                at
            }
        };
        if !to_below.contains(&target) {
            to_below.push(target);
        }
    }
    to_below
}

/// First empty lane, or a new one at the end.
fn free_lane(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(at) => at,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

fn ensure_len(lanes: &mut Vec<Option<&str>>, len: usize) {
    if lanes.len() < len {
        lanes.resize(len, None);
    }
}

/// Drops empty lanes at the right edge so the graph stays narrow.
fn trim(lanes: &mut Vec<Option<&str>>) {
    while lanes.last() == Some(&None) {
        lanes.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(commits: &[(&'static str, &[&'static str])]) -> Vec<GraphRow> {
        let hashes: Vec<&str> = commits.iter().map(|(h, _)| *h).collect();
        let parents: Vec<Vec<&str>> = commits.iter().map(|(_, p)| p.to_vec()).collect();
        layout(&hashes, &parents)
    }

    #[test]
    fn straight_history_stays_in_lane_zero() {
        let rows = run(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
        assert!(rows.iter().all(|r| r.lane == 0 && r.through.is_empty()));
        assert_eq!(rows[0].from_above, Vec::<usize>::new());
        assert_eq!(rows[1].from_above, vec![0]);
        assert_eq!(rows[0].to_below, vec![0]);
        assert!(rows[2].to_below.is_empty());
    }

    #[test]
    fn branch_and_merge_use_two_lanes() {
        // m merges x into b; x and b both come from a.
        let rows = run(&[("m", &["b", "x"]), ("x", &["a"]), ("b", &["a"]), ("a", &[])]);
        assert_eq!(rows[0].to_below, vec![0, 1]);
        assert_eq!((rows[1].lane, rows[1].through.clone()), (1, vec![0]));
        assert_eq!((rows[2].lane, rows[2].through.clone()), (0, vec![1]));
        // Both lanes wait for `a` and join at its dot.
        assert_eq!(rows[3].from_above, vec![0, 1]);
        assert_eq!(rows[3].width, 1);
    }

    #[test]
    fn two_tips_open_two_lanes() {
        let rows = run(&[("t", &["a"]), ("m", &["a"]), ("a", &[])]);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[1].through, vec![0]);
        assert_eq!(rows[2].from_above, vec![0, 1]);
        assert_eq!(rows[1].width, 2);
    }

    #[test]
    fn parent_outside_the_window_keeps_its_lane_open() {
        let rows = run(&[("b", &["a"])]);
        assert_eq!(rows[0].to_below, vec![0]);
        assert_eq!(rows[0].width, 1);
    }
}
