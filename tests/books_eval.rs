#[test]
fn generated_books_quality_gate() {
    use woodpeck::books::eval::{Truth, evaluate};
    let started = std::time::Instant::now();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/books/fixtures");
    let mut expected = 0;
    let mut correct = 0;
    let mut emitted = 0;
    let mut pdf_expected = 0;
    let mut pdf_correct = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|s| s != "json")
            || path.file_name().is_some_and(|s| s == "metrics.json")
        {
            continue;
        }
        let truth: Truth = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let m = evaluate(&dir.join(&truth.file), &truth).unwrap();
        if truth.notation == "PDF two columns" {
            pdf_expected += m.expected;
            pdf_correct += m.correct;
        }
        expected += m.expected;
        correct += m.correct;
        emitted += m.emitted;
        assert_eq!(m.wrong_positions, 0, "{}", m.name);
        assert_eq!(m.correct, m.emitted, "{}: false positive moves", m.name);
        assert_eq!(m.labelled_wrong, 0, "{}: misread diagram image", m.name);
        if truth.notation == "image diagrams" {
            // Screen boards are read; the synthetic engraved JPEG look currently abstains.
            assert!(
                m.correct_diagrams >= 5,
                "{}: {}",
                m.name,
                m.correct_diagrams
            );
            assert_eq!(m.labelled_correct, m.correct_diagrams, "{}", m.name);
        } else {
            assert_eq!(m.correct_diagrams, m.expected_diagrams, "{}", m.name);
        }
        assert_eq!(m.broken, m.broken_correct, "{}", m.name);
        assert!(
            !m.failures.iter().any(|f| f.starts_with("Missing anchor")),
            "{}: oracle anchor missing",
            m.name
        );
        // Descriptive ambiguity is measured explicitly, never removed from the denominator.
        let floor = match truth.notation.as_str() {
            "descriptive-0" | "descriptive-2" => 0.41,
            "descriptive-1" => 0.29,
            "descriptive-3" => 0.43,
            "PDF two columns" => 0.0, // Aggregate PDF gate below; page resets remain a known gap.
            "image diagrams" => 0.5,
            _ => 0.99,
        };
        assert!(m.recall() >= floor, "{}: recall {}", m.name, m.recall());
    }
    assert!(pdf_correct as f64 / pdf_expected as f64 >= 0.97);
    assert!(expected > 60_000);
    assert!(correct as f64 / expected as f64 >= 0.83);
    assert_eq!(correct, emitted);
    assert!(
        started.elapsed().as_secs() < 60,
        "generated eval exceeded CI runtime budget"
    );
}

#[test]
fn directory_without_truth_still_reports_surface_coverage() {
    let temp = tempfile::tempdir().unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/books/fixtures/negative-0.epub");
    std::fs::copy(source, temp.path().join("prose.epub")).unwrap();
    let report = woodpeck::books::eval::run(temp.path()).unwrap();
    assert_eq!(report.overall.expected, 0);
    assert_eq!(report.overall.coverage.tokens, 2);
    assert_eq!(report.overall.coverage.unresolved, 2);
    assert_eq!(report.overall.emitted, 0);
}
