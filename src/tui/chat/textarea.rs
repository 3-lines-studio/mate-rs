use super::ChatScreen;

impl ChatScreen {
    pub fn textarea_value(&self) -> &str {
        &self.textarea
    }

    pub fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }

    pub fn clear_textarea(&mut self) {
        self.textarea.clear();
        self.cursor = 0;
        self.textarea_scroll = 0;
        self.history_idx = -1;
    }

    pub fn update_textarea_layout(&mut self) {
        let cw = self.width.saturating_sub(6);
        let total = textarea_total_rows(&self.textarea, cw);
        let max_h = (self.height / 3).max(3);
        self.textarea_height = total.min(max_h).max(1);
        let (crow, _) = textarea_cursor_xy(&self.textarea, self.cursor, cw);
        let crow = crow as usize;
        let visible = self.textarea_height as usize;
        if crow < self.textarea_scroll {
            self.textarea_scroll = crow;
        } else if crow >= self.textarea_scroll + visible {
            self.textarea_scroll = crow + 1 - visible;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.textarea.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.textarea.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn delete_before_cursor(&mut self) {
        if self.cursor > 0 {
            let prev = self.textarea[..self.cursor]
                .chars()
                .next_back()
                .unwrap()
                .len_utf8();
            self.cursor -= prev;
            self.textarea.remove(self.cursor);
        }
    }

    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            let prev = self.textarea[..self.cursor]
                .chars()
                .next_back()
                .unwrap()
                .len_utf8();
            self.cursor -= prev;
        }
    }

    pub fn cursor_right(&mut self) {
        if self.cursor < self.textarea.len() {
            let next = self.textarea[self.cursor..]
                .chars()
                .next()
                .unwrap()
                .len_utf8();
            self.cursor += next;
        }
    }

    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor = self.textarea.len();
    }

    pub fn set_text(&mut self, text: &str) {
        self.textarea = text.to_string();
        self.cursor = self.textarea.len();
    }

    pub fn complete_prefix(&mut self, prefix: char, value: &str) {
        let Some(start) = self.textarea[..self.cursor].rfind(prefix) else {
            return;
        };
        self.textarea.replace_range(start..self.cursor, value);
        self.cursor = start + value.len();
    }

    pub fn cursor_up(&mut self) -> bool {
        let (line, col) = self.locate();
        if line == 0 {
            return false;
        }
        self.move_to(line - 1, col);
        true
    }

    pub fn cursor_down(&mut self) -> bool {
        let (line, col) = self.locate();
        if line >= self.textarea.matches('\n').count() {
            return false;
        }
        self.move_to(line + 1, col);
        true
    }

    pub fn cursor_left_word(&mut self) {
        let chars: Vec<(usize, char)> = self.textarea.char_indices().collect();
        let mut idx = chars
            .iter()
            .position(|(b, _)| *b >= self.cursor)
            .unwrap_or(chars.len());
        while idx > 0 && chars[idx - 1].1.is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !chars[idx - 1].1.is_whitespace() {
            idx -= 1;
        }
        self.cursor = if idx > 0 { chars[idx].0 } else { 0 };
    }

    pub fn cursor_right_word(&mut self) {
        let chars: Vec<(usize, char)> = self.textarea.char_indices().collect();
        let n = chars.len();
        let mut idx = chars
            .iter()
            .position(|(b, _)| *b >= self.cursor)
            .unwrap_or(n);
        while idx < n && chars[idx].1.is_whitespace() {
            idx += 1;
        }
        while idx < n && !chars[idx].1.is_whitespace() {
            idx += 1;
        }
        self.cursor = if idx < n {
            chars[idx].0
        } else {
            self.textarea.len()
        };
    }

    fn locate(&self) -> (usize, usize) {
        let mut line = 0;
        let mut byte = 0;
        for l in self.textarea.split('\n') {
            let end = byte + l.len();
            if self.cursor <= end {
                let col = self.textarea[byte..self.cursor].chars().count();
                return (line, col);
            }
            byte = end + 1;
            line += 1;
        }
        (line.saturating_sub(1), 0)
    }

    fn move_to(&mut self, line_idx: usize, char_col: usize) {
        let mut byte = 0;
        for (line, l) in self.textarea.split('\n').enumerate() {
            if line == line_idx {
                let mut target = byte;
                for c in l.chars().take(char_col) {
                    target += c.len_utf8();
                }
                self.cursor = target;
                return;
            }
            byte += l.len() + 1;
        }
        self.cursor = self.textarea.len();
    }
}

