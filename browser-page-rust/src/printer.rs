//! Print the existing compiler-generated expression; no semantic or scope passes.
use oxc_allocator::Allocator;
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_parser::{ParseOptions, Parser};
use oxc_span::SourceType;
use std::io;

pub fn expression(source: &str) -> io::Result<String> {
    if source.len() >= crate::artifact::MAX_ARTIFACT {
        return Err(io::Error::other(
            "Generated factory input exceeds its bound",
        ));
    }
    let allocator = Allocator::default();
    let source_type = SourceType::cjs();
    let expression = Parser::new(&allocator, source, source_type)
        .with_options(ParseOptions {
            parse_regular_expression: true,
            preserve_parens: true,
            ..ParseOptions::default()
        })
        .parse_expression()
        .map_err(|_| io::Error::other("Generated factory expression parsing refused"))?;
    let mut printer = Codegen::new()
        .with_source_text(source)
        .with_source_type(source_type)
        .with_options(CodegenOptions::minify());
    printer.print_expression(&expression);
    let output = printer.into_source_text();
    if output.len() >= crate::artifact::MAX_ARTIFACT {
        return Err(io::Error::other(
            "Printed factory expression exceeds its bound",
        ));
    }
    Ok(output)
}
