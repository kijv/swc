extern crate swc_malloc;

use codspeed_criterion_compat::{black_box, criterion_group, criterion_main, Bencher, Criterion};
use swc_common::{input::StringInput, FileName};
use swc_markdown_parser::{lexer::Lexer, parser::Parser};

fn bench_document(b: &mut Bencher, src: &'static str) {
    let _ = ::testing::run_test(false, |cm, _| {
        let fm = cm.new_source_file(FileName::Anon.into(), src);

        b.iter(|| {
            let _ = black_box({
                let lexer = Lexer::new(StringInput::from(&*fm));
                let mut parser = Parser::new(lexer, Default::default());

                parser.parse_document()
            });
        });

        Ok(())
    });
}

fn run_document(c: &mut Criterion, id: &str, src: &'static str) {
    c.bench_function(&format!("markdown/parser/{id}"), |b| {
        bench_document(b, src);
    });
}

fn bench_files(c: &mut Criterion) {
    /*
    block-{bq-{flat,nested},code,fences,heading,hr,html,lheading,list-{flat,nested},ref-{flat,nested}}
    commonmark-readme
    inline={autolink,backtics,em-{flat,nested,worst},entity,escape,html,links-{flat,nested},newlines}
    lorem1
    rawtabs
     */
    run_document(
        c,
        "parser_document/block-bq-flat",
        include_str!("./files/commonmark/block-bq-flat.md"),
    );

    run_document(
        c,
        "parser_document/block-bq-nested",
        include_str!("./files/commonmark/block-bq-nested.md"),
    );

    run_document(
        c,
        "parser_document/block-code",
        include_str!("./files/commonmark/block-code.md"),
    );

    run_document(
        c,
        "parser_document/block-fences",
        include_str!("./files/commonmark/block-fences.md"),
    );

    run_document(
        c,
        "parser_document/block-heading",
        include_str!("./files/commonmark/block-heading.md"),
    );

    run_document(
        c,
        "parser_document/block-hr",
        include_str!("./files/commonmark/block-hr.md"),
    );

    run_document(
        c,
        "parser_document/block-html",
        include_str!("./files/commonmark/block-html.md"),
    );

    run_document(
        c,
        "parser_document/block-lheading",
        include_str!("./files/commonmark/block-lheading.md"),
    );

    run_document(
        c,
        "parser_document/block-list-flat",
        include_str!("./files/commonmark/block-list-flat.md"),
    );

    run_document(
        c,
        "parser_document/block-list-nested",
        include_str!("./files/commonmark/block-list-nested.md"),
    );

    run_document(
        c,
        "parser_document/block-ref-flat",
        include_str!("./files/commonmark/block-ref-flat.md"),
    );

    run_document(
        c,
        "parser_document/block-ref-nested",
        include_str!("./files/commonmark/block-ref-nested.md"),
    );

    run_document(
        c,
        "parser_document/readme",
        include_str!("./files/commonmark/.readme.md"),
    );

    run_document(
        c,
        "parser_document/inline-autolink",
        include_str!("./files/commonmark/inline-autolink.md"),
    );

    run_document(
        c,
        "parser_document/inline-backticks",
        include_str!("./files/commonmark/inline-backticks.md"),
    );

    run_document(
        c,
        "parser_document/inline-em-flat",
        include_str!("./files/commonmark/inline-em-flat.md"),
    );

    run_document(
        c,
        "parser_document/inline-em-nested",
        include_str!("./files/commonmark/inline-em-nested.md"),
    );

    run_document(
        c,
        "parser_document/inline-em-worst",
        include_str!("./files/commonmark/inline-em-worst.md"),
    );

    run_document(
        c,
        "parser_document/inline-entity",
        include_str!("./files/commonmark/inline-entity.md"),
    );

    run_document(
        c,
        "parser_document/inline-escape",
        include_str!("./files/commonmark/inline-escape.md"),
    );

    run_document(
        c,
        "parser_document/inline-html",
        include_str!("./files/commonmark/inline-html.md"),
    );

    run_document(
        c,
        "parser_document/inline-links-flat",
        include_str!("./files/commonmark/inline-links-flat.md"),
    );

    run_document(
        c,
        "parser_document/inline-links-nested",
        include_str!("./files/commonmark/inline-links-nested.md"),
    );

    run_document(
        c,
        "parser_document/inline-newlines",
        include_str!("./files/commonmark/inline-newlines.md"),
    );

    run_document(
        c,
        "parser_document/lorem1",
        include_str!("./files/commonmark/lorem1.md"),
    );

    run_document(
        c,
        "parser_document/rawtabs",
        include_str!("./files/commonmark/rawtabs.md"),
    );
}

criterion_group!(benches, bench_files);
criterion_main!(benches);
