use image::RgbaImage;
use tiny_skia::{
    Color, FillRule, FilterQuality, IntSize, Paint, Path, PathBuilder, Pattern, Pixmap, SpreadMode,
    Stroke, Transform,
};

use crate::graph::Row;
use crate::render::palette::rgb;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub cell_w: f32,
    pub cell_h: f32,
    pub lane_cells: usize,
}

const NODE_HEIGHT_SHARE: f32 = 0.55;
const NODE_GAP: f32 = 4.0;

impl Geometry {
    pub fn for_cell(cell_w: f32, cell_h: f32) -> Geometry {
        let lane_cells = ((cell_h * NODE_HEIGHT_SHARE + NODE_GAP) / cell_w)
            .ceil()
            .max(2.0) as usize;
        Geometry {
            cell_w,
            cell_h,
            lane_cells,
        }
    }

    pub fn lane_x(&self, lane: usize) -> f32 {
        let lane_width = self.lane_cells as f32 * self.cell_w;
        lane as f32 * lane_width + lane_width / 2.0
    }

    pub fn row_center(&self, row: usize) -> f32 {
        (row as f32 + 0.5) * self.cell_h
    }

    pub fn node_radius(&self) -> f32 {
        (self.cell_h * NODE_HEIGHT_SHARE / 2.0)
            .min(self.lane_cells as f32 * self.cell_w / 2.0 - NODE_GAP / 2.0)
    }

    fn line_width(&self) -> f32 {
        (self.cell_w / 6.0).max(2.0)
    }

    fn bend(&self) -> f32 {
        self.cell_w.min(self.cell_h / 2.0)
    }
}

pub struct NodeLook<'a> {
    pub avatar: Option<&'a RgbaImage>,
    pub author_color: usize,
}

pub fn visible_lanes(rows: &[Row], max_lanes: usize) -> usize {
    rows.iter()
        .map(|row| row.width)
        .max()
        .unwrap_or(1)
        .clamp(1, max_lanes.max(1))
}

#[derive(Clone, Copy)]
struct RowSpan {
    top: f32,
    center: f32,
    bottom: f32,
}

fn row_spans(count: usize, geometry: Geometry, heights: &[f32]) -> Vec<RowSpan> {
    let mut top = 0.0;
    (0..count)
        .map(|index| {
            let height = heights.get(index).copied().unwrap_or(geometry.cell_h);
            let span = RowSpan {
                top,
                center: top + geometry.cell_h / 2.0,
                bottom: top + height,
            };
            top += height;
            span
        })
        .collect()
}

pub fn draw_window(
    rows: &[Row],
    nodes: &[NodeLook],
    lanes: usize,
    geometry: Geometry,
    heights: &[f32],
) -> RgbaImage {
    let width = (lanes as f32 * geometry.lane_cells as f32 * geometry.cell_w)
        .round()
        .max(1.0) as u32;
    let spans = row_spans(rows.len(), geometry, heights);
    let height = spans
        .last()
        .map_or(geometry.cell_h, |span| span.bottom)
        .round()
        .max(1.0) as u32;
    let mut pixmap = Pixmap::new(width, height).expect("graph pixmap");
    for (row, span) in rows.iter().zip(&spans) {
        draw_lines(&mut pixmap, row, *span, geometry);
    }
    for (index, (row, span)) in rows.iter().zip(&spans).enumerate() {
        draw_node(&mut pixmap, row, nodes.get(index), span.center, geometry);
    }
    to_image(&pixmap)
}

fn paint(color: usize) -> Paint<'static> {
    let (red, green, blue) = rgb(color);
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(red, green, blue, 255));
    paint.anti_alias = true;
    paint
}

fn stroke(pixmap: &mut Pixmap, path: Option<Path>, color: usize, geometry: Geometry) {
    let Some(path) = path else {
        return;
    };
    let stroke = Stroke {
        width: geometry.line_width(),
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint(color), &stroke, Transform::identity(), None);
}

fn vertical(x: f32, from: f32, to: f32) -> Option<Path> {
    let mut builder = PathBuilder::new();
    builder.move_to(x, from);
    builder.line_to(x, to);
    builder.finish()
}

