use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph},
    Frame,
};

use crate::app::{ActivePane, App};
use crate::network::Status;
use crate::theme::ThemeColors;

// --- MAIN DRAW ---
pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    // Check for minimum size (80x28)
    if size.width < 80 || size.height < 28 {
        draw_size_warning(f, size, app);
        return;
    }

    let colors = app.current_theme.get_colors();

    // Global Background
    // If transparent, we don't render a background block, or render Reset
    if !app.is_transparent {
        let bg_block = Block::default().bg(colors.bg);
        f.render_widget(bg_block, f.area());
    }

    // Main Layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Body
        ])
        .margin(1) // Global padding
        .split(f.area());

    draw_header(f, chunks[0], app, &colors);
    draw_body(f, chunks[1], app, &colors);

    // Theme Selector Overlay
    if app.show_theme_selector {
        draw_theme_selector(f, app, &colors);
    }

    // Project Selector Overlay
    if app.show_project_selector {
        draw_project_selector(f, app, &colors);
    }

    // Error Overlay
    if let Some(err) = &app.error_message {
        draw_error_overlay(f, err, &colors);
    }

    // Context Menu Overlay
    if app.context_menu.is_some() {
        draw_context_menu(f, app, &colors);
    }

    // Confirmation Toast (Render top-center)
    match &app.confirmation_mode {
        crate::app::ConfirmationState::RedeployPending(_, _) => {
            draw_toast(
                f,
                "Press 'r' again to CONFIRM Redeploy",
                colors.status_building,
            );
        }
        crate::app::ConfirmationState::CancelPending(_, _) => {
            draw_toast(f, "Press 'k' again to CONFIRM Cancel", colors.status_error);
        }
        _ => {}
    }

    // Generic Success/Info Toast
    if let Some((msg, color, _)) = &app.toast_message {
        // Only draw if we aren't showing a confirmation toast (avoid overlap)
        if app.confirmation_mode == crate::app::ConfirmationState::None {
            draw_toast(f, msg, *color);
        }
    }

    // Key Legend Overlay
    if app.show_legend {
        let area = f.area();
        let height = 3;
        let legend_area = Rect::new(
            area.x,
            area.height.saturating_sub(height),
            area.width,
            height,
        );
        draw_key_legend(f, legend_area, app, &colors);
    }
}

fn draw_size_warning(f: &mut Frame, area: Rect, app: &App) {
    let colors = app.current_theme.get_colors();
    let bg = if app.is_transparent {
        Color::Reset
    } else {
        colors.bg
    };

    // Clear the screen first (optional, but good if we want to hide the broken UI)
    f.render_widget(Block::default().bg(bg), area);

    // If the area is *really* small, just render a solid block to prevent panics in Paragraph
    if area.width < 20 || area.height < 5 {
        return;
    }

    let _centered = centered_rect(50, 50, area); // Rough centering, we'll refine the content size

    let text = vec![
        Line::from(vec![Span::styled(
            "Terminal size too small:",
            Style::default()
                .fg(colors.text_primary)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            format!("Width = {} Height = {}", area.width, area.height),
            Style::default().fg(colors.status_error),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Needed for current config:",
            Style::default()
                .fg(colors.text_primary)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "Width = 80 Height = 28",
            Style::default().fg(colors.status_success),
        )]),
    ];

    let p = Paragraph::new(text)
        .alignment(Alignment::Center)
        // .block(Block::default().borders(Borders::ALL).title(" Warning ").border_style(Style::default().fg(colors.status_error)))
        ;

    // Calculate a roughly centered area for the text
    let height = 6;
    let width = 40;

    let left = (area.width.saturating_sub(width)) / 2;
    let top = (area.height.saturating_sub(height)) / 2;
    let rect = Rect::new(left, top, width.min(area.width), height.min(area.height));

    f.render_widget(p, rect);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, colors: &ThemeColors) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(9),  // Title
            Constraint::Length(10), // Dots
            Constraint::Min(0),     // Breadcrumbs & Metadata
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Polymer",
        Style::default()
            .fg(colors.text_primary)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(title, layout[0]);

    // Decorative Dots
    let dots_text = Line::from(vec![
        Span::styled("● ", Style::default().fg(colors.status_error)),
        Span::styled("● ", Style::default().fg(colors.status_building)),
        Span::styled("● ", Style::default().fg(colors.status_success)),
        Span::styled("●", Style::default().fg(colors.accent_primary)),
    ]);
    f.render_widget(Paragraph::new(dots_text), layout[1]);

    // Display Github Owner if available
    let owner = if let Some(d) = app.deployments.first() {
        if let Some(idx) = d.repo.find('/') {
            &d.repo[..idx]
        } else {
            &d.creator
        }
    } else {
        "Loading..."
    };

    // Metadata (Right Aligned)
    let meta_text = vec![Line::from(vec![
        Span::styled("Github", Style::default().fg(colors.text_dim)),
        Span::styled(" • ", Style::default().fg(colors.border)),
        Span::styled(owner, Style::default().fg(colors.text_dim)),
        Span::raw("     "),
        Span::styled("Project: ", Style::default().fg(colors.text_dim)),
        Span::styled(
            &app.current_project,
            Style::default().fg(colors.text_primary),
        ),
        Span::raw("   "),
        Span::styled(&app.current_time, Style::default().fg(colors.text_dim)), // Real time
    ])];
    let meta = Paragraph::new(meta_text).alignment(Alignment::Right);
    f.render_widget(meta, layout[2]);
}

