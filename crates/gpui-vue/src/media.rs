//! Native raster image and SVG elements.
//!
//! Sources may be embedded asset paths, URLs handled by the installed HTTP
//! client, filesystem paths, cached render images, or custom loaders.

pub use gpui::{
    Image, ImageSource, Img, ObjectFit, RenderImage, StyledImage, Svg, Transformation, img, svg,
};

/// Builds a raster or animated native image element.
#[must_use]
pub fn image(source: impl Into<ImageSource>) -> Img {
    img(source)
}

/// Builds an SVG element whose path is resolved through installed assets.
#[must_use]
pub fn svg_asset(path: impl Into<gpui::SharedString>) -> Svg {
    svg().path(path)
}

/// Builds an SVG element loaded from an external filesystem path.
#[must_use]
pub fn external_svg(path: impl Into<gpui::SharedString>) -> Svg {
    svg().external_path(path)
}