fn draw_lines(pixmap: &mut Pixmap, row: &Row, span: RowSpan, geometry: Geometry) {
    let RowSpan {
        top,
        center,
        bottom,
    } = span;
    let node_x = geometry.lane_x(row.lane);
    for line in &row.pass_through {
        stroke(
            pixmap,
            vertical(geometry.lane_x(line.lane), top, bottom),
            line.color,
            geometry,
        );
    }
    if row.from_above {
        stroke(pixmap, vertical(node_x, top, center), row.color, geometry);
    }
    if row.continues {
        stroke(
            pixmap,
            vertical(node_x, center, bottom),
            row.color,
            geometry,
        );
    }
    let bend = geometry.bend();
    for line in &row.incoming {
        let lane_x = geometry.lane_x(line.lane);
        let toward_node = if lane_x > node_x { -1.0 } else { 1.0 };
        let mut builder = PathBuilder::new();
        builder.move_to(lane_x, top);
        builder.line_to(lane_x, center - bend);
        builder.quad_to(lane_x, center, lane_x + toward_node * bend, center);
        builder.line_to(node_x, center);
        stroke(pixmap, builder.finish(), line.color, geometry);
    }
    for line in &row.outgoing {
        let lane_x = geometry.lane_x(line.to_lane);
        let toward_node = if lane_x > node_x { -1.0 } else { 1.0 };
        let mut builder = PathBuilder::new();
        builder.move_to(node_x, center);
        builder.line_to(lane_x + toward_node * bend, center);
        builder.quad_to(lane_x, center, lane_x, center + bend);
        builder.line_to(lane_x, bottom);
        stroke(pixmap, builder.finish(), line.color, geometry);
    }
}

fn circle(x: f32, y: f32, radius: f32) -> Option<Path> {
    PathBuilder::from_circle(x, y, radius)
}

fn draw_node(pixmap: &mut Pixmap, row: &Row, look: Option<&NodeLook>, y: f32, geometry: Geometry) {
    let x = geometry.lane_x(row.lane);
    let radius = geometry.node_radius();
    if row.merge {
        fill(pixmap, circle(x, y, radius * 0.35), &paint(row.color));
        return;
    }
    fill(pixmap, circle(x, y, radius), &paint(row.color));
    let inner = radius - geometry.line_width();
    let avatar = look.and_then(|look| look.avatar.and_then(avatar_pixmap));
    match avatar {
        Some(avatar) => {
            let scale = 2.0 * inner / avatar.width() as f32;
            let transform = Transform::from_row(scale, 0.0, 0.0, scale, x - inner, y - inner);
            let avatar_paint = Paint {
                anti_alias: true,
                shader: Pattern::new(
                    avatar.as_ref(),
                    SpreadMode::Pad,
                    FilterQuality::Bicubic,
                    1.0,
                    transform,
                ),
                ..Paint::default()
            };
            fill(pixmap, circle(x, y, inner), &avatar_paint);
        }
        None => fill(
            pixmap,
            circle(x, y, inner),
            &paint(look.map_or(row.color, |look| look.author_color)),
        ),
    }
}

