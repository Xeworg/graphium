//! The PNG rasterizer (PRD §7 RF10, §8.5): `resvg` behind the `png` feature.
//!
//! `gx-core` owns the seam (`gx_core::export::Rasterizer`) and the limits; this
//! module is the only place in the product where an SVG becomes pixels.
//!
//! Two rules it follows:
//!
//! - the fonts are the **embedded** Inter faces, never the ones of the machine
//!   (feature doc `f2-export.md`, L6): the PNG has to look like the SVG, and
//!   the SVG measured its text with those faces;
//! - the pixel ceiling is checked **before** the pixmap exists: a ceiling that
//!   is checked after allocating is not a ceiling (RF10-bis).
//!
//! The PNG is outside the deterministic pipeline (PRD §8.5.1, S10): its bytes
//! depend on the version of `resvg`, so nothing compares them by hash.

use std::sync::Arc;

use gx_core::export::{
    RasterError, RasterImage, RasterOptions, Rasterizer, PNG_MAX_MAX_PX, PNG_MAX_SCALE,
};
use resvg::tiny_skia;
use resvg::usvg::{self, fontdb};

use crate::metrics::{INTER_REGULAR, INTER_SEMIBOLD};

/// The only rasterizer of Graphium: our SVG in, PNG bytes out.
pub struct ResvgRasterizer {
    fonts: Arc<fontdb::Database>,
}

impl ResvgRasterizer {
    /// Builds the rasterizer around the embedded Inter faces.
    pub fn new() -> Self {
        let mut fonts = fontdb::Database::new();
        fonts.load_font_data(INTER_REGULAR.to_vec());
        fonts.load_font_data(INTER_SEMIBOLD.to_vec());
        Self {
            fonts: Arc::new(fonts),
        }
    }
}

