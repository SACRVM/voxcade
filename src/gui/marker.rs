//! The mark of a synthetic pointer: another session driving the IDE
//! (`ide::remote`) has no mouse the pictures could show, so while the last
//! pointer event was its own, a small arrow is drawn over the frame where
//! the pointer is. Pixels, light with a dark edge, built here.

use egui::{Color32, Context, Id, LayerId, Order, Pos2, Rect, Vec2};

/// The arrow, row by row: `#` its edge, `o` its fill.
pub const ARROW: [&str; 17] = [
    "#...........",
    "##..........",
    "#o#.........",
    "#oo#........",
    "#ooo#.......",
    "#oooo#......",
    "#ooooo#.....",
    "#oooooo#....",
    "#ooooooo#...",
    "#oooooooo#..",
    "#ooooo#####.",
    "#oo#oo#.....",
    "#o#.#oo#....",
    "##..#oo#....",
    "#....#oo#...",
    ".....#oo#...",
    "......##....",
];

/// Draws the arrow with its tip at `tip` (egui's points), above everything
/// else, a pixel of it one of the screen's.
pub fn paint(ctx: &Context, tip: Pos2) {
    let painter = ctx.layer_painter(LayerId::new(Order::Debug, Id::unique("synthetic pointer")));
    let px = 1.0 / ctx.pixels_per_point();
    // A mesh of quads, one to a pixel (the tessellator drops rectangles
    // as small as these).
    let mut mesh = egui::Mesh::default();
    for (y, row) in ARROW.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            let colour = match c {
                '#' => Color32::from_rgb(20, 18, 26),
                'o' => Color32::from_rgb(246, 240, 224),
                _ => continue,
            };
            mesh.add_colored_rect(Rect::from_min_size(tip + Vec2::new(x as f32, y as f32) * px, Vec2::splat(px)), colour);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain arrow about 12 by 18 pixels, every row as wide as the others.
    #[test]
    fn the_arrow_is_a_tidy_bitmap() {
        assert!(ARROW.iter().all(|r| r.chars().count() == 12), "rows of one width");
        assert!(ARROW.iter().all(|r| r.chars().all(|c| "#o.".contains(c))));
        assert!(ARROW[0].starts_with('#') && ARROW.iter().any(|r| r.contains('o')));
    }
}
