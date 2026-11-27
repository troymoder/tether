//! Drawing a log on a terminal. Only the rows in view are drawn: a spacer
//! gives the scroller the height of every row, and the terminal stays in
//! view as the spacer scrolls under it.

use std::collections::VecDeque;

use leptos::prelude::*;
use leptos::{ev, html};

use super::log::Log;
use super::rows::Colours;
use crate::app::Theme;
use crate::ui::xterm::{self, Terminal};

/// The rows each shown line takes, kept between draws. Lines that left the
/// log are dropped from its front and new ones added at its end.
#[derive(Default)]
struct Laid {
    /// The `Log::sifted`, terminal width and wrap setting it was laid out
    /// for. A change to any of them lays it out from scratch.
    sifted: u32,
    columns: usize,
    wrap: bool,
    numbers: VecDeque<u64>,
    rows: VecDeque<usize>,
    /// The sum of `rows`.
    all: usize,
}

impl Laid {
    /// The index of the line that has the row `row` rows down the log, and
    /// how many of that line's rows are above it.
    fn at(&self, row: usize) -> (usize, usize) {
        let mut above = 0;

        for (index, rows) in self.rows.iter().enumerate() {
            if above + rows > row {
                return (index, row - above);
            }

            above += rows;
        }

        (self.rows.len(), 0)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub(super) scroller: NodeRef<html::Div>,
    pub(super) height: NodeRef<html::Div>,
    pub(super) holder: NodeRef<html::Div>,
    /// Made on the first draw that has room for it.
    terminal: StoredValue<Option<Terminal>, LocalStorage>,
    size: StoredValue<(i32, i32)>,
    /// The height of a terminal row, in pixels.
    row: StoredValue<f64>,
    coloured: StoredValue<Option<Theme>>,
    laid: StoredValue<Laid>,
    /// The line number on each terminal row as last drawn, to resolve a
    /// click.
    drawn: StoredValue<Vec<Option<u64>>>,
}

impl Frame {
    pub(super) fn new() -> Self {
        Self {
            scroller: NodeRef::new(),
            height: NodeRef::new(),
            holder: NodeRef::new(),
            terminal: StoredValue::new_local(None),
            size: StoredValue::new((0, 0)),
            row: StoredValue::new(15.0),
            coloured: StoredValue::new(None),
            laid: StoredValue::new(Laid::default()),
            drawn: StoredValue::new(Vec::new()),
        }
    }

    pub(super) fn row(&self) -> f64 {
        self.row.get_value()
    }

    /// The pixels between the bottom of the view and the end of the log, as
    /// last laid out.
    pub(super) fn below(&self) -> f64 {
        self.scroller.get_untracked().map_or(0.0, |scroller| {
            self.laid.with_value(|laid| laid.all) as f64 * self.row()
                - scroller.scroll_top()
                - f64::from(scroller.client_height())
        })
    }

    pub(super) fn scroll_to_end(self) {
        if let Some(scroller) = self.scroller.get_untracked() {
            scroller.set_scroll_top(f64::from(scroller.scroll_height()));
        }
    }

    /// The top of the shown line at `index`, in pixels from the log's top.
    pub(super) fn top_of(&self, index: usize) -> f64 {
        self.laid
            .with_value(|laid| laid.rows.iter().take(index).sum::<usize>()) as f64
            * self.row()
    }

    pub(super) fn number_at(&self, event: &ev::MouseEvent) -> Option<u64> {
        let holder = self.holder.get_untracked()?;
        let down = event.client_y() - holder.get_bounding_client_rect().top();

        if down < 0.0 {
            return None;
        }

        let row = (down / self.row()) as usize;

        self.drawn
            .with_value(|drawn| drawn.get(row).copied().flatten())
    }

