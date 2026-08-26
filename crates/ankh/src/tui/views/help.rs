//! `:help [topic]` — the docs, rendered in-app through the same pipeline
//! that renders cards (Markdown → HTML → styled lines).

use ankh_render::{render_html, wrap_document, Align, Document, Options};
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::doc;
use crate::tui::theme::Theme;

pub struct Topic {
    pub name: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

pub const TOPICS: &[Topic] = &[
    Topic { name: "keys", title: "Keys", body: include_str!("../../../../../docs/help/keys.md") },
    Topic { name: "search", title: "Search syntax", body: include_str!("../../../../../docs/help/search.md") },
    Topic { name: "notes", title: "Editing notes", body: include_str!("../../../../../docs/help/notes.md") },
    Topic { name: "sync", title: "Sync", body: include_str!("../../../../../docs/help/sync.md") },
    Topic { name: "lua", title: "Lua API", body: include_str!("../../../../../docs/lua.md") },
    Topic { name: "cli", title: "Command line", body: include_str!("../../../../../docs/cli.md") },
];

pub struct HelpView {
    pub topic: Option<&'static Topic>,
    doc: Document,
    pub scroll: u16,
}

impl HelpView {
    pub fn new(topic: Option<&str>) -> Self {
        let topic = topic.and_then(|t| {
            let t = t.to_ascii_lowercase();
            TOPICS.iter().find(|x| x.name == t || x.title.to_ascii_lowercase().starts_with(&t))
        });
        let doc = match topic {
            Some(t) => {
                let html = ankh_core::markdown::md_to_html(t.body);
                render_html(&html, &Options { default_align: Align::Left, ..Default::default() })
            }
            None => Document::default(),
        };
        HelpView { topic, doc, scroll: 0 }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let title = match self.topic {
            Some(t) => format!(" help · {} ", t.title),
            None => " help ".to_string(),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border())
            .title(Line::from(Span::styled(title, theme.title())))
            .title(Line::from(Span::styled(" :help TOPIC · q back ", theme.muted())).alignment(Alignment::Right));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let inner = Rect { x: inner.x + 2, width: inner.width.saturating_sub(4), ..inner };

        let lines: Vec<Line> = match self.topic {
            None => {
                let mut v = vec![Line::default(), Line::from(Span::styled("  topics", theme.muted())), Line::default()];
                for t in TOPICS {
                    v.push(Line::from(vec![
                        Span::styled(format!("  :help {:<8}", t.name), theme.accent()),
                        Span::raw(t.title),
                    ]));
                }
                v.push(Line::default());
                v.push(Line::from(Span::styled("  ? shows the live keymap of the current view", theme.muted())));
                v
            }
            Some(_) => {
                let width = inner.width as usize;
                let wrapped = wrap_document(&self.doc, width);
                doc::to_lines(&wrapped, width, theme, |_| None).0
            }
        };
        let total = lines.len() as u16;
        let max_scroll = total.saturating_sub(inner.height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
        f.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), inner);
    }
}