// --- BODY ---
fn draw_body(f: &mut Frame, area: Rect, app: &mut App, colors: &ThemeColors) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Left: Deployments
            Constraint::Length(1),      // Gutter
            Constraint::Percentage(50), // Right Side (Stats + Detail + Logs)
        ])
        .split(area);

    // chunks[0] is Left Side
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // List (Flex)
            Constraint::Length(4), // Domain Box (~3 lines + borders)
        ])
        .split(chunks[0]);

    draw_deployments(f, left_chunks[0], app, colors);
    draw_domain_box(f, left_chunks[1], app, colors);

    // chunks[1] is spacer
    draw_right_panel(f, chunks[2], app, colors);
}

fn draw_domain_box(f: &mut Frame, area: Rect, app: &App, colors: &ThemeColors) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.border))
        .title(" Deployment URLs ")
        .title_style(Style::default().fg(colors.text_primary))
        .padding(Padding::new(1, 1, 0, 0)); // tight padding

    let selected_index = app._list_state.selected().unwrap_or(0);
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if let Some(d) = app.filtered_deployments.get(selected_index) {
        let text = vec![
            Line::from(vec![
                Span::styled("● ", Style::default().fg(colors.accent_primary)), // Blue dot?
                Span::styled(
                    &d.domain,
                    Style::default()
                        .fg(colors.accent_primary)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    format!("  {}", d.repo),
                    Style::default().fg(colors.text_dim),
                ), // Subtext
            ]),
        ];
        // Right align "Primary" badge?
        // Left-aligned header text for consistency.
        f.render_widget(Paragraph::new(text), inner_area);

        // Render badge on right
        let badge_text = Span::styled("- Primary", Style::default().fg(colors.text_dim));
        let badge = Paragraph::new(Line::from(badge_text)).alignment(Alignment::Right);
        f.render_widget(badge, inner_area);
    }
}

