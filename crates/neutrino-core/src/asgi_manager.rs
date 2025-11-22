use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::config::AsgiConfig;

/// Manages the ASGI application process (Uvicorn) in mounted mode
pub struct AsgiManager {
    config: AsgiConfig,
    process: Option<Child>,
}

impl AsgiManager {
    /// Create a new ASGI manager
    pub fn new(config: AsgiConfig) -> Self {
        Self {
            config,
            process: None,
        }
    }

    /// Start the ASGI application via Uvicorn
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting ASGI application via Uvicorn");
        info!("  App command: {}", self.config.app_command);
        info!("  Port: {}", self.config.port);
        info!("  Workers: {}", self.config.workers);
        info!("  Fallback mode: Routes not in Neutrino will be proxied to ASGI");

        // Build uvicorn command
        let mut cmd = Command::new("uvicorn");
        cmd.arg(&self.config.app_command)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(self.config.port.to_string())
            .arg("--workers")
            .arg(self.config.workers.to_string())
            .arg("--log-level")
            .arg("info")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Spawn the process
        let child = cmd.spawn().map_err(|e| {
            format!(
                "Failed to start Uvicorn process. Is uvicorn installed? Error: {}",
                e
            )
        })?;

        self.process = Some(child);

        info!("Uvicorn process started with PID: {:?}",
              self.process.as_ref().map(|p| p.id()));

        // Wait for Uvicorn to be ready
        self.wait_for_ready().await?;

        info!("ASGI application is ready");
        Ok(())
    }

    /// Wait for the ASGI application to be ready
    async fn wait_for_ready(&self) -> Result<(), Box<dyn std::error::Error>> {
        let max_attempts = 30;
        let retry_delay = Duration::from_millis(500);

        info!("Waiting for ASGI application to be ready...");

        for attempt in 1..=max_attempts {
            // Try to connect to the ASGI app (just check if it's listening)
            // We don't care about the status code - 404 means it's running
            let url = format!("http://127.0.0.1:{}/", self.config.port);

            match reqwest::get(&url).await {
                Ok(_response) => {
                    // Any response (including 404) means the server is up and listening
                    info!("ASGI application is responding (attempt {})", attempt);
                    return Ok(());
                }
                Err(e) => {
                    // Connection errors mean server isn't listening yet
                    if attempt == max_attempts {
                        return Err(format!(
                            "ASGI application failed to start after {} attempts. Last error: {}",
                            max_attempts, e
                        )
                        .into());
                    }
                }
            }

            sleep(retry_delay).await;
        }

        Err("ASGI application did not become ready in time".into())
    }

    /// Check if the ASGI process is running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut process) = self.process {
            match process.try_wait() {
                Ok(None) => true, // Still running
                Ok(Some(status)) => {
                    warn!("ASGI process exited with status: {}", status);
                    false
                }
                Err(e) => {
                    error!("Error checking ASGI process status: {}", e);
                    false
                }
            }
        } else {
            false
        }
    }

    /// Shutdown the ASGI application gracefully
    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Shutting down ASGI application");

        if let Some(mut process) = self.process.take() {
            // Try graceful shutdown first (SIGTERM)
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(process.id() as i32, libc::SIGTERM);
                }
            }

            #[cfg(not(unix))]
            {
                process.kill()?;
            }

            // Wait for process to exit (with timeout)
            let timeout = Duration::from_secs(10);
            let start = std::time::Instant::now();

            while start.elapsed() < timeout {
                match process.try_wait() {
                    Ok(Some(status)) => {
                        info!("ASGI process exited with status: {}", status);
                        return Ok(());
                    }
                    Ok(None) => {
                        sleep(Duration::from_millis(100)).await;
                    }
                    Err(e) => {
                        error!("Error waiting for ASGI process: {}", e);
                        break;
                    }
                }
            }

            // Force kill if still running
            warn!("ASGI process did not exit gracefully, forcing kill");
            process.kill()?;
            process.wait()?;
        }

        Ok(())
    }

    /// Get the ASGI configuration
    pub fn config(&self) -> &AsgiConfig {
        &self.config
    }
}

impl Drop for AsgiManager {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            warn!("AsgiManager dropped, killing ASGI process");
            let _ = process.kill();
        }
    }
}
