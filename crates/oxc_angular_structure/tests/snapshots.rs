//! Each fixture template's structure, as `parse_template` writes it.

use oxc_angular_structure::{WriteOptions, parse_template};

#[test]
fn fixture_templates() {
    insta::glob!("fixtures/*.html", |path| {
        let template = std::fs::read_to_string(path).unwrap();
        insta::assert_json_snapshot!(parse_template(
            &template,
            "fixture.html",
            WriteOptions::default()
        ));
    });
}

#[test]
fn whitespace_text_is_written_on_request() {
    let template = "<p>\n  <b>x</b>\n</p>";
    let count = |options| {
        serde_json::to_string(&parse_template(template, "t.html", options))
            .unwrap()
            .matches("\"text\"")
            .count()
    };
    assert_eq!(count(WriteOptions::default()), 1);
    assert_eq!(count(WriteOptions { whitespace_text: true }), 3);
}