impl Default for ResvgRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Rasterizer for ResvgRasterizer {
    fn rasterize(&self, svg: &str, options: &RasterOptions) -> Result<RasterImage, RasterError> {
        // Los techos se aplican acá también, no solo donde llegan los `opts`:
        // quien construya `RasterOptions` a mano no puede saltearlos.
        if !options.scale.is_finite() || options.scale <= 0.0 || options.scale > PNG_MAX_SCALE {
            return Err(RasterError::Invalid(format!(
                "escala {scale}: tiene que ser mayor que 0 y no pasar {PNG_MAX_SCALE}",
                scale = options.scale
            )));
        }
        if options.max_px == 0 || options.max_px > PNG_MAX_MAX_PX {
            return Err(RasterError::Invalid(format!(
                "max_px {max_px}: tiene que estar entre 1 y {PNG_MAX_MAX_PX}",
                max_px = options.max_px
            )));
        }

        let tree_options = usvg::Options {
            fontdb: Arc::clone(&self.fonts),
            ..usvg::Options::default()
        };
        let tree = usvg::Tree::from_str(svg, &tree_options)
            .map_err(|error| RasterError::Invalid(format!("el SVG no parsea: {error}")))?;

        let size = tree.size();
        let width = f64::from(size.width()) * options.scale;
        let height = f64::from(size.height()) * options.scale;
        if !width.is_finite() || !height.is_finite() || width < 1.0 || height < 1.0 {
            return Err(RasterError::Invalid(format!(
                "el dibujo mide {width}×{height} px: no hay nada que rasterizar"
            )));
        }
        if width > f64::from(u32::MAX) || height > f64::from(u32::MAX) {
            return Err(RasterError::TooLarge {
                width: u32::MAX,
                height: u32::MAX,
                pixels: u64::MAX,
            });
        }

        let width = width.ceil() as u32;
        let height = height.ceil() as u32;
        let pixels = u64::from(width) * u64::from(height);
        if pixels > options.max_px {
            return Err(RasterError::TooLarge {
                width,
                height,
                pixels,
            });
        }

        let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
            RasterError::Invalid(format!("no se pudo reservar un bitmap de {width}×{height}"))
        })?;
        let scale = options.scale as f32;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let png = pixmap.encode_png().map_err(|error| {
            RasterError::Invalid(format!("el PNG no se pudo codificar: {error}"))
        })?;

        Ok(RasterImage { png, width, height })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gx_core::export::{RasterError, RasterOptions, Rasterizer};
    use gx_core::Palette;
    use resvg::tiny_skia;

    use super::ResvgRasterizer;
    use crate::metrics::InterMetrics;
    use crate::test_support as fixture;
    use crate::SvgRenderer;

    /// The exported SVG of the fixture diagram, painted with the plan palette.
    fn exported_svg() -> String {
        let metrics = InterMetrics::new().expect("the embedded Inter faces parse");
        SvgRenderer::new(Arc::new(metrics)).render_export(
            &fixture::fixture_ir(),
            &fixture::fixture_layout(),
            &Palette::plano(),
        )
    }

    fn rasterizer() -> ResvgRasterizer {
        ResvgRasterizer::new()
    }

    fn options(scale: f64, max_px: u64) -> RasterOptions {
        RasterOptions { scale, max_px }
    }

    fn decode(image: &[u8]) -> tiny_skia::Pixmap {
        tiny_skia::Pixmap::decode_png(image).expect("the PNG decodes")
    }

    /// S7 (PRD §8.5.2): el PNG sale con la paleta del export, fondo incluido.
    /// Un PNG con fondo transparente y texto claro desaparece en una página
    /// blanca, que es justo donde el humano lo va a pegar.
    #[test]
    fn the_png_carries_the_palette_of_the_export() {
        let image = rasterizer()
            .rasterize(&exported_svg(), &RasterOptions::default())
            .expect("the drawing rasterizes");
        assert!(&image.png[..8] == b"\x89PNG\r\n\x1a\n", "PNG signature");

        let pixmap = decode(&image.png);
        let corner = pixmap.pixel(0, 0).expect("the corner exists");
        let bg = Palette::plano().bg;
        assert_eq!(
            (corner.red(), corner.green(), corner.blue()),
            (bg.r, bg.g, bg.b)
        );
    }

    /// S2 (RF10-bis): la escala del PNG agranda el bitmap, no lo reescala.
    #[test]
    fn the_scale_grows_the_png() {
        let svg = exported_svg();
        let single = rasterizer()
            .rasterize(&svg, &options(1.0, 64_000_000))
            .expect("scale 1");
        let double = rasterizer()
            .rasterize(&svg, &options(2.0, 64_000_000))
            .expect("scale 2");

        assert_eq!(double.width, single.width * 2);
        assert_eq!(double.height, single.height * 2);
        let pixmap = decode(&double.png);
        assert_eq!(
            (pixmap.width(), pixmap.height()),
            (double.width, double.height)
        );
    }

    /// S2: el techo de píxeles se cobra con las dimensiones calculadas, no con
    /// un bitmap que ya se reservó.
    #[test]
    fn the_pixel_ceiling_is_refused_with_its_dimensions() {
        let error = rasterizer()
            .rasterize(&exported_svg(), &options(3.0, 1_000))
            .expect_err("over the ceiling");

        let RasterError::TooLarge {
            width,
            height,
            pixels,
        } = error
        else {
            panic!("expected too_large, got a different error");
        };
        assert_eq!(pixels, u64::from(width) * u64::from(height));
        assert!(pixels > 1_000);
        assert_eq!(error.code(), "too_large");
        assert!(error.message().contains(&format!("{width}×{height}")));
    }

    /// S2: la escala tiene techo (3.0) y piso (mayor que cero).
    #[test]
    fn a_scale_out_of_range_is_refused() {
        for scale in [0.0, -1.0, 3.5, f64::NAN] {
            let error = rasterizer()
                .rasterize(&exported_svg(), &options(scale, 64_000_000))
                .expect_err("the scale is out of range");
            assert_eq!(error.code(), "invalid_value");
        }
    }

    #[test]
    fn an_svg_that_does_not_parse_is_refused() {
        let error = rasterizer()
            .rasterize("esto no es un svg", &RasterOptions::default())
            .expect_err("not an svg");
        assert_eq!(error.code(), "invalid_value");
        assert!(error.message().contains("parsea"));
    }

    /// L6: el texto del PNG se dibuja con la Inter embebida. Sin fuente no hay
    /// texto: el rasterizador borraría las etiquetas y el PNG no se parecería
    /// al SVG. El documento tiene tinta **solo** en el texto, así que un solo
    /// píxel distinto del fondo prueba que la fuente está.
    #[test]
    fn the_text_is_drawn_with_the_embedded_inter() {
        let svg = concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="24">"##,
            r##"<rect x="0" y="0" width="60" height="24" fill="#1b1b1b"/>"##,
            r##"<text x="2" y="18" font-family="Inter" font-size="14" fill="#e9e9ec">Hola</text>"##,
            "</svg>"
        );
        let image = rasterizer()
            .rasterize(svg, &RasterOptions::default())
            .expect("the svg rasterizes");
        let pixmap = decode(&image.png);

        let ink = pixmap
            .pixels()
            .iter()
            .filter(|pixel| (pixel.red(), pixel.green(), pixel.blue()) != (0x1b, 0x1b, 0x1b));
        assert!(ink.count() > 0, "the text left no ink: the font is missing");
    }
}
