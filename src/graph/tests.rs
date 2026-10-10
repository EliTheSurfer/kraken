use super::*;
use crate::render::text::render_rows;

fn layout(history: &[(&str, &[&str])]) -> Vec<Row> {
    let mut layout = Layout::new();
    history
        .iter()
        .map(|(hash, parents)| {
            let parents: Vec<String> = parents.iter().map(|parent| parent.to_string()).collect();
            layout.push(hash, &parents)
        })
        .collect()
}

fn snapshot(history: &[(&str, &[&str])]) -> Vec<String> {
    render_rows(&layout(history))
}

#[test]
fn linear_history_stays_in_one_lane() {
    assert_eq!(
        snapshot(&[("C", &["B"]), ("B", &["A"]), ("A", &[])]),
        vec!["●", "●", "●"]
    );
}

#[test]
fn merge_opens_a_lane_that_joins_back_at_the_fork_point() {
    let history: &[(&str, &[&str])] =
        &[("M", &["B", "F"]), ("F", &["A"]), ("B", &["A"]), ("A", &[])];
    assert_eq!(snapshot(history), vec!["○─╮", "│ ●", "● │", "●─╯"]);
}

#[test]
fn two_branch_tips_from_one_commit_converge_on_it() {
    let history: &[(&str, &[&str])] = &[("X", &["A"]), ("Y", &["A"]), ("A", &[])];
    assert_eq!(snapshot(history), vec!["●", "│ ●", "●─╯"]);
}

#[test]
fn octopus_merge_opens_one_lane_per_extra_parent() {
    let history: &[(&str, &[&str])] = &[
        ("O", &["A", "B", "C"]),
        ("A", &["R"]),
        ("B", &["R"]),
        ("C", &["R"]),
        ("R", &[]),
    ];
    assert_eq!(
        snapshot(history),
        vec!["○─┬─╮", "● │ │", "│ ● │", "│ │ ●", "●─┴─╯"]
    );
}

#[test]
fn root_commit_ends_its_lane_and_a_later_root_reuses_it() {
    let rows = layout(&[("X", &[]), ("Y", &[])]);
    assert!(!rows[0].continues);
    assert_eq!(rows[1].lane, 0);
}

#[test]
fn lanes_free_up_after_a_merge_and_the_graph_narrows() {
    let rows = layout(&[
        ("M", &["B", "F"]),
        ("F", &["A"]),
        ("B", &["A"]),
        ("A", &["Z"]),
        ("Z", &[]),
    ]);
    assert_eq!(
        rows.iter().map(|row| row.width).collect::<Vec<_>>(),
        vec![2, 2, 2, 2, 1]
    );
}

#[test]
fn a_lane_keeps_its_color_and_new_lanes_get_the_next_one() {
    let rows = layout(&[
        ("M", &["B", "F"]),
        ("F", &["A"]),
        ("B", &["A"]),
        ("A", &[]),
        ("T", &["U"]),
        ("U", &[]),
    ]);
    assert_eq!(rows[0].color, rows[2].color);
    assert_eq!(rows[1].color, rows[0].outgoing[0].color);
    assert_ne!(rows[0].color, rows[1].color);
    assert_ne!(rows[4].color, rows[0].color);
    assert_ne!(rows[4].color, rows[1].color);
}

#[test]
fn merge_into_a_lane_already_waiting_reuses_it() {
    let history: &[(&str, &[&str])] =
        &[("F", &["A"]), ("M", &["B", "A"]), ("B", &["A"]), ("A", &[])];
    assert_eq!(snapshot(history), vec!["●", "├─○", "│ ●", "●─╯"]);
}

#[test]
fn branch_tip_has_no_line_from_above_but_its_parent_does() {
    let rows = layout(&[("B", &["A"]), ("A", &[])]);
    assert!(!rows[0].from_above);
    assert!(rows[1].from_above);
}
