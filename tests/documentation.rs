mod support;
use support::Case;

const BINDINGS: &str = include_str!("../docs/reference/bindings.md");
const ACCESS: &str = include_str!("../docs/reference/access.md");
const ESCAPE: &str = include_str!("../docs/reference/escape.md");

// Only runnable programs use rusk fences on these pages; output and syntax
// fragments use other fences. Read the published source without a second copy.
fn runnable_examples(document: &str, cases: &[(&str, &str)]) {
    let blocks: Vec<_> = document
        .split("```rusk\n")
        .skip(1)
        .map(|block| block.split_once("```").unwrap().0)
        .collect();
    assert_eq!(blocks.len(), cases.len(), "update example coverage");
    for (source, (input, expected)) in blocks.into_iter().zip(cases) {
        // These include examples assume a main.rusk at the repository root.
        // Tests write main.rusk into a fresh temporary directory instead.
        let source = source.replace(
            "include \"examples/",
            &format!("include \"{}/examples/", env!("CARGO_MANIFEST_DIR")),
        );
        let output = Case::new(&source).run(input);
        assert!(
            output.status.success(),
            "{source}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected.as_bytes(), "{source}");
        assert!(output.stderr.is_empty(), "{source}");
    }
}

#[test]
fn introduction_examples_match_the_published_input_and_output() {
    runnable_examples(include_str!("../README.md"), &[("4\n3 1 4 2\n", "10\n")]);
    runnable_examples(
        include_str!("../docs/index.md"),
        &[("4\n3 1 4 2\n", "10 4\n")],
    );
    runnable_examples(
        include_str!("../docs/getting-started/index.md"),
        &[("10 32\n", "42\n"), ("4\n3 1 4 2\n", "10\n")],
    );
}

#[test]
fn tutorial_examples_match_the_published_input_and_output() {
    runnable_examples(
        include_str!("../docs/getting-started/tutorial.md"),
        &[
            ("5\n3 -2 4 0 -1\n", "7\n"),
            ("", "20 20 10\n30 20 10\n"),
            ("", "8 8 2\n8 9 2\n9 2 100 2\n"),
            ("", "7 3\n12\n"),
            ("", "1\n-1\n"),
            ("4\n3 1 4 2\n", "4\n3 8\n"),
        ],
    );
}

#[test]
fn function_examples_keep_argument_and_capture_sharing() {
    runnable_examples(
        include_str!("../docs/reference/functions.md"),
        &[
            ("", "25\n"),
            ("", "9 8 3\n"),
            ("", "7 abc\n"),
            ("", "22\n22\n"),
        ],
    );
}

#[test]
fn closure_examples_keep_capture_copy_and_lazy_consumption() {
    runnable_examples(
        include_str!("../docs/reference/closures.md"),
        &[
            ("", "4 16 6\n14\n"),
            ("", "21\n21\n"),
            ("", "3 6 6\n7 7 6\n"),
            ("", "1 2 1 2\n"),
            ("", "0\n10\n1\n30 50\n"),
        ],
    );
}

#[test]
fn type_catalogue_examples_define_user_types_and_use_existing_standard_types() {
    runnable_examples(
        include_str!("../docs/reference/types.md"),
        &[
            ("", "42 Yes A abc 7\n"),
            ("", "3 4 5 five 5\n1 Yes\n"),
            ("", "6 2 6 10\n"),
            ("", "2\n7\n"),
            ("9\n", "7\n9\n"),
        ],
    );
}

#[test]
fn record_examples_run_without_undeclared_example_types() {
    runnable_examples(
        include_str!("../docs/reference/records.md"),
        &[
            ("", "3 3\n3 9\n"),
            ("", "1 2\n"),
            ("", "2 hello\n2 9\n"),
            ("", "1 one\n2 two\n"),
            ("", "2 1\n1 2\n"),
            ("", "1\n"),
            ("3\n", "Yes\n"),
        ],
    );
}

#[test]
fn algorithm_examples_include_the_required_libraries() {
    runnable_examples(
        include_str!("../docs/library/algorithms.md"),
        &[
            ("", "2 4 6\n16\n0 1 3 6\nYes Yes\n"),
            ("", "7\n"),
            ("", "1 9\n2 0\n"),
            ("", "0 0 1\n"),
            ("", "1 3\n"),
            ("", "9\n10 17\n2\n2\n"),
        ],
    );
}

#[test]
fn data_structure_examples_include_and_define_the_types_they_use() {
    runnable_examples(
        include_str!("../docs/library/data-structures.md"),
        &[
            ("", "Yes No\nNo Yes\n"),
            ("", "6 9\n15\n0\n"),
            ("", "9\n17\nabcdef\nabXef\n"),
            ("", "10\n"),
        ],
    );
}

#[test]
fn sequence_examples_preserve_copies_and_character_lifetimes() {
    runnable_examples(
        include_str!("../docs/library/sequences.md"),
        &[
            ("", "0 7 0\n0 0 0\n"),
            ("", "3 9 2\n9 2 3\n"),
            ("", "bat\nbat!\n"),
        ],
    );
}

#[test]
fn operator_examples_preserve_short_circuiting_updates_and_extreme_copies() {
    runnable_examples(
        include_str!("../docs/reference/operators.md"),
        &[
            ("", "10 4 21\n-3 2 -3 -2\n2 7 5\nC 2\n"),
            ("", "Yes 1\nNo 2\n"),
            ("", "No Yes 0\nYes Yes 2\n"),
            ("", "5 3 5 7\n"),
            ("", "10 10 6\n"),
            ("", "11\n11 99\n"),
            ("", "7\n6\nYes\n"),
        ],
    );
}

// Read the published examples, rather than maintaining a second copy of them.
// Counts below make added/removed examples require an explicit coverage choice.
fn examples(document: &str, count: usize) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = None;
    for line in document.lines() {
        if line == "```" {
            if let Some(source) = current.take() {
                blocks.push(source);
            }
        } else if matches!(line, "```text" | "```rusk") {
            assert!(current.is_none());
            current = Some(String::new());
        } else if let Some(source) = &mut current {
            source.push_str(line);
            source.push('\n');
        }
    }
    assert!(current.is_none(), "unterminated example");
    assert_eq!(blocks.len(), count, "update coverage for changed examples");
    blocks
}