fn fill(pixmap: &mut Pixmap, path: Option<Path>, paint: &Paint) {
    if let Some(path) = path {
        pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn avatar_pixmap(avatar: &RgbaImage) -> Option<Pixmap> {
    let premultiplied = avatar
        .pixels()
        .flat_map(|pixel| {
            let [red, green, blue, alpha] = pixel.0;
            let scale = |channel: u8| ((u16::from(channel) * u16::from(alpha) + 127) / 255) as u8;
            [scale(red), scale(green), scale(blue), alpha]
        })
        .collect();
    Pixmap::from_vec(
        premultiplied,
        IntSize::from_wh(avatar.width(), avatar.height())?,
    )
}

pub(crate) fn to_image(pixmap: &Pixmap) -> RgbaImage {
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    RgbaImage::from_raw(pixmap.width(), pixmap.height(), rgba).expect("graph image")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Layout;
    use crate::render::palette::rgb;

    const GEOMETRY: Geometry = Geometry {
        cell_w: 18.0,
        cell_h: 40.0,
        lane_cells: 2,
    };

    fn rows(history: &[(&str, &[&str])]) -> Vec<Row> {
        let mut layout = Layout::new();
        history
            .iter()
            .map(|(hash, parents)| {
                layout.push(
                    hash,
                    &parents
                        .iter()
                        .map(|parent| parent.to_string())
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    fn plain_nodes(count: usize) -> Vec<NodeLook<'static>> {
        (0..count)
            .map(|_| NodeLook {
                avatar: None,
                author_color: 4,
            })
            .collect()
    }

    fn pixel(image: &RgbaImage, x: f32, y: f32) -> [u8; 4] {
        image.get_pixel(x as u32, y as u32).0
    }

    #[test]
    fn geometry_centers_lanes_on_two_cell_columns() {
        assert_eq!(GEOMETRY.lane_x(0), 18.0);
        assert_eq!(GEOMETRY.lane_x(1), 54.0);
        assert_eq!(GEOMETRY.row_center(2), 100.0);
        assert!((GEOMETRY.node_radius() - 11.0).abs() < 0.01);
    }

    #[test]
    fn nodes_are_sized_from_the_row_height_and_lanes_widen_to_fit_them() {
        let geometry = Geometry::for_cell(18.0, 80.0);
        assert_eq!(geometry.lane_cells, 3);
        assert!((geometry.node_radius() - 22.0).abs() < 0.01);
        assert_eq!(geometry.lane_x(1), 81.0);
    }

    #[test]
    fn visible_lanes_follow_the_widest_row_up_to_the_cap() {
        let history = rows(&[
            ("O", &["A", "B", "C"]),
            ("A", &["R"]),
            ("B", &["R"]),
            ("C", &["R"]),
            ("R", &[]),
        ]);
        assert_eq!(visible_lanes(&history, 10), 3);
        assert_eq!(visible_lanes(&history, 2), 2);
    }

    #[test]
    fn image_covers_the_lanes_and_rows() {
        let history = rows(&[("B", &["A"]), ("A", &[])]);
        let image = draw_window(&history, &plain_nodes(2), 2, GEOMETRY, &[]);
        assert_eq!((image.width(), image.height()), (72, 80));
    }

    #[test]
    fn pass_through_lane_is_painted_in_its_color_and_the_gap_stays_transparent() {
        let history = rows(&[("X", &["A"]), ("Y", &["A"]), ("A", &[])]);
        let image = draw_window(&history, &plain_nodes(3), 2, GEOMETRY, &[]);
        let (red, green, blue) = rgb(history[0].color);
        assert_eq!(
            pixel(&image, GEOMETRY.lane_x(0), GEOMETRY.row_center(1)),
            [red, green, blue, 255]
        );
        assert_eq!(pixel(&image, 36.0, GEOMETRY.row_center(1) - 15.0)[3], 0);
    }

    #[test]
    fn merge_node_is_a_small_dot_and_commit_node_a_full_circle() {
        let history = rows(&[("M", &["B", "F"]), ("F", &["A"]), ("B", &["A"]), ("A", &[])]);
        let image = draw_window(&history, &plain_nodes(4), 2, GEOMETRY, &[]);
        let offset = GEOMETRY.node_radius() * 0.7;
        assert_eq!(
            pixel(&image, GEOMETRY.lane_x(0) - offset, GEOMETRY.row_center(0))[3],
            0
        );
        assert_eq!(
            pixel(&image, GEOMETRY.lane_x(1) - offset, GEOMETRY.row_center(1))[3],
            255
        );
    }

    #[test]
    fn taller_rows_stretch_their_lines_and_push_the_next_nodes_down() {
        let history = rows(&[("X", &["A"]), ("Y", &["A"]), ("A", &[])]);
        let heights = [40.0, 80.0, 40.0];
        let image = draw_window(&history, &plain_nodes(3), 2, GEOMETRY, &heights);
        assert_eq!(image.height(), 160);
        let (red, green, blue) = rgb(history[0].color);
        assert_eq!(
            pixel(&image, GEOMETRY.lane_x(0), 40.0 + 75.0),
            [red, green, blue, 255]
        );
        assert_eq!(pixel(&image, GEOMETRY.lane_x(0), 120.0 + 20.0)[3], 255);
        assert_eq!(pixel(&image, GEOMETRY.lane_x(1), 40.0 + 2.0)[3], 0);
        assert_eq!(pixel(&image, GEOMETRY.lane_x(1), 40.0 + 75.0)[3], 255);
    }

    #[test]
    fn avatar_fills_the_inside_of_the_node() {
        let history = rows(&[("A", &[])]);
        let avatar = RgbaImage::from_pixel(64, 64, image::Rgba([250, 10, 10, 255]));
        let nodes = vec![NodeLook {
            avatar: Some(&avatar),
            author_color: 0,
        }];
        let image = draw_window(&history, &nodes, 1, GEOMETRY, &[]);
        assert_eq!(
            pixel(&image, GEOMETRY.lane_x(0), GEOMETRY.row_center(0)),
            [250, 10, 10, 255]
        );
    }
}
