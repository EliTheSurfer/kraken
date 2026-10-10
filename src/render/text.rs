use crate::graph::Row;

pub fn render_rows(rows: &[Row]) -> Vec<String> {
    rows.iter().map(render_row).collect()
}

pub fn render_row(row: &Row) -> String {
    let mut cells = vec![' '; row.width * 2 - 1];
    for line in &row.pass_through {
        cells[line.lane * 2] = '│';
    }
    let incoming: Vec<usize> = row.incoming.iter().map(|line| line.lane).collect();
    let outgoing: Vec<usize> = row.outgoing.iter().map(|line| line.to_lane).collect();
    draw_connections(&mut cells, row.lane, &incoming, Direction::Up);
    draw_connections(&mut cells, row.lane, &outgoing, Direction::Down);
    cells[row.lane * 2] = if row.merge { '○' } else { '●' };
    cells.iter().collect::<String>().trim_end().to_string()
}

pub fn render_continuation(row: &Row) -> String {
    let mut cells = vec![' '; row.width * 2 - 1];
    let lanes = row
        .pass_through
        .iter()
        .map(|line| line.lane)
        .chain(row.outgoing.iter().map(|line| line.to_lane))
        .chain(row.continues.then_some(row.lane));
    for lane in lanes {
        cells[lane * 2] = '│';
    }
    cells.iter().collect::<String>().trim_end().to_string()
}

#[derive(Clone, Copy)]
enum Direction {
    Up,
    Down,
}

fn draw_connections(cells: &mut [char], node_lane: usize, lanes: &[usize], direction: Direction) {
    let node = node_lane * 2;
    for &lane in lanes {
        let target = lane * 2;
        let (low, high) = (node.min(target), node.max(target));
        for cell in cells.iter_mut().take(high).skip(low + 1) {
            *cell = cross(*cell);
        }
    }
    let farthest_right = lanes
        .iter()
        .filter(|&&lane| lane > node_lane)
        .max()
        .copied();
    let farthest_left = lanes
        .iter()
        .filter(|&&lane| lane < node_lane)
        .min()
        .copied();
    for &lane in lanes {
        let target = lane * 2;
        let end = Some(lane) == farthest_right || Some(lane) == farthest_left;
        cells[target] = corner(cells[target], lane > node_lane, end, direction);
    }
}

fn cross(current: char) -> char {
    match current {
        '│' => '┼',
        ' ' => '─',
        other => other,
    }
}

fn corner(current: char, right_of_node: bool, end: bool, direction: Direction) -> char {
    if current == '│' || current == '┼' {
        return if right_of_node { '┤' } else { '├' };
    }
    match (direction, right_of_node, end) {
        (Direction::Down, true, true) => '╮',
        (Direction::Down, false, true) => '╭',
        (Direction::Up, true, true) => '╯',
        (Direction::Up, false, true) => '╰',
        (Direction::Down, _, false) => '┬',
        (Direction::Up, _, false) => '┴',
    }
}
