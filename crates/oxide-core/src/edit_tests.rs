use super::*;

fn one(text: &str, old: &str, new: &str) -> Result<EditResult, EditError> {
    apply_edits(text, &[Edit::new(old, new)])
}

#[test]
fn exact_match_replaces_once() {
    let r = one("a\nb\nc\n", "b", "B").expect("applies");
    assert_eq!(r.text, "a\nB\nc\n");
    assert_eq!(r.applied[0].matched, MatchKind::Exact);
    assert_eq!(r.ranges, vec![LineRange { start: 2, count: 1 }]);
    assert_eq!(r.summary(), "1 edit applied: line 2.");
}

#[test]
fn crlf_files_keep_their_line_endings() {
    let text = "fn a() {\r\n    1\r\n}\r\n";
    let r = one(text, "fn a() {\n    1\n}", "fn a() {\n    2\n}").expect("applies");
    assert_eq!(r.text, "fn a() {\r\n    2\r\n}\r\n");
    assert_eq!(r.applied[0].matched, MatchKind::LineEndings);
    // And the other way round: an LF file edited with CRLF text.
    let r = one("x\ny\n", "x\r\ny", "x\r\nz").expect("applies");
    assert_eq!(r.text, "x\nz\n");
}

#[test]
fn whitespace_around_the_passage_is_forgiven() {
    let r = one(
        "let a = 1; let b = 2;",
        "  let b = 2; \n",
        "  let b = 3; \n",
    )
    .expect("applies");
    assert_eq!(r.text, "let a = 1; let b = 3;");
    assert_eq!(r.applied[0].matched, MatchKind::Trimmed);
}

#[test]
fn trailing_spaces_on_lines_are_ignored() {
    let text = "a  \nb\t\nc\n";
    let r = one(text, "a\nb\n", "A\nB\n").expect("applies");
    assert_eq!(r.text, "A\nB\nc\n");
    assert_eq!(r.applied[0].matched, MatchKind::TrailingWhitespace);
}

#[test]
fn indentation_is_ignored_and_the_new_text_reindented() {
    let text = "class A {\n    fn f() {\n        x();\n    }\n}\n";
    // The model quoted the method without its indentation.
    let r = one(
        text,
        "fn f() {\n    x();\n}",
        "fn f() {\n    x();\n    y();\n}",
    )
    .expect("applies");
    assert_eq!(
        r.text,
        "class A {\n    fn f() {\n        x();\n        y();\n    }\n}\n"
    );
    assert_eq!(r.applied[0].matched, MatchKind::Indentation);
    assert_eq!(r.ranges, vec![LineRange { start: 2, count: 4 }]);

    // Quoted with more indentation than the file has.
    let r = one(
        "if a:\n  b()\n",
        "        if a:\n          b()",
        "        if a:\n          c()",
    )
    .expect("applies");
    assert_eq!(r.text, "if a:\n  c()\n");
}

#[test]
fn ambiguous_text_must_be_made_unique() {
    let e = one("x = 1\nx = 1\ny\n", "x = 1", "x = 2").expect_err("ambiguous");
    assert_eq!(
        e.kind,
        EditErrorKind::Ambiguous {
            count: 2,
            lines: vec![1, 2]
        }
    );
    let msg = e.message("old_str");
    assert!(msg.contains("found 2 times"), "{msg}");
    assert!(msg.contains("replace_all"), "{msg}");
}

#[test]
fn replace_all_changes_every_place() {
    let r = apply_edits("a b a b a", &[Edit::all("a", "c")]).expect("applies");
    assert_eq!(r.text, "c b c b c");
    assert_eq!(r.applied[0].replacements, 3);
    let r = apply_edits("foo\nbar\nfoo\n", &[Edit::all("foo", "baz")]).expect("applies");
    assert_eq!(r.text, "baz\nbar\nbaz\n");
    assert_eq!(
        r.ranges,
        vec![
            LineRange { start: 1, count: 1 },
            LineRange { start: 3, count: 1 }
        ]
    );
    assert_eq!(r.summary(), "1 edit applied (2 places): line 1, line 3.");
}