// --- DEPLOYMENTS ---
fn draw_deployments(f: &mut Frame, area: Rect, app: &mut App, colors: &ThemeColors) {
    // Save area for mouse interaction
    app.deployments_area = area;

    // Define Block
    let border_color = if app.active_pane == ActivePane::Deployments && !app.is_filter_mode {
        colors.accent_primary
    } else {
        colors.border
    };
    // Title update if filtering
    let title_text = if !app.filter_query.is_empty() || app.is_filter_mode {
        format!(" Deployments (Filter: {}) ", app.filter_query)
    } else {
        " Deployments ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(title_text, Style::default().fg(border_color)))
        .title_alignment(Alignment::Left)
        .padding(Padding::new(1, 1, 0, 1));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Layout: Search Bar (Conditional) + List
    let (search_area, list_area) = if app.is_filter_mode || !app.filter_query.is_empty() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search Input
                Constraint::Min(0),    // List
            ])
            .split(inner_area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, inner_area)
    };

    // Render Search Bar
    if let Some(r) = search_area {
        let border_style = if app.is_filter_mode {
            Style::default().fg(colors.accent_primary)
        } else {
            Style::default().fg(colors.border)
        };
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Filter Branch (Enter/Esc to close) ")
            .border_style(border_style);

        let query_text = format!("{}█", app.filter_query); // Cursor emulation
        let input = Paragraph::new(query_text)
            .style(Style::default().fg(colors.text_primary))
            .block(input_block);

        f.render_widget(input, r);
    }

    // --- LIST RENDERING using filtered_deployments ---
    let deployments = &app.filtered_deployments;

    // Virtual Scrolling
    let visible_height = list_area.height as usize;
    let total_items = deployments.len();

    // Deployment list item rendering logic
    let item_height = 6;

    let visible_items = (visible_height / item_height).max(1);

    let selected_index = app._list_state.selected().unwrap_or(0);

    // Ensure scroll_offset keeps selected item in view
    if selected_index >= app.scroll_offset + visible_items {
        app.scroll_offset = selected_index + 1 - visible_items;
    }
    if selected_index < app.scroll_offset {
        app.scroll_offset = selected_index;
    }

    // Draw List
    let mut current_y = list_area.y;

    for i in app.scroll_offset..core::cmp::min(app.scroll_offset + visible_items + 1, total_items) {
        if i >= deployments.len() {
            break;
        }
        let d = &deployments[i];

        if current_y >= list_area.y + list_area.height {
            break;
        }

        let is_selected = i == selected_index;

        // Colors
        let name_color = if is_selected && app.is_transparent {
            Color::White
        } else if is_selected {
            colors.accent_primary
        } else {
            colors.text_primary
        };
        let dim_color = if is_selected && app.is_transparent {
            Color::White
        } else {
            colors.text_dim
        };
        let status_color = match d.status {
            Status::Ready => colors.status_success,
            Status::Error => colors.status_error,
            Status::Building => colors.status_building,
            Status::Canceled => colors.text_dim,
            Status::Initializing => colors.status_building, // Reuse building color
        };
        let (dot_icon, status_label) = match d.status {
            Status::Ready => ("●", "Successful"),
            Status::Error => ("●", "Failed"),
            Status::Building => ("⠋", "Building"),
            Status::Canceled => ("○", "Canceled"),
            Status::Initializing => ("⧖", "Initializing"),
        };

        let final_dot_icon =
            if matches!(d.status, Status::Building) || matches!(d.status, Status::Initializing) {
                let frames = ["⠖", "⠲", "⠴", "⠦"];
                frames[app.spinner_frame % frames.len()]
            } else {
                dot_icon
            };

        // Calculate Spacing
        let content_width = inner_area.width.saturating_sub(0);

        // Badge Logic
        let (badge_text, badge_color) = if d.target == "production" {
            ("/prod", colors.accent_primary) // Blue/Cyan
        } else {
            ("/prev", colors.text_dim)
        };

        let left_len = 2 + 2 + d.short_id.len() + 1 + badge_text.len(); // Icon + Space + ID + Space + Badge
        let right_len = 2 + status_label.len();
        let spacer_len = (content_width as usize).saturating_sub(left_len + right_len);

        // Highlight Background
        let highlight_bg = if is_selected {
            if app.is_transparent {
                colors.text_dim
            } else {
                colors.border
            }
        } else if app.is_transparent {
            Color::Reset
        } else {
            colors.bg
        };

        // Render 5 Lines of Content
        for line_idx in 0..5 {
            if current_y >= list_area.y + list_area.height {
                break;
            }
            let area_line = Rect::new(list_area.x, current_y, list_area.width, 1);

            // Fill background for the line
            f.render_widget(Block::default().bg(highlight_bg), area_line);

            // Render Content on top
            let content = match line_idx {
                1 => {
                    // Line 1
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("{} ", final_dot_icon),
                            Style::default().fg(status_color),
                        ),
                        Span::styled(
                            &d.short_id,
                            Style::default().fg(name_color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(badge_text, Style::default().fg(badge_color)),
                        Span::raw(" ".repeat(spacer_len)),
                        Span::styled(status_label, Style::default().fg(status_color)),
                        Span::raw("  "),
                    ])
                }
                3 => {
                    // Line 2
                    let padding_left = 4; // "    "
                    let padding_right = 2; // "  "
                    let min_gap = 4; // Increased gap for better spacing
                    let time_len = d.time.len();

                    let available_width = inner_area.width as usize;
                    let max_msg_len = available_width
                        .saturating_sub(padding_left + padding_right + time_len + min_gap)
                        .max(5);
                    let msg_truncated = truncate(&d.commit_msg, max_msg_len);

                    let used_len =
                        padding_left + msg_truncated.chars().count() + time_len + padding_right;
                    let spacer_len = available_width.saturating_sub(used_len);

                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(msg_truncated, Style::default().fg(dim_color)),
                        Span::raw(" ".repeat(spacer_len)),
                        Span::styled(&d.time, Style::default().fg(dim_color)),
                        Span::raw("  "),
                    ])
                }
                _ => Line::from(""), // Empty padding lines
            };
            f.render_widget(Paragraph::new(content), area_line);
            current_y += 1;
        }

        // Render Separator (Not Selected, Not Backgrounded)
        if i < deployments.len() - 1 && current_y < list_area.y + list_area.height {
            let sep_area = Rect::new(list_area.x, current_y, list_area.width, 1);
            // Explicitly use app bg to "cut" any bleed, or just transparent
            let sep_bg = if app.is_transparent {
                Color::Reset
            } else {
                colors.bg
            };
            let separator_char = "─";
            let separator = Line::from(Span::styled(
                separator_char.repeat(inner_area.width as usize),
                Style::default()
                    .fg(colors.border)
                    .bg(sep_bg)
                    .add_modifier(Modifier::DIM),
            ));
            f.render_widget(Paragraph::new(separator), sep_area);
            current_y += 1;
        }
    }
}