pub(super) fn wrap_line(line: &str, max_w: usize) -> Vec<(usize, usize)> {
    if max_w == 0 {
        return vec![(0, 0)];
    }
    let mut rows: Vec<(usize, usize)> = Vec::new();
    let mut line_start = 0usize;
    let mut line_w = 0usize;
    let mut content_end = 0usize;
    let mut i = 0usize;
    let mut iter = line.char_indices().peekable();
    loop {
        let gap_start = i;
        while let Some(&(_, c)) = iter.peek() {
            if c == ' ' || c == '\t' {
                let (b, c) = iter.next().unwrap();
                i = b + c.len_utf8();
            } else {
                break;
            }
        }
        let gap_w = unicode_width::UnicodeWidthStr::width(&line[gap_start..i]);

        let word_start = i;
        while let Some(&(_, c)) = iter.peek() {
            if c != ' ' && c != '\t' {
                let (b, c) = iter.next().unwrap();
                i = b + c.len_utf8();
            } else {
                break;
            }
        }
        let word_end = i;
        if word_start == word_end {
            break;
        }
        let word_w = unicode_width::UnicodeWidthStr::width(&line[word_start..word_end]);

        if line_w + gap_w + word_w <= max_w {
            line_w += gap_w + word_w;
            content_end = word_end;
        } else if word_w <= max_w {
            if content_end > line_start {
                rows.push((line_start, content_end));
            }
            line_start = word_start;
            line_w = word_w;
            content_end = word_end;
        } else {
            if content_end > line_start {
                rows.push((line_start, content_end));
            }
            let mut seg_start = word_start;
            let mut w = 0usize;
            for (b, c) in line[word_start..word_end].char_indices() {
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                let abs_b = word_start + b;
                if w + cw > max_w && w > 0 {
                    rows.push((seg_start, abs_b));
                    seg_start = abs_b;
                    w = cw;
                } else {
                    w += cw;
                }
            }
            rows.push((seg_start, word_end));
            line_start = word_end;
            line_w = 0;
            content_end = word_end;
        }
    }
    if i > content_end {
        content_end = i;
    }
    if content_end > line_start {
        rows.push((line_start, content_end));
    }
    if rows.is_empty() {
        rows.push((0, 0));
    }
    rows
}

pub(super) fn textarea_total_rows(text: &str, max_w: u16) -> u16 {
    let max_w = (max_w as usize).max(1);
    let mut total = 0u16;
    for line in text.split('\n') {
        total = total.saturating_add(wrap_line(line, max_w).len() as u16);
    }
    total.max(1)
}

