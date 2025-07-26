extern crate swc_malloc;

use codspeed_criterion_compat::{black_box, criterion_group, criterion_main, Bencher, Criterion};
use swc_common::{input::StringInput, FileName};
use swc_markdown_parser::lexer::Lexer;

fn bench_document(b: &mut Bencher, src: &'static str) {
    let _ = ::testing::run_test(false, |cm, _| {
        let fm = cm.new_source_file(FileName::Anon.into(), src);

        b.iter(|| {
            let lexer = Lexer::new(StringInput::from(&*fm));

            for t in lexer {
                black_box(t);
            }
        });

        Ok(())
    });
}

fn run(c: &mut Criterion, id: &str, src: &'static str) {
    c.bench_function(&format!("markdown/lexer/{id}"), |b| {
        bench_document(b, src);
    });
}

fn bench_files(c: &mut Criterion) {
    run(
        c,
        "block-bq-flat",
        include_str!("./files/commonmark/block-bq-flat.md"),
    );

    run(
        c,
        "block-bq-nested",
        include_str!("./files/commonmark/block-bq-nested.md"),
    );

    run(
        c,
        "block-code",
        include_str!("./files/commonmark/block-code.md"),
    );

    run(
        c,
        "block-fences",
        include_str!("./files/commonmark/block-fences.md"),
    );

    run(
        c,
        "block-heading",
        include_str!("./files/commonmark/block-heading.md"),
    );

    run(
        c,
        "block-hr",
        include_str!("./files/commonmark/block-hr.md"),
    );

    run(
        c,
        "block-html",
        include_str!("./files/commonmark/block-html.md"),
    );

    run(
        c,
        "block-lheading",
        include_str!("./files/commonmark/block-lheading.md"),
    );

    run(
        c,
        "block-list-flat",
        include_str!("./files/commonmark/block-list-flat.md"),
    );

    run(
        c,
        "block-list-nested",
        include_str!("./files/commonmark/block-list-nested.md"),
    );

    run(
        c,
        "block-ref-flat",
        include_str!("./files/commonmark/block-ref-flat.md"),
    );

    run(
        c,
        "block-ref-nested",
        include_str!("./files/commonmark/block-ref-nested.md"),
    );

    run(c, "readme", include_str!("./files/commonmark/.readme.md"));

    run(
        c,
        "inline-autolink",
        include_str!("./files/commonmark/inline-autolink.md"),
    );

    run(
        c,
        "inline-backticks",
        include_str!("./files/commonmark/inline-backticks.md"),
    );

    run(
        c,
        "inline-em-flat",
        include_str!("./files/commonmark/inline-em-flat.md"),
    );

    run(
        c,
        "inline-em-nested",
        include_str!("./files/commonmark/inline-em-nested.md"),
    );

    run(
        c,
        "inline-em-worst",
        include_str!("./files/commonmark/inline-em-worst.md"),
    );

    run(
        c,
        "inline-entity",
        include_str!("./files/commonmark/inline-entity.md"),
    );

    run(
        c,
        "inline-escape",
        include_str!("./files/commonmark/inline-escape.md"),
    );

    run(
        c,
        "inline-html",
        include_str!("./files/commonmark/inline-html.md"),
    );

    run(
        c,
        "inline-links-flat",
        include_str!("./files/commonmark/inline-links-flat.md"),
    );

    run(
        c,
        "inline-links-nested",
        include_str!("./files/commonmark/inline-links-nested.md"),
    );

    run(
        c,
        "inline-newlines",
        include_str!("./files/commonmark/inline-newlines.md"),
    );

    run(c, "lorem1", include_str!("./files/commonmark/lorem1.md"));

    run(c, "rawtabs", include_str!("./files/commonmark/rawtabs.md"));
}

criterion_group!(benches, bench_files);
criterion_main!(benches);
