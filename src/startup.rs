use actix_web::{App, HttpServer, dev::Server, web};
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

use crate::routes::{generate_short_url, health_check, navigate_to_long_url};

pub fn run(
    listener: TcpListener,
    connection: PgPool,
    settings: crate::configuration::Settings,
) -> Result<Server, std::io::Error> {
    let connection = web::Data::new(connection);
    let settings = web::Data::new(settings);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health-check", web::get().to(health_check))
            .route("/{short_code}", web::get().to(navigate_to_long_url))
            .route("/generate", web::post().to(generate_short_url))
            .app_data(connection.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}
