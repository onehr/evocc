use evocc_chir::{CHirBlockItem, CHirDecl, CHirExpr, CHirStmt, ParameterSyntax, TagMembers};
use evocc_common::{Arena, FileId, SourceFile, SourceSpan};
use evocc_cpp::{Lexer, Token, TokenKind, TokenStream, TokenText};
use evocc_parser::Parser;

fn borrowed_tokens(file: &SourceFile) -> Vec<Token> {
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    tokens
}

/// Materialize the spelling of every token before its source buffer leaves the
/// preprocessor boundary. This is the public representation used by included
/// files and macro expansions; the parser must not recover these spellings by
/// slicing the main file.
fn decoded_tokens(file: &SourceFile, keep_eof: bool) -> Vec<Token> {
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if keep_eof || token.kind != TokenKind::Eof {
            let spelling = file.contents
                [token.span.byte_start as usize..token.span.byte_end as usize]
                .to_owned();
            tokens.push(Token {
                text: TokenText::Decoded(spelling),
                ..token
            });
        }
    }
    TokenStream::new(tokens).into_vec()
}

fn assert_origin(
    actual: SourceSpan,
    file: &SourceFile,
    byte_start: u32,
    byte_end: u32,
    line: u32,
    column: u32,
    spelling: &str,
) {
    let expected = SourceSpan::new(file.id, byte_start, byte_end, line, column);
    assert_eq!(actual, expected, "wrong span for {spelling:?}");
    assert_eq!(
        file.line_col(byte_start as usize),
        (line, column),
        "frozen line/column does not match source file {}",
        file.path
    );
    assert_eq!(
        &file.contents[byte_start as usize..byte_end as usize],
        spelling,
        "span does not select the expected bytes from {}",
        file.path
    );
}

fn file_by_id<'a>(files: &'a [&SourceFile], id: FileId) -> &'a SourceFile {
    files
        .iter()
        .copied()
        .find(|file| file.id == id)
        .unwrap_or_else(|| panic!("unregistered FileId {}", id.raw()))
}

