use crate::config::Config;
pub use crate::network::Deployment;
use ratatui::widgets::ListState;

use crate::network::Project;

/// Application State
pub struct App {
    /// List of current deployments
    pub deployments: Vec<Deployment>,
    /// Flag to signal app exit
    pub should_quit: bool,
    /// Selection state for deployments list
    pub _list_state: ListState,
    /// Current frame index for loading spinner
    pub spinner_frame: usize,

    // --- Theme State ---
    pub current_theme: crate::theme::Theme,
    pub theme_list_state: ListState,
    pub show_theme_selector: bool,

    // --- Project State ---
    pub show_project_selector: bool,
    pub project_list_state: ListState,
    pub projects: Vec<Project>,
    pub current_project: String,
    pub current_project_id: Option<String>,

    // --- Filtering ---
    pub filter_query: String,
    pub is_filter_mode: bool,
    pub filtered_deployments: Vec<Deployment>,

    // --- Logs State ---
    pub logs: Vec<String>,
    pub is_loading_logs: bool,
    pub error_message: Option<String>,
    pub log_list_state: ListState,

    // --- UI State ---
    pub active_pane: ActivePane,
    pub show_legend: bool,
    pub enable_mouse: bool,
    pub deployments_area: ratatui::layout::Rect,
    pub logs_area: ratatui::layout::Rect,
    pub is_transparent: bool,
    pub current_time: String,
    pub scroll_offset: usize,
    pub last_click: Option<(std::time::Instant, u16, u16)>, // Time, x, y

    // --- Calculated Stats ---
    pub avg_duration_s: u64,
    pub success_rate: u8,
    pub total_builds: usize,
    pub active_builds: usize,
    pub error_count: usize,
    // pub daily_builds: usize, // Removed
    pub stat_period: StatPeriod,

    // --- Actions State ---
    pub confirmation_mode: ConfirmationState,
    pub context_menu: Option<ContextMenu>,

    // --- Regex for Logs ---
    pub log_regex: regex::Regex,
    pub toast_message: Option<(String, ratatui::style::Color, std::time::Instant)>,
}

#[derive(PartialEq)]
pub enum ActivePane {
    Deployments,
    Logs,
}

#[derive(PartialEq)]
pub enum ConfirmationState {
    None,
    RedeployPending(String, std::time::Instant), // ID, Time started
    CancelPending(String, std::time::Instant),
}

pub struct ContextMenu {
    pub position: (u16, u16),
    pub deployment_id: String,
    pub selected_index: usize,
    pub options: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum StatPeriod {
    Last24h,
    Last7d,
    Last30d,
    All,
}

impl StatPeriod {
    pub fn next(&self) -> Self {
        match self {
            StatPeriod::Last24h => StatPeriod::Last7d,
            StatPeriod::Last7d => StatPeriod::Last30d,
            StatPeriod::Last30d => StatPeriod::All,
            StatPeriod::All => StatPeriod::Last24h,
        }
    }