#[test]
fn missing_text_quotes_the_closest_lines() {
    let text = "fn main() {\n    let title_color = \"red\";\n    println!(\"hi\");\n}\n";
    let e = one(text, "    let title_colour = \"red\";", "x").expect_err("missing");
    let EditErrorKind::NotFound { closest: Some(c) } = &e.kind else {
        panic!("expected closest lines: {e:?}");
    };
    assert_eq!(c.start, 2);
    assert!(c.lines[0].contains("title_color"));
    let msg = e.message("old_str");
    assert!(msg.contains("wasn't found"), "{msg}");
    assert!(msg.contains("line 2"), "{msg}");
    assert!(msg.contains("    2      let title_color"), "{msg}");

    let e = one(text, "something completely different here", "x").expect_err("missing");
    assert_eq!(e.kind, EditErrorKind::NotFound { closest: None });
}

#[test]
fn edits_apply_in_sequence_all_or_nothing() {
    let text = "one\ntwo\nthree\n";
    let r = apply_edits(
        text,
        &[Edit::new("one", "uno"), Edit::new("uno\ntwo", "uno\ndos")],
    )
    .expect("applies");
    assert_eq!(r.text, "uno\ndos\nthree\n");
    assert_eq!(r.ranges, vec![LineRange { start: 1, count: 2 }]);
    assert_eq!(r.summary(), "2 edits applied: lines 1–2.");

    let e = apply_edits(text, &[Edit::new("one", "1"), Edit::new("four", "4")])
        .expect_err("second fails");
    assert_eq!(e.index, 1);
    assert_eq!(e.total, 2);
    let msg = e.message("old");
    assert!(msg.starts_with("Edit 2 of 2: "), "{msg}");
    assert!(msg.contains("Nothing was changed"), "{msg}");
}

#[test]
fn overlapping_edits_see_the_earlier_result() {
    // The second edit targets text the first one replaced: it is gone.
    let e = apply_edits(
        "alpha beta gamma",
        &[Edit::new("beta", "BETA"), Edit::new("beta gamma", "x")],
    )
    .expect_err("gone");
    assert_eq!(e.index, 1);
    // A second edit spanning the first one's result works.
    let r = apply_edits(
        "alpha beta gamma",
        &[Edit::new("beta", "BETA"), Edit::new("BETA gamma", "x")],
    )
    .expect("applies");
    assert_eq!(r.text, "alpha x");
}

#[test]
fn ranges_follow_later_edits_and_deletions() {
    let text = "1\n2\n3\n4\n5\n6\n";
    let r = apply_edits(
        text,
        &[Edit::new("5\n", "five\nFIVE\n"), Edit::new("2\n3\n", "")],
    )
    .expect("applies");
    assert_eq!(r.text, "1\n4\nfive\nFIVE\n6\n");
    assert_eq!(
        r.ranges,
        vec![
            LineRange { start: 2, count: 0 },
            LineRange { start: 3, count: 2 }
        ]
    );
    assert_eq!(
        r.summary(),
        "2 edits applied: removed at line 2, lines 3–4."
    );
    let ex = r.excerpt(10);
    assert!(ex.contains("(lines removed before line 2)"), "{ex}");
    assert!(ex.contains("    3  five"), "{ex}");
}

#[test]
fn unicode_text_is_handled_by_characters() {
    let r = one("Grüße, 世界 👋\nnext\n", "世界 👋", "verden 🌍").expect("applies");
    assert_eq!(r.text, "Grüße, verden 🌍\nnext\n");
    let e = one("Grüße, 世界\n", "Grüsse, 世界!", "x").expect_err("missing");
    assert!(matches!(
        e.kind,
        EditErrorKind::NotFound { closest: Some(_) }
    ));
}

