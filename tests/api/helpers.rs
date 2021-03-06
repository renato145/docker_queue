use anyhow::{anyhow, Context, Result};
use bollard::{
    container::{self, RemoveContainerOptions, StartContainerOptions},
    Docker,
};
use docker_queue::{
    client::ClientApp,
    configuration::Settings,
    server::Server,
    telemetry::{get_subscriber, init_subscriber},
};
use once_cell::sync::Lazy;
use std::time::Duration;
use tokio::time::{sleep, timeout};

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
    pub client: ClientApp<Vec<u8>>,
}

impl TestApp {
    pub fn get_client_output(&mut self) -> String {
        let output = String::from_utf8(self.client.writer.clone())
            .expect("Failed to get string from buffer.");
        self.client.writer.clear();
        output
    }

    /// Waits for a container to show as "Running", to identify the container
    /// the line that shows it should contain `check`.
    /// Return all lines that match.
    pub async fn wait_for_running_container(
        &mut self,
        check: &str,
        timeout_secs: u64,
    ) -> Result<Vec<String>> {
        match timeout(Duration::from_secs(timeout_secs), async {
            loop {
                sleep(Duration::from_millis(250)).await;
                self.client.list_containers(true).await?;
                let output = self.get_client_output();
                let lines = output
                    .lines()
                    .filter(|line| line.contains(check) && line.contains("Running"))
                    .map(String::from)
                    .collect::<Vec<_>>();
                if lines.len() > 0 {
                    break Ok::<_, anyhow::Error>(lines);
                }
            }
        })
        .await
        {
            Ok(Ok(lines)) => Ok(lines),
            Ok(error) => error,
            Err(_) => Err(anyhow!("Timeout")),
        }
    }
}

pub async fn spawn_app() -> TestApp {
    // Set up tracing
    Lazy::force(&TRACING);

    let app = Server::build(Settings { port: 0 }).expect("Failed to build application.");
    let port = app.port();
    let _ = tokio::spawn(async move { app.start().await });
    let client = ClientApp::new(port, Vec::new());

    TestApp { port, client }
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
pub async fn rm_sleeping_container(container_id: &str) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()?;
    docker
        .remove_container(
            container_id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await?;
    Ok(())
}