// --- RIGHT PANEL ---
fn draw_right_panel(f: &mut Frame, area: Rect, app: &mut App, colors: &ThemeColors) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Stats Banner (Expanded for spacing)
            Constraint::Min(0),     // Split: Details (Top) + Logs (Bottom)
        ])
        .split(area);

    draw_build_stats(f, app, colors, chunks[0]);

    let bottom_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30), // Details
            Constraint::Percentage(70), // Logs (Larger)
        ])
        .split(chunks[1]);

    draw_details(f, app, colors, bottom_chunks[0]);
    draw_logs(f, app, bottom_chunks[1], colors);
}

// --- BUILD STATS ---
fn draw_build_stats(f: &mut Frame, app: &mut App, colors: &ThemeColors, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.border))
        .title(Span::styled(
            " Build Overview ",
            Style::default().fg(colors.accent_primary),
        ))
        .padding(Padding::new(1, 1, 0, 0));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Vertically Center Content (Height 10 -> Inner 8. Content is ~6. 1 top, 1 bottom padding basically)
    let v_center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Top Spacer
            Constraint::Length(6), // Content (2 rows x 3 lines)
            Constraint::Min(1),    // Bottom Spacer
        ])
        .split(inner_area);

    let content_area = v_center[1];

    // Split into 2 Rows
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(content_area);

    // Split Row 1
    let row1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[0]);

    // Split Row 2
    let row2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[1]);

    // Row 1 Metrics
    let count_str = if app.total_builds == 100 && app.deployments.len() >= 100 {
        "100+".to_string()
    } else {
        app.total_builds.to_string()
    };

    render_metric(
        f,
        row1[0],
        "Total Builds",
        &count_str,
        colors.text_primary,
        colors,
    );
    render_metric(
        f,
        row1[1],
        "Success Rate",
        &format!("{}%", app.success_rate),
        colors.status_success,
        colors,
    );
    render_metric(
        f,
        row1[2],
        "Avg Duration",
        &format!("{}s", app.avg_duration_s),
        colors.accent_primary,
        colors,
    );

    // Row 2 Metrics
    render_metric(
        f,
        row2[0],
        "Active Jobs",
        &app.active_builds.to_string(),
        colors.status_building,
        colors,
    );
    render_metric(
        f,
        row2[1],
        "Failed",
        &app.error_count.to_string(),
        colors.status_error,
        colors,
    );
    render_metric(
        f,
        row2[2],
        "Time Range",
        app.stat_period.display_text(),
        colors.text_primary,
        colors,
    );
}

// ... render_metric ... (unchanged, but included in block usually if logic changed, here logic is same)

fn render_metric(
    f: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    color: Color,
    colors: &ThemeColors,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(area);

    f.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(Style::default().fg(colors.text_dim)),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new(value)
            .alignment(Alignment::Center)
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
        chunks[1],
    );
}

