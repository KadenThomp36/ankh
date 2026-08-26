//! Home screen: the deck tree with due counts.

use ankh_core::{DeckNode, DeckTree};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::banner;
use crate::tui::theme::{CountKind, Theme};

#[derive(Default)]
pub struct DecksView {
    pub tree: Option<DeckTree>,
    pub selected: usize,
    pub scroll: usize,
    /// Deck ids the user collapsed locally (session only; persisted later).
    collapsed: Vec<i64>,
}

impl DecksView {
    pub fn set_tree(&mut self, mut tree: DeckTree) {
        fn apply(n: &mut DeckNode, collapsed: &[i64]) {
            if collapsed.contains(&n.id.0) {
                n.collapsed = true;
            }
            for c in &mut n.children {
                apply(c, collapsed);
            }
        }
        for r in &mut tree.roots {
            apply(r, &self.collapsed);
        }
        self.tree = Some(tree);
        self.clamp();
    }

    pub fn rows(&self) -> Vec<&DeckNode> {
        self.tree.as_ref().map(|t| t.visible()).unwrap_or_default()
    }

    pub fn selected_deck(&self) -> Option<&DeckNode> {
        self.rows().get(self.selected).copied()
    }

    fn clamp(&mut self) {
        let n = self.rows().len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        let n = self.rows().len() as isize;
        if n == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, n - 1) as usize;
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
    }

    pub fn go_bottom(&mut self) {
        self.selected = self.rows().len().saturating_sub(1);
    }

    fn toggle(&mut self, want_collapsed: Option<bool>) {
        let Some(id) = self.selected_deck().map(|d| d.id.0) else { return };
        let Some(tree) = self.tree.as_mut() else { return };
        fn find(n: &mut DeckNode, id: i64) -> Option<&mut DeckNode> {
            if n.id.0 == id {
                return Some(n);
            }
            n.children.iter_mut().find_map(|c| find(c, id))
        }
        for r in &mut tree.roots {
            if let Some(n) = find(r, id) {
                if n.children.is_empty() {
                    return;
                }
                n.collapsed = want_collapsed.unwrap_or(!n.collapsed);
                if n.collapsed {
                    if !self.collapsed.contains(&id) {
                        self.collapsed.push(id);
                    }
                } else {
                    self.collapsed.retain(|c| *c != id);
                }
                return;
            }
        }
    }

    pub fn collapse(&mut self) {
        // `h` on an already-collapsed or leaf deck jumps to the parent, like a file tree.
        let rows = self.rows();
        if let Some(d) = rows.get(self.selected) {
            if d.children.is_empty() || d.collapsed {
                let level = d.level;
                let parent = rows[..self.selected].iter().rposition(|r| r.level < level);
                if let Some(p) = parent {
                    self.selected = p;
                }
                return;
            }
        }
        self.toggle(Some(true));
    }

    pub fn expand(&mut self) {
        self.toggle(Some(false));
    }

    pub fn toggle_fold(&mut self) {
        self.toggle(None);
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border())
            .title(Line::from(vec![Span::styled(" decks ", theme.title())]));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let banner_lines = banner::lines("ankh", theme);
        let banner_h = banner_lines.len() as u16;
        let show_banner = inner.height > banner_h + 8 && inner.width > banner::width(&banner_lines) + 4;
        let chunks = Layout::vertical([
            Constraint::Length(if show_banner { banner_h + 1 } else { 0 }),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

        if show_banner {
            let mut lines = banner_lines;
            let tag = match &self.tree {
                Some(t) => {
                    let (n, l, r) = t.totals();
                    Line::from(vec![
                        Span::styled("  today  ", theme.muted()),
                        Span::styled(format!("{n}"), theme.count(CountKind::New, n)),
                        Span::styled(" new  ", theme.muted()),
                        Span::styled(format!("{l}"), theme.count(CountKind::Learn, l)),
                        Span::styled(" learning  ", theme.muted()),
                        Span::styled(format!("{r}"), theme.count(CountKind::Review, r)),
                        Span::styled(" to review", theme.muted()),
                    ])
                }
                None => Line::from(Span::styled("  loading…", theme.muted())),
            };
            lines.push(tag);
            f.render_widget(Paragraph::new(lines), chunks[0]);
        }

        // Header row
        let name_w = chunks[1].width.saturating_sub(3 * 7 + 2);
        let header = Line::from(vec![
            Span::styled(format!(" {:<w$}", "DECK", w = name_w as usize), theme.muted().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>7}", "NEW"), theme.muted().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>7}", "LEARN"), theme.muted().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>7}", "DUE"), theme.muted().add_modifier(Modifier::BOLD)),
        ]);
        f.render_widget(Paragraph::new(header), chunks[1]);

        let list_area = chunks[2];
        let h = list_area.height as usize;
        if h == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + h {
            self.scroll = self.selected + 1 - h;
        }
        let rows = self.rows();
        let mut lines = Vec::with_capacity(h);
        for (i, d) in rows.iter().enumerate().skip(self.scroll).take(h) {
            let is_sel = i == self.selected;
            let indent = "  ".repeat(d.level.saturating_sub(1) as usize);
            let marker = if d.children.is_empty() {
                "  "
            } else if d.collapsed {
                "▸ "
            } else {
                "▾ "
            };
            let name_style = if d.due() == 0 && !is_sel { theme.muted() } else { Style::default().fg(theme.fg) };
            let mut name = format!("{indent}{marker}{}", d.name);
            let max = name_w as usize;
            if unicode_width::UnicodeWidthStr::width(name.as_str()) > max {
                let mut acc = 0;
                name = name
                    .chars()
                    .take_while(|c| {
                        acc += unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
                        acc < max.saturating_sub(1)
                    })
                    .collect::<String>()
                    + "…";
            }
            let pad = max.saturating_sub(unicode_width::UnicodeWidthStr::width(name.as_str()));
            let mut spans = vec![
                Span::styled(if is_sel && focused { "▌" } else { " " }, theme.accent()),
                Span::styled(name, name_style),
                Span::raw(" ".repeat(pad)),
                Span::styled(format!("{:>7}", d.new), theme.count(CountKind::New, d.new)),
                Span::styled(format!("{:>7}", d.learn), theme.count(CountKind::Learn, d.learn)),
                Span::styled(format!("{:>7}", d.review), theme.count(CountKind::Review, d.review)),
            ];
            if is_sel {
                for s in &mut spans {
                    s.style = s.style.patch(theme.selected());
                }
            }
            lines.push(Line::from(spans));
        }
        if rows.is_empty() {
            lines.push(
                Line::from(Span::styled("  no decks yet — press S to sync", theme.muted())).alignment(Alignment::Left),
            );
        }
        f.render_widget(Paragraph::new(lines), list_area);
    }
}
