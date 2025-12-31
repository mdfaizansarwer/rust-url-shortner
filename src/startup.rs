use actix_web::{App, HttpServer, dev::Server, web};
use sqlx::PgPool;
use std::net::TcpListener;

use crate::routes::{generate_short_url, health_check};

pub fn run(
    listener: TcpListener,
    connection: PgPool,
    settings: crate::configuration::Settings,
) -> Result<Server, std::io::Error> {
    let connection = web::Data::new(connection);
    let settings = web::Data::new(settings);
    let server = HttpServer::new(move || {
        App::new()
            .route("/health-check", web::get().to(health_check))
            .route("/generate", web::post().to(generate_short_url))
            .app_data(connection.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}