// --- DEPLOYMENT DETAILS ---
fn draw_details(f: &mut Frame, app: &mut App, colors: &ThemeColors, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.border))
        .title(Span::styled(
            " Details ",
            Style::default().fg(colors.accent_primary),
        ))
        .padding(Padding::new(2, 2, 1, 1));

    let selected_index = app._list_state.selected().unwrap_or(0);

    if let Some(d) = app.filtered_deployments.get(selected_index) {
        // Calculate max width for content: Area width - Padding (4) - Label ("Commit: " ~8) - Safety (2)
        let max_len = (area.width as usize).saturating_sub(14).max(10);

        let text = vec![
            Line::from(vec![
                Span::styled("Project: ", Style::default().fg(colors.text_dim)),
                Span::styled(
                    &d.name,
                    Style::default()
                        .fg(colors.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Commit: ", Style::default().fg(colors.text_dim)),
                Span::styled(
                    truncate(&d.commit_msg, max_len),
                    Style::default().fg(colors.text_primary),
                ),
            ]),
            Line::from(vec![
                Span::styled("Branch: ", Style::default().fg(colors.text_dim)),
                Span::styled(
                    truncate(&d.branch, max_len),
                    Style::default().fg(colors.accent_primary),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(colors.text_dim)),
                match d.status {
                    Status::Ready => {
                        Span::styled("Successful", Style::default().fg(colors.status_success))
                    }
                    Status::Error => {
                        Span::styled("Failed", Style::default().fg(colors.status_error))
                    }
                    Status::Building => {
                        let frames = ["⠖", "⠲", "⠴", "⠦"];
                        let spinner = frames[app.spinner_frame % frames.len()];
                        Span::styled(
                            format!("{} Building...", spinner),
                            Style::default().fg(colors.status_building),
                        )
                    }
                    Status::Canceled => {
                        Span::styled("Canceled", Style::default().fg(colors.text_dim))
                    }
                    Status::Initializing => {
                        let frames = ["⠖", "⠲", "⠴", "⠦"];
                        let spinner = frames[app.spinner_frame % frames.len()];
                        Span::styled(
                            format!("{} Initializing...", spinner),
                            Style::default().fg(colors.status_building),
                        )
                    }
                },
            ]),
            Line::from(vec![
                Span::styled("Duration: ", Style::default().fg(colors.text_dim)),
                {
                    let duration_s = if matches!(d.status, Status::Building)
                        || matches!(d.status, Status::Initializing)
                    {
                        let now = chrono::Utc::now().timestamp_millis() as u64;
                        now.saturating_sub(d.timestamp) / 1000
                    } else {
                        d.duration_ms / 1000
                    };
                    Span::styled(
                        format!("{}s", duration_s),
                        Style::default().fg(colors.text_primary),
                    )
                },
            ]),
        ];

        f.render_widget(Paragraph::new(text).block(block), area);
    } else {
        f.render_widget(
            Block::default().borders(Borders::ALL).title(" Details "),
            area,
        );
    }
}

// --- LOGS ---
fn draw_logs(f: &mut Frame, app: &mut App, area: Rect, colors: &ThemeColors) {
    let border_color = if app.active_pane == ActivePane::Logs {
        colors.accent_primary
    } else {
        colors.border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" Build Logs ")
        .title_style(Style::default().fg(colors.text_primary))
        .padding(Padding::new(1, 1, 1, 1));

    let inner = block.inner(area);
    app.logs_area = inner;
    f.render_widget(block, area);

    if app.is_loading_logs {
        let frames = ["⠖", "⠲", "⠴", "⠦"];
        let spinner = frames[app.spinner_frame % frames.len()];
        f.render_widget(
            Paragraph::new(format!("{} Loading logs...", spinner))
                .style(Style::default().fg(colors.text_dim)),
            inner,
        );
        return;
    }

    if app.logs.is_empty() {
        f.render_widget(
            Paragraph::new("No logs available").style(Style::default().fg(colors.text_dim)),
            inner,
        );
        return;
    }

    let inner_width = inner.width.saturating_sub(4).max(10) as usize; // -2 for bullet, -2 for safety

    // Optimization: Only regex highlight visible items
    // Calculate visible window approximation
    let selected_idx = app.log_list_state.selected().unwrap_or(0);
    // Be generous with the window (e.g. 2x height) to avoid pop-in during fast scroll
    let window_height = area.height as usize * 2;
    let start_window = selected_idx.saturating_sub(window_height);
    let end_window = selected_idx.saturating_add(window_height);

    // Creates the ListItems
    let items: Vec<ListItem> = app
        .logs
        .iter()
        .enumerate()
        .map(|(idx, msg)| {
            let is_visible = idx >= start_window && idx <= end_window;
            let is_selected = Some(idx) == app.log_list_state.selected();

            // Bullet Color logic (Always needed for visual consistency)
            let dot_palette = [
                colors.accent_primary,
                colors.status_success,
                colors.status_building,
                colors.text_primary,
            ];
            let mut dot_color = dot_palette[idx % dot_palette.len()];
            let lower = msg.to_lowercase();
            if lower.contains("error") || lower.contains("fail") {
                dot_color = colors.status_error;
            }

            let wrapped_lines = wrap_text(msg, inner_width);
            let mut lines = Vec::new();

            if wrapped_lines.is_empty() {
                let mut spans = if is_visible {
                    highlight_line(msg, &app.log_regex, colors)
                } else {
                    vec![Span::styled(msg, Style::default().fg(colors.text_dim))]
                };

                // Override color if selected for contrast
                if is_selected {
                    for span in &mut spans {
                        if span.style.fg == Some(colors.text_dim)
                            || span.style.fg == Some(colors.text_primary)
                        {
                            span.style = span.style.fg(Color::White);
                        }
                    }
                }

                let mut full_spans = vec![Span::styled("● ", Style::default().fg(dot_color))];
                full_spans.extend(spans);
                lines.push(Line::from(full_spans));
            } else {
                for (i, line) in wrapped_lines.iter().enumerate() {
                    let mut spans = if is_visible {
                        highlight_line(line, &app.log_regex, colors)
                    } else {
                        vec![Span::styled(
                            line.clone(),
                            Style::default().fg(colors.text_dim),
                        )]
                    };

                    // Override color if selected for contrast
                    if is_selected {
                        for span in &mut spans {
                            if span.style.fg == Some(colors.text_dim)
                                || span.style.fg == Some(colors.text_primary)
                            {
                                span.style = span.style.fg(Color::White);
                            }
                        }
                    }

                    if i == 0 {
                        let mut full_spans =
                            vec![Span::styled("● ", Style::default().fg(dot_color))];
                        full_spans.extend(spans);
                        lines.push(Line::from(full_spans));
                    } else {
                        let mut full_spans = vec![Span::raw("  ")];
                        full_spans.extend(spans);
                        lines.push(Line::from(full_spans));
                    }
                }
            }

            ListItem::new(lines)
        })
        .collect();

    // Style logic: if transparent, use text_dim for bg (subtle), else use border color ?
    let highlight_bg = if app.is_transparent {
        colors.text_dim
    } else {
        colors.border
    };
    let highlight_style = Style::default().bg(highlight_bg).fg(Color::White);

    let list = List::new(items)
        .highlight_symbol("")
        .highlight_style(highlight_style);

    f.render_stateful_widget(list, inner, &mut app.log_list_state);
}

fn highlight_line<'a>(text: &str, regex: &regex::Regex, colors: &ThemeColors) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut last_idx = 0;

    for caps in regex.captures_iter(text) {
        if let Some(m) = caps.get(0) {
            // Push plain text before match
            if m.start() > last_idx {
                spans.push(Span::styled(
                    text[last_idx..m.start()].to_string(),
                    Style::default().fg(colors.text_dim),
                ));
            }

            // Determine color for the match
            // Capture groups:
            // 1. Keywords
            // 2. IP
            // 3. Time
            // 4. Quotes
            // 5. KV
            // 6. HTTP Methods
            // Capture Group 8: HTTP Status Codes (e.g., 200, 404, 500).
            // Regex Groups:
            // 1. Keywords
            // 2. IP
            // 3. Time
            // 4. Quotes
            // 5. KV
            // 6. HTTP (Outer)
            // 7. HTTP (Inner)
            // 8. Status Codes
            // 9. Durations
            // 10. Sizes
            // 11. Paths
            // 12. Git Hashes

            let match_str = m.as_str();
            let style = if caps.get(1).is_some() {
                // Keywords
                let lower = match_str.to_lowercase();
                if lower.contains("error") || lower.contains("fail") {
                    Style::default()
                        .fg(colors.status_error)
                        .add_modifier(Modifier::BOLD)
                } else if lower.contains("warn") {
                    Style::default()
                        .fg(colors.status_building)
                        .add_modifier(Modifier::BOLD)
                } else if lower.contains("success") || lower.contains("ready") {
                    Style::default()
                        .fg(colors.status_success)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(colors.accent_primary)
                        .add_modifier(Modifier::BOLD)
                }
            } else if caps.get(2).is_some() {
                // IP
                Style::default().fg(colors.accent_primary)
            } else if caps.get(3).is_some() {
                // Time
                Style::default().fg(colors.text_primary)
            } else if caps.get(4).is_some() {
                // Quotes
                Style::default().fg(colors.status_success)
            } else if caps.get(5).is_some() {
                // Key=Value
                Style::default().fg(colors.accent_primary)
            } else if caps.get(6).is_some() {
                // HTTP Method
                Style::default()
                    .fg(colors.status_building)
                    .add_modifier(Modifier::BOLD)
            } else if caps.get(8).is_some() {
                // Status Code
                let code: u16 = match_str.parse().unwrap_or(200);
                if code >= 500 {
                    Style::default().fg(colors.status_error)
                } else if code >= 400 {
                    Style::default().fg(colors.status_building)
                } else {
                    Style::default().fg(colors.status_success)
                }
            } else if caps.get(9).is_some() {
                // Durations
                Style::default().fg(colors.accent_primary)
            } else if caps.get(10).is_some() {
                // Sizes
                Style::default().fg(colors.accent_primary)
            } else if caps.get(11).is_some() {
                // Paths
                Style::default()
                    .fg(colors.text_primary)
                    .add_modifier(Modifier::UNDERLINED)
            } else if caps.get(12).is_some() {
                // Git Hashes
                Style::default().fg(colors.text_dim)
            } else {
                Style::default().fg(colors.text_primary)
            };

            spans.push(Span::styled(match_str.to_string(), style));
            last_idx = m.end();
        }
    }

    // Push remaining
    if last_idx < text.len() {
        spans.push(Span::styled(
            text[last_idx..].to_string(),
            Style::default().fg(colors.text_dim),
        ));
    }

    // Fallback if no matches?
    if spans.is_empty() && !text.is_empty() {
        spans.push(Span::styled(
            text.to_string(),
            Style::default().fg(colors.text_dim),
        ));
    }

    spans
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.len() + word.len() + 1 > max_width {
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
            }

            if word.len() > max_width {
                let mut char_iter = word.chars().peekable();
                while char_iter.peek().is_some() {
                    let chunk: String = char_iter.by_ref().take(max_width).collect();
                    if chunk.len() == max_width && char_iter.peek().is_some() {
                        lines.push(chunk);
                    } else {
                        current_line = chunk;
                    }
                }
            } else {
                current_line.push_str(word);
            }
        } else {
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

// --- THEME SELECTOR ---
fn draw_theme_selector(f: &mut Frame, app: &mut App, colors: &ThemeColors) {
    let area = centered_rect(60, 60, f.area());

    // Clear underlying content
    f.render_widget(Clear, area);

    let bg_color = if app.is_transparent {
        Color::Reset
    } else {
        colors.bg
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" Select Theme ")
        .style(Style::default().bg(bg_color).fg(colors.text_primary));

    f.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1), // Instructions
        ])
        .split(inner);

    let themes = crate::theme::Theme::all();
    let items: Vec<ListItem> = themes
        .iter()
        .map(|t| {
            let is_selected = *t == app.current_theme;
            let prefix = if is_selected { "> " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(colors.accent_primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors.text_primary)
            };
            ListItem::new(format!("{}{}", prefix, t.name())).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .padding(Padding::new(1, 1, 0, 0)),
        )
        .highlight_symbol("> ")
        .highlight_style(
            Style::default()
                .fg(colors.accent_primary)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, chunks[0], &mut app.theme_list_state);

    // Bottom Bar with Transparency Toggle
    let checkbox = if app.is_transparent { "[x]" } else { "[ ]" };
    let instructions = format!("{} Transp. (X) │ ↕ Select │ ↵ Close", checkbox);
    let p = Paragraph::new(instructions)
        .alignment(Alignment::Center)
        .style(Style::default().fg(colors.text_dim));
    f.render_widget(p, chunks[1]);
}