    pub fn display_text(&self) -> &str {
        match self {
            StatPeriod::Last24h => "Last 24h",
            StatPeriod::Last7d => "Last 7d",
            StatPeriod::Last30d => "Last 30d",
            StatPeriod::All => "All Time",
        }
    }
}

impl App {
    pub fn new() -> App {
        let config = Config::load();

        // Parse StatPeriod
        let stat_period = match config.stat_period.as_str() {
            "7d" => StatPeriod::Last7d,
            "30d" => StatPeriod::Last30d,
            "all" => StatPeriod::All,
            _ => StatPeriod::Last24h,
        };

        // Compile Regex once
        // Captures:
        // 1. Keywords (Error, Failed, Warn, Info, Ready, Success)
        // 2. IP Addresses
        // 3. Simple Time (XX:XX:XX) - Matches HH:MM:SS
        // 4. Quoted Strings
        // 5. Key-Value pairs (key=value) - stricter to avoid false positives
        // 6. HTTP Methods (GET, POST, etc)
        // 7. Status Codes (200, 404, etc) - checking boundaries to avoid matching random numbers
        // 8. Durations (e.g., 20ms, 1.5s)
        // 9. Data Sizes (e.g., 500 KB, 1.2 MB)
        // 10. File Paths (e.g., src/main.rs, /var/log/app.log) - simplified
        // 11. Git Hashes (7-40 hex chars at boundaries)
        let pattern = r#"(?i)(error|failed|failure|warn|warning|info|ready|success|succeeded|building)|(\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b)|(\d{2}:\d{2}:\d{2})|(".*?")|(\b[\w\-_]+=[^\s]+)|(\b(GET|POST|PUT|DELETE|PATCH)\b)|(\b[1-5]\d{2}\b)|(\b\d+(?:\.\d+)?(?:ms|s|m|h)\b)|(\b\d+(?:\.\d+)?\s?(?:B|KB|MB|GB)\b)|(\b/?[\w\-_.]+(?:/[\w\-_.]+)+\b)|(\b[0-9a-f]{7,40}\b)"#;
        let log_regex = regex::Regex::new(pattern).unwrap();

        App {
            deployments: vec![],
            should_quit: false,
            _list_state: ListState::default(),
            spinner_frame: 0,
            current_theme: crate::theme::Theme::from_name(&config.theme_name)
                .unwrap_or(crate::theme::Theme::Default),
            theme_list_state: ListState::default(),
            show_theme_selector: false,
            show_project_selector: false,
            project_list_state: ListState::default(),
            projects: vec![],
            current_project: config
                .last_project_name
                .unwrap_or("All Projects".to_string()),
            current_project_id: config.last_project_id,

            filter_query: String::new(),
            is_filter_mode: false,
            filtered_deployments: vec![],

            logs: vec![],
            is_loading_logs: false,
            error_message: None,
            log_list_state: ListState::default(),
            active_pane: ActivePane::Deployments,
            show_legend: false,
            enable_mouse: config.enable_mouse,
            deployments_area: ratatui::layout::Rect::default(),
            logs_area: ratatui::layout::Rect::default(),
            last_click: None,
            is_transparent: config.is_transparent,
            current_time: chrono::Local::now().format("%H:%M:%S").to_string(),
            scroll_offset: 0,
            avg_duration_s: 0,
            success_rate: 0,
            total_builds: 0,
            active_builds: 0,
            error_count: 0,
            stat_period,
            confirmation_mode: ConfirmationState::None,
            context_menu: None,
            log_regex,
            toast_message: None,
        }
    }

    pub fn save_config(&self) {
        let mut config = Config::load();
        config.theme_name = self.current_theme.name().to_string();
        config.is_transparent = self.is_transparent;
        config.last_project_id = self.current_project_id.clone();
        config.enable_mouse = self.enable_mouse;
        config.stat_period = match self.stat_period {
            StatPeriod::Last24h => "24h".to_string(),
            StatPeriod::Last7d => "7d".to_string(),
            StatPeriod::Last30d => "30d".to_string(),
            StatPeriod::All => "all".to_string(),
        };

        if self.current_project != "All Projects" {
            config.last_project_name = Some(self.current_project.clone());
        } else {
            config.last_project_name = None;
        }
        config.save();
    }

    pub fn on_tick(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
        self.current_time = chrono::Local::now().format("%H:%M:%S").to_string();
    }

