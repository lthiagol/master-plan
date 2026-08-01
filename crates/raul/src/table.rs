use std::fmt;

/// Lightweight ASCII table for CLI output (replaces comfy-table).
#[derive(Debug, Default)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_header(&mut self, headers: Vec<impl Into<String>>) -> &mut Self {
        self.headers = headers.into_iter().map(Into::into).collect();
        self
    }

    pub fn add_row(&mut self, row: Vec<impl Into<String>>) -> &mut Self {
        self.rows.push(row.into_iter().map(Into::into).collect());
        self
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.headers.is_empty() && self.rows.is_empty() {
            return Ok(());
        }

        let col_count = self
            .headers
            .len()
            .max(self.rows.iter().map(|r| r.len()).max().unwrap_or(0));

        let mut widths = vec![0usize; col_count];
        for (i, h) in self.headers.iter().enumerate() {
            widths[i] = widths[i].max(visible_width(h));
        }
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    widths[i] = widths[i].max(visible_width(cell));
                }
            }
        }

        if !self.headers.is_empty() {
            write_border(f, &widths, BorderLine::Top)?;
            write_row(f, &widths, &self.headers)?;
            write_border(f, &widths, BorderLine::Mid)?;
        } else if !self.rows.is_empty() {
            write_border(f, &widths, BorderLine::Top)?;
        }

        for row in &self.rows {
            write_row(f, &widths, row)?;
        }

        if !self.rows.is_empty() || !self.headers.is_empty() {
            write_border(f, &widths, BorderLine::Bottom)?;
        }

        Ok(())
    }
}

enum BorderLine {
    Top,
    Mid,
    Bottom,
}

fn write_border(f: &mut fmt::Formatter<'_>, widths: &[usize], kind: BorderLine) -> fmt::Result {
    let (left, mid, cross, right) = match kind {
        BorderLine::Top => ('┌', '┬', '─', '┐'),
        BorderLine::Mid => ('├', '┼', '─', '┤'),
        BorderLine::Bottom => ('└', '┴', '─', '┘'),
    };
    write!(f, "{left}")?;
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            write!(f, "{mid}")?;
        }
        for _ in 0..*w + 2 {
            write!(f, "{cross}")?;
        }
    }
    writeln!(f, "{right}")
}

fn write_row(f: &mut fmt::Formatter<'_>, widths: &[usize], cells: &[String]) -> fmt::Result {
    write!(f, "│")?;
    for (i, w) in widths.iter().enumerate() {
        let cell = cells.get(i).map(String::as_str).unwrap_or("");
        let pad = w.saturating_sub(visible_width(cell));
        write!(f, " {cell}")?;
        for _ in 0..pad {
            write!(f, " ")?;
        }
        write!(f, " │")?;
    }
    writeln!(f)
}

fn visible_width(s: &str) -> usize {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            while chars.next().is_some_and(|n| n != 'm') {}
            continue;
        }
        out.push(c);
    }
    out.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_header_and_rows() {
        let mut table = Table::new();
        table.set_header(vec!["Metric", "Value"]);
        table.add_row(vec![String::from("Mode"), String::from("autonomous")]);
        let rendered = table.to_string();
        assert!(rendered.contains("Metric"));
        assert!(rendered.contains("autonomous"));
        assert!(rendered.contains('┌'));
        assert!(rendered.contains('└'));
    }

    #[test]
    fn visible_width_strips_ansi() {
        assert_eq!(visible_width("\x1b[1mhi\x1b[0m"), 2);
    }

    #[test]
    fn colored_cells_keep_border_alignment() {
        let mut table = Table::new();
        table.set_header(vec!["Spec", "Exec"]);
        table.add_row(vec![
            "\x1b[32mverified\x1b[0m".to_string(),
            "\x1b[32mdone\x1b[0m".to_string(),
        ]);
        table.add_row(vec![
            "\x1b[33mready\x1b[0m".to_string(),
            "\x1b[36min-progress\x1b[0m".to_string(),
        ]);

        let rendered = table.to_string();
        let line_widths: Vec<usize> = rendered
            .lines()
            .map(strip_ansi_for_test)
            .map(|line| line.chars().count())
            .collect();
        let expected = line_widths[0];
        assert!(
            line_widths.iter().all(|w| *w == expected),
            "misaligned rows: {line_widths:?}\n{rendered}"
        );
    }

    fn strip_ansi_for_test(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                while chars.next().is_some_and(|n| n != 'm') {}
                continue;
            }
            out.push(c);
        }
        out
    }
}