pub(super) fn textarea_cursor_xy(text: &str, cursor: usize, max_w: u16) -> (u16, u16) {
    let max_w = (max_w as usize).max(1);
    let mut row: u16 = 0;
    let mut byte = 0usize;
    for line in text.split('\n') {
        let llen = line.len();
        if cursor <= byte + llen {
            let rel = cursor - byte;
            let rows = wrap_line(line, max_w);
            let mut idx = 0usize;
            for (k, (s, _)) in rows.iter().enumerate() {
                if *s <= rel {
                    idx = k;
                } else {
                    break;
                }
            }
            let (s, e) = rows[idx];
            let col = unicode_width::UnicodeWidthStr::width(&line[s..rel.min(e)]);
            return (row + idx as u16, col as u16);
        }
        row += wrap_line(line, max_w).len() as u16;
        byte += llen + 1;
    }
    (row, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrapped(line: &str, w: usize) -> Vec<String> {
        wrap_line(line, w)
            .iter()
            .map(|(s, e)| line[*s..*e].to_string())
            .collect()
    }

    #[test]
    fn wrap_matches_ratatui_long_sentence() {
        let line =
            "abcd efghij klmnopabcd efgh ijklmnopabcdefg hijkl mnopab c d e f g h i j k l m n o";
        assert_eq!(
            wrapped(line, 20),
            vec![
                "abcd efghij",
                "klmnopabcd efgh",
                "ijklmnopabcdefg",
                "hijkl mnopab c d e f",
                "g h i j k l m n o",
            ]
        );
    }

    #[test]
    fn wrap_long_word_breaks_at_width() {
        let line: String = "a".repeat(75);
        let rows = wrapped(&line, 20);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].len(), 20);
        assert_eq!(rows[1].len(), 20);
        assert_eq!(rows[3].len(), 15);
    }

    #[test]
    fn cursor_xy_single_line() {
        assert_eq!(textarea_cursor_xy("hello", 0, 80), (0, 0));
        assert_eq!(textarea_cursor_xy("hello", 2, 80), (0, 2));
        assert_eq!(textarea_cursor_xy("hello", 5, 80), (0, 5));
    }

    #[test]
    fn cursor_xy_multiline() {
        assert_eq!(textarea_cursor_xy("ab\ncd", 5, 80), (1, 2));
        assert_eq!(textarea_cursor_xy("ab\ncd", 3, 80), (1, 0));
    }

    #[test]
    fn cursor_xy_wraps_at_word_boundary() {
        assert_eq!(textarea_cursor_xy("hello world", 11, 11), (0, 11));
        assert_eq!(textarea_cursor_xy("hello world", 11, 5), (1, 5));
    }

    #[test]
    fn cursor_xy_trailing_space() {
        assert_eq!(textarea_cursor_xy("hello ", 6, 80), (0, 6));
        assert_eq!(textarea_cursor_xy("hello   ", 7, 80), (0, 7));
        assert_eq!(textarea_cursor_xy("   ", 3, 80), (0, 3));
    }

    #[test]
    fn total_rows_empty() {
        assert_eq!(textarea_total_rows("", 80), 1);
    }

    #[test]
    fn total_rows_single_line() {
        assert_eq!(textarea_total_rows("hello", 80), 1);
    }

    #[test]
    fn total_rows_newlines() {
        assert_eq!(textarea_total_rows("a\nb\nc", 80), 3);
    }

    #[test]
    fn total_rows_word_wrap() {
        assert_eq!(textarea_total_rows("hello world hello world", 11), 2);
        assert_eq!(textarea_total_rows("a b c d e", 3), 3);
    }

    #[test]
    fn total_rows_long_word() {
        let line: String = "a".repeat(75);
        assert_eq!(textarea_total_rows(&line, 20), 4);
    }

    #[test]
    fn layout_grows_and_scrolls() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_size(80, 24);
        s.update_textarea_layout();
        assert_eq!(s.textarea_height, 1);
        assert_eq!(s.textarea_scroll, 0);

        let long: String = "word ".repeat(80);
        s.set_text(&long);
        s.update_textarea_layout();
        assert!(s.textarea_height >= 1);
        let max_h = 24 / 3;
        assert!(s.textarea_height <= max_h);
        let (crow, _) = textarea_cursor_xy(&s.textarea, s.cursor, 76);
        assert!((crow as usize) >= s.textarea_scroll);
        assert!((crow as usize) < s.textarea_scroll + s.textarea_height as usize);
    }

    #[test]
    fn multibyte_char_keeps_cursor_on_boundary() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_size(80, 24);
        s.insert_char('▸');
        assert!(s.textarea.is_char_boundary(s.cursor));
        assert_eq!(s.cursor, 3);
        s.update_textarea_layout();
        let _ = textarea_cursor_xy(&s.textarea, s.cursor, 76);

        s.cursor_left();
        assert_eq!(s.cursor, 0);
        s.cursor_right();
        assert_eq!(s.cursor, 3);

        s.delete_before_cursor();
        assert_eq!(s.textarea, "");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn cursor_up_down_moves_between_lines() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("hello world\nfoo\nbar");
        s.cursor_end();
        assert_eq!(s.cursor, s.textarea.len());
        assert!(s.cursor_up());
        assert_eq!(&s.textarea[s.cursor..], "\nbar");
        let line1_start = s.cursor;
        assert!(s.cursor_up());
        assert!(s.cursor < line1_start);
        assert!(s.cursor_down());
        assert_eq!(s.cursor, line1_start);
    }

    #[test]
    fn cursor_up_returns_false_on_first_line() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("abc\ndef");
        s.cursor_home();
        assert!(!s.cursor_up());
    }

    #[test]
    fn cursor_down_returns_false_on_last_line() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("abc\ndef");
        s.cursor_end();
        assert!(!s.cursor_down());
    }

    #[test]
    fn cursor_up_down_clamps_short_line() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("hello world\nab");
        s.cursor_home();
        for _ in 0..5 {
            s.cursor_right();
        }
        assert_eq!(s.cursor, 5);
        assert!(s.cursor_down());
        assert_eq!(s.cursor, s.textarea.len());
        assert!(s.cursor_up());
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn word_jump_skips_words() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("hello world foo");
        s.cursor_home();
        s.cursor_right_word();
        assert_eq!(s.cursor, 5);
        s.cursor_right_word();
        assert_eq!(s.cursor, 11);
        s.cursor_right_word();
        assert_eq!(s.cursor, s.textarea.len());
    }

    #[test]
    fn word_jump_left() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("hello world foo");
        s.cursor_end();
        s.cursor_left_word();
        assert_eq!(s.cursor, 12);
        s.cursor_left_word();
        assert_eq!(s.cursor, 6);
        s.cursor_left_word();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn complete_prefix_template_replaces_only_query() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("review /cr");
        s.cursor_end();
        s.complete_prefix('/', "/code_review ");
        assert_eq!(s.textarea, "review /code_review ");
        assert_eq!(s.cursor, s.textarea.len());
    }

    #[test]
    fn complete_prefix_file_keeps_at_and_surrounding_text() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("see @sr");
        s.cursor_end();
        s.complete_prefix('@', "@src/main.rs ");
        assert_eq!(s.textarea, "see @src/main.rs ");
        assert_eq!(s.cursor, s.textarea.len());
    }

    #[test]
    fn complete_prefix_uses_last_trigger_before_cursor() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("/a and /b");
        s.cursor_end();
        s.complete_prefix('/', "/beta ");
        assert_eq!(s.textarea, "/a and /beta ");
    }

    #[test]
    fn complete_prefix_noop_without_trigger() {
        let mut s = ChatScreen::new(".".into(), vec![], true, true);
        s.set_text("no trigger here");
        s.cursor_end();
        s.complete_prefix('/', "/x ");
        assert_eq!(s.textarea, "no trigger here");
    }
}