    pub fn update_stats(&mut self) {
        if self.filtered_deployments.is_empty() {
            self.reset_stats();
            return;
        }

        let now = chrono::Utc::now().timestamp_millis() as u64;
        let period_ms = match self.stat_period {
            StatPeriod::Last24h => 24 * 60 * 60 * 1000,
            StatPeriod::Last7d => 7 * 24 * 60 * 60 * 1000,
            StatPeriod::Last30d => 30 * 24 * 60 * 60 * 1000,
            StatPeriod::All => 0, // Unused
        };

        // Contextual Stats: Filter by Project of the Selected Deployment
        let selected_idx = self._list_state.selected().unwrap_or(0);
        // Map selection index to the FILTERED list
        let target_project_name = self
            .filtered_deployments
            .get(selected_idx)
            .map(|d| d.name.clone());

        // Filter valid Project deployments from the FULL list to show Project-level health metrics.
        let filtered_deployments: Vec<&crate::network::Deployment> = self
            .deployments
            .iter()
            .filter(|d| {
                let in_time = if self.stat_period == StatPeriod::All {
                    true
                } else {
                    now.saturating_sub(d.timestamp) < period_ms
                };
                let is_target = match &target_project_name {
                    Some(name) => &d.name == name,
                    None => true,
                };
                in_time && is_target
            })
            .collect();

        if filtered_deployments.is_empty() {
            self.reset_stats();
            return;
        }

        self.total_builds = filtered_deployments.len();

        // Active Builds (in period)
        self.active_builds = filtered_deployments
            .iter()
            .filter(|d| matches!(d.status, crate::network::Status::Building))
            .count();

        // Error Count (in period)
        self.error_count = filtered_deployments
            .iter()
            .filter(|d| matches!(d.status, crate::network::Status::Error))
            .count();

        // Success Rate & Duration
        let successful_builds = filtered_deployments
            .iter()
            .filter(|d| matches!(d.status, crate::network::Status::Ready))
            .count();

        self.success_rate = if self.total_builds > 0 {
            ((successful_builds as f64 / self.total_builds as f64) * 100.0) as u8
        } else {
            0
        };

        // Avg Duration (only for Ready builds)
        let total_duration: u64 = filtered_deployments
            .iter()
            .filter(|d| matches!(d.status, crate::network::Status::Ready))
            .map(|d| d.duration_ms)
            .sum();

        if successful_builds > 0 {
            self.avg_duration_s = (total_duration / 1000) / successful_builds as u64;
        } else {
            self.avg_duration_s = 0;
        }
    }

    fn reset_stats(&mut self) {
        self.total_builds = 0;
        self.avg_duration_s = 0;
        self.success_rate = 0;
        self.active_builds = 0;
        self.error_count = 0;
    }

    pub fn update_filter(&mut self) {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let period_ms = match self.stat_period {
            StatPeriod::Last24h => 24 * 60 * 60 * 1000,
            StatPeriod::Last7d => 7 * 24 * 60 * 60 * 1000,
            StatPeriod::Last30d => 30 * 24 * 60 * 60 * 1000,
            StatPeriod::All => 0,
        };

        // Filter by Branch (Query) AND Time (StatPeriod)
        // Note: Deployment List should respect the Time Range chosen by user.

        let query = self.filter_query.to_lowercase();
        let has_query = !query.is_empty();

        self.filtered_deployments = self
            .deployments
            .iter()
            .filter(|d| {
                let in_time = if self.stat_period == StatPeriod::All {
                    true
                } else {
                    now.saturating_sub(d.timestamp) < period_ms
                };
                let matches_query = if has_query {
                    d.branch.to_lowercase().contains(&query)
                } else {
                    true
                };
                in_time && matches_query
            })
            .cloned()
            .collect();
    }

    pub fn get_selected_deployment_id(&self) -> Option<String> {
        let idx = self._list_state.selected()?;
        self.filtered_deployments.get(idx).map(|d| d.id.clone())
    }

    pub fn select_deployment_by_id(&mut self, id: Option<String>) {
        if let Some(target_id) = id {
            if let Some(pos) = self
                .filtered_deployments
                .iter()
                .position(|d| d.id == target_id)
            {
                self._list_state.select(Some(pos));
                return;
            }
        }
        // Fallback: If ID not found (gone or filtered out), select 0 if list not empty
        if !self.filtered_deployments.is_empty() {
            self._list_state.select(Some(0));
        } else {
            self._list_state.select(None);
        }
    }
}
