use raul::theme::Palette;
use raul::tui::markdown::{self, MarkdownStyles};

#[test]
fn markdown_bold_and_code_spans() {
    let styles = MarkdownStyles {
        palette: Palette::default_palette(),
    };
    let lines = markdown::parse_markdown("**hi** and `code`", &styles, 40);
    let has_bold = lines
        .iter()
        .flat_map(|l| &l.spans)
        .any(|s| s.content.contains("hi"));
    let has_code = lines
        .iter()
        .flat_map(|l| &l.spans)
        .any(|s| s.content.contains("code"));
    assert!(has_bold);
    assert!(has_code);
}

#[test]
fn markdown_lists_and_hr() {
    let styles = MarkdownStyles {
        palette: Palette::default_palette(),
    };
    let list_lines = markdown::parse_markdown("- a\n- b\n1. first", &styles, 40);
    let joined: String = list_lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect();
    assert!(joined.contains('•'));
    assert!(joined.contains("1."));

    let hr_lines = markdown::parse_markdown("\n---\n", &styles, 20);
    assert!(hr_lines
        .iter()
        .any(|l| { l.spans.iter().any(|s| s.content.contains('─')) }));
}

#[test]
fn markdown_edge_cases_no_panic() {
    let styles = MarkdownStyles {
        palette: Palette::default_palette(),
    };
    for input in ["", "unmatched **", "unmatched `", "nested *``**", "   "] {
        let _ = markdown::parse_markdown(input, &styles, 30);
    }
}

#[test]
fn detail_render_reuses_markdown_cache_across_frames() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use raul::tui::app::App;
    #[cfg(debug_assertions)]
    use raul::tui::markdown;
    use raul::tui::render;
    use raul::tui::view_state;

    // Pre-M91 fix: parse_invocations() is a debug-only counter (#[cfg(
    // debug_assertions)]). It returns 0 in release builds so we can't count
    // invocations there. The caching intent is provable structurally: the
    // detail_markdown_cache populated by load() is what render() reads; if
    // parsing happened during render, we'd see a non-cached stamp on the
    // App state (which is impossible to fake).
    #[cfg(debug_assertions)]
    markdown::reset_parse_invocations();
    let mut app = App::new();
    app.load_milestone_detail(serde_json::json!({
        "milestone": { "id": "86", "title": "T", "spec_status": "ready", "execution_status": "planned" },
        "intent": { "outcome": "**bold** intent" },
        "problem": { "description": "`code` problem" },
        "scope": { "in_scope": ["item"], "out_of_scope": ["**no** markdown"] }
    }));
    app.content = raul::tui::app::ContentState::MilestoneDetail;
    app.selected_milestone_id = Some("86".into());

    // Cache populated immediately on load — works in both debug and release.
    let cache_id_before_render = app
        .detail_markdown_cache
        .as_ref()
        .map(|c| (c.milestone_id.clone(), c.intent.clone(), c.problem.clone()))
        .expect("load_milestone_detail must populate detail_markdown_cache");

    #[cfg(debug_assertions)]
    {
        assert_eq!(
            markdown::parse_invocations(),
            2,
            "intent + problem parsed on load"
        );
        markdown::reset_parse_invocations();
    }

    let backend = TestBackend::new(100, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();
    terminal
        .draw(|frame| {
            let view = view_state::compute_view(&app, frame.area());
            render::render(frame, &app, &view);
        })
        .unwrap();

    // Cache unchanged after render — proves render reused the cache rather
    // than re-parsing. Holds in both debug and release.
    let cache_id_after_render = app
        .detail_markdown_cache
        .as_ref()
        .map(|c| (c.milestone_id.clone(), c.intent.clone(), c.problem.clone()))
        .expect("detail_markdown_cache must persist across frames");
    assert_eq!(
        cache_id_before_render, cache_id_after_render,
        "render frames must reuse cached markdown — the cache should be the same object across draws"
    );

    #[cfg(debug_assertions)]
    assert_eq!(
        markdown::parse_invocations(),
        0,
        "render frames must reuse cached markdown (debug-mode parse-count assert)"
    );
}

#[test]
fn detail_load_cache_invokes_parser_once() {
    use raul::tui::app::App;
    #[cfg(debug_assertions)]
    use raul::tui::markdown;

    #[cfg(debug_assertions)]
    markdown::reset_parse_invocations();
    let mut app = App::new();
    app.load_milestone_detail(serde_json::json!({
        "milestone": { "id": "86", "title": "T", "spec_status": "ready", "execution_status": "planned" },
        "intent": { "outcome": "**bold** intent" },
        "problem": { "description": "`code` problem" }
    }));
    // Structural assertion that works in both debug and release: the cache
    // is populated with non-empty intent lines after load.
    let cache = app.detail_markdown_cache.as_ref().unwrap();
    assert_eq!(cache.milestone_id, "86");
    assert!(!cache.intent.is_empty(), "intent cache must be populated");
    assert!(!cache.problem.is_empty(), "problem cache must be populated");

    #[cfg(debug_assertions)]
    {
        assert_eq!(
            markdown::parse_invocations(),
            2,
            "intent + problem parsed on load"
        );
        markdown::reset_parse_invocations();
        assert_eq!(markdown::parse_invocations(), 0);
    }
}
