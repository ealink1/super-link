//! Minimal VT100/xterm screen model.
//!
//! Handles the subset a shell session needs: printing with wrap, CR/LF/BS/TAB,
//! cursor movement, erase, scroll regions, SGR colors and scrollback. Parsing
//! is delegated to vte; this module owns the grid and the rendering runs.

use std::collections::VecDeque;

use unicode_width::UnicodeWidthChar;
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
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    combining: String,
    continuation: bool,
    /// Set on the last cell of a row that soft-wraps into the next row, so
    /// resizing can join the run back into one logical line.
    wrap: bool,
    pub ch: char,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            combining: String::new(),
            continuation: false,
            wrap: false,
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
    alternate: Option<(Vec<Vec<Cell>>, usize, usize)>,
    cursor_visible: bool,
    application_cursor: bool,
    bracketed_paste: bool,
    replies: Vec<u8>,
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
            alternate: None,
            cursor_visible: true,
            application_cursor: false,
            bracketed_paste: false,
            replies: Vec::new(),
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

    pub fn application_cursor(&self) -> bool {
        self.application_cursor
    }
    pub fn paste(&self, text: &str) -> Vec<u8> {
        if self.bracketed_paste {
            format!("\x1b[200~{text}\x1b[201~").into_bytes()
        } else {
            text.as_bytes().to_vec()
        }
    }
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }
    fn reply(&mut self, bytes: &[u8]) {
        if self.replies.len() + bytes.len() <= 1024 {
            self.replies.extend_from_slice(bytes);
        }
    }

    /// Parses terminal output. The parser is owned by the caller so the grid
    /// can stay free of interior mutability.
    pub fn feed(&mut self, parser: &mut vte::Parser, bytes: &[u8]) {
        for byte in bytes {
            parser.advance(self, *byte);
        }
    }

    /// Re-wraps the whole buffer at the new size.
    ///
    /// Rows are first joined back into logical lines using the soft-wrap marks
    /// left by `put`, so narrowing the window re-wraps long lines instead of
    /// clipping them, and widening joins the fragments back together.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.clamp(2, 500);
        let rows = rows.clamp(2, 200);
        if cols == self.cols && rows == self.rows {
            return;
        }
        if self.alternate.is_some() {
            // Full-screen programs repaint themselves; re-wrapping their
            // display would corrupt cursor-addressed output.
            self.crop(cols, rows);
            return;
        }
        let (lines, cursor_line, cursor_column) = self.logical_layout();
        let mut wrapped: Vec<Vec<Cell>> = Vec::new();
        let mut starts: Vec<usize> = Vec::with_capacity(lines.len());
        let mut columns: Vec<usize> = Vec::with_capacity(lines.len());
        for line in lines {
            let (rows_of_line, column) = wrap_line(line, cols);
            starts.push(wrapped.len());
            columns.push(column);
            wrapped.extend(rows_of_line);
        }
        let line = cursor_line.min(starts.len().saturating_sub(1));
        let first_row = starts.get(line).copied().unwrap_or(0);
        let column = cursor_column.min(columns.get(line).copied().unwrap_or(0).saturating_sub(1));
        let cursor_row_abs = first_row + column / cols;
        let grid_start = wrapped.len().saturating_sub(rows);
        let mut grid = wrapped.split_off(grid_start);
        grid.resize_with(rows, || vec![Cell::default(); cols]);
        let mut scrollback: VecDeque<Vec<Cell>> = wrapped.into();
        while scrollback.len() > SCROLLBACK_LIMIT {
            scrollback.pop_front();
        }
        self.grid = grid;
        self.scrollback = scrollback;
        self.cols = cols;
        self.rows = rows;
        self.cursor_row = cursor_row_abs.saturating_sub(grid_start).min(rows - 1);
        self.cursor_col = (column % cols).min(cols - 1);
        self.saved = (self.saved.0.min(rows - 1), self.saved.1.min(cols - 1));
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
    }

    fn crop(&mut self, cols: usize, rows: usize) {
        let mut grid = vec![vec![Cell::default(); cols]; rows];
        for (row, line) in self.grid.iter().take(rows).enumerate() {
            for (col, cell) in line.iter().take(cols).enumerate() {
                grid[row][col] = cell.clone();
            }
        }
        self.grid = grid;
        self.cols = cols;
        self.rows = rows;
        self.cursor_row = self.cursor_row.min(rows - 1);
        self.cursor_col = self.cursor_col.min(cols - 1);
        self.saved = (self.saved.0.min(rows - 1), self.saved.1.min(cols - 1));
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
    }

    /// Joins soft-wrapped rows back into logical lines and reports where the
    /// cursor sits inside its own logical line.
    fn logical_layout(&self) -> (Vec<Vec<Cell>>, usize, usize) {
        let mut lines: Vec<Vec<Cell>> = Vec::new();
        let mut open = false;
        let mut line = 0usize;
        let mut lead = 0usize;
        let mut cursor = (0usize, 0usize);
        let scrollback = self.scrollback.len();
        for index in 0..scrollback + self.grid.len() {
            let row = if index < scrollback {
                &self.scrollback[index]
            } else {
                &self.grid[index - scrollback]
            };
            if open {
                if let Some(last) = lines.last_mut() {
                    last.extend(row.iter().cloned());
                }
            } else {
                if index > 0 {
                    line += 1;
                }
                lead = 0;
                lines.push(row.clone());
            }
            if index == scrollback + self.cursor_row {
                cursor = (line, lead + self.cursor_col);
            }
            lead += self.cols;
            open = row.last().is_some_and(|cell| cell.wrap);
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        // Blank rows below the content and trailing blanks on each line carry
        // no text; keeping them would wrap a short buffer into extra empty
        // rows and push the real content out of view.
        for line in &mut lines {
            while line.len() > 1 && line.last() == Some(&Cell::default()) {
                line.pop();
            }
        }
        while lines.len() > 1
            && lines
                .last()
                .is_some_and(|line| line.len() == 1 && line[0] == Cell::default())
        {
            lines.pop();
        }
        (lines, cursor.0, cursor.1)
    }

    pub fn visible_row(
        &self,
        index: usize,
        scroll: usize,
    ) -> (String, Vec<(std::ops::Range<usize>, Style)>) {
        let absolute = self.scrollback.len().saturating_sub(scroll) + index;
        if absolute >= self.scrollback.len() {
            return self.row(absolute - self.scrollback.len());
        }
        let Some(line) = self.scrollback.get(absolute) else {
            return (String::new(), Vec::new());
        };
        let mut text = String::new();
        let mut runs: Vec<(std::ops::Range<usize>, Style)> = Vec::new();
        for cell in line {
            if cell.continuation {
                continue;
            }
            let start = text.len();
            text.push(cell.ch);
            text.push_str(&cell.combining);
            match runs.last_mut() {
                Some((range, style)) if *style == cell.style => range.end = text.len(),
                _ => runs.push((start..text.len(), cell.style)),
            }
        }
        (text, runs)
    }

    /// One visible row as text plus styled runs, merging equal neighbors.
    pub fn row(&self, index: usize) -> (String, Vec<(std::ops::Range<usize>, Style)>) {
        let Some(line) = self.grid.get(index) else {
            return (String::new(), Vec::new());
        };
        let mut text = String::with_capacity(self.cols);
        let mut runs: Vec<(std::ops::Range<usize>, Style)> = Vec::new();
        for cell in line {
            if cell.continuation {
                continue;
            }
            let start = text.len();
            text.push(cell.ch);
            text.push_str(&cell.combining);
            match runs.last_mut() {
                Some((range, style)) if *style == cell.style && range.end == start => {
                    range.end = text.len();
                }
                _ => runs.push((start..text.len(), cell.style)),
            }
        }
        (text, runs)
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }
    pub fn byte_index(&self, row: usize, col: usize) -> usize {
        self.grid
            .get(row)
            .map(|line| {
                line.iter()
                    .take(col)
                    .filter(|c| !c.continuation)
                    .map(|c| c.ch.len_utf8() + c.combining.len())
                    .sum()
            })
            .unwrap_or(0)
    }
    fn put(&mut self, ch: char) {
        let width = ch.width().unwrap_or(0).min(2);
        if width == 0 {
            let mut column = self.cursor_col.min(self.cols).saturating_sub(1);
            if self.grid[self.cursor_row][column].continuation {
                column = column.saturating_sub(1);
            }
            let cell = &mut self.grid[self.cursor_row][column];
            if cell.combining.len() + ch.len_utf8() <= 32 {
                cell.combining.push(ch);
            }
            return;
        }
        if self.cursor_col + width > self.cols {
            self.mark_wrap();
            self.cursor_col = 0;
            self.line_feed();
        }
        let col = self.cursor_col;
        if self.grid[self.cursor_row][col].continuation && col > 0 {
            self.grid[self.cursor_row][col - 1] = Cell::default();
        }
        if col + 1 < self.cols && self.grid[self.cursor_row][col + 1].continuation {
            self.grid[self.cursor_row][col + 1] = Cell::default();
        }
        self.grid[self.cursor_row][col] = Cell {
            ch,
            style: self.style,
            combining: String::new(),
            continuation: false,
            wrap: false,
        };
        if width == 2 {
            self.grid[self.cursor_row][col + 1] = Cell {
                continuation: true,
                style: self.style,
                ..Default::default()
            };
        }
        self.cursor_col += width;
    }

    fn line_feed(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        }
    }

    /// Marks the cell just written as the end of a soft-wrapped row.
    fn mark_wrap(&mut self) {
        let column = self.cursor_col.min(self.cols.saturating_sub(1));
        if let Some(cell) = self.grid[self.cursor_row].get_mut(column)
            && !cell.continuation
        {
            cell.wrap = true;
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
                3 => self.style.italic = true,
                23 => self.style.italic = false,
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

/// Splits one logical line into fixed `cols`-wide rows, re-marking the
/// soft-wrap ends for the new width. Returns the rows and the column count of
/// the line, which the caller uses to place the cursor.
fn wrap_line(line: Vec<Cell>, cols: usize) -> (Vec<Vec<Cell>>, usize) {
    let mut rows: Vec<Vec<Cell>> = vec![Vec::new()];
    let mut used = 0usize;
    let mut columns = 0usize;
    for cell in line {
        let width = if cell.continuation {
            0
        } else {
            cell.ch.width().unwrap_or(1).min(2)
        };
        if width > 0 && used + width > cols {
            rows.push(Vec::new());
            used = 0;
        }
        used += width;
        if !cell.continuation {
            columns += width;
        }
        if let Some(row) = rows.last_mut() {
            row.push(cell);
        }
    }
    let total = rows.len();
    for (index, row) in rows.iter_mut().enumerate() {
        if index + 1 < total
            && let Some(cell) = row.iter_mut().rev().find(|cell| !cell.continuation)
        {
            cell.wrap = true;
        }
        row.resize_with(cols, Cell::default);
    }
    (rows, columns.max(1))
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

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        if intermediates.contains(&b'?') && matches!(action, 'h' | 'l') {
            let enabled = action == 'h';
            for mode in params.iter().filter_map(|p| p.first()) {
                match *mode {
                    1 => self.application_cursor = enabled,
                    2004 => self.bracketed_paste = enabled,
                    25 => self.cursor_visible = enabled,
                    47 | 1047 | 1049 if enabled && self.alternate.is_none() => {
                        let grid = std::mem::replace(
                            &mut self.grid,
                            vec![vec![Cell::default(); self.cols]; self.rows],
                        );
                        self.alternate = Some((grid, self.cursor_row, self.cursor_col));
                        self.cursor_row = 0;
                        self.cursor_col = 0;
                    }
                    47 | 1047 | 1049 if !enabled => {
                        if let Some((mut grid, row, col)) = self.alternate.take() {
                            grid.resize(self.rows, vec![Cell::default(); self.cols]);
                            for line in &mut grid {
                                line.resize(self.cols, Cell::default());
                            }
                            self.grid = grid;
                            self.cursor_row = row.min(self.rows - 1);
                            self.cursor_col = col.min(self.cols - 1);
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        match action {
            'n' => match param(params, 0, 0) {
                5 => self.reply(b"\x1b[0n"),
                6 => self.reply(
                    format!(
                        "\x1b[{};{}R",
                        self.cursor_row + 1,
                        self.cursor_col.min(self.cols - 1) + 1
                    )
                    .as_bytes(),
                ),
                _ => {}
            },
            'c' if param(params, 0, 0) == 0 => self.reply(b"\x1b[?1;2c"),
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
    fn narrowing_rewraps_instead_of_clipping() {
        let mut screen = Screen::new(10, 4);
        feed(&mut screen, b"0123456789abcdefghij");
        assert_eq!(text(&screen, 0), "0123456789");
        assert_eq!(text(&screen, 1), "abcdefghij");
        screen.resize(5, 4);
        assert_eq!(text(&screen, 0), "01234");
        assert_eq!(text(&screen, 1), "56789");
        assert_eq!(text(&screen, 2), "abcde");
        assert_eq!(text(&screen, 3), "fghij");
    }

    #[test]
    fn widening_joins_soft_wrapped_rows_and_keeps_cursor() {
        let mut screen = Screen::new(5, 4);
        feed(&mut screen, b"0123456789");
        assert_eq!(screen.cursor(), (1, 5));
        screen.resize(10, 4);
        assert_eq!(text(&screen, 0), "0123456789");
        assert_eq!(screen.cursor(), (0, 9));
    }

    #[test]
    fn explicit_newlines_stay_separate_when_resizing() {
        let mut screen = Screen::new(10, 4);
        feed(&mut screen, b"one\r\ntwo");
        screen.resize(4, 4);
        assert_eq!(text(&screen, 0), "one");
        assert_eq!(text(&screen, 1), "two");
    }

    #[test]
    fn shrinking_rows_keeps_the_newest_lines() {
        let mut screen = Screen::new(10, 5);
        feed(&mut screen, b"a\r\nb\r\nc\r\nd\r\ne");
        screen.resize(10, 2);
        assert_eq!(text(&screen, 0), "d");
        assert_eq!(text(&screen, 1), "e");
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

#[cfg(test)]
mod unicode_tests {
    use super::*;
    #[test]
    fn cjk_combining_and_cursor_use_terminal_cells() {
        let mut screen = Screen::new(12, 3);
        let mut parser = vte::Parser::new();
        screen.feed(&mut parser, "中e\u{301}😀".as_bytes());
        assert_eq!(screen.cursor(), (0, 5));
        assert!(screen.row(0).0.starts_with("中e\u{301}😀"));
        assert_eq!(screen.byte_index(0, 5), "中e\u{301}😀".len());
    }
    #[test]
    fn alternate_screen_preserves_shell_output() {
        let mut screen = Screen::new(12, 3);
        let mut parser = vte::Parser::new();
        screen.feed(&mut parser, b"shell\x1b[?1049hvim\x1b[?1049l");
        assert!(screen.row(0).0.starts_with("shell"));
        assert_eq!(screen.cursor(), (0, 5));
    }
    #[test]
    fn terminal_modes_and_device_replies_follow_application_state() {
        let mut screen = Screen::new(12, 3);
        let mut parser = vte::Parser::new();
        screen.feed(
            &mut parser,
            b"\x1b[?1h\x1b[?2004h\x1b[2;4H\x1b[6n\x1b[5n\x1b[c",
        );
        assert!(screen.application_cursor());
        assert_eq!(screen.paste("echo ok\n"), b"\x1b[200~echo ok\n\x1b[201~");
        assert_eq!(screen.take_replies(), b"\x1b[2;4R\x1b[0n\x1b[?1;2c");
        assert!(screen.take_replies().is_empty());
        screen.feed(&mut parser, b"\x1b[?1l\x1b[?2004l");
        assert!(!screen.application_cursor());
        assert_eq!(screen.paste("plain"), b"plain");
    }
    #[test]
    fn ansi_styles_are_reset_independently() {
        let mut screen = Screen::new(12, 3);
        let mut parser = vte::Parser::new();
        screen.feed(&mut parser, b"\x1b[1;3;4;7mA\x1b[23mB\x1b[0mC");
        let (_, runs) = screen.row(0);
        assert!(runs[0].1.bold && runs[0].1.italic && runs[0].1.underline && runs[0].1.reverse);
        assert!(!runs[1].1.italic && runs[1].1.bold);
        assert_eq!(runs[2].1, Style::default());
    }
}