fn runs(source: &str, suffix: &str, expected: &str) {
    let source = format!("{source}\n{suffix}");
    let output = Case::new(&source).run("");
    assert!(
        output.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected.as_bytes(), "{source}");
    assert!(
        output.stderr.is_empty(),
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn rejects(source: &str, expected: &str) -> String {
    let case = Case::new(source);
    let mut previous = None;
    for stage in ["hir", "lir", "cpp"] {
        let output = case.compile_with_options(&["--emit", stage]);
        let message = String::from_utf8(output.stderr).unwrap();
        assert!(!output.status.success(), "{source}");
        assert!(message.contains(expected), "{source}\n{message}");
        assert!(!case.0.join("main.cpp").exists());
        if let Some(previous) = &previous {
            assert_eq!(previous, &message);
        }
        previous = Some(message);
    }
    previous.unwrap()
}

fn replace(source: &str, from: &str, to: &str) -> String {
    assert_eq!(source.matches(from).count(), 1, "example changed: {from}");
    source.replacen(from, to, 1)
}

#[test]
fn fixed_examples_run_and_check_each_initialization_branch() {
    let blocks = examples(BINDINGS, 4);
    for (index, suffix, expected) in [
        (0, "", "5 9\n"),
        (1, "wt increment(left),left,right,next(3);", "2 2 2 4\n"),
        (2, "wt values,alias;", "3 2 4 9\n"),
        (3, "wt choose(true),choose(false);", "1 2\n"),
    ] {
        runs(&blocks[index], suffix, expected);
    }
}

#[test]
fn the_documented_fixed_rebinding_is_rejected_before_output() {
    let blocks = examples(BINDINGS, 4);
    rejects(
        &replace(&blocks[0], "// count := 9;", "count := 9;"),
        "cannot rebind fixed variable 'count'",
    );
    rejects(
        &(blocks[1].clone() + "\nright := 3;"),
        "cannot rebind fixed variable 'right'",
    );
}

#[test]
fn readonly_examples_keep_shared_updates_and_detach_copies() {
    let blocks = examples(ACCESS, 8);
    for (index, suffix, expected) in [
        (0, "wt original,detached;", "2\n2 3\n"),
        (1, "wt twice(left),next(right),working;", "2 3 3\n"),
        (2, "wt value;", "2\n"),
        (3, "", "2 1\n"),
        (4, "", "2\n"),
        (5, "wt result,detached;", "1 2\n"),
    ] {
        runs(&blocks[index], suffix, expected);
    }
}

#[test]
fn commented_readonly_violations_and_the_diagnostic_excerpt_match_the_compiler() {
    let blocks = examples(ACCESS, 8);
    for (index, comment) in [
        (0, "// view = 3;"),
        (0, "// let alias := view; alias += 1;"),
        (2, "// value = 3;"),
        (3, "// values[0] = 3;"),
        (3, "// values.push_back(3);"),
        (4, "// bump(x);"),
        (5, "// result = 2;"),
    ] {
        rejects(
            &replace(
                &blocks[index],
                comment,
                comment.strip_prefix("// ").unwrap(),
            ),
            "cannot modify readonly access",
        );
    }
    rejects(
        &(blocks[1].clone() + "\nright = 3;"),
        "cannot modify readonly access",
    );
    let message = rejects(&blocks[6], "cannot modify readonly access");
    let mut rest = message.split("readonly propagation path:").nth(1).unwrap();
    for line in blocks[7].lines() {
        let index = rest
            .find(line)
            .unwrap_or_else(|| panic!("missing diagnostic step {line:?}: {message}"));
        rest = &rest[index + line.len()..];
    }
}

#[test]
fn noescape_examples_are_instantiated_and_run_including_generic_callbacks() {
    let blocks = examples(ESCAPE, 10);
    for (index, suffix, expected) in [
        (
            0,
            "let value:=1;increment(value);let separate:=detached(value);let alias:=shared(value);value=7;wt value,separate,alias;",
            "7 2 7\n",
        ),
        (1, "wt twice(3),next(3);", "6 4\n"),
        (2, "wt copied_first([7]);", "7\n"),
        (
            3,
            "let values:Vec<Int>:=[];append(values,1);wt values;",
            "1\n",
        ),
        (
            4,
            "let value:=1;discard(value);let separate:=detached(value);value=7;wt separate,value;",
            "1 7\n",
        ),
        (5, "", "4\n"),
        (6, "wt local(3);", "4\n"),
        (7, "wt nested(3);", "4\n"),
        (8, "wt via_call(3);", "4\n"),
        (9, "wt via_container(3);", "4\n"),
    ] {
        runs(&blocks[index], suffix, expected);
    }
}

#[test]
fn noescape_documented_rejections_are_checked_after_instantiation() {
    let blocks = examples(ESCAPE, 10);
    let source = replace(&blocks[0], "fn shared(escape", "fn shared(noescape");
    rejects(&source, "noescape parameter 'value' may escape");
    let source = replace(&blocks[2], "// fn first", "fn first") + "\nwt first([7]);";
    rejects(&source, "noescape parameter 'values' may escape");
    let source = replace(&blocks[3], "escape value:", "noescape value:")
        + "\nlet values:Vec<Int>:=[];append(values,1);";
    rejects(&source, "noescape parameter 'value' may escape");
    let source = replace(
        &blocks[5],
        "// wt invoke(() => value);",
        "wt invoke(() => value);",
    );
    rejects(&source, "noescape parameter 'callback' may escape");
    let source = replace(&blocks[9], "box.value := value;", "held[0].value := value;")
        + "\nwt via_container(3);";
    rejects(&source, "cannot prove noescape for parameter 'value'");
    let source = replace(&blocks[9], "return held[0].value + 1;", "return box.value;")
        + "\nwt via_container(3);";
    rejects(&source, "cannot prove noescape for parameter 'value'");
}
