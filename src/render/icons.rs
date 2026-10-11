use image::RgbaImage;
use svgtypes::{SimplePathSegment, SimplifyingPathParser};
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Transform};

use crate::render::pixels::to_image;

const VIEW_BOX: f32 = 24.0;
const ICON_HEIGHT_SHARE: f32 = 0.95;
const LAPTOP: &str = "M4,6H20V16H4M20,18A2,2 0 0,0 22,16V6C22,4.89 21.1,4 20,4H4C2.89,4 2,4.89 2,6V16A2,2 0 0,0 4,18H0V20H24V18H20Z";
const GITHUB: &str = "M12,2A10,10 0 0,0 2,12C2,16.42 4.87,20.17 8.84,21.5C9.34,21.58 9.5,21.27 9.5,21C9.5,20.77 9.5,20.14 9.5,19.31C6.73,19.91 6.14,17.97 6.14,17.97C5.68,16.81 5.03,16.5 5.03,16.5C4.12,15.88 5.1,15.9 5.1,15.9C6.1,15.97 6.63,16.93 6.63,16.93C7.5,18.45 8.97,18 9.54,17.76C9.63,17.11 9.89,16.67 10.17,16.42C7.95,16.17 5.62,15.31 5.62,11.5C5.62,10.39 6,9.5 6.65,8.79C6.55,8.54 6.2,7.5 6.75,6.15C6.75,6.15 7.59,5.88 9.5,7.17C10.29,6.95 11.15,6.84 12,6.84C12.85,6.84 13.71,6.95 14.5,7.17C16.41,5.88 17.25,6.15 17.25,6.15C17.8,7.5 17.45,8.54 17.35,8.79C18,9.5 18.38,10.39 18.38,11.5C18.38,15.32 16.04,16.16 13.81,16.41C14.17,16.72 14.5,17.33 14.5,18.26C14.5,19.6 14.5,20.68 14.5,21C14.5,21.27 14.66,21.59 15.17,21.5C19.14,20.16 22,16.42 22,12A10,10 0 0,0 12,2Z";
const GITLAB: &str =
    "M12,21.4L2.2,13.9L3.3,10.4L5.8,2.6L8.4,10.4H15.6L18.2,2.6L20.7,10.4L21.8,13.9Z";
const GIT: &str = "M12,1.5L22.5,12L12,22.5L1.5,12Z";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Logo {
    Laptop,
    Github,
    Gitlab,
    Git,
}

impl Logo {
    fn path_data(self) -> &'static str {
        match self {
            Logo::Laptop => LAPTOP,
            Logo::Github => GITHUB,
            Logo::Gitlab => GITLAB,
            Logo::Git => GIT,
        }
    }
}

pub fn draw_logo(
    logo: Logo,
    width: u32,
    height: u32,
    background: (u8, u8, u8),
    ink: (u8, u8, u8),
) -> RgbaImage {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1)).expect("logo pixmap");
    pixmap.fill(opaque(background));
    let size = height as f32 * ICON_HEIGHT_SHARE;
    let scale = size / VIEW_BOX;
    let transform = Transform::from_scale(scale, scale)
        .post_translate((width as f32 - size) / 2.0, (height as f32 - size) / 2.0);
    if let Some(path) = parse_path(logo.path_data()) {
        let mut paint = Paint::default();
        paint.set_color(opaque(ink));
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::EvenOdd, transform, None);
    }
    to_image(&pixmap)
}

fn opaque((red, green, blue): (u8, u8, u8)) -> Color {
    Color::from_rgba8(red, green, blue, 255)
}

fn parse_path(data: &str) -> Option<Path> {
    let builder = SimplifyingPathParser::from(data).try_fold(
        PathBuilder::new(),
        |mut builder, segment| {
            match segment.ok()? {
                SimplePathSegment::MoveTo { x, y } => builder.move_to(x as f32, y as f32),
                SimplePathSegment::LineTo { x, y } => builder.line_to(x as f32, y as f32),
                SimplePathSegment::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => builder.cubic_to(
                    x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32,
                ),
                SimplePathSegment::Quadratic { x1, y1, x, y } => {
                    builder.quad_to(x1 as f32, y1 as f32, x as f32, y as f32)
                }
                SimplePathSegment::ClosePath => builder.close(),
            }
            Some(builder)
        },
    )?;
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKGROUND: (u8, u8, u8) = (86, 182, 230);
    const INK: (u8, u8, u8) = (16, 18, 26);

    fn ink_pixels(image: &RgbaImage) -> usize {
        image
            .pixels()
            .filter(|pixel| pixel.0 == [INK.0, INK.1, INK.2, 255])
            .count()
    }

    #[test]
    fn every_logo_path_parses() {
        for logo in [Logo::Laptop, Logo::Github, Logo::Gitlab, Logo::Git] {
            assert!(parse_path(logo.path_data()).is_some(), "{logo:?}");
        }
    }

    #[test]
    fn logo_image_has_the_requested_size() {
        let image = draw_logo(Logo::Github, 36, 40, BACKGROUND, INK);
        assert_eq!(image.dimensions(), (36, 40));
    }

    #[test]
    fn logo_corners_keep_the_label_background() {
        let image = draw_logo(Logo::Laptop, 36, 40, BACKGROUND, INK);
        assert_eq!(image.get_pixel(0, 0).0, [86, 182, 230, 255]);
    }

    #[test]
    fn github_logo_is_drawn_in_ink() {
        let image = draw_logo(Logo::Github, 36, 40, BACKGROUND, INK);
        assert!(ink_pixels(&image) > 300, "{}", ink_pixels(&image));
    }

    #[test]
    fn laptop_screen_is_hollow() {
        let image = draw_logo(Logo::Laptop, 36, 40, BACKGROUND, INK);
        assert_eq!(image.get_pixel(18, 17).0, [86, 182, 230, 255]);
    }
}
