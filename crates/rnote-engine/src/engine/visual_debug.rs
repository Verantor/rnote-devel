// Imports
use crate::engine::DeskewDebugData;
#[cfg(feature = "ui")]
use p2d::math::Vector2;
use rnote_compose::Color;
pub const COLOR_POS: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
pub const COLOR_STROKE_HITBOX: Color = Color {
    r: 0.0,
    g: 0.8,
    b: 0.2,
    a: 0.5,
};
pub const COLOR_STROKE_BOUNDS: Color = Color {
    r: 0.0,
    g: 0.8,
    b: 0.8,
    a: 1.0,
};
pub const COLOR_IMAGE_BOUNDS: Color = Color {
    r: 0.0,
    g: 0.5,
    b: 1.0,
    a: 1.0,
};
pub const COLOR_STROKE_RENDERING_DIRTY: Color = Color {
    r: 0.9,
    g: 0.0,
    b: 0.8,
    a: 0.10,
};
pub const COLOR_STROKE_RENDERING_BUSY: Color = Color {
    r: 0.0,
    g: 0.8,
    b: 1.0,
    a: 0.10,
};
pub const COLOR_SELECTOR_BOUNDS: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.8,
    a: 1.0,
};
pub const COLOR_DOC_BOUNDS: Color = Color {
    r: 0.8,
    g: 0.0,
    b: 0.8,
    a: 1.0,
};

#[cfg(feature = "ui")]
pub(crate) fn draw_bounds_to_gtk_snapshot(
    bounds: p2d::bounding_volume::Aabb,
    color: Color,
    snapshot: &gtk4::Snapshot,
    width: f64,
) {
    use crate::ext::GdkRGBAExt;
    use gtk4::{gdk, graphene, gsk, prelude::*};

    let bounds = graphene::Rect::new(
        bounds.mins[0] as f32,
        bounds.mins[1] as f32,
        (bounds.extents()[0]) as f32,
        (bounds.extents()[1]) as f32,
    );

    let rounded_rect = gsk::RoundedRect::new(
        bounds,
        graphene::Size::zero(),
        graphene::Size::zero(),
        graphene::Size::zero(),
        graphene::Size::zero(),
    );

    snapshot.append_border(
        &rounded_rect,
        &[width as f32, width as f32, width as f32, width as f32],
        &[
            gdk::RGBA::from_compose_color(color),
            gdk::RGBA::from_compose_color(color),
            gdk::RGBA::from_compose_color(color),
            gdk::RGBA::from_compose_color(color),
        ],
    )
}

#[cfg(feature = "ui")]
pub(crate) fn draw_pos_to_gtk_snapshot(
    snapshot: &gtk4::Snapshot,
    pos: Vector2,
    color: Color,
    width: f64,
) {
    use crate::ext::GdkRGBAExt;
    use gtk4::{gdk, graphene, prelude::*};

    snapshot.append_color(
        &gdk::RGBA::from_compose_color(color),
        &graphene::Rect::new(
            (pos[0] - 0.5 * width) as f32,
            (pos[1] - 0.5 * width) as f32,
            width as f32,
            width as f32,
        ),
    );
}

#[cfg(feature = "ui")]
pub(crate) fn draw_fill_to_gtk_snapshot(
    snapshot: &gtk4::Snapshot,
    rect: p2d::bounding_volume::Aabb,
    color: Color,
) {
    use crate::ext::{GdkRGBAExt, GrapheneRectExt};
    use gtk4::{gdk, graphene, prelude::*};

    snapshot.append_color(
        &gdk::RGBA::from_compose_color(color),
        &graphene::Rect::from_p2d_aabb(rect),
    );
}

