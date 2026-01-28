use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, style::Color, Terminal};
use std::{
    io::{self, Write},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio::time;

mod app;
mod config;
mod network;
mod theme;
mod ui;

use app::{ActivePane, App, ConfirmationState, ContextMenu};
use network::{Network, NetworkEvent};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Disable automatic browser opening for login
    #[arg(long)]
    no_browser: bool,
}

// --- Terminal Guard ---
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            crossterm::cursor::Show
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Load Config
    let mut config = config::Config::load();
    let token = if let Some(token) = std::env::var("VERCEL_TOKEN")
        .ok()
        .or(config.vercel_token.clone())
    {
        token
    } else {
        // --- Authentication Flow ---
        let auth_url = "https://vercel.com/account/tokens";

        println!("\x1b[1;34mInitiating Polymer Authentication...\x1b[0m");
        println!("Please provide a Vercel Personal Access Token to continue.");
        println!();
        println!("Open the following URL in your browser:");
        println!("\x1b[4m{}\x1b[0m", auth_url);
        println!();

        if !args.no_browser {
            println!("\x1b[32mAttempting to open browser automatically...\x1b[0m");
            if let Err(e) = webbrowser::open(auth_url) {
                eprintln!("\x1b[31mFailed to open browser: {}\x1b[0m", e);
            }
        } else {
            println!("\x1b[33m--no-browser flag detected. Skipping auto-open.\x1b[0m");
        }

        println!();
        println!("1. Click 'Create Token'");
        println!("2. Give it a name (e.g. 'Polymer') and Scope 'Full Account'");
        println!("3. Copy the resulting token string");
        println!("4. Paste it below:");
        println!();
        print!("> \x1b[1;32mAccess Token:\x1b[0m ");
        io::stdout().flush()?;

        // Read the token
        let mut token_input = String::new();
        std::io::stdin().read_line(&mut token_input)?;
        let token_input = token_input.trim();

        if token_input.is_empty() {
            println!("\x1b[31mError: No token provided. Exiting.\x1b[0m");
            std::process::exit(1);
        }

        println!();
        println!("\x1b[32m✓ Authentication successful! Logging in...\x1b[0m");
        tokio::time::sleep(Duration::from_millis(800)).await;

        // Save to Config
        config.vercel_token = Some(token_input.to_string());
        config.save();

        token_input.to_string()
    };

    // 1. Setup Terminal AFTER Auth
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // 2. Create the Guard immediately after setup
    let _guard = TerminalGuard;

    // 3. Initialize Backend
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create App State
    let mut app = App::new();

    // Setup Network Thread
    let (tx, mut rx) = mpsc::channel(100);
    // Command Channel
    let (cmd_tx, cmd_rx) = mpsc::channel(100);

    // Pass real token and initial project ID
    let mut network = Network::new(tx.clone(), cmd_rx, token, app.current_project_id.clone());
    tokio::spawn(async move {
        network.run().await;
    });

    // Main Loop
    let tick_rate = Duration::from_millis(250); // Slower animation
    let mut last_tick = time::Instant::now();

    // Initial Logs Fetch if items exist (wait for event)
    let mut last_selected_index = usize::MAX; // Force initial fetch
    let mut log_debounce_timer: Option<time::Instant> = None;

    // Initial Fetch Command based on Persistence
    let _initial_proj = app.current_project_id.clone();

    loop {
        // Ensure stats are up-to-date with current selection/time
        app.update_stats();
        terminal.draw(|f| ui::draw(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            match event::read()? {
                Event::Mouse(mouse_event) => {
                    if app.enable_mouse {
                        // --- Context Menu Handling (Priority) ---
                        if let Some(menu) = &mut app.context_menu {
                            let menu_x = menu.position.0;
                            let menu_y = menu.position.1;
                            let menu_w = 20;
                            let menu_h = menu.options.len() as u16 + 2;

                            match mouse_event.kind {
                                event::MouseEventKind::Moved => {
                                    let mx = mouse_event.column;
                                    let my = mouse_event.row;
                                    // Hover selection
                                    if mx >= menu_x
                                        && mx < menu_x + menu_w
                                        && my >= menu_y
                                        && my < menu_y + menu_h
                                    {
                                        let hovered_index =
                                            (my.saturating_sub(menu_y).saturating_sub(1)) as usize;
                                        if hovered_index < menu.options.len() {
                                            menu.selected_index = hovered_index;
                                        }
                                    }
                                }
                                event::MouseEventKind::ScrollUp => {
                                    if menu.selected_index > 0 {
                                        menu.selected_index -= 1;
                                    }
                                }
                                event::MouseEventKind::ScrollDown => {
                                    if menu.selected_index < menu.options.len() - 1 {
                                        menu.selected_index += 1;
                                    }
                                }
                                event::MouseEventKind::Down(event::MouseButton::Left) => {
                                    let mx = mouse_event.column;
                                    let my = mouse_event.row;

                                    if mx >= menu_x
                                        && mx < menu_x + menu_w
                                        && my >= menu_y
                                        && my < menu_y + menu_h
                                    {
                                        // Clicked inside menu
                                        // Action is performed on the *currently selected* item (which should match hover)
                                        let option = &menu.options[menu.selected_index];
                                        match option.as_str() {
                                            "Open in Browser" => {
                                                if let Some(d) = app
                                                    .deployments
                                                    .iter()
                                                    .find(|d| d.id == menu.deployment_id)
                                                {
                                                    let url = format!("https://{}", d.domain);
                                                    let _ = webbrowser::open(&url);
                                                }
                                            }
                                            "Redeploy" => {
                                                app.confirmation_mode =
                                                    ConfirmationState::RedeployPending(
                                                        menu.deployment_id.clone(),
                                                        std::time::Instant::now(),
                                                    );
                                            }
                                            "Kill" => {
                                                // Only allow if building
                                                if let Some(d) = app
                                                    .deployments
                                                    .iter()
                                                    .find(|d| d.id == menu.deployment_id)
                                                {
                                                    if matches!(d.status, network::Status::Building)
                                                    {
                                                        app.confirmation_mode =
                                                            ConfirmationState::CancelPending(
                                                                menu.deployment_id.clone(),
                                                                std::time::Instant::now(),
                                                            );
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                        app.context_menu = None; // Close after action
                                    } else {
                                        // Clicked outside
                                        app.context_menu = None;
                                    }
                                }
                                _ => {}
                            }
                            // Swallow interactions when menu is open
                            continue;
                        }

                        match mouse_event.kind {
                            event::MouseEventKind::ScrollUp => {
                                let mx = mouse_event.column;
                                let my = mouse_event.row;

                                let in_logs = mx >= app.logs_area.x
                                    && mx < app.logs_area.x + app.logs_area.width
                                    && my >= app.logs_area.y
                                    && my < app.logs_area.y + app.logs_area.height;
                                let in_deployments = mx >= app.deployments_area.x
                                    && mx < app.deployments_area.x + app.deployments_area.width
                                    && my >= app.deployments_area.y
                                    && my < app.deployments_area.y + app.deployments_area.height;

                                if in_logs {
                                    let i = match app.log_list_state.selected() {
                                        Some(i) => {
                                            if i == 0 {
                                                0
                                            } else {
                                                i - 1
                                            }
                                        }
                                        None => app.logs.len().saturating_sub(1),
                                    };
                                    app.log_list_state.select(Some(i));
                                } else if in_deployments {
                                    let i = match app._list_state.selected() {
                                        Some(i) => {
                                            if i == 0 {
                                                app.filtered_deployments.len().saturating_sub(1)
                                            } else {
                                                i - 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    app._list_state.select(Some(i));
                                } else {
                                    // Fallback to active pane
                                    match app.active_pane {
                                        ActivePane::Deployments => {
                                            let i = match app._list_state.selected() {
                                                Some(i) => {
                                                    if i == 0 {
                                                        app.filtered_deployments
                                                            .len()
                                                            .saturating_sub(1)
                                                    } else {
                                                        i - 1
                                                    }
                                                }
                                                None => 0,
                                            };
                                            app._list_state.select(Some(i));
                                        }
                                        ActivePane::Logs => {
                                            let i = match app.log_list_state.selected() {
                                                Some(i) => {
                                                    if i == 0 {
                                                        0
                                                    } else {
                                                        i - 1
                                                    }
                                                }
                                                None => app.logs.len().saturating_sub(1),
                                            };
                                            app.log_list_state.select(Some(i));
                                        }
                                    }
                                }
                            }
                            event::MouseEventKind::ScrollDown => {
                                let mx = mouse_event.column;
                                let my = mouse_event.row;

                                let in_logs = mx >= app.logs_area.x
                                    && mx < app.logs_area.x + app.logs_area.width
                                    && my >= app.logs_area.y
                                    && my < app.logs_area.y + app.logs_area.height;
                                let in_deployments = mx >= app.deployments_area.x
                                    && mx < app.deployments_area.x + app.deployments_area.width
                                    && my >= app.deployments_area.y
                                    && my < app.deployments_area.y + app.deployments_area.height;

                                if in_logs {
                                    let i = match app.log_list_state.selected() {
                                        Some(i) => {
                                            if i >= app.logs.len().saturating_sub(1) {
                                                app.logs.len().saturating_sub(1)
                                            } else {
                                                i + 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    app.log_list_state.select(Some(i));
                                } else if in_deployments {
                                    let i = match app._list_state.selected() {
                                        Some(i) => {
                                            if i >= app.filtered_deployments.len().saturating_sub(1)
                                            {
                                                0
                                            } else {
                                                i + 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    app._list_state.select(Some(i));
                                } else {
                                    match app.active_pane {
                                        ActivePane::Deployments => {
                                            let i = match app._list_state.selected() {
                                                Some(i) => {
                                                    if i >= app
                                                        .filtered_deployments
                                                        .len()
                                                        .saturating_sub(1)
                                                    {
                                                        0
                                                    } else {
                                                        i + 1
                                                    }
                                                }
                                                None => 0,
                                            };
                                            app._list_state.select(Some(i));
                                        }
                                        ActivePane::Logs => {
                                            let i = match app.log_list_state.selected() {
                                                Some(i) => {
                                                    if i >= app.logs.len().saturating_sub(1) {
                                                        app.logs.len().saturating_sub(1)
                                                    } else {
                                                        i + 1
                                                    }
                                                }
                                                None => 0,
                                            };
                                            app.log_list_state.select(Some(i));
                                        }
                                    }
                                }
                            }
                            event::MouseEventKind::Down(event::MouseButton::Left) => {
                                // Hit testing for Deployments
                                let mx = mouse_event.column;
                                let my = mouse_event.row;

                                // Check if inside deployments area
                                let r = app.deployments_area;
                                if mx >= r.x
                                    && mx < r.x + r.width
                                    && my >= r.y
                                    && my < r.y + r.height
                                {
                                    let row_in_list = my.saturating_sub(r.y);
                                    let item_stride = 6;
                                    if row_in_list > 0 {
                                        let clicked_offset = (row_in_list as usize) / item_stride;
                                        let target_index = app.scroll_offset + clicked_offset;

                                        if target_index < app.filtered_deployments.len() {
                                            app._list_state.select(Some(target_index));
                                            app.active_pane = ActivePane::Deployments;

                                            // Double Click Detection
                                            let now = std::time::Instant::now();
                                            if let Some((last_time, lx, ly)) = app.last_click {
                                                if now.duration_since(last_time)
                                                    < Duration::from_millis(500)
                                                    && lx == mx
                                                    && ly == my
                                                {
                                                    // Double Click Action: Open Browser
                                                    if let Some(d) =
                                                        app.filtered_deployments.get(target_index)
                                                    {
                                                        let url = format!("https://{}", d.domain);
                                                        let _ = webbrowser::open(&url);
                                                    }
                                                    app.last_click = None; // Reset
                                                } else {
                                                    app.last_click = Some((now, mx, my));
                                                }
                                            } else {
                                                app.last_click = Some((now, mx, my));
                                            }
                                        }
                                    }
                                } else {
                                    app.last_click = None;
                                    app.context_menu = None; // Click outside closes menu
                                    app.confirmation_mode = ConfirmationState::None;
                                }
                            }
                            event::MouseEventKind::Down(event::MouseButton::Right) => {
                                // Right Click -> Context Menu
                                let mx = mouse_event.column;
                                let my = mouse_event.row;
                                let r = app.deployments_area;

                                if mx >= r.x
                                    && mx < r.x + r.width
                                    && my >= r.y
                                    && my < r.y + r.height
                                {
                                    let row_in_list = my.saturating_sub(r.y);
                                    let item_stride = 6;
                                    if row_in_list > 0 {
                                        let clicked_offset = (row_in_list as usize) / item_stride;
                                        let target_index = app.scroll_offset + clicked_offset;

                                        if target_index < app.filtered_deployments.len() {
                                            // Select item
                                            app._list_state.select(Some(target_index));
                                            app.active_pane = ActivePane::Deployments;

                                            let d = &app.filtered_deployments[target_index];

                                            // Open Menu
                                            app.context_menu = Some(ContextMenu {
                                                position: (mx, my),
                                                deployment_id: d.id.clone(),
                                                selected_index: 0,
                                                options: vec![
                                                    "Open in Browser".to_string(),
                                                    "Redeploy".to_string(),
                                                    "Kill".to_string(),
                                                ],
                                            });
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Event::Key(key) => {
                    // --- Filter Mode (Traps Focus) ---
                    if app.is_filter_mode {
                        match key.code {
                            KeyCode::Esc => {
                                app.is_filter_mode = false;
                                app.filter_query.clear();
                                app.update_filter();
                                app._list_state.select(Some(0)); // Explicit reset
                            }
                            KeyCode::Enter => {
                                app.is_filter_mode = false;
                                // Keep query active
                            }
                            KeyCode::Backspace => {
                                app.filter_query.pop();
                                app.update_filter();
                                app._list_state.select(Some(0)); // Explicit reset
                            }
                            KeyCode::Char(c) => {
                                app.filter_query.push(c);
                                app.update_filter();
                                app._list_state.select(Some(0)); // Explicit reset
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Handle Context Menu Keys
                    if let Some(menu) = &mut app.context_menu {
                        match key.code {
                            KeyCode::Esc => app.context_menu = None,
                            KeyCode::Up | KeyCode::Char('k') => {
                                if menu.selected_index > 0 {
                                    menu.selected_index -= 1;
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if menu.selected_index < menu.options.len() - 1 {
                                    menu.selected_index += 1;
                                }
                            }
                            KeyCode::Enter => {
                                // Process selected context menu option
                                let option = &menu.options[menu.selected_index];
                                let id = menu.deployment_id.clone();
                                match option.as_str() {
                                    "Open in Browser" => {
                                        if let Some(d) = app.deployments.iter().find(|d| d.id == id)
                                        {
                                            let url = format!("https://{}", d.domain);
                                            let _ = webbrowser::open(&url);
                                        }
                                    }
                                    "Redeploy" => {
                                        app.confirmation_mode = ConfirmationState::RedeployPending(
                                            id,
                                            std::time::Instant::now(),
                                        );
                                    }
                                    "Kill" => {
                                        if let Some(d) = app.deployments.iter().find(|d| d.id == id)
                                        {
                                            if matches!(d.status, network::Status::Building) {
                                                app.confirmation_mode =
                                                    ConfirmationState::CancelPending(
                                                        d.id.clone(),
                                                        std::time::Instant::now(),
                                                    );
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                                app.context_menu = None;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // --- Overlay Modes (Traps Focus) ---
                    if app.show_theme_selector {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('t') => {
                                app.show_theme_selector = false
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let len = crate::theme::Theme::all().len();
                                let i = match app.theme_list_state.selected() {
                                    Some(i) => {
                                        if i == 0 {
                                            len - 1
                                        } else {
                                            i - 1
                                        }
                                    }
                                    None => 0,
                                };
                                app.theme_list_state.select(Some(i));
                                if let Some(theme) = crate::theme::Theme::from_index(i) {
                                    app.current_theme = theme;
                                    app.save_config();
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let len = crate::theme::Theme::all().len();
                                let i = match app.theme_list_state.selected() {
                                    Some(i) => {
                                        if i >= len - 1 {
                                            0
                                        } else {
                                            i + 1
                                        }
                                    }
                                    None => 0,
                                };
                                app.theme_list_state.select(Some(i));
                                if let Some(theme) = crate::theme::Theme::from_index(i) {
                                    app.current_theme = theme;
                                    app.save_config();
                                }
                            }
                            KeyCode::Char('x') => {
                                app.is_transparent = !app.is_transparent;
                                app.save_config();
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.show_project_selector {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('p') => app.show_project_selector = false,
                            KeyCode::Enter => {
                                if let Some(i) = app.project_list_state.selected() {
                                    if i < app.projects.len() {
                                        let p = &app.projects[i];
                                        app.current_project = p.name.clone();
                                        app.current_project_id = Some(p.id.clone());
                                        // Force switch to "All" time range
                                        app.stat_period = app::StatPeriod::All;

                                        // CLEAR DATA IMMEDIATELY
                                        app.deployments.clear();
                                        app.filtered_deployments.clear();
                                        app.logs.clear();
                                        app._list_state.select(None);

                                        app.save_config();
                                        // Trigger fetch
                                        let _ = cmd_tx
                                            .send(network::NetworkCommand::Deployments(Some(
                                                p.id.clone(),
                                            )))
                                            .await;
                                    }
                                }
                                app.show_project_selector = false;
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let len = app.projects.len();
                                if len > 0 {
                                    let i = match app.project_list_state.selected() {
                                        Some(i) => {
                                            if i == 0 {
                                                len - 1
                                            } else {
                                                i - 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    app.project_list_state.select(Some(i));
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let len = app.projects.len();
                                if len > 0 {
                                    let i = match app.project_list_state.selected() {
                                        Some(i) => {
                                            if i >= len - 1 {
                                                0
                                            } else {
                                                i + 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    app.project_list_state.select(Some(i));
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Global Clear Error & Confirmation
                    if key.code == KeyCode::Esc {
                        if app.error_message.is_some() {
                            app.error_message = None;
                        }
                        app.confirmation_mode = ConfirmationState::None;
                        continue;
                    }

                    // --- Main Navigation & Global Commands ---
                    match key.code {
                        KeyCode::Right | KeyCode::Char('l') => {
                            app.active_pane = ActivePane::Logs;
                        }
                        KeyCode::Left | KeyCode::Char('h') => {
                            app.active_pane = ActivePane::Deployments;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            // Check if 'k' is for Kill Confirmation
                            if key.code == KeyCode::Char('k')
                                && app.active_pane == ActivePane::Deployments
                            {
                                if let Some(i) = app._list_state.selected() {
                                    if let Some(d) = app.filtered_deployments.get(i) {
                                        if matches!(d.status, network::Status::Building) {
                                            // Check confirmation
                                            if let ConfirmationState::CancelPending(target_id, _) =
                                                &app.confirmation_mode
                                            {
                                                if &d.id == target_id {
                                                    // CONFIRMED
                                                    let _ = cmd_tx
                                                        .send(network::NetworkCommand::Cancel(
                                                            d.id.clone(),
                                                        ))
                                                        .await;
                                                    app.confirmation_mode = ConfirmationState::None;
                                                    continue;
                                                }
                                            }
                                            // Pending
                                            app.confirmation_mode =
                                                ConfirmationState::CancelPending(
                                                    d.id.clone(),
                                                    std::time::Instant::now(),
                                                );
                                            continue;
                                        }
                                    }
                                }
                            }

                            match app.active_pane {
                                ActivePane::Deployments => {
                                    let i = match app._list_state.selected() {
                                        Some(i) => {
                                            if i == 0 {
                                                app.filtered_deployments.len().saturating_sub(1)
                                            } else {
                                                i - 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    app._list_state.select(Some(i));
                                }
                                ActivePane::Logs => {
                                    if !app.logs.is_empty() {
                                        let i = match app.log_list_state.selected() {
                                            Some(i) => {
                                                if i == 0 {
                                                    0
                                                } else {
                                                    i - 1
                                                }
                                            }
                                            None => app.logs.len().saturating_sub(1),
                                        };
                                        app.log_list_state.select(Some(i));
                                    }
                                }
                            }
                            // Reset confirmation if navigating
                            app.confirmation_mode = ConfirmationState::None;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            match app.active_pane {
                                ActivePane::Deployments => {
                                    let i = match app._list_state.selected() {
                                        Some(i) => {
                                            if i >= app.filtered_deployments.len().saturating_sub(1)
                                            {
                                                0
                                            } else {
                                                i + 1
                                            }
                                        }
                                        None => 0,
                                    };
                                    app._list_state.select(Some(i));
                                }
                                ActivePane::Logs => {
                                    if !app.logs.is_empty() {
                                        let i = match app.log_list_state.selected() {
                                            Some(i) => {
                                                if i >= app.logs.len().saturating_sub(1) {
                                                    app.logs.len().saturating_sub(1)
                                                } else {
                                                    i + 1
                                                }
                                            }
                                            None => 0,
                                        };
                                        app.log_list_state.select(Some(i));
                                    }
                                }
                            }
                            // Reset confirmation if navigating
                            app.confirmation_mode = ConfirmationState::None;
                        }
                        KeyCode::Char('g') => {
                            // Top
                            match app.active_pane {
                                ActivePane::Deployments => app._list_state.select(Some(0)),
                                ActivePane::Logs => app.log_list_state.select(Some(0)),
                            }
                        }
                        KeyCode::Char('G') => {
                            // Bottom
                            match app.active_pane {
                                ActivePane::Deployments => app
                                    ._list_state
                                    .select(Some(app.filtered_deployments.len().saturating_sub(1))),
                                ActivePane::Logs => app
                                    .log_list_state
                                    .select(Some(app.logs.len().saturating_sub(1))),
                            }
                        }

                        KeyCode::Enter => {
                            if let Some(i) = app._list_state.selected() {
                                if i < app.filtered_deployments.len() {
                                    app.logs.clear();
                                    app.is_loading_logs = true;
                                    let id = app.filtered_deployments[i].id.clone();
                                    let _ = cmd_tx.send(network::NetworkCommand::Logs(id)).await;
                                    // app.active_pane = ActivePane::Logs; // Optional: switch focus
                                    app.log_list_state.select(None);
                                }
                            }
                        }

                        // --- Actions ---
                        KeyCode::Char('r') => {
                            if app.active_pane == ActivePane::Deployments {
                                if let Some(i) = app._list_state.selected() {
                                    if let Some(d) = app.filtered_deployments.get(i) {
                                        // Check confirmation
                                        if let ConfirmationState::RedeployPending(target_id, _) =
                                            &app.confirmation_mode
                                        {
                                            if &d.id == target_id {
                                                // CONFIRMED
                                                let _ = cmd_tx
                                                    .send(network::NetworkCommand::Redeploy(
                                                        d.id.clone(),
                                                    ))
                                                    .await;
                                                app.confirmation_mode = ConfirmationState::None;
                                                continue;
                                            }
                                        }
                                        // Pending
                                        app.confirmation_mode = ConfirmationState::RedeployPending(
                                            d.id.clone(),
                                            std::time::Instant::now(),
                                        );
                                    }
                                }
                            }
                        }

                        // --- Command Mode Logic ---
                        KeyCode::Char(' ') => {
                            app.show_legend = !app.show_legend;
                        }

                        // --- Global Action Keys (Ungated) ---
                        KeyCode::Char('q') => app.should_quit = true,
                        KeyCode::Char('t') => {
                            app.show_theme_selector = true;
                            app.theme_list_state
                                .select(Some(app.current_theme.as_index()));
                            app.show_legend = false;
                        }
                        KeyCode::Char('s') => {
                            app.stat_period = app.stat_period.next();
                            app.save_config();

                            // Re-apply filter with new period
                            let current_id = app.get_selected_deployment_id();
                            app.update_filter();
                            app.select_deployment_by_id(current_id);
                        }
                        KeyCode::Char('p') => {
                            app.show_project_selector = true;
                            if app.projects.is_empty() {
                                let _ = cmd_tx.send(network::NetworkCommand::Projects).await;
                            }
                            app.project_list_state.select(Some(0));
                            app.show_legend = false;
                        }
                        KeyCode::Char('/') => {
                            app.is_filter_mode = true;
                            app.active_pane = ActivePane::Deployments;
                            // Don't clear query, allow refinement. Esc clears it.
                            app.show_legend = false;
                        }
                        KeyCode::Char('o') => {
                            if let Some(i) = app._list_state.selected() {
                                if let Some(d) = app.filtered_deployments.get(i) {
                                    let url = format!("https://{}", d.domain);
                                    let _ = webbrowser::open(&url);
                                }
                            }
                            app.show_legend = false;
                        }
                        KeyCode::Char('m') => {
                            app.enable_mouse = !app.enable_mouse;
                            app.save_config();
                        }
                        _ => {}
                    } // End match key
                } // End Event::Key
                _ => {}
            }
        }

        // Auto-fetch logs on selection change logic (Debounce)
        if let Some(i) = app._list_state.selected() {
            if i != last_selected_index && i < app.filtered_deployments.len() {
                last_selected_index = i;
                app.logs.clear();
                app.is_loading_logs = true;
                // Set debounce timer
                log_debounce_timer = Some(time::Instant::now() + Duration::from_millis(400));
            }
        }

        // Handle Debounce Timer
        if let Some(deadline) = log_debounce_timer {
            if time::Instant::now() >= deadline {
                if let Some(i) = app._list_state.selected() {
                    if i < app.filtered_deployments.len() {
                        let id = app.filtered_deployments[i].id.clone();
                        let _ = cmd_tx
                            .send(network::NetworkCommand::StartStream(id.clone()))
                            .await;
                        let _ = cmd_tx.send(network::NetworkCommand::Logs(id)).await;
                    }
                }
                log_debounce_timer = None;
            }
        }

        // Clear Toast Message after 4 seconds
        if let Some((_, _, time)) = app.toast_message {
            if time.elapsed() > Duration::from_secs(4) {
                app.toast_message = None;
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = time::Instant::now();
        }

        // Add app.update_stats() before terminal.draw() in the main loop.
        // Assuming terminal.draw() would be called after all event processing and on_tick.
        // This placement is a best guess given the provided snippet does not contain terminal.draw().
        app.update_stats();

        // Handle Network Events
        while let Ok(event) = rx.try_recv() {
            match event {
                NetworkEvent::Deployments(deployments) => {
                    app.error_message = None;

                    // Capture current selection
                    let current_id = app.get_selected_deployment_id();

                    app.deployments = deployments;

                    // Re-apply filter on new data to ensure list consistency
                    app.update_filter();

                    // Restore selection by ID
                    app.select_deployment_by_id(current_id);
                }
                NetworkEvent::Projects(projects) => {
                    app.projects = projects;
                }
                NetworkEvent::Logs(id, logs) => {
                    // Check if the log belongs to currently selected item
                    if let Some(i) = app._list_state.selected() {
                        if i < app.filtered_deployments.len()
                            && app.filtered_deployments[i].id == id
                        {
                            app.logs = logs;
                            app.is_loading_logs = false;
                        }
                    }
                }
                NetworkEvent::LogChunk(id, new_lines) => {
                    if let Some(i) = app._list_state.selected() {
                        if i < app.filtered_deployments.len()
                            && app.filtered_deployments[i].id == id
                        {
                            // Deduplication is now handled in network.rs
                            app.logs.extend(new_lines);
                            // Auto-scroll logic could go here
                        }
                    }
                }
                NetworkEvent::Info(msg) => {
                    app.toast_message = Some((msg, Color::Green, std::time::Instant::now()));
                    app.error_message = None;
                }
                NetworkEvent::Error(msg) => {
                    app.error_message = Some(msg);
                    app.is_loading_logs = false;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    // Handled by TerminalGuard
    Ok(())
}