    /// Returns how many rows left the front.
    fn lay(&self, log: &Log, columns: usize, wrap: bool) -> usize {
        let mut gone = 0;

        self.laid.update_value(|laid| {
            if (laid.sifted, laid.columns, laid.wrap) != (log.sifted, columns, wrap) {
                *laid = Laid {
                    sifted: log.sifted,
                    columns,
                    wrap,
                    ..Laid::default()
                };
            }

            let first = log.shown.front().copied();

            while laid
                .numbers
                .front()
                .is_some_and(|number| first.is_none_or(|first| *number < first))
            {
                laid.numbers.pop_front();
                gone += laid.rows.pop_front().unwrap_or(0);
            }

            laid.all -= gone;

            let from = match laid.numbers.back() {
                Some(last) => log.shown.partition_point(|number| number <= last),
                None => 0,
            };

            for number in log.shown.iter().skip(from) {
                let rows = log.rows_of(*number, columns, wrap);

                laid.numbers.push_back(*number);
                laid.rows.push_back(rows);
                laid.all += rows;
            }
        });

        gone
    }

    /// `marked` is the search hit and the picked line.
    pub(super) fn draw(
        &self,
        log: &Log,
        marked: (Option<u64>, Option<u64>),
        wrap: bool,
        theme: Theme,
    ) {
        let Some(scroller) = self.scroller.get_untracked() else {
            return;
        };

        let Some(height) = self.height.get_untracked() else {
            return;
        };

        let Some(holder) = self.holder.get_untracked() else {
            return;
        };

        let wide = scroller.client_width();
        let high = scroller.client_height();

        // A hidden log has no size, as when the other log is in view.
        if wide <= 0 || high <= 0 {
            return;
        }

        if self.terminal.with_value(Option::is_none) {
            let Ok(made) = xterm::log() else {
                return;
            };

            let _ = holder.append_child(&made.host());

            self.terminal.set_value(Some(made));
        }

        self.terminal.with_value(|terminal| {
            let Some(terminal) = terminal else {
                return;
            };

            if self.coloured.get_value() != Some(theme) {
                terminal.colour(&xterm::colours());
                self.coloured.set_value(Some(theme));
            }

            if self.size.get_value() != (wide, high) {
                let _ = (*holder)
                    .style()
                    .set_property("height", &format!("{high}px"));
                terminal.fit();

                self.size.set_value((wide, high));

                let row = terminal.row_height();

                if row >= 1.0 {
                    self.row.set_value(row);
                }
            }

            let columns = usize::from(terminal.columns());
            let rows = usize::from(terminal.rows());
            let row = self.row();
            // Scrolled back by the rows that left the front, so the lines
            // being read stay in place.
            let gone = self.lay(log, columns, wrap);

            if gone > 0 {
                scroller.set_scroll_top((scroller.scroll_top() - gone as f64 * row).max(0.0));
            }

            let all = self.laid.with_value(|laid| laid.all);
            let _ = (*height)
                .style()
                .set_property("height", &format!("{}px", all as f64 * row));
            // The row at the top of the view. At the end of the log this
            // keeps its last row whole in view.
            let top = ((scroller.scroll_top() / row - 0.01).ceil().max(0.0) as usize)
                .min(all.saturating_sub(rows.max(1)));
            let (mut index, mut above) = self.laid.with_value(|laid| laid.at(top));
            let colours = Colours::now();
            let mut told = Vec::with_capacity(rows);
            let mut drawn = Vec::with_capacity(rows);

            while told.len() < rows {
                let Some(number) = log.shown.get(index) else {
                    // Past the last line, the rows are empty.
                    told.push("\u{1b}[0m\u{1b}[K".to_owned());
                    drawn.push(None);
                    continue;
                };

                for row in log
                    .rows(*number, (columns, wrap), colours, marked)
                    .into_iter()
                    .skip(above)
                    .take(rows - told.len())
                {
                    told.push(row);
                    drawn.push(Some(*number));
                }

                index += 1;
                above = 0;
            }

            // Drawn from the home position, every row whole.
            terminal.draw(&format!("\u{1b}[H{}", told.join("\r\n")));
            self.drawn.set_value(drawn);
        });
    }
}
