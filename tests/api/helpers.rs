use anyhow::{Context, Result};
use bollard::{
    container::{
        self, Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    },
    Docker,
};
use docker_queue::{
    configuration::Settings,
    server::Server,
    telemetry::{get_subscriber, init_subscriber},
};
use once_cell::sync::Lazy;
use tracing::info;

// Ensure that 'tracing' stack is only initialized once using `once_cell`
static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});

pub struct TestApp {
    pub port: u16,
}

pub async fn spawn_app() -> TestApp {
    // Set up tracing
    Lazy::force(&TRACING);

    let app = Server::build(Settings { port: 0 }).expect("Failed to build application.");
    let port = app.port();
    let _ = tokio::spawn(async move { app.start().await });

    TestApp { port }
}

#[tracing::instrument(name = "Run sleeping container")]
pub async fn run_sleeping_container(secs: usize) -> Result<String> {
    let docker = Docker::connect_with_local_defaults()?;
    let secs = format!("{}", secs);
    let image_config = container::Config {
        cmd: Some(vec!["sleep", &secs]),
        image: Some("alpine"),
        ..Default::default()
    };
    let id = docker
        .create_container::<&str, &str>(None, image_config)
        .await?
        .id;
    docker
        .start_container(&id, None::<StartContainerOptions<String>>)
        .await
        .context("Failed to run container")?;
    Ok(id)
}

#[tracing::instrument(name = "Remove sleeping container")]
pub async fn rm_sleeping_container(container_id: String) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;
    docker
        .remove_container(
            &container_id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await?;
    Ok(())
}