#[test]
fn no_op_and_empty_edits_are_refused() {
    let e = one("abc", "b", "b").expect_err("same");
    assert_eq!(e.kind, EditErrorKind::Unchanged);
    let e = one("abc", "", "x").expect_err("empty old");
    assert_eq!(e.kind, EditErrorKind::EmptyOld);
    let e = apply_edits("abc", &[]).expect_err("none");
    assert_eq!(e.kind, EditErrorKind::NoEdits);
    // An empty file takes the new text whole.
    let r = one("", "", "hello\n").expect("applies");
    assert_eq!(r.text, "hello\n");
    assert_eq!(r.applied[0].matched, MatchKind::EmptyFile);
}

#[test]
fn a_passage_at_the_end_without_a_line_break() {
    let r = one("a\n  b", "b\n", "c\n").expect("applies");
    assert_eq!(r.text, "a\n  c");
}

#[test]
fn closest_stays_bounded_on_large_files() {
    let text: String = (0..30_000).map(|i| format!("line number {i}\n")).collect();
    let e = one(&text, "line nunber 12345\nline number 12346", "x").expect_err("missing");
    let EditErrorKind::NotFound { closest: Some(c) } = e.kind else {
        panic!("expected closest");
    };
    assert!(c.lines[0].starts_with("line number"));
}

#[test]
fn the_old_artifact_update_cases_still_work() {
    // From the artifact tool's own matcher, which this engine replaced.
    assert_eq!(one("a b c", "b", "B").expect("applies").text, "a B c");
    assert_eq!(
        one("fn a() {}\n", "  fn a() {}  \n\n", "fn b() {}")
            .expect("applies")
            .text,
        "fn b() {}\n"
    );
    assert_eq!(
        one("x\ny\nz", "x\r\ny", "X\nY").expect("applies").text,
        "X\nY\nz"
    );
}

#[test]
fn line_changes_count_a_line_diff() {
    assert_eq!(line_changes("a\nb\nc\n", "a\nB\nc\n"), (1, 1));
    assert_eq!(line_changes("a\nb\n", "a\nb\nc\nd\n"), (2, 0));
    assert_eq!(line_changes("a\nb\nc\n", "c\n"), (0, 2));
    assert_eq!(line_changes("x\ny\nz\n", "y\nz\nx\n"), (1, 1));
    assert_eq!(line_changes("same", "same"), (0, 0));
}

#[test]
fn checkpoint_changes_list_added_changed_and_deleted_files() {
    use crate::app_builder::{FileChangeKind, checkpoint_changes};
    let f = |p: &str, t: &str| (p.to_owned(), t.as_bytes().to_vec());
    let c = checkpoint_changes(
        &[
            f("App.tsx", "a\nb\n"),
            f("old.ts", "x\ny\n"),
            f("same.ts", "s"),
        ],
        &[
            f("App.tsx", "a\nB\nc\n"),
            f("new.ts", "n\n"),
            f("same.ts", "s"),
        ],
    );
    let got: Vec<(&str, FileChangeKind, u32, u32)> = c
        .iter()
        .map(|c| (c.path.as_str(), c.kind, c.added, c.removed))
        .collect();
    assert_eq!(
        got,
        vec![
            ("App.tsx", FileChangeKind::Changed, 2, 1),
            ("new.ts", FileChangeKind::Added, 1, 0),
            ("old.ts", FileChangeKind::Deleted, 0, 2),
        ]
    );
}

#[test]
fn what_matches_is_applied_and_the_rest_reported() {
    let text = "a\nb\nc\n";
    let (done, failed) = apply_what_matches(
        text,
        &[
            Edit::new("a", "A"),
            Edit::new("invented line", "x"),
            Edit::new("c", "C"),
        ],
    );
    let done = done.expect("two matched");
    assert_eq!(done.text, "A\nb\nC\n");
    assert_eq!(done.applied.len(), 2);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].index, 1);
    assert_eq!(failed[0].total, 3);
    assert!(failed[0].message("old").starts_with("Edit 2 of 3: "));

    let (done, failed) = apply_what_matches(text, &[Edit::new("zzz", "y")]);
    assert!(done.is_none());
    assert_eq!(failed.len(), 1);
    let (done, failed) = apply_what_matches(text, &[Edit::new("b", "B")]);
    assert_eq!(done.expect("matched").text, "a\nB\nc\n");
    assert!(failed.is_empty());
}
