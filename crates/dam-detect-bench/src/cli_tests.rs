use super::*;

#[test]
fn parse_args_defaults_to_text_format() {
    let format = parse_args(Vec::<String>::new()).expect("args should parse");

    assert_eq!(format, OutputFormat::Text);
}

#[test]
fn parse_args_accepts_markdown_format() {
    let format =
        parse_args(["--format".to_string(), "markdown".to_string()]).expect("args should parse");

    assert_eq!(format, OutputFormat::Markdown);
}

#[test]
fn parse_args_rejects_unknown_format() {
    let error = parse_args(["--format".to_string(), "xml".to_string()])
        .expect_err("unsupported format should fail");

    assert!(error.contains("unsupported format"));
}
