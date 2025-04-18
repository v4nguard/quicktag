use eframe::{
    egui::{self, *},
    emath::Rot2,
};

pub trait UiExt {
    fn chip(&mut self, text: &str, color: Color32, text_color: Color32) -> Response;
}

impl UiExt for egui::Ui {
    fn chip(&mut self, text: &str, color: Color32, text_color: Color32) -> Response {
        self.label(
            RichText::from(format!(" {text} "))
                .background_color(color)
                .color(text_color),
        )
    }
}

pub fn format_file_size(size: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;
    const TB: usize = GB * 1024;

    if size < KB {
        format!("{} B", size)
    } else if size < MB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else if size < GB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size < TB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else {
        format!("{:.2} TB", size as f64 / TB as f64)
    }
}

pub fn ui_image_rotated(
    painter: &egui::Painter,
    texture_id: egui::TextureId,
    rect: egui::Rect,
    angle: f32,
    flip_x: bool,
) {
    let mut mesh = egui::Mesh::with_texture(texture_id);
    mesh.add_rect_with_uv(
        rect,
        egui::Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.0)),
        egui::Color32::WHITE,
    );

    mesh.rotate(Rot2::from_angle(angle.to_radians()), rect.center());

    if flip_x {
        for vertex in &mut mesh.vertices {
            vertex.uv.y = 1.0 - vertex.uv.y;
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}
