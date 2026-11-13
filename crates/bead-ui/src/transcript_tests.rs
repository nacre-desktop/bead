// SPDX-License-Identifier: MIT
use crate::transcript::{Entry, Meter, Style, Transcript};

#[test]
fn five_hundred_markdown_messages_materialize_only_the_viewport_window() {
    let mut transcript = Transcript::new(
        Style::new(420., "Fixture Sans", "Fixture Mono"),
        Meter::fixed(Vec::new()),
    );
    let entries: Vec<_> = (0..500)
        .map(|index| Entry {
            key: format!("message-{index:03}"),
            label: "Assistant".into(),
            fill: "surfaceContainer".into(),
            text: if index % 10 == 0 {
                format!("Summary {index}\n\n```rust\nlet row = {index};\n```\n")
            } else {
                format!("Synthetic message {index}: {}", "markdown text ".repeat(8))
            },
            markdown: true,
            actions: vec![],
            code_action: Some(("Copy code".into(), "code".into())),
            align_end: false,
            max_width: 0.,
        })
        .collect();
    let extent = transcript.layout(&entries);
    assert!(extent > 500.);
    let first = transcript.window(0., 460., &entries);
    assert!(first.len() < 100, "{} cells materialized", first.len());
    assert!(first.iter().any(|cell| cell["b"] == "surfaceContainerHigh"));
    assert!(transcript.materialized_rows() < 100);

    let last = transcript.window(extent - 460., 460., &entries);
    assert!(last.len() < 100, "{} cells materialized", last.len());
    assert!(
        last.iter()
            .any(|cell| cell["id"].as_str().unwrap().starts_with("message-499:"))
    );
    assert!(transcript.materialized_rows() < 100);
}

/// B0274: a message may be right-aligned and capped to a fraction of the row, and one without a
/// label drops the caption band; the others keep their full-row layout.
#[test]
fn entries_align_to_the_end_with_a_max_width_and_no_label_band() {
    let entry = |key: &str, label: &str, align_end: bool, max_width: f32| Entry {
        key: key.into(),
        label: label.into(),
        fill: "primaryContainer".into(),
        text: "Hello there, a short question".into(),
        markdown: false,
        actions: vec![],
        code_action: None,
        align_end,
        max_width,
    };
    let mut transcript = Transcript::new(
        Style::new(400., "Fixture Sans", "Fixture Mono"),
        Meter::fixed(Vec::new()),
    );
    let entries = [
        entry("a", "Assistant", false, 0.),
        entry("u", "", true, 0.88),
    ];
    transcript.layout(&entries);
    let cells = transcript.window(0., 1000., &entries);
    let bubble = |key: &str| {
        cells
            .iter()
            .find(|c| c["id"].as_str().unwrap().starts_with(&format!("{key}:")) && c["k"] == "fill")
            .unwrap()
    };
    let (a, u) = (bubble("a"), bubble("u"));
    assert_eq!((a["x"].as_f64(), a["w"].as_f64()), (Some(0.), Some(400.)));
    assert_eq!((u["x"].as_f64(), u["w"].as_f64()), (Some(48.), Some(352.)));
    assert!(u["h"].as_f64() < a["h"].as_f64(), "no label band");
    let text = cells
        .iter()
        .find(|c| c["id"].as_str().unwrap().starts_with("u:") && c["k"] == "text")
        .unwrap();
    assert!(text["x"].as_f64().unwrap() > 48. && text["w"].as_f64().unwrap() < 352.);
}
