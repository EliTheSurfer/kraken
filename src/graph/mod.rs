pub const PALETTE_SIZE: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub lane: usize,
    pub color: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outgoing {
    pub to_lane: usize,
    pub color: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub lane: usize,
    pub color: usize,
    pub merge: bool,
    pub pass_through: Vec<Line>,
    pub incoming: Vec<Line>,
    pub outgoing: Vec<Outgoing>,
    pub continues: bool,
    pub from_above: bool,
    pub width: usize,
}

#[derive(Clone, Debug)]
struct Lane {
    waiting_for: String,
    color: usize,
}

#[derive(Default)]
pub struct Layout {
    lanes: Vec<Option<Lane>>,
    next_color: usize,
}

impl Layout {
    pub fn new() -> Self {
        Layout::default()
    }

    pub fn push(&mut self, hash: &str, parents: &[String]) -> Row {
        let waiting: Vec<usize> = self.lanes_waiting_for(hash);
        let lane = waiting.first().copied().unwrap_or_else(|| self.free_lane());
        let color = match &self.lanes[lane] {
            Some(existing) if waiting.contains(&lane) => existing.color,
            _ => self.take_color(),
        };
        let incoming = self.close_lanes(&waiting[waiting.len().min(1)..]);
        let pass_through = self.open_lines(lane);

        let continues = !parents.is_empty();
        self.lanes[lane] = parents.first().map(|parent| Lane {
            waiting_for: parent.clone(),
            color,
        });
        let outgoing = parents
            .iter()
            .skip(1)
            .map(|parent| self.route_parent(parent))
            .collect::<Vec<_>>();

        let used = incoming
            .iter()
            .map(|line| line.lane)
            .chain(pass_through.iter().map(|line| line.lane))
            .chain(outgoing.iter().map(|line| line.to_lane))
            .chain([lane])
            .max()
            .unwrap_or(0);
        self.trim();
        Row {
            lane,
            color,
            merge: parents.len() > 1,
            pass_through,
            incoming,
            outgoing,
            continues,
            from_above: waiting.contains(&lane),
            width: (used + 1).max(self.lanes.len()),
        }
    }

    fn lanes_waiting_for(&self, hash: &str) -> Vec<usize> {
        self.lanes
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.as_ref().is_some_and(|lane| lane.waiting_for == hash))
            .map(|(index, _)| index)
            .collect()
    }

    fn free_lane(&mut self) -> usize {
        if let Some(index) = self.lanes.iter().position(Option::is_none) {
            return index;
        }
        self.lanes.push(None);
        self.lanes.len() - 1
    }

    fn take_color(&mut self) -> usize {
        let color = self.next_color;
        self.next_color = (self.next_color + 1) % PALETTE_SIZE;
        color
    }

    fn close_lanes(&mut self, lanes: &[usize]) -> Vec<Line> {
        lanes
            .iter()
            .filter_map(|&index| {
                self.lanes[index].take().map(|lane| Line {
                    lane: index,
                    color: lane.color,
                })
            })
            .collect()
    }

    fn open_lines(&self, node_lane: usize) -> Vec<Line> {
        self.lanes
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != node_lane)
            .filter_map(|(index, slot)| {
                slot.as_ref().map(|lane| Line {
                    lane: index,
                    color: lane.color,
                })
            })
            .collect()
    }

    fn route_parent(&mut self, parent: &str) -> Outgoing {
        if let Some(&index) = self.lanes_waiting_for(parent).first() {
            let color = self.lanes[index].as_ref().map_or(0, |lane| lane.color);
            return Outgoing {
                to_lane: index,
                color,
            };
        }
        let index = self.free_lane();
        let color = self.take_color();
        self.lanes[index] = Some(Lane {
            waiting_for: parent.to_string(),
            color,
        });
        Outgoing {
            to_lane: index,
            color,
        }
    }

    fn trim(&mut self) {
        while matches!(self.lanes.last(), Some(None)) {
            self.lanes.pop();
        }
    }
}

#[cfg(test)]
mod tests;
