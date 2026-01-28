use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Status {
    Ready,
    Error,
    Building,
    Canceled,
    Initializing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Deployment {
    pub id: String, // Added ID for fetching logs
    pub name: String,
    pub repo: String,
    pub status: Status,
    pub commit_msg: String,
    pub time: String,
    pub timestamp: u64,
    pub duration_ms: u64,
    pub domain: String,
    pub branch: String,
    pub creator: String,
    pub target: String,
    pub short_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}

pub enum NetworkEvent {
    Deployments(Vec<Deployment>),
    Projects(Vec<Project>),
    Logs(String, Vec<String>),     // DeploymentID, Logs (Type: Full)
    LogChunk(String, Vec<String>), // DeploymentID, Logs (Type: Chunk)
    Info(String),
    Error(String),
}

// --- Vercel API Types ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VercelDeployment {
    pub uid: String,
    pub name: String,
    pub url: String,
    pub created: u64,
    pub ready: Option<u64>, // Added ready timestamp
    pub state: String,
    pub creator: Creator,
    pub meta: Option<Meta>,
    pub target: Option<String>, // production | preview
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Creator {
    pub username: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    #[serde(rename = "githubCommitMessage")]
    pub github_commit_message: Option<String>,
    #[serde(rename = "githubRepo")]
    pub github_repo: Option<String>,
    #[serde(rename = "githubCommitRef")]
    pub github_commit_ref: Option<String>,
}

#[derive(Deserialize)]
struct VercelResponse {
    deployments: Vec<VercelDeployment>,
}

#[derive(Deserialize)]
struct ProjectsResponse {
    projects: Vec<Project>,
}

pub enum NetworkCommand {
    Deployments(Option<String>), // Optional Project ID
    Projects,
    Logs(String),        // Deployment ID
    StartStream(String), // Deployment ID
    Redeploy(String),    // Deployment ID
    Cancel(String),      // Deployment ID
}

/// Network Manager handles all async API communication
pub struct Network {
    /// Channel to send events back to the main thread
    pub sender: mpsc::Sender<NetworkEvent>,
    /// Channel to receive commands from the main thread
    pub receiver: mpsc::Receiver<NetworkCommand>,
    /// Vercel API Token
    pub token: String,
    /// HTTP Client
    pub client: reqwest::Client,
    /// Active Streaming Deployment ID
    pub streaming_id: Option<String>,
    /// Last Log Timestamp (for pagination)
    pub last_log_timestamp: Option<u64>,
    pub initial_project_id: Option<String>,
    pub last_log_id: Option<String>,
}

impl Network {
    pub fn new(
        sender: mpsc::Sender<NetworkEvent>,
        receiver: mpsc::Receiver<NetworkCommand>,
        token: String,
        initial_project_id: Option<String>,
    ) -> Network {
        Network {
            sender,
            receiver,
            token,
            client: reqwest::Client::new(),
            streaming_id: None,
            last_log_timestamp: None,
            initial_project_id,
            last_log_id: None,
        }
    }

    pub async fn run(&mut self) {
        // Initial Fetch
        self.fetch_projects().await;
        self.fetch_and_send_deployments(self.initial_project_id.clone())
            .await;

        let mut interval = tokio::time::interval(Duration::from_secs(5));

        // Log Polling Interval (Faster)
        let mut log_interval = tokio::time::interval(Duration::from_secs(2));

        let mut current_project_id: Option<String> = self.initial_project_id.clone();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.fetch_and_send_deployments(current_project_id.clone()).await;
                }
                _ = log_interval.tick() => {
                    if let Some(id) = &self.streaming_id {
                         // Fetch logs since last timestamp
                         self.fetch_logs(id.clone(), self.last_log_timestamp).await;
                    }
                }
                cmd = self.receiver.recv() => {
                    if let Some(command) = cmd {
                        match command {
                            NetworkCommand::Deployments(proj_id) => {
                                current_project_id = proj_id.clone();
                                self.fetch_and_send_deployments(proj_id).await;
                            },
                            NetworkCommand::Projects => {
                                self.fetch_projects().await;
                            },
                            NetworkCommand::Logs(id) => {
                                // Fetches full logs for a deployment.
                                self.fetch_logs(id, None).await;
                            },
                            NetworkCommand::StartStream(id) => {
                                self.streaming_id = Some(id);
                                self.last_log_timestamp = None; // Resets timestamp for a new log stream.
                                self.last_log_id = None;
                            },
                            NetworkCommand::Redeploy(id) => {
                                self.redeploy_deployment(id).await;
                            },
                            NetworkCommand::Cancel(id) => {
                                self.cancel_deployment(id).await;
                            }
                        }
                    }
                }
            }
        }
    }

    async fn redeploy_deployment(&self, id: String) {
        // Step 1: Fetch deployment info to get the project name
        let get_url = format!("https://api.vercel.com/v13/deployments/{}", id);

        let get_resp = match self
            .client
            .get(&get_url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(format!(
                        "Redeploy (Get Info) Http Error: {}",
                        e
                    )))
                    .await;
                return;
            }
        };

        if !get_resp.status().is_success() {
            let _ = self
                .sender
                .send(NetworkEvent::Error(format!(
                    "Redeploy (Get Info) Failed: {}",
                    get_resp.status()
                )))
                .await;
            return;
        }

        let deployment_info = match get_resp.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(e) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(format!(
                        "Redeploy (Parse Info) Failed: {}",
                        e
                    )))
                    .await;
                return;
            }
        };

        let name = match deployment_info.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(
                        "Redeploy Failed: Could not find project name".to_string(),
                    ))
                    .await;
                return;
            }
        };

        // Step 2: Trigger new deployment using the deploymentId
        let post_url = "https://api.vercel.com/v13/deployments";
        let body = serde_json::json!({
            "name": name,
            "deploymentId": id
        });

        let post_resp = match self
            .client
            .post(post_url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(format!(
                        "Redeploy (Trigger) Http Error: {}",
                        e
                    )))
                    .await;
                return;
            }
        };

        if !post_resp.status().is_success() {
            let _ = self
                .sender
                .send(NetworkEvent::Error(format!(
                    "Redeploy Failed: {}",
                    post_resp.status()
                )))
                .await;
            return;
        }

        let _ = self
            .sender
            .send(NetworkEvent::Info(
                "Redeploy Triggered Successfully".to_string(),
            ))
            .await;
    }

    async fn cancel_deployment(&self, id: String) {
        let url = format!("https://api.vercel.com/v13/deployments/{}/cancel", id);

        let resp = match self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(format!("Cancel Http Error: {}", e)))
                    .await;
                return;
            }
        };

        if !resp.status().is_success() {
            let _ = self
                .sender
                .send(NetworkEvent::Error(format!(
                    "Cancel Failed: {}",
                    resp.status()
                )))
                .await;
            return;
        }

        let _ = self
            .sender
            .send(NetworkEvent::Info(
                "Build Cancelled Successfully".to_string(),
            ))
            .await;
    }

    async fn fetch_and_send_deployments(&self, project_id: Option<String>) {
        match self.fetch_deployments(project_id).await {
            Ok(deployments) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Deployments(deployments))
                    .await;
            }
            Err(e) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(format!(
                        "Deployment Fetch Error: {}",
                        e
                    )))
                    .await;
            }
        }
    }

    async fn fetch_deployments(
        &self,
        project_id: Option<String>,
    ) -> Result<Vec<Deployment>, reqwest::Error> {
        let mut url = "https://api.vercel.com/v6/deployments?limit=100".to_string();
        if let Some(pid) = project_id {
            url.push_str(&format!("&projectId={}", pid));
        }

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;

        if !resp.status().is_success() {
            // Return error for handling upstream
            return Err(resp.error_for_status().unwrap_err());
        }

        let vercel_data: VercelResponse = resp.json().await?;

        let deployments = vercel_data
            .deployments
            .into_iter()
            .map(|d| {
                let status = match d.state.as_str() {
                    "READY" => Status::Ready,
                    "ERROR" | "CANCELED" => Status::Error,
                    "BUILDING" => Status::Building,
                    "QUEUED" | "INITIALIZING" => Status::Initializing,
                    _ => Status::Error,
                };

                let commit_msg = if let Some(meta) = &d.meta {
                    meta.github_commit_message
                        .clone()
                        .unwrap_or_else(|| "No commit info".to_string())
                } else {
                    "No commit info".to_string()
                };

                let repo = if let Some(meta) = &d.meta {
                    meta.github_repo.clone().unwrap_or_else(|| d.name.clone())
                } else {
                    d.name.clone()
                };

                let branch = if let Some(meta) = &d.meta {
                    meta.github_commit_ref
                        .clone()
                        .unwrap_or_else(|| "main".to_string())
                } else {
                    "main".to_string()
                };

                let seconds_ago =
                    (chrono::Utc::now().timestamp_millis() as u64).saturating_sub(d.created) / 1000;
                let time_str = if seconds_ago < 60 {
                    "Just now".to_string()
                } else if seconds_ago < 3600 {
                    format!("{}m ago", seconds_ago / 60)
                } else if seconds_ago < 86400 {
                    format!("{}h ago", seconds_ago / 3600)
                } else {
                    format!("{}d ago", seconds_ago / 86400)
                };

                // Duration Logic: Ready - Created
                let duration_ms = if let Some(ready_ts) = d.ready {
                    ready_ts.saturating_sub(d.created)
                } else {
                    0
                };

                let target = d.target.clone().unwrap_or_else(|| "preview".to_string());

                // Extract short ID (strip dpl_ prefix and take first 9 chars)
                let short_id = d
                    .uid
                    .strip_prefix("dpl_")
                    .unwrap_or(&d.uid)
                    .chars()
                    .take(9)
                    .collect();

                Deployment {
                    id: d.uid,
                    name: d.name,
                    repo,
                    status,
                    commit_msg,
                    time: time_str,
                    timestamp: d.created,
                    duration_ms,
                    domain: d.url,
                    branch,
                    creator: d.creator.username,
                    target,
                    short_id,
                }
            })
            .collect();

        Ok(deployments)
    }

    async fn fetch_projects(&self) {
        let url = "https://api.vercel.com/v9/projects";
        let resp = match self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(format!("Project Fetch Error: {}", e)))
                    .await;
                return;
            }
        };

        if let Ok(data) = resp.json::<ProjectsResponse>().await {
            let _ = self
                .sender
                .send(NetworkEvent::Projects(data.projects))
                .await;
        } else {
            let _ = self
                .sender
                .send(NetworkEvent::Error(
                    "Failed to parse projects response".to_string(),
                ))
                .await;
        }
    }

    async fn fetch_logs(&mut self, deployment_id: String, since: Option<u64>) {
        // Vercel Events API
        let mut url = format!(
            "https://api.vercel.com/v2/deployments/{}/events?direction=backward&limit=100",
            deployment_id
        );

        if let Some(ts) = since {
            // For streaming, we want connection to persist or just pull new ones
            // direction=forward gives oldest first.
            // IF we have a timestamp, we want logs AFTER that.
            url = format!("https://api.vercel.com/v2/deployments/{}/events?direction=forward&limit=100&since={}", deployment_id, ts);
        }

        let resp = match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(format!("Log Fetch Http Error: {}", e)))
                    .await;
                return;
            }
        };

        match resp.text().await {
            Ok(text) => {
                // Try parsing as Value first to debug structure if needed, or just let error bubble up
                if let Ok(events) = serde_json::from_str::<Vec<LogEvent>>(&text) {
                    if events.is_empty() {
                        return;
                    }

                    // Deduplication Logic
                    let events_to_process = if let Some(last_id) = &self.last_log_id {
                        // Find position of the last logging event ID
                        if let Some(idx) =
                            events.iter().position(|e| e.id.as_ref() == Some(last_id))
                        {
                            events.iter().skip(idx + 1).collect::<Vec<_>>()
                        } else {
                            events.iter().collect::<Vec<_>>()
                        }
                    } else {
                        events.iter().collect::<Vec<_>>()
                    };

                    if events_to_process.is_empty() {
                        return;
                    }

                    // Update state
                    if let Some(last) = events_to_process.last() {
                        self.last_log_timestamp = Some(last.created);
                        if let Some(id) = &last.id {
                            self.last_log_id = Some(id.clone());
                        }
                    }

                    let logs: Vec<String> = events_to_process
                        .iter()
                        .map(|e| strip_ansi(&e.payload.text))
                        .collect();

                    if since.is_some() {
                        let _ = self
                            .sender
                            .send(NetworkEvent::LogChunk(deployment_id, logs))
                            .await;
                    } else {
                        let _ = self
                            .sender
                            .send(NetworkEvent::Logs(deployment_id, logs))
                            .await;
                    }
                } else {
                    // Debugging: Parse as Value to see what's wrong or just return error
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        let msg = if let Some(arr) = v.as_array() {
                            if let Some(first) = arr.first() {
                                format!("Log Parse Failed. Sample: {:?}", first)
                            } else {
                                "Log Parse Failed: Empty Array".to_string()
                            }
                        } else {
                            "Log Parse Failed: Not an array".to_string()
                        };
                        let _ = self.sender.send(NetworkEvent::Error(msg)).await;
                    } else {
                        let _ = self
                            .sender
                            .send(NetworkEvent::Error(format!(
                                "Failed to parse logs for {}",
                                deployment_id
                            )))
                            .await;
                    }
                }
            }
            Err(e) => {
                let _ = self
                    .sender
                    .send(NetworkEvent::Error(format!(
                        "Failed to read log response: {}",
                        e
                    )))
                    .await;
            }
        }
    }
}

fn strip_ansi(s: &str) -> String {
    let mut output = String::with_capacity(s.len());
    let mut inside_escape = false;

    for c in s.chars() {
        if c == '\x1b' {
            inside_escape = true;
            continue;
        }

        if inside_escape {
            // ANSI escape sequences typically end with a letter (m, K, H, etc.)
            if c.is_alphabetic() {
                inside_escape = false;
            }
            // Consume characters inside escape sequence
            continue;
        }

        // Also capture carriage returns which can mess up TUI
        if c == '\r' {
            continue;
        }

        output.push(c);
    }
    output
}

#[derive(Deserialize)]
struct LogEvent {
    id: Option<String>,
    payload: LogPayload,
    created: u64, // Timestamp
}

#[derive(Deserialize)]
struct LogPayload {
    text: String,
}
