use arborium::theme::builtin;
use axum::http::header;
use axum::response::IntoResponse;

/// Serves arborium syntax highlighting CSS for both light and dark modes,
/// scoped via CSS `@media (prefers-color-scheme: ...)`.
pub async fn api_arborium_theme_css() -> impl IntoResponse {
    let light = builtin::github_light().to_css(".arborium-hl");
    let dark = builtin::catppuccin_mocha().to_css(".arborium-hl");

    let css = format!(
        "@media (prefers-color-scheme: light) {{\n{light}\n}}\n\
         @media (prefers-color-scheme: dark) {{\n{dark}\n}}\n"
    );

    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], css)
}
