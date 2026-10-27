//! Minimal VT100/xterm screen model.
//!
//! Handles the subset a shell session needs: printing with wrap, CR/LF/BS/TAB,
//! cursor movement, erase, scroll regions, SGR colors and scrollback. Parsing
//! is delegated to vte; this module owns the grid and the rendering runs.

use std::collections::VecDeque;

use vte::{Params, Perform};

pub const SCROLLBACK_LIMIT: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub underline: bool,
    pub reverse: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
        }
    }
}

#[derive(Clone)]
pub struct Screen {
    cols: usize,
    rows: usize,
    grid: Vec<Vec<Cell>>,
    scrollback: VecDeque<Vec<Cell>>,
    cursor_row: usize,
    cursor_col: usize,
    saved: (usize, usize),
    style: Style,
    scroll_top: usize,
    scroll_bottom: usize,
}

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.clamp(2, 500);
        let rows = rows.clamp(2, 200);
        Self {
            cols,
            rows,
            grid: vec![vec![Cell::default(); cols]; rows],
            scrollback: VecDeque::new(),
            cursor_row: 0,
            cursor_col: 0,
            saved: (0, 0),
            style: Style::default(),
            scroll_top: 0,
            scroll_bottom: rows - 1,
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Parses terminal output. The parser is owned by the caller so the grid
    /// can stay free of interior mutability.
    pub fn feed(&mut self, parser: &mut vte::Parser, bytes: &[u8]) {
        for byte in bytes {
            parser.advance(self, *byte);
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.clamp(2, 500);
        let rows = rows.clamp(2, 200);
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut grid = vec![vec![Cell::default(); cols]; rows];
        for (row, line) in self.grid.iter().take(rows).enumerate() {
            for (col, cell) in line.iter().take(cols).enumerate() {
                grid[row][col] = *cell;
            }
        }
        self.grid = grid;
        self.cols = cols;
        self.rows = rows;
        self.cursor_row = self.cursor_row.min(rows - 1);
        self.cursor_col = self.cursor_col.min(cols - 1);
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
    }

    /// One visible row as text plus styled runs, merging equal neighbors.
    pub fn row(&self, index: usize) -> (String, Vec<(std::ops::Range<usize>, Style)>) {
        let Some(line) = self.grid.get(index) else {
            return (String::new(), Vec::new());
        };
        let mut text = String::with_capacity(self.cols);
        let mut runs: Vec<(std::ops::Range<usize>, Style)> = Vec::new();
        for cell in line {
            let start = text.len();
            text.push(cell.ch);
            match runs.last_mut() {
                Some((range, style)) if *style == cell.style && range.end == start => {
                    range.end = text.len();
                }
                _ => runs.push((start..text.len(), cell.style)),
            }
        }
        (text, runs)
    }

    fn put(&mut self, ch: char) {
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.line_feed();
        }
        let cell = Cell {
            ch,
            style: self.style,
        };
        self.grid[self.cursor_row][self.cursor_col] = cell;
        self.cursor_col += 1;
    }

    fn line_feed(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        }
    }

    fn scroll_up(&mut self, count: usize) {
        for _ in 0..count.min(self.rows) {
            let line = self.grid.remove(self.scroll_top);
            self.grid
                .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
            if self.scroll_top == 0 && self.scroll_bottom == self.rows - 1 {
                if self.scrollback.len() == SCROLLBACK_LIMIT {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(line);
            }
        }
    }

    fn erase_in_line(&mut self, mode: usize) {
        match mode {
            0 => self.grid[self.cursor_row][self.cursor_col..].fill(Cell::default()),
            1 => self.grid[self.cursor_row][..=self.cursor_col.min(self.cols - 1)]
                .fill(Cell::default()),
            _ => self.grid[self.cursor_row].fill(Cell::default()),
        }
    }

    fn erase_in_display(&mut self, mode: usize) {
        match mode {
            0 => {
                self.erase_in_line(0);
                for row in self.cursor_row + 1..self.rows {
                    self.grid[row].fill(Cell::default());
                }
            }
            1 => {
                self.erase_in_line(1);
                for row in 0..self.cursor_row {
                    self.grid[row].fill(Cell::default());
                }
            }
            _ => {
                for row in &mut self.grid {
                    row.fill(Cell::default());
                }
            }
        }
    }

    fn sgr(&mut self, params: &Params) {
        let values: Vec<usize> = params
            .iter()
            .map(|param| param.first().copied().unwrap_or(0) as usize)
            .collect();
        if values.is_empty() {
            self.style = Style::default();
            return;
        }
        let mut index = 0;
        while index < values.len() {
            let value = values[index];
            match value {
                0 => self.style = Style::default(),
                1 => self.style.bold = true,
                4 => self.style.underline = true,
                7 => self.style.reverse = true,
                22 => self.style.bold = false,
                24 => self.style.underline = false,
                27 => self.style.reverse = false,
                30..=37 => self.style.fg = Color::Indexed((value - 30) as u8),
                39 => self.style.fg = Color::Default,
                40..=47 => self.style.bg = Color::Indexed((value - 40) as u8),
                49 => self.style.bg = Color::Default,
                90..=97 => self.style.fg = Color::Indexed((value - 90 + 8) as u8),
                100..=107 => self.style.bg = Color::Indexed((value - 100 + 8) as u8),
                38 | 48 => {
                    let color = match values.get(index + 1).copied() {
                        Some(5) => values
                            .get(index + 2)
                            .map(|value| Color::Indexed((*value).min(255) as u8)),
                        Some(2) => match (
                            values.get(index + 2),
                            values.get(index + 3),
                            values.get(index + 4),
                        ) {
                            (Some(r), Some(g), Some(b)) => Some(Color::Rgb(
                                (*r).min(255) as u8,
                                (*g).min(255) as u8,
                                (*b).min(255) as u8,
                            )),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(color) = color {
                        if value == 38 {
                            self.style.fg = color;
                        } else {
                            self.style.bg = color;
                        }
                    }
                    index += if values.get(index + 1) == Some(&5) {
                        2
                    } else {
                        4
                    };
                }
                _ => {}
            }
            index += 1;
        }
    }
}

fn param(params: &Params, index: usize, default: usize) -> usize {
    params
        .iter()
        .nth(index)
        .and_then(|param| param.first().copied())
        .map(|value| value as usize)
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

impl Perform for Screen {
    fn print(&mut self, c: char) {
        self.put(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.line_feed(),
            b'\r' => self.cursor_col = 0,
            8 => self.cursor_col = self.cursor_col.saturating_sub(1),
            b'\t' => {
                let next = (self.cursor_col / 8 + 1) * 8;
                self.cursor_col = next.min(self.cols - 1);
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        match action {
            'A' => self.cursor_row = self.cursor_row.saturating_sub(param(params, 0, 1)),
            'B' => {
                self.cursor_row = (self.cursor_row + param(params, 0, 1)).min(self.rows - 1);
            }
            'C' => {
                self.cursor_col = (self.cursor_col + param(params, 0, 1)).min(self.cols - 1);
            }
            'D' => self.cursor_col = self.cursor_col.saturating_sub(param(params, 0, 1)),
            'G' => self.cursor_col = param(params, 0, 1).saturating_sub(1).min(self.cols - 1),
            'd' => self.cursor_row = param(params, 0, 1).saturating_sub(1).min(self.rows - 1),
            'H' | 'f' => {
                self.cursor_row = param(params, 0, 1).saturating_sub(1).min(self.rows - 1);
                self.cursor_col = param(params, 1, 1).saturating_sub(1).min(self.cols - 1);
            }
            'J' => self.erase_in_display(param(params, 0, 0)),
            'K' => self.erase_in_line(param(params, 0, 0)),
            'S' => self.scroll_up(param(params, 0, 1)),
            'm' => self.sgr(params),
            'r' => {
                let top = param(params, 0, 1).saturating_sub(1);
                let bottom = param(params, 1, self.rows)
                    .saturating_sub(1)
                    .min(self.rows - 1);
                if top < bottom {
                    self.scroll_top = top;
                    self.scroll_bottom = bottom;
                    self.cursor_row = top;
                    self.cursor_col = 0;
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => self.saved = (self.cursor_row, self.cursor_col),
            b'8' => {
                self.cursor_row = self.saved.0.min(self.rows - 1);
                self.cursor_col = self.saved.1.min(self.cols - 1);
            }
            b'M' => {
                if self.cursor_row == self.scroll_top {
                    for row in (self.scroll_top..self.scroll_bottom).rev() {
                        let next = self.grid[row].clone();
                        self.grid[row + 1] = next;
                    }
                    self.grid[self.scroll_top].fill(Cell::default());
                } else {
                    self.cursor_row = self.cursor_row.saturating_sub(1);
                }
            }
            b'c' => {
                let cols = self.cols;
                let rows = self.rows;
                *self = Screen::new(cols, rows);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(screen: &Screen, row: usize) -> String {
        screen.row(row).0.trim_end().to_string()
    }

    fn feed(screen: &mut Screen, bytes: &[u8]) {
        let mut parser = vte::Parser::new();
        screen.feed(&mut parser, bytes);
    }

    #[test]
    fn prints_and_wraps() {
        let mut screen = Screen::new(5, 3);
        feed(&mut screen, b"abcdefg");
        assert_eq!(text(&screen, 0), "abcde");
        assert_eq!(text(&screen, 1), "fg");
    }

    #[test]
    fn carriage_return_and_erase() {
        let mut screen = Screen::new(10, 2);
        feed(&mut screen, b"hello\rbye");
        assert_eq!(text(&screen, 0), "byelo");
        feed(&mut screen, b"\x1b[K");
        assert_eq!(text(&screen, 0), "bye");
    }

    #[test]
    fn scrolling_moves_lines_into_scrollback() {
        let mut screen = Screen::new(10, 2);
        feed(&mut screen, b"a\r\nb\r\nc");
        assert_eq!(screen.scrollback_len(), 1);
        assert_eq!(text(&screen, 0), "b");
        assert_eq!(text(&screen, 1), "c");
    }

    #[test]
    fn sgr_sets_and_resets_colors() {
        let mut screen = Screen::new(10, 2);
        feed(&mut screen, b"\x1b[31mred\x1b[0m normal");
        let (_, runs) = screen.row(0);
        assert_eq!(runs[0].1.fg, Color::Indexed(1));
        assert_eq!(runs[1].1.fg, Color::Default);
    }

    #[test]
    fn truecolor_is_parsed() {
        let mut screen = Screen::new(10, 2);
        feed(&mut screen, b"\x1b[38;2;10;20;30mx");
        let (_, runs) = screen.row(0);
        assert_eq!(runs[0].1.fg, Color::Rgb(10, 20, 30));
    }

    #[test]
    fn cursor_positioning_and_erase_display() {
        let mut screen = Screen::new(5, 3);
        feed(&mut screen, b"aaaaa\r\nbbbbb");
        feed(&mut screen, b"\x1b[2J");
        assert_eq!(text(&screen, 0), "");
        assert_eq!(text(&screen, 1), "");
    }

    #[test]
    fn resize_keeps_the_top_left_content() {
        let mut screen = Screen::new(5, 2);
        feed(&mut screen, b"abc");
        screen.resize(3, 2);
        assert_eq!(text(&screen, 0), "abc");
        screen.resize(8, 4);
        assert_eq!(text(&screen, 0), "abc");
    }

    #[test]
    fn tab_advances_to_the_next_stop() {
        let mut screen = Screen::new(20, 1);
        feed(&mut screen, b"a\tb");
        assert_eq!(text(&screen, 0), "a       b");
    }
}