// --- PROJECT SELECTOR ---
fn draw_project_selector(f: &mut Frame, app: &mut App, colors: &ThemeColors) {
    let area = centered_rect(50, 40, f.area());
    f.render_widget(Clear, area);

    let bg_color = if app.is_transparent {
        Color::Reset
    } else {
        colors.bg
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" Select Project ")
        .style(Style::default().bg(bg_color).fg(colors.text_primary));

    f.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            let is_selected = p.name == app.current_project; // Compare name
            let prefix = if is_selected { "> " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(colors.accent_primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors.text_primary)
            };
            ListItem::new(format!("{}{}", prefix, p.name)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .padding(Padding::new(1, 1, 0, 0)),
        )
        .highlight_symbol("> ")
        .highlight_style(
            Style::default()
                .fg(colors.accent_primary)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, chunks[0], &mut app.project_list_state);

    let checkbox = if app.is_transparent { "[x]" } else { "[ ]" };
    let instructions = format!("{} Transp. (Space) │ ↕ Navigate │ ↵ Select", checkbox);
    let p = Paragraph::new(instructions)
        .alignment(Alignment::Center)
        .style(Style::default().fg(colors.text_dim));
    f.render_widget(p, chunks[1]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_error_overlay(f: &mut Frame, msg: &str, colors: &ThemeColors) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.status_error))
        .border_type(ratatui::widgets::BorderType::Double)
        .title(" Error ")
        .style(Style::default().fg(colors.text_primary).bg(colors.bg));

    let p = Paragraph::new(msg)
        .block(block)
        .wrap(ratatui::widgets::Wrap { trim: true })
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}

