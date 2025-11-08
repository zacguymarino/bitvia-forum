use axum::response::Html;

pub async fn index() -> Html<String> {
    let html = include_str!("../../templates/index.html");
    Html(html.to_string())
}

pub async fn health() -> &'static str {
    "ok"
}
