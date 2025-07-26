extern crate swc_malloc;

use codspeed_criterion_compat::{black_box, criterion_group, criterion_main, Bencher, Criterion};
use swc_common::{input::StringInput, FileName, Span, DUMMY_SP};
use swc_markdown_ast::Document;
use swc_markdown_parser::{lexer::Lexer, parser::Parser};
use swc_markdown_visit::{Fold, FoldWith, VisitMut, VisitMutWith};

static SOURCE: &str = include_str!("files/commonmark/.readme.md");

fn run_document<F>(b: &mut Bencher, mut op: F)
where
    F: FnMut(Document) -> Document,
{
    let _ = ::testing::run_test(false, |cm, _| {
        let fm = cm.new_source_file(FileName::Anon.into(), SOURCE);

        let lexer = Lexer::new(StringInput::from(&*fm));
        let mut parser = Parser::new(lexer, Default::default());
        let document: Document = parser.parse_document().unwrap();

        b.iter(|| {
            let document = document.clone();
            let document = op(document);

            black_box(document)
        });

        Ok(())
    });
}

fn bench_cases(c: &mut Criterion) {
    c.bench_function("markdown/document/visitor/compare/clone", |b| {
        run_document(b, |m: Document| m)
    });

    c.bench_function("markdown/document/visitor/compare/visit_mut_span", |b| {
        struct RespanVisitMut;

        impl VisitMut for RespanVisitMut {
            fn visit_mut_span(&mut self, span: &mut Span) {
                *span = DUMMY_SP;
            }
        }

        run_document(b, |mut m| {
            m.visit_mut_with(&mut RespanVisitMut);

            m
        });
    });

    c.bench_function(
        "markdown/document/visitor/compare/visit_mut_span_panic",
        |b| {
            struct RespanVisitMut;

            impl VisitMut for RespanVisitMut {
                fn visit_mut_span(&mut self, span: &mut Span) {
                    *span = DUMMY_SP;
                }
            }

            run_document(b, |mut m| {
                m.visit_mut_with(&mut RespanVisitMut);

                m
            });
        },
    );

    c.bench_function("markdown/document/visitor/compare/fold_span", |b| {
        struct RespanFold;

        impl Fold for RespanFold {
            fn fold_span(&mut self, _: Span) -> Span {
                DUMMY_SP
            }
        }

        run_document(b, |m| m.fold_with(&mut RespanFold));
    });

    c.bench_function("markdown/document/visitor/compare/fold_span_panic", |b| {
        struct RespanFold;

        impl Fold for RespanFold {
            fn fold_span(&mut self, _: Span) -> Span {
                DUMMY_SP
            }
        }

        run_document(b, |m| m.fold_with(&mut RespanFold));
    });
}

criterion_group!(benches, bench_cases);
criterion_main!(benches);