/// Draw some engine statistics for debugging purposes.
///
/// Expects that the snapshot is untransformed in surface coordinate space.
#[cfg(feature = "ui")]
pub(crate) fn draw_statistics_to_gtk_snapshot(
    snapshot: &gtk4::Snapshot,
    engine: &crate::Engine,
    surface_bounds: p2d::bounding_volume::Aabb,
) -> anyhow::Result<()> {
    use crate::ext::GrapheneRectExt;
    use gtk4::{graphene, prelude::*};
    use p2d::bounding_volume::Aabb;
    use piet::{RenderContext, Text, TextLayoutBuilder};
    use rnote_compose::ext::{AabbExt, Vector2Ext};

    // A statistics overlay
    {
        let text_bounds = Aabb::new(
            Vector2::new(
                surface_bounds.maxs[0] - 320.0,
                surface_bounds.mins[1] + 20.0,
            ),
            Vector2::new(
                surface_bounds.maxs[0] - 20.0,
                surface_bounds.mins[1] + 120.0,
            ),
        );
        let cairo_cx = snapshot.append_cairo(&graphene::Rect::from_p2d_aabb(text_bounds));
        let mut piet_cx = piet_cairo::CairoRenderContext::new(&cairo_cx);

        // Gather statistics
        let strokes_total = engine.store.keys_unordered();
        let strokes_in_viewport = engine
            .store
            .keys_unordered_intersecting_bounds(engine.camera.viewport());
        let selected_strokes = engine.store.selection_keys_unordered();
        let trashed_strokes = engine.store.trashed_keys_unordered();
        let strokes_hold_image = strokes_total
            .iter()
            .filter(|&&key| engine.store.holds_images(key))
            .count();

        let statistics_text_string = format!(
            "strokes in store:   {}\nstrokes in current viewport:   {}\nstrokes selected: {}\nstroke trashed: {}\nstrokes holding images: {}",
            strokes_total.len(),
            strokes_in_viewport.len(),
            selected_strokes.len(),
            trashed_strokes.len(),
            strokes_hold_image,
        );
        let text_layout = piet_cx
            .text()
            .new_text_layout(statistics_text_string)
            .text_color(piet::Color::rgba(0.8, 1.0, 1.0, 1.0))
            .max_width(text_bounds.extents()[0] - 20.0)
            .alignment(piet::TextAlignment::End)
            .font(piet::FontFamily::MONOSPACE, 10.0)
            .build()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

        piet_cx.fill(
            text_bounds.to_kurbo_rect(),
            &piet::Color::rgba(0.1, 0.1, 0.1, 0.8),
        );
        piet_cx.draw_text(
            &text_layout,
            (text_bounds.mins + Vector2::splat(10.0)).to_kurbo_point(),
        );
        piet_cx.finish().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    Ok(())
}

#[cfg(feature = "ui")]
pub(crate) fn draw_recognition_text_to_gtk_snapshot(
    snapshot: &gtk4::Snapshot,
    engine: &crate::Engine,
    surface_bounds: p2d::bounding_volume::Aabb,
) -> anyhow::Result<()> {
    if let Some(text) = &engine.recognition_debug_text {
        use crate::ext::GrapheneRectExt;
        use gtk4::{graphene, prelude::*};
        use p2d::bounding_volume::Aabb;
        use piet::{RenderContext, Text, TextLayout, TextLayoutBuilder};
        use rnote_compose::ext::{AabbExt, Vector2Ext};

        // 20 px padding from statistics
        let start_y = surface_bounds.mins[1] + 140.0;

        let max_cairo_bounds = Aabb::new(
            Vector2::new(surface_bounds.maxs[0] - 320.0, start_y),
            Vector2::new(surface_bounds.maxs[0] - 20.0, start_y + 800.0),
        );

        let cairo_cx = snapshot.append_cairo(&graphene::Rect::from_p2d_aabb(max_cairo_bounds));
        let mut piet_cx = piet_cairo::CairoRenderContext::new(&cairo_cx);

        let display_string = format!("recognized text:\n{}", text);
        let max_text_width = 280.0;

        let text_layout = piet_cx
            .text()
            .new_text_layout(display_string)
            .text_color(piet::Color::rgba(0.8, 1.0, 1.0, 1.0))
            .max_width(max_text_width)
            .alignment(piet::TextAlignment::End)
            .font(piet::FontFamily::MONOSPACE, 10.0)
            .build()
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;

        let text_size = text_layout.size();
        let padding = 10.0;

        let box_bounds = Aabb::new(
            Vector2::new(surface_bounds.maxs[0] - 320.0, start_y),
            Vector2::new(
                surface_bounds.maxs[0] - 20.0,
                start_y + text_size.height + (padding * 2.0),
            ),
        );

        piet_cx.fill(
            box_bounds.to_kurbo_rect(),
            &piet::Color::rgba(0.1, 0.1, 0.1, 0.8),
        );

        piet_cx.draw_text(
            &text_layout,
            (box_bounds.mins + Vector2::splat(padding)).to_kurbo_point(),
        );

        piet_cx.finish().map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    Ok(())
}

/// Draw stroke bounds, positions, etc. for visual debugging purposes.
#[cfg(feature = "ui")]
pub(crate) fn draw_stroke_debug_to_gtk_snapshot(
    snapshot: &gtk4::Snapshot,
    engine: &crate::Engine,
    surface_bounds: p2d::bounding_volume::Aabb,
) -> anyhow::Result<()> {
    use crate::drawable::DrawableOnDoc;
    use crate::engine_view;
    use p2d::bounding_volume::BoundingVolume;

    let viewport = engine.camera.viewport();
    let total_zoom = engine.camera.total_zoom();
    let doc_bounds = engine.document.bounds();
    let border_widths = 1.0 / total_zoom;

    draw_bounds_to_gtk_snapshot(doc_bounds, COLOR_DOC_BOUNDS, snapshot, border_widths);

    let tightened_viewport = viewport.tightened(2.0 / total_zoom);
    draw_bounds_to_gtk_snapshot(
        tightened_viewport,
        COLOR_STROKE_BOUNDS,
        snapshot,
        border_widths,
    );
    if !engine.deskew_debug_data.is_empty() {
        draw_deskew_debug_to_gtk_snapshot(snapshot, &engine.deskew_debug_data, total_zoom);
    }

    // Draw the strokes
    engine
        .store
        .draw_debug_to_gtk_snapshot(snapshot, engine, surface_bounds)?;

    // Draw the current pen bounds
    if let Some(bounds) = engine.penholder.bounds_on_doc(&engine_view!(engine)) {
        draw_bounds_to_gtk_snapshot(bounds, COLOR_SELECTOR_BOUNDS, snapshot, border_widths);
    }

    Ok(())
}

#[cfg(feature = "ui")]
pub(crate) fn draw_deskew_debug_to_gtk_snapshot(
    snapshot: &gtk4::Snapshot,
    debug_data_list: &[DeskewDebugData],
    total_zoom: f64,
) {
    use gtk4::{graphene, prelude::*};

    let border_widths = 1.0 / total_zoom;
    let point_size = 5.0 / total_zoom;

    for data in debug_data_list {
        snapshot.save();

        snapshot.translate(&graphene::Point::new(
            data.center.x as f32,
            data.center.y as f32,
        ));
        snapshot.rotate(data.angle_rad.to_degrees() as f32);
        snapshot.translate(&graphene::Point::new(
            -data.center.x as f32,
            -data.center.y as f32,
        ));

        draw_bounds_to_gtk_snapshot(
            data.aabb_deskewed,
            COLOR_STROKE_BOUNDS,
            snapshot,
            border_widths,
        );

        snapshot.restore();

        draw_pos_to_gtk_snapshot(snapshot, data.center, COLOR_SELECTOR_BOUNDS, point_size);
    }
}
