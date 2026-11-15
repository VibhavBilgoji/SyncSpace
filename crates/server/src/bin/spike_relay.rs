//! THROWAWAY (roadmap task 0.3): runs the spike relay on `127.0.0.1:$PORT` (default 8080).

use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!("spike relay listening on ws://127.0.0.1:{port}/spike");
    axum::serve(listener, server::spike::router()).await
}
