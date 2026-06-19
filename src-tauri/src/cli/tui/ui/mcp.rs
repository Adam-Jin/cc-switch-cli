use super::*;

pub(super) fn mcp_rows_filtered<'a>(app: &App, data: &'a UiData) -> Vec<&'a McpRow> {
    let query = app.filter.query_lower();
    data.mcp
        .rows
        .iter()
        .filter(|row| match &query {
            None => true,
            Some(q) => {
                row.server.name.to_lowercase().contains(q) || row.id.to_lowercase().contains(q)
            }
        })
        .collect()
}

pub(super) fn render_mcp(
    frame: &mut Frame<'_>,
    app: &App,
    data: &UiData,
    area: Rect,
    theme: &super::theme::Theme,
) {
    let visible = mcp_rows_filtered(app, data);

    let header = Row::new(vec![
        Cell::from(texts::header_name()),
        Cell::from(crate::app_config::AppType::Claude.as_str()),
        Cell::from(crate::app_config::AppType::Codex.as_str()),
        Cell::from(crate::app_config::AppType::Gemini.as_str()),
        Cell::from(crate::app_config::AppType::OpenCode.as_str()),
    ])
    .style(Style::default().fg(theme.dim).add_modifier(Modifier::BOLD));

    let rows = visible.iter().map(|row| {
        let scoped = !row.server.machine_selector.is_empty();
        let active_here = row.server.machine_selector.matches(&app.machine_labels);
        let name = if !scoped {
            row.server.name.clone()
        } else if active_here {
            format!("{} ⊙", row.server.name)
        } else {
            format!("{} ⊘", row.server.name)
        };
        let mut r = Row::new(vec![
            Cell::from(name),
            Cell::from(if row.server.apps.claude {
                texts::tui_marker_active()
            } else {
                texts::tui_marker_inactive()
            }),
            Cell::from(if row.server.apps.codex {
                texts::tui_marker_active()
            } else {
                texts::tui_marker_inactive()
            }),
            Cell::from(if row.server.apps.gemini {
                texts::tui_marker_active()
            } else {
                texts::tui_marker_inactive()
            }),
            Cell::from(if row.server.apps.opencode {
                texts::tui_marker_active()
            } else {
                texts::tui_marker_inactive()
            }),
        ]);
        // 当前机器不匹配 selector：灰显整行，提示本机不生效。
        if scoped && !active_here {
            r = r.style(Style::default().fg(theme.dim));
        }
        r
    });

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(pane_border_style(app, Focus::Content, theme))
        .title(texts::menu_manage_mcp());
    frame.render_widget(outer.clone(), area);
    let inner = outer.inner(area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(inner);

    if app.focus == Focus::Content {
        render_key_bar_center(
            frame,
            chunks[0],
            theme,
            &[
                ("x", texts::tui_key_toggle()),
                ("m", texts::tui_key_apps()),
                ("s", crate::t!("scope", "作用域")),
                ("a", texts::tui_key_add()),
                ("e", texts::tui_key_edit()),
                ("i", texts::tui_mcp_action_import_existing()),
                ("d", texts::tui_key_delete()),
            ],
        );
    }

    let summary = texts::tui_mcp_server_counts(
        data.mcp
            .rows
            .iter()
            .filter(|row| row.server.apps.claude)
            .count(),
        data.mcp
            .rows
            .iter()
            .filter(|row| row.server.apps.codex)
            .count(),
        data.mcp
            .rows
            .iter()
            .filter(|row| row.server.apps.gemini)
            .count(),
        data.mcp
            .rows
            .iter()
            .filter(|row| row.server.apps.opencode)
            .count(),
    );
    render_summary_bar(frame, chunks[1], theme, summary);

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(50),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::NONE))
    .row_highlight_style(selection_style(theme))
    .highlight_symbol(highlight_symbol(theme));

    let mut state = TableState::default();
    state.select(Some(app.mcp_idx));

    frame.render_stateful_widget(table, inset_left(chunks[2], CONTENT_INSET_LEFT), &mut state);
}
