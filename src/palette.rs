use opaline::Theme;
use ratatui::prelude::*;

/// All ratatui styles used by crabmander, pre-computed from an opaline theme.
///
/// Build once with `Palette::from(theme)` and pass `&palette` to every draw call.
#[derive(Clone, Debug)]
pub struct Palette {
    // Panels
    pub border_active: Style,
    pub border_inactive: Style,
    pub entry_dir: Style,
    pub entry_symlink: Style,
    pub entry_hardlink: Style,
    pub entry_marked: Style,

    // Status bar
    pub status_normal: Style,
    pub status_error: Style,
    pub status_size: Style,

    // Filter bar
    pub filter_bar: Style,

    // Func bar key chips
    pub funcbar_normal: Style,
    pub funcbar_git: Style,

    // Dialog borders
    pub dlg_confirm: Style,
    pub dlg_input: Style,
    pub dlg_menu: Style,
    pub dlg_error: Style,
    pub dlg_qcd: Style,

    // Git working-tree status
    pub git_wt_modified: Style,
    pub git_wt_deleted: Style,
    pub git_wt_untracked: Style,

    // Git index (staging area) status
    pub git_idx_added: Style,
    pub git_idx_modified: Style,
    pub git_idx_deleted: Style,
    pub git_idx_renamed: Style,

    // Git status bar
    pub git_status_bar: Style,
}

impl From<&Theme> for Palette {
    fn from(t: &Theme) -> Self {
        let c = |tok: &str| -> Color { t.color(tok).into() };

        Self {
            border_active: Style::default().fg(c("border.focused")),
            border_inactive: Style::default()
                .fg(c("border.unfocused"))
                .add_modifier(Modifier::DIM),
            entry_dir: Style::default().fg(c("accent.secondary")),
            entry_symlink: Style::default().fg(c("accent.tertiary")),
            entry_hardlink: Style::default().fg(c("success")),
            entry_marked: Style::default().fg(c("warning")),

            status_normal: Style::default().add_modifier(Modifier::BOLD),
            status_error: Style::default()
                .fg(c("error"))
                .add_modifier(Modifier::BOLD),
            status_size: Style::default().fg(c("warning")),

            filter_bar: Style::default().fg(c("bg.base")).bg(c("warning")),

            funcbar_normal: Style::default().fg(c("bg.base")).bg(c("accent.secondary")),
            funcbar_git: Style::default().fg(c("bg.base")).bg(c("success")),

            dlg_confirm: Style::default().fg(c("warning")),
            dlg_input: Style::default().fg(c("accent.secondary")),
            dlg_menu: Style::default().fg(c("success")),
            dlg_error: Style::default().fg(c("error")),
            dlg_qcd: Style::default().fg(c("accent.primary")),

            git_wt_modified: Style::default().fg(c("warning")),
            git_wt_deleted: Style::default().fg(c("error")),
            git_wt_untracked: Style::default().fg(c("text.primary")),

            git_idx_added: Style::default().fg(c("success")),
            git_idx_modified: Style::default().fg(c("info")),
            git_idx_deleted: Style::default().fg(c("error")),
            git_idx_renamed: Style::default().fg(c("info")),

            git_status_bar: Style::default()
                .fg(c("success"))
                .add_modifier(Modifier::BOLD),
        }
    }
}