fn draw_key_legend(f: &mut Frame, area: Rect, app: &App, colors: &ThemeColors) {
    // Clear the area first to prevent bleed-through
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(colors.border))
        .style(Style::default().bg(colors.bg));

    f.render_widget(block, area);

    let mouse_status = if app.enable_mouse { "ON" } else { "OFF" };

    // Items to show
    let items = vec![
        ("Theme", "T"),
        ("Open in Browser", "O"),
        ("Change Timerange", "S"),
        ("Projects", "P"),
        ("Mouse Interaction", mouse_status), // Toggle M
        ("Quit", "Q"),
    ];

    let mut spans = vec![];
    for (label, key) in items {
        spans.push(Span::styled(
            format!(" {} ", label),
            Style::default().fg(colors.text_primary),
        ));
        let key_text = if label == "Mouse Interaction" {
            format!("(M [{}])", key)
        } else {
            format!("({})", key)
        };
        spans.push(Span::styled(
            key_text,
            Style::default()
                .fg(colors.accent_primary)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw("   "));
    }

    let p = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Center)
        .style(Style::default().bg(colors.bg));

    // Draw centered vertically in the area (skipping top border)
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Margin for Top Border
            Constraint::Length(1), // The Text Line
            Constraint::Min(0),    // Bottom Margin
        ])
        .split(area);

    f.render_widget(p, layout[1]);
}

