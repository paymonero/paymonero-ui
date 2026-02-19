use qrcode::QrCode;
use qrcode::render::svg;

/// Generate a QR code as an inline SVG string.
/// No external services, works fully offline and in Tor.
pub fn generate_svg(data: &str) -> String {
    match QrCode::new(data.as_bytes()) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(200, 200)
            .max_dimensions(300, 300)
            .quiet_zone(true)
            .build(),
        Err(_) => String::from(
            "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='200'>\
             <text x='50%' y='50%' text-anchor='middle' fill='#666'>QR Error</text></svg>",
        ),
    }
}
