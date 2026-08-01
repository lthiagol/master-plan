use raul::table::Table;

#[test]
fn table_renders_header_and_rows() {
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
fn table_empty_renders_nothing() {
    let table = Table::new();
    assert!(table.to_string().is_empty());
}

#[test]
fn table_colored_cells_align_borders() {
    let mut table = Table::new();
    table.set_header(vec!["Spec", "Exec"]);
    table.add_row(vec![
        String::from("\x1b[32mverified\x1b[0m"),
        String::from("\x1b[32mdone\x1b[0m"),
    ]);
    table.add_row(vec![
        String::from("\x1b[33mready\x1b[0m"),
        String::from("\x1b[36min-progress\x1b[0m"),
    ]);
    let rendered = table.to_string();
    let widths: Vec<usize> = rendered
        .lines()
        .map(|line| strip_ansi(line).chars().count())
        .collect();
    assert!(
        widths.iter().all(|w| *w == widths[0]),
        "border misalignment: {widths:?}"
    );
}

fn strip_ansi(s: &str) -> String {
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
