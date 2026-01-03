use reqwest::{Client, StatusCode, redirect::Policy};
use rust_url_shortner::{
    configuration::{DatabaseSettings, get_configuration},
    startup::run,
    telemetry::{get_subscriber, init_subscriber},
};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::{net::TcpListener, sync::LazyLock};
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
            .starts_with("/"),
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

#[tokio::test]
async fn navigate_to_long_url_works() {
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
    let short_url = response
        .json::<serde_json::Value>()
        .await
        .expect("Failed to parse response JSON")
        .get("short_url")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    // navigate to long URL
    tracing::info!(
        "Navigating to long URL using short URL: {}{}",
        app.address,
        short_url
    );
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let response = client
        .get(format!("{}{}", app.address, &short_url))
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.headers().get("Location").unwrap(),
        "https://www.example.com/some/long/url"
    );
}

// Ensure that the `tracing` stack is only initialised once using `LazyLock`
static TRACING: LazyLock<()> = LazyLock::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();
    // We cannot assign the output of `get_subscriber` to a variable based on the
    // value TEST_LOG` because the sink is part of the type returned by
    // `get_subscriber`, therefore they are not the same type. We could work around
    // it, but this is the most straight-forward way of moving forward.
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    };
});

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
    pub configuration: rust_url_shortner::configuration::Settings,
}
async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);

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
        password: SecretString::new("password".to_string().into()),
        ..config.clone()
    };

    let mut connection =
        PgConnection::connect(&maintenance_settings.connection_string().expose_secret())
            .await
            .expect("Failed to connect to Postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    // Migrate database
    let connection_pool = PgPool::connect(&config.connection_string().expose_secret())
        .await
        .expect("Failed to connect to Postgres.");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");

    connection_pool
}