#[test]
fn multiline_declaration_surface_spans_are_exact() {
    let source = SourceFile::new(
        FileId::new(601),
        "/virtual/src/surface.c".into(),
        concat!(
            "struct Packet {\n",
            "    int field;\n",
            "};\n",
            "enum State {\n",
            "    READY = 7\n",
            "};\n",
            "int compute(\n",
            "    int *input\n",
            ");\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let mut parser = Parser::new(borrowed_tokens(&source), source.id, &source.contents);
    let unit = parser.parse_translation_unit_in(&arena);

    assert!(parser.diagnostic().is_none());
    assert_eq!(unit.len(), 3);

    let CHirDecl::Tag(packet) = unit[0] else {
        panic!("first declaration must be struct Packet")
    };
    assert_origin(
        packet.span,
        &source,
        0,
        33,
        1,
        1,
        "struct Packet {\n    int field;\n};",
    );
    assert_origin(
        packet.specifier_span,
        &source,
        0,
        32,
        1,
        1,
        "struct Packet {\n    int field;\n}",
    );
    assert_origin(
        packet.name.as_ref().expect("named struct").span,
        &source,
        7,
        13,
        1,
        8,
        "Packet",
    );
    let TagMembers::Struct(members) = &packet.members else {
        panic!("Packet must have struct members")
    };
    assert_eq!(members.len(), 1);
    let field = &members[0];
    assert_origin(field.span, &source, 20, 30, 2, 5, "int field;");
    assert_origin(field.specifier.span(), &source, 20, 23, 2, 5, "int");
    assert_origin(
        field.name.as_ref().expect("named member").span,
        &source,
        24,
        29,
        2,
        9,
        "field",
    );
    assert_origin(field.written.span, &source, 24, 29, 2, 9, "field");

    let CHirDecl::Tag(state) = unit[1] else {
        panic!("second declaration must be enum State")
    };
    assert_origin(
        state.span,
        &source,
        34,
        63,
        4,
        1,
        "enum State {\n    READY = 7\n};",
    );
    let TagMembers::Enum(enumerators) = &state.members else {
        panic!("State must have enumerators")
    };
    assert_eq!(enumerators.len(), 1);
    let ready = &enumerators[0];
    assert_origin(ready.span, &source, 51, 60, 5, 5, "READY = 7");
    assert_origin(ready.name.span, &source, 51, 56, 5, 5, "READY");
    assert_origin(
        ready.written_value.expect("written enum value").span(),
        &source,
        59,
        60,
        5,
        13,
        "7",
    );

    let CHirDecl::Function(compute) = unit[2] else {
        panic!("third declaration must be compute")
    };
    assert_origin(
        compute.span,
        &source,
        64,
        94,
        7,
        1,
        "int compute(\n    int *input\n);",
    );
    assert_origin(compute.name.span, &source, 68, 75, 7, 5, "compute");
    assert_origin(
        compute.written.span,
        &source,
        68,
        93,
        7,
        5,
        "compute(\n    int *input\n)",
    );
    assert_eq!(compute.written.layers.len(), 1);
    let function_layer_span = match &compute.written.layers[0] {
        evocc_chir::WrittenDeclaratorLayer::Function { span, .. } => *span,
        other => panic!("expected function layer, got {other:?}"),
    };
    assert_origin(
        function_layer_span,
        &source,
        75,
        93,
        7,
        12,
        "(\n    int *input\n)",
    );
    let ParameterSyntax::Prototype { parameters, .. } = &compute.parameters else {
        panic!("compute must have a prototype")
    };
    assert_eq!(parameters.len(), 1);
    let input = &parameters[0];
    assert_origin(input.span, &source, 81, 91, 8, 5, "int *input");
    assert_origin(
        input.name.as_ref().expect("named parameter").span,
        &source,
        86,
        91,
        8,
        10,
        "input",
    );
    assert_origin(input.written.span, &source, 85, 91, 8, 9, "*input");
    let pointer_layer_span = match &input.written.layers[0] {
        evocc_chir::WrittenDeclaratorLayer::Pointer { span, .. } => *span,
        other => panic!("expected pointer layer, got {other:?}"),
    };
    assert_origin(pointer_layer_span, &source, 85, 86, 8, 9, "*");
}

#[test]
fn multiline_statement_label_and_expression_spans_are_exact() {
    let source = SourceFile::new(
        FileId::new(602),
        "/virtual/src/body.c".into(),
        concat!(
            "int run(\n",
            "    int value\n",
            ") {\n",
            "entry:\n",
            "    return value + 1;\n",
            "}\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let mut parser = Parser::new(borrowed_tokens(&source), source.id, &source.contents);
    let unit = parser.parse_translation_unit_in(&arena);

    assert!(parser.diagnostic().is_none());
    assert_eq!(unit.len(), 1);
    let CHirDecl::Function(run) = unit[0] else {
        panic!("expected run function definition")
    };
    assert_origin(
        run.span,
        &source,
        0,
        57,
        1,
        1,
        "int run(\n    int value\n) {\nentry:\n    return value + 1;\n}",
    );
    let body = run.body.expect("function definition body");
    assert_origin(
        body.span(),
        &source,
        25,
        57,
        3,
        3,
        "{\nentry:\n    return value + 1;\n}",
    );
    let CHirStmt::Compound(items, _) = body else {
        panic!("function body must be compound")
    };
    assert_eq!(items.len(), 1);
    let CHirBlockItem::Statement(CHirStmt::Label(label, nested, label_span)) = items[0] else {
        panic!("body must contain one label statement")
    };
    assert_origin(
        *label_span,
        &source,
        27,
        55,
        4,
        1,
        "entry:\n    return value + 1;",
    );
    assert_origin(label.span, &source, 27, 32, 4, 1, "entry");
    let CHirStmt::Return(Some(value), return_span) = nested else {
        panic!("label must contain a value-return")
    };
    assert_origin(*return_span, &source, 38, 55, 5, 5, "return value + 1;");
    assert_origin(value.span(), &source, 45, 54, 5, 12, "value + 1");
    let CHirExpr::Binary(_, left, right, _) = value else {
        panic!("return value must be a binary expression")
    };
    assert_origin(left.span(), &source, 45, 50, 5, 12, "value");
    assert_origin(right.span(), &source, 53, 54, 5, 20, "1");
}

// Day 4 G4 behavioral RED: decoded leaf tokens retain the header's identity,
// but composite spans are currently rebuilt with the parser's main-file ID.
#[test]
fn decoded_header_nodes_keep_their_own_file_identity() {
    let header = SourceFile::new(
        FileId::new(701),
        "/virtual/include/model.h".into(),
        "struct Header {\n    int from_header;\n};\n".into(),
    );
    let main = SourceFile::new(
        FileId::new(702),
        "/virtual/src/main.c".into(),
        "struct Header value;\n".into(),
    );
    let mut tokens = decoded_tokens(&header, false);
    tokens.extend(decoded_tokens(&main, true));

    let arena = Arena::default();
    let mut parser = Parser::new(tokens, main.id, &main.contents);
    let unit = parser.parse_translation_unit_in(&arena);

    assert!(parser.diagnostic().is_none());
    assert_eq!(unit.len(), 2);
    let CHirDecl::Tag(header_tag) = unit[0] else {
        panic!("included definition must produce a tag declaration")
    };
    assert_origin(
        header_tag.span,
        &header,
        0,
        39,
        1,
        1,
        "struct Header {\n    int from_header;\n};",
    );
    assert_eq!(
        file_by_id(&[&header, &main], header_tag.span.file).path,
        "/virtual/include/model.h"
    );
    assert_origin(
        header_tag.specifier_span,
        &header,
        0,
        38,
        1,
        1,
        "struct Header {\n    int from_header;\n}",
    );
    let TagMembers::Struct(members) = &header_tag.members else {
        panic!("Header must have struct members")
    };
    assert_eq!(members.len(), 1);
    assert_origin(members[0].span, &header, 20, 36, 2, 5, "int from_header;");
    assert_origin(
        members[0].name.as_ref().expect("named member").span,
        &header,
        24,
        35,
        2,
        9,
        "from_header",
    );

    let CHirDecl::Object(value) = unit[1] else {
        panic!("main file must produce value object")
    };
    assert_origin(value.span, &main, 0, 20, 1, 1, "struct Header value;");
    assert_eq!(
        file_by_id(&[&header, &main], value.span.file).path,
        "/virtual/src/main.c"
    );
}

#[test]
fn malformed_decoded_header_diagnostic_keeps_header_path_and_position() {
    let header = SourceFile::new(
        FileId::new(711),
        "/virtual/include/broken.h".into(),
        "struct Broken {\n    int member\n};\n".into(),
    );
    let main = SourceFile::new(
        FileId::new(712),
        "/virtual/src/consumer.c".into(),
        "int fallback;\n".into(),
    );
    let mut tokens = decoded_tokens(&header, false);
    tokens.extend(decoded_tokens(&main, true));

    let arena = Arena::default();
    let mut parser = Parser::new(tokens, main.id, &main.contents);
    let _ = parser.parse_translation_unit_in(&arena);
    let diagnostic = parser
        .diagnostic()
        .expect("missing member semicolon is invalid");

    assert_eq!(diagnostic.code, "E1002");
    assert_eq!(diagnostic.message, "expected-token: ;");
    assert_origin(diagnostic.span, &header, 31, 32, 3, 1, "}");
    assert_eq!(
        file_by_id(&[&header, &main], diagnostic.span.file).path,
        "/virtual/include/broken.h"
    );
}
