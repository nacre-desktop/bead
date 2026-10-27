// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::transcript::{Entry, Meter, Style, Transcript};

#[test]
fn five_hundred_markdown_messages_materialize_only_the_viewport_window() {
    let mut transcript = Transcript::new(
        Style::new(420., "IBM Plex Sans", "JetBrainsMono Nerd Font"),
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