fn draw_context_menu(f: &mut Frame, app: &App, colors: &ThemeColors) {
    if let Some(menu) = &app.context_menu {
        let area = Rect::new(
            menu.position.0,
            menu.position.1,
            20,
            menu.options.len() as u16 + 2,
        ); // +2 for borders

        // Ensure menu doesn't go off screen
        let f_area = f.area();
        let x = area.x.min(f_area.width.saturating_sub(area.width));
        let y = area.y.min(f_area.height.saturating_sub(area.height));
        let fixed_area = Rect::new(x, y, area.width, area.height);

        f.render_widget(Clear, fixed_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors.border))
            .bg(colors.bg);

        let items: Vec<ListItem> = menu
            .options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let style = if i == menu.selected_index {
                    Style::default().fg(colors.bg).bg(colors.accent_primary)
                } else {
                    Style::default().fg(colors.text_primary)
                };
                ListItem::new(Span::styled(format!(" {} ", opt), style))
            })
            .collect();

        let list = List::new(items).block(block);

        f.render_widget(list, fixed_area);
    }
}

fn draw_toast(f: &mut Frame, msg: &str, color: Color) {
    let area = f.area();
    let width = (msg.len() as u16) + 4;
    let height = 3;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = 1; // Top padding

    let rect = Rect::new(x, y, width, height);

    f.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .bg(Color::Reset); // Or a specific background

    let p = Paragraph::new(Span::styled(
        msg,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Center)
    .block(block);

    f.render_widget(p, rect);
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        // Ensure we don't subtract with overflow if max_chars < 3
        let len = max_chars.saturating_sub(3);
        let mut truncated: String = s.chars().take(len).collect();
        truncated.push_str("...");
        truncated
    } else {
        s.to_string()
    }
}
