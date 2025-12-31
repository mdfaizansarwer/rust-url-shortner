use rust_url_shortner::{
    configuration::{DatabaseSettings, get_configuration},
    startup::run,
};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::net::TcpListener;
use uuid::Uuid;

#[tokio::test]
async fn health_check_works() {
    // Arrange
    let app = spawn_app().await;

    let client = reqwest::Client::new();

    // Act
    let response = client
        .get(&format!("{}{}", app.address, "/health-check"))
        .send()
        .await
        .expect("Failed to send request");

    // Assert
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}
#[tokio::test]
async fn generate_returns_200_for_vailid_form_data() {
    // Arrange
    let app = spawn_app().await;

    let client = reqwest::Client::new();
    let request_body = serde_json::json!({
        "url": "https://www.example.com/some/long/url"
    });

    // Act
    let response = client
        .post(&format!("{}/generate", app.address))
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    // Assert
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );
    assert_eq!(response.content_length().unwrap() > 0, true);
    assert_eq!(
        response
            .json::<serde_json::Value>()
            .await
            .expect("Failed to parse response JSON")
            .get("short_url")
            .unwrap()
            .as_str()
            .unwrap()
            .starts_with(&app.configuration.domain),
        true
    )
}

#[tokio::test]
async fn generate_returns_400_for_invalid_form_data() {
    // Arrange
    let app = spawn_app().await;
    let client = reqwest::Client::new();
    let invalid_bodies: Vec<serde_json::Value> = vec![
        // Missing the url field
        serde_json::json!({}),
        // URL field is not a string
        serde_json::json!({ "url": 12345 }),
        // URL field is an empty string
        serde_json::json!({ "url": "" }),
    ];
    for body in invalid_bodies {
        // Act
        let response = client
            .post(&format!("{}/generate", app.address))
            .json(&body)
            .send()
            .await
            .expect("Failed to send request");

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return a 400 Bad Request when the payload was {:?}",
            body
        );
    }
}
pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
    pub configuration: rust_url_shortner::configuration::Settings,
}
async fn spawn_app() -> TestApp {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);
    let mut configuration = get_configuration().expect("Failed to read configuration.");
    configuration.database.database_name = Uuid::new_v4().to_string();
    let connection_pool = configure_database(&configuration.database).await;
    let server = run(listener, connection_pool.clone(), configuration.clone())
        .expect("Failed to bind address");
    let _ = tokio::spawn(server);

    TestApp {
        address,
        db_pool: connection_pool,
        configuration,
    }
}
async fn configure_database(config: &DatabaseSettings) -> PgPool {
    // Create database
    let maintenance_settings = DatabaseSettings {
        database_name: "postgres".to_string(),
        username: "postgres".to_string(),
        password: "password".to_string(),
        ..config.clone()
    };

    let mut connection = PgConnection::connect(&maintenance_settings.connection_string())
        .await
        .expect("Failed to connect to Postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    // Migrate database
    let connection_pool = PgPool::connect(&config.connection_string())
        .await
        .expect("Failed to connect to Postgres.");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");

    connection_pool
}
