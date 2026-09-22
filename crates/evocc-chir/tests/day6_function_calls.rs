use evocc_chir::interp::{
    interpret_scalar_function, InterpreterError, InterpreterLimits, ScalarValue,
};
use evocc_chir::*;
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_types::{
    CanonicalType, FunctionParameters, IntRank, Qualifiers, TypeId, TypeInterner, TypeKind,
};

fn span(start: u32, end: u32) -> SourceSpan {
    SourceSpan::new(FileId::new(6_002), start, end, 1, start + 1)
}

fn alloc<'arena, T: 'arena>(arena: &'arena Arena, value: T) -> &'arena T {
    // Safety: these C-HIR test values contain no Arena handle and their
    // transitive destructors only release owned source-fact buffers.
    unsafe { arena.alloc_shared(value) }
}

fn function_type(types: &mut TypeInterner, result: TypeId, parameters: Vec<TypeId>) -> TypeId {
    function_type_with_variadic(types, result, parameters, false)
}

fn function_type_with_variadic(
    types: &mut TypeInterner,
    result: TypeId,
    parameters: Vec<TypeId>,
    variadic: bool,
) -> TypeId {
    types.intern(CanonicalType {
        kind: TypeKind::Function {
            return_type: result,
            parameters: FunctionParameters::Prototype {
                types: parameters,
                variadic,
            },
        },
        qualifiers: Qualifiers::default(),
    })
}

fn literal<'arena>(arena: &'arena Arena, value: u64, at: SourceSpan) -> &'arena CHirExpr<'arena> {
    alloc(
        arena,
        CHirExpr::Lit(
            IntLit {
                value,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            at,
        ),
    )
}

fn suffixed_literal<'arena>(
    arena: &'arena Arena,
    value: u64,
    suffix: IntSuffix,
    at: SourceSpan,
) -> &'arena CHirExpr<'arena> {
    alloc(
        arena,
        CHirExpr::Lit(
            IntLit {
                value,
                suffix,
                base: IntBase::Dec,
            },
            at,
        ),
    )
}

fn object_ident<'arena>(
    arena: &'arena Arena,
    symbol: SymbolId,
    spelling: &str,
    ty: TypeId,
    at: SourceSpan,
) -> &'arena CHirExpr<'arena> {
    alloc(
        arena,
        CHirExpr::Ident {
            symbol,
            spelling: spelling.to_owned(),
            facts: ExprFacts::lvalue(ty, true, true).unwrap(),
            span: at,
        },
    )
}

fn function_ident<'arena>(
    arena: &'arena Arena,
    symbol: SymbolId,
    spelling: &str,
    ty: TypeId,
    at: SourceSpan,
) -> &'arena CHirExpr<'arena> {
    alloc(
        arena,
        CHirExpr::Ident {
            symbol,
            spelling: spelling.to_owned(),
            facts: ExprFacts::function_designator(ty),
            span: at,
        },
    )
}

fn binary<'arena>(
    arena: &'arena Arena,
    operator: BinaryOp,
    left: &'arena CHirExpr<'arena>,
    right: &'arena CHirExpr<'arena>,
    at: SourceSpan,
) -> &'arena CHirExpr<'arena> {
    alloc(arena, CHirExpr::Binary(operator, left, right, at))
}

fn call<'arena>(
    arena: &'arena Arena,
    function: &'arena CHirExpr<'arena>,
    arguments: Vec<&'arena CHirExpr<'arena>>,
    at: SourceSpan,
) -> &'arena CHirExpr<'arena> {
    alloc(arena, CHirExpr::Call(function, arguments, at))
}

#[allow(clippy::too_many_arguments)]
fn assignment<'arena>(
    arena: &'arena Arena,
    symbols: &SymbolTable,
    types: &TypeInterner,
    symbol: SymbolId,
    spelling: &str,
    ty: TypeId,
    op: AssignmentOp,
    value: &'arena CHirExpr<'arena>,
    at: SourceSpan,
) -> &'arena CHirExpr<'arena> {
    let place = ScalarPlace::bind(symbols, types, symbol).unwrap();
    alloc(
        arena,
        CHirExpr::Assign {
            op,
            place,
            target: object_ident(arena, symbol, spelling, ty, at),
            value,
            facts: ExprFacts::value(ty),
            span: at,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn update<'arena>(
    arena: &'arena Arena,
    symbols: &SymbolTable,
    types: &TypeInterner,
    symbol: SymbolId,
    spelling: &str,
    ty: TypeId,
    op: UpdateOp,
    at: SourceSpan,
) -> &'arena CHirExpr<'arena> {
    let place = ScalarPlace::bind(symbols, types, symbol).unwrap();
    alloc(
        arena,
        CHirExpr::Update {
            op,
            place,
            target: object_ident(arena, symbol, spelling, ty, at),
            facts: ExprFacts::value(ty),
            span: at,
        },
    )
}

fn object_declaration<'arena>(
    arena: &'arena Arena,
    symbol: SymbolId,
    name: &str,
    ty: TypeId,
    initializer: Option<&'arena CHirExpr<'arena>>,
    at: SourceSpan,
) -> &'arena CHirDecl<'arena> {
    let initializer = initializer.map(|value| {
        ObjectInitializer::new(
            WrittenInitializer::Expression(value),
            InitPlan::new(
                ty,
                vec![InitAction::store_scalar(
                    SubobjectPath::root(),
                    ty,
                    value,
                    value.span(),
                )],
            ),
        )
    });
    alloc(
        arena,
        CHirDecl::Object(ObjectDecl {
            attributes: Vec::new(),
            cleanup: None,
            symbol,
            name: SpannedName {
                spelling: name.to_owned(),
                span: at,
            },
            ty,
            specifier: TypeSpecifier::Builtin { ty, span: at },
            written: WrittenDeclarator {
                layers: Vec::new(),
                span: at,
            },
            storage: StorageClass::None,
            initializer,
            asm_label: None,
            asm_label_span: None,
            gnu_extension_spans: Vec::new(),
            span: at,
        }),
    )
}

fn parameter(symbol: SymbolId, spelling: &str, ty: TypeId, at: SourceSpan) -> ParameterDecl {
    ParameterDecl {
        attributes: Vec::new(),
        symbol: Some(symbol),
        name: Some(SpannedName {
            spelling: spelling.to_owned(),
            span: at,
        }),
        ty,
        storage: StorageClass::None,
        specifier: TypeSpecifier::Builtin { ty, span: at },
        written: WrittenDeclarator {
            layers: Vec::new(),
            span: at,
        },
        span: at,
    }
}

struct Program<'arena> {
    unit: CHirUnit<'arena>,
    int: TypeId,
    add: SymbolId,
    fib: SymbolId,
    call_add: SymbolId,
    call_fib: SymbolId,
    linear: SymbolId,
    call_linear: SymbolId,
}

fn program(arena: &Arena) -> Program<'_> {
    program_with_add_fault(arena, AddFault::None)
}

#[derive(Debug, Clone, Copy)]
enum AddFault {
    None,
    IndirectCallee,
    VariadicDefinition,
    UnnamedParameter,
    MissingParameterSymbol,
    ForgedDesignator,
    ForgedWrittenDeclarator,
    ForgedParameterStorage,
    FileScopeParameter,
    DuplicateParameterSpelling,
}

fn program_with_add_fault(arena: &Arena, fault: AddFault) -> Program<'_> {
    program_with_options(arena, fault, None)
}

#[derive(Debug, Clone, Copy)]
enum RedeclarationKind {
    UnspecifiedThenPrototype,
    ExternThenDefinition,
}

fn program_with_redeclaration(arena: &Arena, kind: RedeclarationKind) -> Program<'_> {
    program_with_options(arena, AddFault::None, Some(kind))
}

fn program_with_options(
    arena: &Arena,
    fault: AddFault,
    redeclaration: Option<RedeclarationKind>,
) -> Program<'_> {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let short = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Short),
        qualifiers: Qualifiers::default(),
    });
    let uint = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::UInt),
        qualifiers: Qualifiers::default(),
    });
    let left_ty = if matches!(
        redeclaration,
        Some(RedeclarationKind::UnspecifiedThenPrototype)
    ) {
        int
    } else {
        short
    };
    let add_ty = function_type_with_variadic(
        &mut types,
        int,
        vec![left_ty, uint],
        matches!(fault, AddFault::VariadicDefinition),
    );
    let unspecified_add_ty = matches!(
        redeclaration,
        Some(RedeclarationKind::UnspecifiedThenPrototype)
    )
    .then(|| {
        types.intern(CanonicalType {
            kind: TypeKind::Function {
                return_type: int,
                parameters: FunctionParameters::Unspecified,
            },
            qualifiers: Qualifiers::default(),
        })
    });
    let fib_ty = function_type(&mut types, int, vec![int]);
    let entry_ty = function_type(&mut types, int, Vec::new());

    let mut symbols = SymbolTable::new();
    let file_scope = symbols.file_scope();
    let add = symbols
        .declare(
            file_scope,
            "add",
            SymbolData::Function {
                storage: if matches!(redeclaration, Some(RedeclarationKind::ExternThenDefinition)) {
                    StorageClass::Extern
                } else {
                    StorageClass::None
                },
            },
            unspecified_add_ty.unwrap_or(add_ty),
            span(0, 3),
        )
        .unwrap();
    if unspecified_add_ty.is_some() {
        symbols.update_symbol_type(add, add_ty).unwrap();
    }
    let fib = symbols
        .declare(
            file_scope,
            "fib",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            fib_ty,
            span(40, 43),
        )
        .unwrap();
    let call_add = symbols
        .declare(
            file_scope,
            "call_add",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            entry_ty,
            span(120, 128),
        )
        .unwrap();
    let call_fib = symbols
        .declare(
            file_scope,
            "call_fib",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            entry_ty,
            span(160, 168),
        )
        .unwrap();
    let linear = symbols
        .declare(
            file_scope,
            "linear",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            fib_ty,
            span(600, 606),
        )
        .unwrap();
    let call_linear = symbols
        .declare(
            file_scope,
            "call_linear",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            entry_ty,
            span(680, 691),
        )
        .unwrap();

    let add_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(4, 39)))
        .unwrap();
    let left = symbols
        .declare(
            if matches!(fault, AddFault::FileScopeParameter) {
                file_scope
            } else {
                add_scope
            },
            "left",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            left_ty,
            span(8, 12),
        )
        .unwrap();
    let right_scope = if matches!(fault, AddFault::DuplicateParameterSpelling) {
        symbols
            .create_scope(file_scope, ScopeKind::Function, Some(span(720, 739)))
            .unwrap()
    } else {
        add_scope
    };
    let right_spelling = if matches!(fault, AddFault::DuplicateParameterSpelling) {
        "left"
    } else {
        "right"
    };
    let right = symbols
        .declare(
            right_scope,
            right_spelling,
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            uint,
            span(14, 19),
        )
        .unwrap();

    let fib_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(44, 119)))
        .unwrap();
    let n = symbols
        .declare(
            fib_scope,
            "n",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            int,
            span(48, 49),
        )
        .unwrap();
    let linear_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(607, 679)))
        .unwrap();
    let depth = symbols
        .declare(
            linear_scope,
            "depth",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            int,
            span(611, 616),
        )
        .unwrap();

    let left_place = ScalarPlace::bind(&symbols, &types, left).unwrap();
    let mutate_left = alloc(
        arena,
        CHirExpr::Assign {
            op: AssignmentOp::Add,
            place: left_place,
            target: object_ident(arena, left, "left", left_ty, span(20, 24)),
            value: object_ident(arena, right, right_spelling, uint, span(28, 33)),
            facts: ExprFacts::value(left_ty),
            span: span(20, 33),
        },
    );
    let mutate_left = alloc(arena, CHirStmt::Expr(mutate_left, span(20, 34)));
    let add_left = object_ident(arena, left, "left", left_ty, span(35, 39));
    let hundred = literal(arena, 100, span(30, 33));
    let scaled_left = binary(arena, BinaryOp::Mul, add_left, hundred, span(25, 33));
    let add_right = object_ident(arena, right, right_spelling, uint, span(34, 39));
    let sum = binary(arena, BinaryOp::Add, scaled_left, add_right, span(25, 39));
    let add_return = alloc(arena, CHirStmt::Return(Some(sum), span(18, 38)));
    let add_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(mutate_left),
                CHirBlockItem::Statement(add_return),
            ],
            span(19, 40),
        ),
    );

    let fib_condition_n = object_ident(arena, n, "n", int, span(57, 58));
    let one = literal(arena, 1, span(62, 63));
    let base_condition = binary(arena, BinaryOp::Le, fib_condition_n, one, span(57, 63));
    let base_value = object_ident(arena, n, "n", int, span(72, 73));
    let base_return = alloc(arena, CHirStmt::Return(Some(base_value), span(65, 74)));
    let base_case = alloc(
        arena,
        CHirStmt::If(base_condition, base_return, None, span(53, 74)),
    );

    let fib_left_callee = function_ident(arena, fib, "fib", fib_ty, span(82, 85));
    let fib_left_n = object_ident(arena, n, "n", int, span(86, 87));
    let fib_left_one = literal(arena, 1, span(90, 91));
    let fib_left_arg = binary(arena, BinaryOp::Sub, fib_left_n, fib_left_one, span(86, 91));
    let fib_left = call(arena, fib_left_callee, vec![fib_left_arg], span(82, 92));

    let fib_right_callee = function_ident(arena, fib, "fib", fib_ty, span(95, 98));
    let fib_right_n = object_ident(arena, n, "n", int, span(99, 100));
    let fib_right_two = literal(arena, 2, span(103, 104));
    let fib_right_arg = binary(
        arena,
        BinaryOp::Sub,
        fib_right_n,
        fib_right_two,
        span(99, 104),
    );
    let fib_right = call(arena, fib_right_callee, vec![fib_right_arg], span(95, 105));
    let fib_sum = binary(arena, BinaryOp::Add, fib_left, fib_right, span(82, 105));
    let recursive_return = alloc(arena, CHirStmt::Return(Some(fib_sum), span(75, 106)));
    let fib_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(base_case),
                CHirBlockItem::Statement(recursive_return),
            ],
            span(51, 108),
        ),
    );

    let direct_add_callee = function_ident(
        arena,
        add,
        if matches!(fault, AddFault::ForgedDesignator) {
            "forged_add"
        } else {
            "add"
        },
        add_ty,
        span(139, 142),
    );
    let add_callee = if matches!(fault, AddFault::IndirectCallee) {
        alloc(
            arena,
            CHirExpr::Unary(UnaryOp::Plus, direct_add_callee, span(138, 142)),
        )
    } else {
        direct_add_callee
    };
    let add_call = call(
        arena,
        add_callee,
        vec![
            literal(arena, 2, span(143, 144)),
            alloc(
                arena,
                CHirExpr::Lit(
                    IntLit {
                        value: 7,
                        suffix: IntSuffix::U,
                        base: IntBase::Dec,
                    },
                    span(146, 148),
                ),
            ),
        ],
        span(139, 150),
    );
    let call_add_body = alloc(arena, CHirStmt::Return(Some(add_call), span(132, 151)));

    let fib_callee = function_ident(arena, fib, "fib", fib_ty, span(179, 182));
    let fib_call = call(
        arena,
        fib_callee,
        vec![literal(arena, 10, span(183, 185))],
        span(179, 186),
    );
    let call_fib_body = alloc(arena, CHirStmt::Return(Some(fib_call), span(172, 187)));

    let linear_condition = binary(
        arena,
        BinaryOp::Eq,
        object_ident(arena, depth, "depth", int, span(622, 627)),
        literal(arena, 0, span(631, 632)),
        span(622, 632),
    );
    let linear_base = alloc(
        arena,
        CHirStmt::Return(Some(literal(arena, 7, span(642, 643))), span(635, 644)),
    );
    let linear_if = alloc(
        arena,
        CHirStmt::If(linear_condition, linear_base, None, span(619, 644)),
    );
    let linear_argument = binary(
        arena,
        BinaryOp::Sub,
        object_ident(arena, depth, "depth", int, span(658, 663)),
        literal(arena, 1, span(666, 667)),
        span(658, 667),
    );
    let linear_call = call(
        arena,
        function_ident(arena, linear, "linear", fib_ty, span(651, 657)),
        vec![linear_argument],
        span(651, 668),
    );
    let linear_return = alloc(arena, CHirStmt::Return(Some(linear_call), span(644, 669)));
    let linear_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(linear_if),
                CHirBlockItem::Statement(linear_return),
            ],
            span(617, 670),
        ),
    );
    let call_linear_body = alloc(
        arena,
        CHirStmt::Return(
            Some(call(
                arena,
                function_ident(arena, linear, "linear", fib_ty, span(700, 706)),
                vec![literal(arena, 20_000, span(707, 712))],
                span(700, 713),
            )),
            span(693, 714),
        ),
    );

    let function_decl = |symbol, spelling: &str, ty, parameters, variadic, body, at: SourceSpan| {
        alloc(
            arena,
            CHirDecl::Function(FunctionDecl {
                attributes: Vec::new(),
                symbol,
                name: SpannedName {
                    spelling: spelling.to_owned(),
                    span: at,
                },
                ty,
                return_specifier: TypeSpecifier::Builtin { ty: int, span: at },
                written: WrittenDeclarator {
                    layers: if matches!(fault, AddFault::ForgedWrittenDeclarator) && symbol == add {
                        vec![WrittenDeclaratorLayer::Pointer {
                            qualifiers: Qualifiers::default(),
                            attributes: Vec::new(),
                            span: at,
                        }]
                    } else {
                        vec![WrittenDeclaratorLayer::Function {
                            parameters: None,
                            span: at,
                        }]
                    },
                    span: at,
                },
                parameters: ParameterSyntax::Prototype {
                    parameters,
                    variadic,
                },
                storage: StorageClass::None,
                asm_label: None,
                asm_label_span: None,
                inline_spans: Vec::new(),
                gnu_extension_spans: Vec::new(),
                body: Some(body),
                span: at,
            }),
        )
    };

    let mut declarations = Vec::new();
    if let Some(kind) = redeclaration {
        let (ty, parameters, storage) = match kind {
            RedeclarationKind::UnspecifiedThenPrototype => (
                unspecified_add_ty.expect("old-style type created for old-style declaration"),
                ParameterSyntax::Unspecified,
                StorageClass::None,
            ),
            RedeclarationKind::ExternThenDefinition => (
                add_ty,
                ParameterSyntax::Prototype {
                    parameters: vec![
                        ParameterDecl {
                            attributes: Vec::new(),
                            symbol: None,
                            name: None,
                            ty: left_ty,
                            storage: StorageClass::None,
                            specifier: TypeSpecifier::Builtin {
                                ty: left_ty,
                                span: span(0, 1),
                            },
                            written: WrittenDeclarator {
                                layers: Vec::new(),
                                span: span(0, 1),
                            },
                            span: span(0, 1),
                        },
                        ParameterDecl {
                            attributes: Vec::new(),
                            symbol: None,
                            name: None,
                            ty: uint,
                            storage: StorageClass::None,
                            specifier: TypeSpecifier::Builtin {
                                ty: uint,
                                span: span(2, 3),
                            },
                            written: WrittenDeclarator {
                                layers: Vec::new(),
                                span: span(2, 3),
                            },
                            span: span(2, 3),
                        },
                    ],
                    variadic: false,
                },
                StorageClass::Extern,
            ),
        };
        declarations.push(alloc(
            arena,
            CHirDecl::Function(FunctionDecl {
                attributes: Vec::new(),
                symbol: add,
                name: SpannedName {
                    spelling: "add".to_owned(),
                    span: span(0, 3),
                },
                ty,
                return_specifier: TypeSpecifier::Builtin {
                    ty: int,
                    span: span(0, 3),
                },
                written: WrittenDeclarator {
                    layers: vec![WrittenDeclaratorLayer::Function {
                        parameters: None,
                        span: span(3, 5),
                    }],
                    span: span(0, 6),
                },
                parameters,
                storage,
                asm_label: None,
                asm_label_span: None,
                inline_spans: Vec::new(),
                gnu_extension_spans: Vec::new(),
                body: None,
                span: span(0, 7),
            }),
        ));
    }
    declarations.extend([
        function_decl(
            add,
            "add",
            add_ty,
            {
                let mut left_parameter = parameter(left, "left", int, span(8, 12));
                if matches!(fault, AddFault::UnnamedParameter) {
                    left_parameter.symbol = None;
                    left_parameter.name = None;
                }
                if matches!(fault, AddFault::MissingParameterSymbol) {
                    left_parameter.symbol = None;
                }
                if matches!(fault, AddFault::ForgedParameterStorage) {
                    left_parameter.storage = StorageClass::Register;
                }
                left_parameter.ty = left_ty;
                left_parameter.specifier = TypeSpecifier::Builtin {
                    ty: left_ty,
                    span: span(8, 12),
                };
                vec![
                    left_parameter,
                    parameter(right, right_spelling, uint, span(14, 19)),
                ]
            },
            matches!(fault, AddFault::VariadicDefinition),
            add_body,
            span(0, 39),
        ),
        function_decl(
            fib,
            "fib",
            fib_ty,
            vec![parameter(n, "n", int, span(48, 49))],
            false,
            fib_body,
            span(40, 108),
        ),
        function_decl(
            call_add,
            "call_add",
            entry_ty,
            Vec::new(),
            false,
            call_add_body,
            span(120, 151),
        ),
        function_decl(
            call_fib,
            "call_fib",
            entry_ty,
            Vec::new(),
            false,
            call_fib_body,
            span(160, 187),
        ),
        function_decl(
            linear,
            "linear",
            fib_ty,
            vec![parameter(depth, "depth", int, span(611, 616))],
            false,
            linear_body,
            span(600, 670),
        ),
        function_decl(
            call_linear,
            "call_linear",
            entry_ty,
            Vec::new(),
            false,
            call_linear_body,
            span(680, 714),
        ),
    ]);

    Program {
        unit: CHirUnit::new(declarations, types, symbols),
        int,
        add,
        fib,
        call_add,
        call_fib,
        linear,
        call_linear,
    }
}

struct CompositionProgram<'arena> {
    unit: CHirUnit<'arena>,
    int: TypeId,
    main: SymbolId,
}

fn malformed_call_free_signature_program(arena: &Arena) -> CompositionProgram<'_> {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let main_ty = function_type(&mut types, int, Vec::new());
    let mut symbols = SymbolTable::new();
    let main = symbols
        .declare(
            symbols.file_scope(),
            "main",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            main_ty,
            span(500, 504),
        )
        .unwrap();
    let body = alloc(
        arena,
        CHirStmt::Return(Some(literal(arena, 7, span(520, 521))), span(513, 522)),
    );
    let declaration = alloc(
        arena,
        CHirDecl::Function(FunctionDecl {
            attributes: Vec::new(),
            symbol: main,
            name: SpannedName {
                spelling: "main".to_owned(),
                span: span(500, 504),
            },
            ty: main_ty,
            return_specifier: TypeSpecifier::Builtin {
                ty: int,
                span: span(500, 503),
            },
            written: WrittenDeclarator {
                layers: vec![WrittenDeclaratorLayer::Function {
                    parameters: None,
                    span: span(504, 506),
                }],
                span: span(500, 512),
            },
            parameters: ParameterSyntax::Unspecified,
            storage: StorageClass::None,
            asm_label: None,
            asm_label_span: None,
            inline_spans: Vec::new(),
            gnu_extension_spans: Vec::new(),
            body: Some(body),
            span: span(500, 522),
        }),
    );
    CompositionProgram {
        unit: CHirUnit::new(vec![declaration], types, symbols),
        int,
        main,
    }
}

fn composition_program(arena: &Arena, select_default: bool) -> CompositionProgram<'_> {
    composition_program_with_signature(arena, select_default, false)
}

fn composition_program_with_signature(
    arena: &Arena,
    select_default: bool,
    malformed_main_signature: bool,
) -> CompositionProgram<'_> {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let identity_ty = function_type(&mut types, int, vec![int]);
    let main_ty = function_type(&mut types, int, Vec::new());

    let mut symbols = SymbolTable::new();
    let file_scope = symbols.file_scope();
    let identity = symbols
        .declare(
            file_scope,
            "identity",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            identity_ty,
            span(200, 208),
        )
        .unwrap();
    let main = symbols
        .declare(
            file_scope,
            "main",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            main_ty,
            span(240, 244),
        )
        .unwrap();
    let identity_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(209, 239)))
        .unwrap();
    let value = symbols
        .declare(
            identity_scope,
            "value",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            int,
            span(213, 218),
        )
        .unwrap();
    let main_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(245, 500)))
        .unwrap();
    let x = symbols
        .declare(
            main_scope,
            "x",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(250, 251),
        )
        .unwrap();
    let sum = symbols
        .declare(
            main_scope,
            "sum",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(443, 446),
        )
        .unwrap();
    let retry_scope = symbols
        .create_scope(main_scope, ScopeKind::Block, Some(span(520, 570)))
        .unwrap();
    let retry_local = symbols
        .declare(
            retry_scope,
            "retry_local",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(525, 536),
        )
        .unwrap();
    let return_scope = symbols
        .create_scope(main_scope, ScopeKind::Block, Some(span(571, 610)))
        .unwrap();
    let return_guard = symbols
        .declare(
            return_scope,
            "return_guard",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(576, 588),
        )
        .unwrap();

    let identity_body = alloc(
        arena,
        CHirStmt::Return(
            Some(object_ident(arena, value, "value", int, span(224, 229))),
            span(217, 230),
        ),
    );

    let direct_call = |argument, at| {
        call(
            arena,
            function_ident(arena, identity, "identity", identity_ty, at),
            vec![argument],
            at,
        )
    };
    let initial_call = direct_call(literal(arena, 1, span(255, 256)), span(246, 257));
    let initializer = ObjectInitializer::new(
        WrittenInitializer::Expression(initial_call),
        InitPlan::new(
            int,
            vec![InitAction::store_scalar(
                SubobjectPath::root(),
                int,
                initial_call,
                initial_call.span(),
            )],
        ),
    );
    let declaration = alloc(
        arena,
        CHirDecl::Object(ObjectDecl {
            attributes: Vec::new(),
            cleanup: None,
            symbol: x,
            name: SpannedName {
                spelling: "x".to_owned(),
                span: span(250, 251),
            },
            ty: int,
            specifier: TypeSpecifier::Builtin {
                ty: int,
                span: span(246, 249),
            },
            written: WrittenDeclarator {
                layers: Vec::new(),
                span: span(246, 257),
            },
            storage: StorageClass::None,
            initializer: Some(initializer),
            asm_label: None,
            asm_label_span: None,
            gnu_extension_spans: Vec::new(),
            span: span(246, 258),
        }),
    );
    let sum_declaration = object_declaration(arena, sum, "sum", int, None, span(443, 447));

    let simple_value = direct_call(literal(arena, 2, span(263, 264)), span(254, 265));
    let simple_assignment = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        AssignmentOp::Simple,
        simple_value,
        span(259, 265),
    );
    let simple_statement = alloc(arena, CHirStmt::Expr(simple_assignment, span(259, 266)));

    let compound_value = direct_call(literal(arena, 3, span(272, 273)), span(263, 274));
    let compound_assignment = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        AssignmentOp::Add,
        compound_value,
        span(267, 274),
    );
    let compound_statement = alloc(arena, CHirStmt::Expr(compound_assignment, span(267, 275)));

    let initial_update = update(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        UpdateOp::PostIncrement,
        span(276, 279),
    );
    let initial_update_statement = alloc(arena, CHirStmt::Expr(initial_update, span(276, 280)));

    let while_compare = binary(
        arena,
        BinaryOp::Lt,
        object_ident(arena, x, "x", int, span(287, 288)),
        literal(arena, 8, span(291, 292)),
        span(287, 292),
    );
    let while_condition = direct_call(while_compare, span(278, 293));
    let while_update = update(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        UpdateOp::PostIncrement,
        span(296, 299),
    );
    let while_body = alloc(arena, CHirStmt::Expr(while_update, span(296, 300)));
    let while_statement = alloc(
        arena,
        CHirStmt::While(while_condition, while_body, span(281, 300)),
    );

    let for_compare = binary(
        arena,
        BinaryOp::Lt,
        object_ident(arena, x, "x", int, span(307, 308)),
        literal(arena, 10, span(311, 313)),
        span(307, 313),
    );
    let for_condition = direct_call(for_compare, span(301, 314));
    let for_step = update(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        UpdateOp::PostIncrement,
        span(316, 319),
    );
    let for_body = alloc(arena, CHirStmt::Null(span(321, 322)));
    let for_statement = alloc(
        arena,
        CHirStmt::For(
            CHirForInit::Empty,
            Some(for_condition),
            Some(for_step),
            for_body,
            span(301, 322),
        ),
    );

    let switch_argument = literal(arena, if select_default { 11 } else { 10 }, span(330, 332));
    let switch_expression = direct_call(switch_argument, span(323, 333));
    let case_value = literal(arena, 10, span(340, 342));
    let case_add_value = direct_call(literal(arena, 20, span(354, 356)), span(345, 357));
    let case_assignment = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        AssignmentOp::Add,
        case_add_value,
        span(343, 357),
    );
    let case_expression = alloc(arena, CHirStmt::Expr(case_assignment, span(343, 358)));
    let labelled_case = alloc(
        arena,
        CHirStmt::Label(
            SpannedName {
                spelling: "case_path".to_owned(),
                span: span(343, 352),
            },
            case_expression,
            span(343, 358),
        ),
    );
    let case_statement = alloc(
        arena,
        CHirStmt::Case(case_value, labelled_case, span(335, 358)),
    );
    let case_break = alloc(arena, CHirStmt::Break(span(359, 365)));

    let default_add_value = direct_call(literal(arena, 30, span(380, 382)), span(371, 383));
    let default_assignment = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        AssignmentOp::Add,
        default_add_value,
        span(369, 383),
    );
    let default_expression = alloc(arena, CHirStmt::Expr(default_assignment, span(369, 384)));
    let default_statement = alloc(arena, CHirStmt::Default(default_expression, span(366, 384)));
    let default_break = alloc(arena, CHirStmt::Break(span(385, 391)));
    let switch_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(case_statement),
                CHirBlockItem::Statement(case_break),
                CHirBlockItem::Statement(default_statement),
                CHirBlockItem::Statement(default_break),
            ],
            span(334, 392),
        ),
    );
    let switch_statement = alloc(
        arena,
        CHirStmt::Switch(switch_expression, switch_body, span(323, 392)),
    );

    let label_add_value = direct_call(literal(arena, 1, span(407, 408)), span(398, 409));
    let label_assignment = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        AssignmentOp::Add,
        label_add_value,
        span(396, 409),
    );
    let label_expression = alloc(arena, CHirStmt::Expr(label_assignment, span(396, 410)));
    let label_statement = alloc(
        arena,
        CHirStmt::Label(
            SpannedName {
                spelling: "after_switch".to_owned(),
                span: span(393, 405),
            },
            label_expression,
            span(393, 410),
        ),
    );

    let save_switch_value = assignment(
        arena,
        &symbols,
        &types,
        sum,
        "sum",
        int,
        AssignmentOp::Simple,
        object_ident(arena, x, "x", int, span(448, 449)),
        span(443, 449),
    );
    let save_switch_statement = alloc(arena, CHirStmt::Expr(save_switch_value, span(443, 450)));
    let reset_x = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        AssignmentOp::Simple,
        literal(arena, 0, span(454, 455)),
        span(451, 455),
    );
    let reset_x_statement = alloc(arena, CHirStmt::Expr(reset_x, span(451, 456)));
    let unwind_condition = direct_call(
        binary(
            arena,
            BinaryOp::Lt,
            object_ident(arena, x, "x", int, span(463, 464)),
            literal(arena, 3, span(467, 468)),
            span(463, 468),
        ),
        span(457, 469),
    );
    let unwind_increment = update(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        UpdateOp::PostIncrement,
        span(472, 475),
    );
    let unwind_increment_statement = alloc(arena, CHirStmt::Expr(unwind_increment, span(472, 476)));
    let unwind_switch_expression = direct_call(
        object_ident(arena, x, "x", int, span(485, 486)),
        span(476, 487),
    );
    let continue_case = alloc(
        arena,
        CHirStmt::Case(
            literal(arena, 2, span(493, 494)),
            alloc(arena, CHirStmt::Continue(span(496, 504))),
            span(488, 504),
        ),
    );
    let break_default = alloc(
        arena,
        CHirStmt::Default(
            alloc(arena, CHirStmt::Break(span(511, 517))),
            span(505, 517),
        ),
    );
    let unwind_switch_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(continue_case),
                CHirBlockItem::Statement(break_default),
            ],
            span(487, 518),
        ),
    );
    let unwind_switch = alloc(
        arena,
        CHirStmt::Switch(unwind_switch_expression, unwind_switch_body, span(476, 518)),
    );
    let add_after_switch = assignment(
        arena,
        &symbols,
        &types,
        sum,
        "sum",
        int,
        AssignmentOp::Add,
        object_ident(arena, x, "x", int, span(522, 523)),
        span(519, 523),
    );
    let add_after_switch_statement = alloc(arena, CHirStmt::Expr(add_after_switch, span(519, 524)));
    let unwind_loop_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(unwind_increment_statement),
                CHirBlockItem::Statement(unwind_switch),
                CHirBlockItem::Statement(add_after_switch_statement),
            ],
            span(470, 524),
        ),
    );
    let unwind_loop = alloc(
        arena,
        CHirStmt::While(unwind_condition, unwind_loop_body, span(457, 524)),
    );

    let retry_reset = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        AssignmentOp::Simple,
        literal(arena, 0, span(528, 529)),
        span(525, 529),
    );
    let retry_reset_statement = alloc(arena, CHirStmt::Expr(retry_reset, span(525, 530)));
    let retry_initializer = direct_call(literal(arena, 1, span(540, 541)), span(531, 542));
    let retry_declaration = object_declaration(
        arena,
        retry_local,
        "retry_local",
        int,
        Some(retry_initializer),
        span(531, 543),
    );
    let retry_increment = update(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        UpdateOp::PostIncrement,
        span(544, 547),
    );
    let retry_increment_statement = alloc(arena, CHirStmt::Expr(retry_increment, span(544, 548)));
    let retry_condition = direct_call(
        binary(
            arena,
            BinaryOp::Lt,
            object_ident(arena, x, "x", int, span(554, 555)),
            literal(arena, 2, span(558, 559)),
            span(554, 559),
        ),
        span(548, 560),
    );
    let retry_goto = alloc(
        arena,
        CHirStmt::Goto(
            SpannedName {
                spelling: "retry".to_owned(),
                span: span(566, 571),
            },
            span(561, 571),
        ),
    );
    let retry_if = alloc(
        arena,
        CHirStmt::If(retry_condition, retry_goto, None, span(548, 571)),
    );
    let retry_block = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Declaration(retry_declaration),
                CHirBlockItem::Statement(retry_increment_statement),
                CHirBlockItem::Statement(retry_if),
            ],
            span(531, 572),
        ),
    );
    let retry_label = alloc(
        arena,
        CHirStmt::Label(
            SpannedName {
                spelling: "retry".to_owned(),
                span: span(525, 530),
            },
            retry_block,
            span(525, 572),
        ),
    );
    let add_retry_count = assignment(
        arena,
        &symbols,
        &types,
        sum,
        "sum",
        int,
        AssignmentOp::Add,
        object_ident(arena, x, "x", int, span(576, 577)),
        span(573, 577),
    );
    let add_retry_statement = alloc(arena, CHirStmt::Expr(add_retry_count, span(573, 578)));

    let ternary_true = direct_call(
        object_ident(arena, sum, "sum", int, span(594, 597)),
        span(585, 598),
    );
    let ternary_false = direct_call(literal(arena, 0, span(610, 611)), span(601, 612));
    let ternary = alloc(
        arena,
        CHirExpr::Ternary(
            literal(arena, 1, span(582, 583)),
            ternary_true,
            ternary_false,
            span(582, 612),
        ),
    );
    let guard_initializer = direct_call(literal(arena, 0, span(618, 619)), span(613, 620));
    let guard_declaration = object_declaration(
        arena,
        return_guard,
        "return_guard",
        int,
        Some(guard_initializer),
        span(613, 621),
    );
    let return_statement = alloc(arena, CHirStmt::Return(Some(ternary), span(582, 613)));
    let nested_return = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Declaration(guard_declaration),
                CHirBlockItem::Statement(return_statement),
            ],
            span(579, 622),
        ),
    );
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Declaration(declaration),
                CHirBlockItem::Declaration(sum_declaration),
                CHirBlockItem::Statement(simple_statement),
                CHirBlockItem::Statement(compound_statement),
                CHirBlockItem::Statement(initial_update_statement),
                CHirBlockItem::Statement(while_statement),
                CHirBlockItem::Statement(for_statement),
                CHirBlockItem::Statement(switch_statement),
                CHirBlockItem::Statement(label_statement),
                CHirBlockItem::Statement(save_switch_statement),
                CHirBlockItem::Statement(reset_x_statement),
                CHirBlockItem::Statement(unwind_loop),
                CHirBlockItem::Statement(retry_reset_statement),
                CHirBlockItem::Statement(retry_label),
                CHirBlockItem::Statement(add_retry_statement),
                CHirBlockItem::Statement(nested_return),
            ],
            span(245, 623),
        ),
    );

    let function = |symbol, name: &str, ty, parameters, body, at| {
        alloc(
            arena,
            CHirDecl::Function(FunctionDecl {
                attributes: Vec::new(),
                symbol,
                name: SpannedName {
                    spelling: name.to_owned(),
                    span: at,
                },
                ty,
                return_specifier: TypeSpecifier::Builtin { ty: int, span: at },
                written: WrittenDeclarator {
                    layers: vec![WrittenDeclaratorLayer::Function {
                        parameters: None,
                        span: at,
                    }],
                    span: at,
                },
                parameters: if malformed_main_signature && name == "main" {
                    ParameterSyntax::Unspecified
                } else {
                    ParameterSyntax::Prototype {
                        parameters,
                        variadic: false,
                    }
                },
                storage: StorageClass::None,
                asm_label: None,
                asm_label_span: None,
                inline_spans: Vec::new(),
                gnu_extension_spans: Vec::new(),
                body: Some(body),
                span: at,
            }),
        )
    };
    let declarations = vec![
        function(
            identity,
            "identity",
            identity_ty,
            vec![parameter(value, "value", int, span(213, 218))],
            identity_body,
            span(200, 230),
        ),
        function(main, "main", main_ty, Vec::new(), main_body, span(240, 623)),
    ];
    CompositionProgram {
        unit: CHirUnit::new(declarations, types, symbols),
        int,
        main,
    }
}

#[allow(clippy::too_many_arguments)]
fn function_declaration<'arena>(
    arena: &'arena Arena,
    symbol: SymbolId,
    name: &str,
    function_ty: TypeId,
    return_ty: TypeId,
    parameters: Vec<ParameterDecl>,
    body: &'arena CHirStmt<'arena>,
    at: SourceSpan,
) -> &'arena CHirDecl<'arena> {
    alloc(
        arena,
        CHirDecl::Function(FunctionDecl {
            attributes: Vec::new(),
            symbol,
            name: SpannedName {
                spelling: name.to_owned(),
                span: at,
            },
            ty: function_ty,
            return_specifier: TypeSpecifier::Builtin {
                ty: return_ty,
                span: at,
            },
            written: WrittenDeclarator {
                layers: vec![WrittenDeclaratorLayer::Function {
                    parameters: None,
                    span: at,
                }],
                span: at,
            },
            parameters: ParameterSyntax::Prototype {
                parameters,
                variadic: false,
            },
            storage: StorageClass::None,
            asm_label: None,
            asm_label_span: None,
            inline_spans: Vec::new(),
            gnu_extension_spans: Vec::new(),
            body: Some(body),
            span: at,
        }),
    )
}

fn nested_label_program(arena: &Arena) -> CompositionProgram<'_> {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let identity_ty = function_type(&mut types, int, vec![int]);
    let main_ty = function_type(&mut types, int, Vec::new());

    let mut symbols = SymbolTable::new();
    let file_scope = symbols.file_scope();
    let identity = symbols
        .declare(
            file_scope,
            "identity",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            identity_ty,
            span(800, 808),
        )
        .unwrap();
    let main = symbols
        .declare(
            file_scope,
            "main",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            main_ty,
            span(840, 844),
        )
        .unwrap();
    let identity_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(809, 839)))
        .unwrap();
    let value = symbols
        .declare(
            identity_scope,
            "value",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            int,
            span(813, 818),
        )
        .unwrap();
    let main_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(845, 940)))
        .unwrap();
    let x = symbols
        .declare(
            main_scope,
            "x",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(850, 851),
        )
        .unwrap();

    let identity_body = alloc(
        arena,
        CHirStmt::Return(
            Some(object_ident(arena, value, "value", int, span(824, 829))),
            span(817, 830),
        ),
    );
    let declaration = object_declaration(
        arena,
        x,
        "x",
        int,
        Some(literal(arena, 0, span(854, 855))),
        span(846, 856),
    );
    let jump = alloc(
        arena,
        CHirStmt::Goto(
            SpannedName {
                spelling: "inner".to_owned(),
                span: span(862, 867),
            },
            span(857, 868),
        ),
    );
    let assign = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        AssignmentOp::Simple,
        literal(arena, 7, span(902, 903)),
        span(898, 903),
    );
    let labelled = alloc(
        arena,
        CHirStmt::Label(
            SpannedName {
                spelling: "inner".to_owned(),
                span: span(891, 896),
            },
            alloc(arena, CHirStmt::Expr(assign, span(898, 904))),
            span(891, 904),
        ),
    );
    let nested = alloc(
        arena,
        CHirStmt::Compound(vec![CHirBlockItem::Statement(labelled)], span(889, 906)),
    );
    let condition_call = call(
        arena,
        function_ident(arena, identity, "identity", identity_ty, span(873, 881)),
        vec![literal(arena, 0, span(882, 883))],
        span(873, 884),
    );
    let conditional = alloc(
        arena,
        CHirStmt::If(condition_call, nested, None, span(869, 906)),
    );
    let result = alloc(
        arena,
        CHirStmt::Return(
            Some(object_ident(arena, x, "x", int, span(914, 915))),
            span(907, 916),
        ),
    );
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Declaration(declaration),
                CHirBlockItem::Statement(jump),
                CHirBlockItem::Statement(conditional),
                CHirBlockItem::Statement(result),
            ],
            span(845, 917),
        ),
    );

    CompositionProgram {
        unit: CHirUnit::new(
            vec![
                function_declaration(
                    arena,
                    identity,
                    "identity",
                    identity_ty,
                    int,
                    vec![parameter(value, "value", int, span(813, 818))],
                    identity_body,
                    span(800, 830),
                ),
                function_declaration(
                    arena,
                    main,
                    "main",
                    main_ty,
                    int,
                    Vec::new(),
                    main_body,
                    span(840, 917),
                ),
            ],
            types,
            symbols,
        ),
        int,
        main,
    }
}

#[derive(Debug, Clone, Copy)]
enum NestedWhileGotoKind {
    RepeatToThree,
    AssignSeven,
}

fn call_free_nested_while_goto_program(
    arena: &Arena,
    kind: NestedWhileGotoKind,
) -> CompositionProgram<'_> {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let main_ty = function_type(&mut types, int, Vec::new());

    let mut symbols = SymbolTable::new();
    let file_scope = symbols.file_scope();
    let main = symbols
        .declare(
            file_scope,
            "main",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            main_ty,
            span(1_050, 1_054),
        )
        .unwrap();
    let main_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(1_055, 1_180)))
        .unwrap();
    let x = symbols
        .declare(
            main_scope,
            "x",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(1_060, 1_061),
        )
        .unwrap();

    let declaration = object_declaration(
        arena,
        x,
        "x",
        int,
        Some(literal(arena, 0, span(1_064, 1_065))),
        span(1_056, 1_066),
    );
    let jump = alloc(
        arena,
        CHirStmt::Goto(
            SpannedName {
                spelling: "inner".to_owned(),
                span: span(1_072, 1_077),
            },
            span(1_067, 1_078),
        ),
    );
    let (assignment_op, assignment_value) = match kind {
        NestedWhileGotoKind::RepeatToThree => (AssignmentOp::Add, 1),
        NestedWhileGotoKind::AssignSeven => (AssignmentOp::Simple, 7),
    };
    let assign = assignment(
        arena,
        &symbols,
        &types,
        x,
        "x",
        int,
        assignment_op,
        literal(arena, assignment_value, span(1_105, 1_106)),
        span(1_100, 1_106),
    );
    let labelled = alloc(
        arena,
        CHirStmt::Label(
            SpannedName {
                spelling: "inner".to_owned(),
                span: span(1_090, 1_095),
            },
            alloc(arena, CHirStmt::Expr(assign, span(1_100, 1_107))),
            span(1_090, 1_107),
        ),
    );
    let mut while_items = vec![CHirBlockItem::Statement(labelled)];
    if matches!(kind, NestedWhileGotoKind::RepeatToThree) {
        let retry_condition = binary(
            arena,
            BinaryOp::Lt,
            object_ident(arena, x, "x", int, span(1_111, 1_112)),
            literal(arena, 3, span(1_115, 1_116)),
            span(1_111, 1_116),
        );
        let retry = alloc(
            arena,
            CHirStmt::Goto(
                SpannedName {
                    spelling: "inner".to_owned(),
                    span: span(1_123, 1_128),
                },
                span(1_118, 1_129),
            ),
        );
        while_items.push(CHirBlockItem::Statement(alloc(
            arena,
            CHirStmt::If(retry_condition, retry, None, span(1_108, 1_129)),
        )));
    }
    while_items.push(CHirBlockItem::Statement(alloc(
        arena,
        CHirStmt::Break(span(1_130, 1_136)),
    )));
    let while_body = alloc(arena, CHirStmt::Compound(while_items, span(1_088, 1_138)));
    let nested_while = alloc(
        arena,
        CHirStmt::While(
            literal(arena, 0, span(1_085, 1_086)),
            while_body,
            span(1_079, 1_138),
        ),
    );
    let result = alloc(
        arena,
        CHirStmt::Return(
            Some(object_ident(arena, x, "x", int, span(1_146, 1_147))),
            span(1_139, 1_148),
        ),
    );
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Declaration(declaration),
                CHirBlockItem::Statement(jump),
                CHirBlockItem::Statement(nested_while),
                CHirBlockItem::Statement(result),
            ],
            span(1_055, 1_149),
        ),
    );
    let main_declaration = function_declaration(
        arena,
        main,
        "main",
        main_ty,
        int,
        Vec::new(),
        main_body,
        span(1_050, 1_149),
    );

    CompositionProgram {
        unit: CHirUnit::new(vec![main_declaration], types, symbols),
        int,
        main,
    }
}

fn switch_case_conversion_program(arena: &Arena, with_call: bool) -> CompositionProgram<'_> {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let uint = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::UInt),
        qualifiers: Qualifiers::default(),
    });
    let identity_ty = function_type(&mut types, uint, vec![uint]);
    let main_ty = function_type(&mut types, int, Vec::new());

    let mut symbols = SymbolTable::new();
    let file_scope = symbols.file_scope();
    let identity = symbols
        .declare(
            file_scope,
            "identity_uint",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            identity_ty,
            span(1_200, 1_213),
        )
        .unwrap();
    let main = symbols
        .declare(
            file_scope,
            "main",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            main_ty,
            span(1_240, 1_244),
        )
        .unwrap();
    let identity_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(1_214, 1_239)))
        .unwrap();
    let value = symbols
        .declare(
            identity_scope,
            "value",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            uint,
            span(1_218, 1_223),
        )
        .unwrap();
    let main_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(1_245, 1_380)))
        .unwrap();
    let selector = symbols
        .declare(
            main_scope,
            "selector",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            uint,
            span(1_250, 1_258),
        )
        .unwrap();
    let result = symbols
        .declare(
            main_scope,
            "result",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(1_275, 1_281),
        )
        .unwrap();

    let identity_body = alloc(
        arena,
        CHirStmt::Return(
            Some(object_ident(
                arena,
                value,
                "value",
                uint,
                span(1_228, 1_233),
            )),
            span(1_221, 1_234),
        ),
    );
    let selector_declaration = object_declaration(
        arena,
        selector,
        "selector",
        uint,
        Some(suffixed_literal(
            arena,
            u64::from(u32::MAX),
            IntSuffix::U,
            span(1_261, 1_272),
        )),
        span(1_246, 1_273),
    );
    let result_declaration = object_declaration(
        arena,
        result,
        "result",
        int,
        Some(literal(arena, 0, span(1_284, 1_285))),
        span(1_274, 1_286),
    );
    let selector_expression = object_ident(arena, selector, "selector", uint, span(1_294, 1_302));
    let controlling = if with_call {
        call(
            arena,
            function_ident(
                arena,
                identity,
                "identity_uint",
                identity_ty,
                span(1_280, 1_293),
            ),
            vec![selector_expression],
            span(1_280, 1_303),
        )
    } else {
        selector_expression
    };
    let one_long = suffixed_literal(arena, 1, IntSuffix::L, span(1_314, 1_316));
    let minus_one_long = alloc(
        arena,
        CHirExpr::Unary(UnaryOp::Neg, one_long, span(1_313, 1_316)),
    );
    let case_assignment = assignment(
        arena,
        &symbols,
        &types,
        result,
        "result",
        int,
        AssignmentOp::Simple,
        literal(arena, 7, span(1_328, 1_329)),
        span(1_319, 1_329),
    );
    let case_statement = alloc(
        arena,
        CHirStmt::Case(
            minus_one_long,
            alloc(arena, CHirStmt::Expr(case_assignment, span(1_319, 1_330))),
            span(1_307, 1_330),
        ),
    );
    let case_break = alloc(arena, CHirStmt::Break(span(1_331, 1_337)));
    let default_assignment = assignment(
        arena,
        &symbols,
        &types,
        result,
        "result",
        int,
        AssignmentOp::Simple,
        literal(arena, 3, span(1_356, 1_357)),
        span(1_347, 1_357),
    );
    let default_statement = alloc(
        arena,
        CHirStmt::Default(
            alloc(
                arena,
                CHirStmt::Expr(default_assignment, span(1_347, 1_358)),
            ),
            span(1_339, 1_358),
        ),
    );
    let switch_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(case_statement),
                CHirBlockItem::Statement(case_break),
                CHirBlockItem::Statement(default_statement),
            ],
            span(1_305, 1_360),
        ),
    );
    let switch_statement = alloc(
        arena,
        CHirStmt::Switch(controlling, switch_body, span(1_287, 1_360)),
    );
    let return_statement = alloc(
        arena,
        CHirStmt::Return(
            Some(object_ident(
                arena,
                result,
                "result",
                int,
                span(1_368, 1_374),
            )),
            span(1_361, 1_375),
        ),
    );
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Declaration(selector_declaration),
                CHirBlockItem::Declaration(result_declaration),
                CHirBlockItem::Statement(switch_statement),
                CHirBlockItem::Statement(return_statement),
            ],
            span(1_245, 1_376),
        ),
    );

    CompositionProgram {
        unit: CHirUnit::new(
            vec![
                function_declaration(
                    arena,
                    identity,
                    "identity_uint",
                    identity_ty,
                    uint,
                    vec![parameter(value, "value", uint, span(1_218, 1_223))],
                    identity_body,
                    span(1_200, 1_234),
                ),
                function_declaration(
                    arena,
                    main,
                    "main",
                    main_ty,
                    int,
                    Vec::new(),
                    main_body,
                    span(1_240, 1_376),
                ),
            ],
            types,
            symbols,
        ),
        int,
        main,
    }
}

fn void_call_program(arena: &Arena) -> CompositionProgram<'_> {
    let mut types = TypeInterner::new();
    let void = types.intern(CanonicalType {
        kind: TypeKind::Void,
        qualifiers: Qualifiers::default(),
    });
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let sink_ty = function_type(&mut types, void, vec![int]);
    let main_ty = function_type(&mut types, int, Vec::new());

    let mut symbols = SymbolTable::new();
    let file_scope = symbols.file_scope();
    let sink = symbols
        .declare(
            file_scope,
            "sink",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            sink_ty,
            span(950, 954),
        )
        .unwrap();
    let main = symbols
        .declare(
            file_scope,
            "main",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            main_ty,
            span(990, 994),
        )
        .unwrap();
    let sink_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(955, 989)))
        .unwrap();
    let value = symbols
        .declare(
            sink_scope,
            "value",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            int,
            span(960, 965),
        )
        .unwrap();

    let sink_body = alloc(arena, CHirStmt::Return(None, span(975, 982)));
    let sink_call = call(
        arena,
        function_ident(arena, sink, "sink", sink_ty, span(1000, 1004)),
        vec![literal(arena, 41, span(1005, 1007))],
        span(1000, 1008),
    );
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(alloc(arena, CHirStmt::Expr(sink_call, span(1000, 1009)))),
                CHirBlockItem::Statement(alloc(
                    arena,
                    CHirStmt::Return(Some(literal(arena, 42, span(1017, 1019))), span(1010, 1020)),
                )),
            ],
            span(995, 1021),
        ),
    );

    CompositionProgram {
        unit: CHirUnit::new(
            vec![
                function_declaration(
                    arena,
                    sink,
                    "sink",
                    sink_ty,
                    void,
                    vec![parameter(value, "value", int, span(960, 965))],
                    sink_body,
                    span(950, 982),
                ),
                function_declaration(
                    arena,
                    main,
                    "main",
                    main_ty,
                    int,
                    Vec::new(),
                    main_body,
                    span(990, 1021),
                ),
            ],
            types,
            symbols,
        ),
        int,
        main,
    }
}

fn void_calls_in_for_clauses_program(arena: &Arena) -> CompositionProgram<'_> {
    let mut types = TypeInterner::new();
    let void = types.intern(CanonicalType {
        kind: TypeKind::Void,
        qualifiers: Qualifiers::default(),
    });
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let touch_ty = function_type(&mut types, void, Vec::new());
    let main_ty = function_type(&mut types, int, Vec::new());

    let mut symbols = SymbolTable::new();
    let file_scope = symbols.file_scope();
    let touch = symbols
        .declare(
            file_scope,
            "touch",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            touch_ty,
            span(1_400, 1_405),
        )
        .unwrap();
    let main = symbols
        .declare(
            file_scope,
            "main",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            main_ty,
            span(1_430, 1_434),
        )
        .unwrap();
    let main_scope = symbols
        .create_scope(file_scope, ScopeKind::Function, Some(span(1_435, 1_510)))
        .unwrap();
    let i = symbols
        .declare(
            main_scope,
            "i",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(1_440, 1_441),
        )
        .unwrap();

    let touch_body = alloc(arena, CHirStmt::Return(None, span(1_415, 1_422)));
    let i_declaration = object_declaration(
        arena,
        i,
        "i",
        int,
        Some(literal(arena, 0, span(1_443, 1_444))),
        span(1_436, 1_445),
    );
    let touch_call = |at| {
        call(
            arena,
            function_ident(arena, touch, "touch", touch_ty, at),
            Vec::new(),
            at,
        )
    };
    let initializer = touch_call(span(1_450, 1_457));
    let condition = binary(
        arena,
        BinaryOp::Lt,
        object_ident(arena, i, "i", int, span(1_458, 1_459)),
        literal(arena, 1, span(1_460, 1_461)),
        span(1_458, 1_461),
    );
    let step = touch_call(span(1_462, 1_469));
    let increment = update(
        arena,
        &symbols,
        &types,
        i,
        "i",
        int,
        UpdateOp::PostIncrement,
        span(1_472, 1_475),
    );
    let for_statement = alloc(
        arena,
        CHirStmt::For(
            CHirForInit::Expression(initializer),
            Some(condition),
            Some(step),
            alloc(arena, CHirStmt::Expr(increment, span(1_472, 1_476))),
            span(1_446, 1_477),
        ),
    );
    let return_statement = alloc(
        arena,
        CHirStmt::Return(
            Some(object_ident(arena, i, "i", int, span(1_485, 1_486))),
            span(1_478, 1_487),
        ),
    );
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Declaration(i_declaration),
                CHirBlockItem::Statement(for_statement),
                CHirBlockItem::Statement(return_statement),
            ],
            span(1_435, 1_488),
        ),
    );

    CompositionProgram {
        unit: CHirUnit::new(
            vec![
                function_declaration(
                    arena,
                    touch,
                    "touch",
                    touch_ty,
                    void,
                    Vec::new(),
                    touch_body,
                    span(1_400, 1_423),
                ),
                function_declaration(
                    arena,
                    main,
                    "main",
                    main_ty,
                    int,
                    Vec::new(),
                    main_body,
                    span(1_430, 1_488),
                ),
            ],
            types,
            symbols,
        ),
        int,
        main,
    }
}

fn unreachable_missing_label_program(arena: &Arena) -> CompositionProgram<'_> {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let main_ty = function_type(&mut types, int, Vec::new());

    let mut symbols = SymbolTable::new();
    let file_scope = symbols.file_scope();
    let main = symbols
        .declare(
            file_scope,
            "main",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            main_ty,
            span(1_520, 1_524),
        )
        .unwrap();
    let return_statement = alloc(
        arena,
        CHirStmt::Return(
            Some(literal(arena, 0, span(1_540, 1_541))),
            span(1_533, 1_542),
        ),
    );
    let missing_span = span(1_543, 1_556);
    let goto_statement = alloc(
        arena,
        CHirStmt::Goto(
            SpannedName {
                spelling: "missing".to_owned(),
                span: missing_span,
            },
            missing_span,
        ),
    );
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![
                CHirBlockItem::Statement(return_statement),
                CHirBlockItem::Statement(goto_statement),
            ],
            span(1_525, 1_557),
        ),
    );

    CompositionProgram {
        unit: CHirUnit::new(
            vec![function_declaration(
                arena,
                main,
                "main",
                main_ty,
                int,
                Vec::new(),
                main_body,
                span(1_520, 1_557),
            )],
            types,
            symbols,
        ),
        int,
        main,
    }
}

#[test]
fn same_translation_unit_call_binds_parameters_and_returns_the_real_value() {
    let arena = Arena::default();
    let program = program(&arena);

    let outcome = interpret_scalar_function(
        &program.unit,
        program.call_add,
        InterpreterLimits::default(),
    )
    .unwrap();

    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 907,
        })
    );
    assert!(outcome.steps > 0);
    assert_ne!(program.add, program.call_add);
}

#[test]
fn recursive_fib_uses_iterative_call_frames_and_one_shared_fuel_counter() {
    let arena = Arena::default();
    let program = program(&arena);

    let first = interpret_scalar_function(
        &program.unit,
        program.call_fib,
        InterpreterLimits::new(1_000_000),
    )
    .unwrap();
    let second = interpret_scalar_function(
        &program.unit,
        program.call_fib,
        InterpreterLimits::new(1_000_000),
    )
    .unwrap();
    let third = interpret_scalar_function(
        &program.unit,
        program.call_fib,
        InterpreterLimits::new(1_000_000),
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(second, third);
    assert_eq!(
        first.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 55,
        })
    );
    assert!(
        first.steps > 100,
        "nested calls must debit the invocation-wide counter"
    );
    assert_ne!(program.fib, program.call_fib);

    let cutoff = first.steps - 1;
    let exhausted = interpret_scalar_function(
        &program.unit,
        program.call_fib,
        InterpreterLimits::new(cutoff),
    )
    .unwrap_err();
    assert_eq!(exhausted.code(), "E3008");
    assert_eq!(exhausted.message(), "interpreter-fuel-exhausted");
    assert!(matches!(
        exhausted,
        InterpreterError::FuelExhausted {
            limit,
            consumed,
            ..
        } if limit == cutoff && consumed == cutoff
    ));
}

#[test]
fn deep_linear_recursion_uses_the_explicit_stack_and_shared_budget() {
    let arena = Arena::default();
    let program = program(&arena);
    let outcome = interpret_scalar_function(
        &program.unit,
        program.call_linear,
        InterpreterLimits::new(2_000_000),
    )
    .unwrap();
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 7,
        })
    );
    assert!(outcome.steps > 200_000);
    assert_ne!(program.linear, program.call_linear);

    let cutoff = outcome.steps - 1;
    let error = interpret_scalar_function(
        &program.unit,
        program.call_linear,
        InterpreterLimits::new(cutoff),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        InterpreterError::FuelExhausted {
            limit,
            consumed,
            ..
        } if limit == cutoff && consumed == cutoff
    ));
}

#[test]
fn calls_compose_with_initialization_mutation_loops_switch_and_labels() {
    for (select_default, expected) in [(false, 37_u64), (true, 47_u64)] {
        let arena = Arena::default();
        let program = composition_program(&arena, select_default);
        let outcome =
            interpret_scalar_function(&program.unit, program.main, InterpreterLimits::new(100_000))
                .unwrap();
        assert_eq!(
            outcome.return_value,
            Some(ScalarValue {
                ty: program.int,
                raw_bits: u128::from(expected),
            }),
            "select_default={select_default}"
        );
    }
}

#[test]
fn signature_validation_is_identical_before_call_or_call_free_dispatch() {
    let call_free_arena = Arena::default();
    let call_free = malformed_call_free_signature_program(&call_free_arena);
    let call_free_error = interpret_scalar_function(
        &call_free.unit,
        call_free.main,
        InterpreterLimits::default(),
    )
    .expect_err("canonical prototype and written unspecified parameters must not disagree");
    assert_eq!(call_free_error.code(), "E3007");
    assert_eq!(
        call_free_error.message(),
        "unsupported-interpreter-function-signature"
    );
    assert_eq!(call_free_error.span(), Some(span(500, 522)));
    assert!(matches!(
        call_free_error,
        InterpreterError::UnsupportedFunctionSignature { .. }
    ));

    let call_arena = Arena::default();
    let call_bearing = composition_program_with_signature(&call_arena, false, true);
    let call_error = interpret_scalar_function(
        &call_bearing.unit,
        call_bearing.main,
        InterpreterLimits::default(),
    )
    .expect_err("dispatch must not change signature acceptance");
    assert_eq!(call_error.code(), "E3007");
    assert_eq!(
        call_error.message(),
        "unsupported-interpreter-function-signature"
    );
    assert_eq!(call_error.span(), Some(span(240, 623)));
    assert!(matches!(
        call_error,
        InterpreterError::UnsupportedFunctionSignature { .. }
    ));
}

#[test]
fn a_variadic_definition_accepts_a_call_without_anonymous_arguments() {
    let arena = Arena::default();
    let program = program_with_add_fault(&arena, AddFault::VariadicDefinition);
    let outcome = interpret_scalar_function(
        &program.unit,
        program.call_add,
        InterpreterLimits::default(),
    )
    .unwrap();
    assert_eq!(outcome.return_value.unwrap().raw_bits, 907);
}

#[test]
fn call_execution_fails_closed_on_non_direct_or_unsupported_function_contracts() {
    for fault in [
        AddFault::IndirectCallee,
        AddFault::UnnamedParameter,
        AddFault::MissingParameterSymbol,
        AddFault::ForgedDesignator,
        AddFault::ForgedWrittenDeclarator,
        AddFault::ForgedParameterStorage,
    ] {
        let arena = Arena::default();
        let program = program_with_add_fault(&arena, fault);
        let error = interpret_scalar_function(
            &program.unit,
            program.call_add,
            InterpreterLimits::default(),
        )
        .expect_err("unsupported or inconsistent calls must not produce a scalar value");
        let (expected_code, expected_message, expected_span) = match fault {
            AddFault::IndirectCallee => (
                "E3007",
                "unsupported-interpreter-expression: indirect scalar function call",
                span(138, 142),
            ),
            AddFault::UnnamedParameter => ("E3007", "invalid-scalar-place", span(20, 24)),
            AddFault::MissingParameterSymbol => (
                "E3007",
                "unsupported-interpreter-function-signature",
                span(8, 12),
            ),
            AddFault::ForgedDesignator => (
                "E3007",
                "unsupported-interpreter-expression: inconsistent function designator",
                span(139, 142),
            ),
            AddFault::ForgedWrittenDeclarator => (
                "E3007",
                "unsupported-interpreter-function-signature",
                span(0, 39),
            ),
            AddFault::ForgedParameterStorage => (
                "E3007",
                "unsupported-interpreter-function-signature",
                span(8, 12),
            ),
            AddFault::VariadicDefinition
            | AddFault::None
            | AddFault::FileScopeParameter
            | AddFault::DuplicateParameterSpelling => unreachable!(),
        };
        assert_eq!(error.code(), expected_code, "fault {fault:?}: {error:?}");
        assert_eq!(
            error.message(),
            expected_message,
            "fault {fault:?}: {error:?}"
        );
        assert_eq!(
            error.span(),
            Some(expected_span),
            "fault {fault:?}: {error:?}"
        );
        match fault {
            AddFault::IndirectCallee | AddFault::ForgedDesignator => assert!(matches!(
                error,
                InterpreterError::UnsupportedExpression { .. }
            )),
            AddFault::UnnamedParameter => {
                assert!(matches!(error, InterpreterError::InvalidScalarPlace { .. }))
            }
            AddFault::MissingParameterSymbol
            | AddFault::ForgedWrittenDeclarator
            | AddFault::ForgedParameterStorage => assert!(matches!(
                error,
                InterpreterError::UnsupportedFunctionSignature { .. }
            )),
            AddFault::VariadicDefinition
            | AddFault::None
            | AddFault::FileScopeParameter
            | AddFault::DuplicateParameterSpelling => unreachable!(),
        }
    }
}

#[test]
fn compatible_unspecified_declaration_composes_with_a_prototype_definition() {
    let arena = Arena::default();
    let program = program_with_redeclaration(&arena, RedeclarationKind::UnspecifiedThenPrototype);
    let outcome = interpret_scalar_function(
        &program.unit,
        program.call_add,
        InterpreterLimits::default(),
    )
    .expect("a compatible historical declaration must not poison its definition");
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 907,
        })
    );
}

#[test]
fn extern_declaration_composes_with_a_plain_definition() {
    let arena = Arena::default();
    let program = program_with_redeclaration(&arena, RedeclarationKind::ExternThenDefinition);
    let outcome = interpret_scalar_function(
        &program.unit,
        program.call_add,
        InterpreterLimits::default(),
    )
    .expect("extern and plain declarations of one external function must compose");
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 907,
        })
    );
}

#[test]
fn signature_rejects_a_parameter_symbol_declared_at_file_scope() {
    let arena = Arena::default();
    let program = program_with_add_fault(&arena, AddFault::FileScopeParameter);
    let error = interpret_scalar_function(
        &program.unit,
        program.call_add,
        InterpreterLimits::default(),
    )
    .expect_err("a file-scope Parameter SymbolId is not a function parameter binding");
    assert_eq!(error.code(), "E3007");
    assert_eq!(
        error.message(),
        "unsupported-interpreter-function-signature"
    );
    assert_eq!(error.span(), Some(span(8, 12)));
}

#[test]
fn signature_rejects_duplicate_parameter_spelling_across_distinct_ids() {
    let arena = Arena::default();
    let program = program_with_add_fault(&arena, AddFault::DuplicateParameterSpelling);
    let error = interpret_scalar_function(
        &program.unit,
        program.call_add,
        InterpreterLimits::default(),
    )
    .expect_err("distinct SymbolIds must not hide a duplicate parameter name");
    assert_eq!(error.code(), "E3007");
    assert_eq!(
        error.message(),
        "unsupported-interpreter-function-signature"
    );
    assert_eq!(error.span(), Some(span(14, 19)));
}

#[test]
fn goto_can_enter_a_nested_label_without_evaluating_the_guard() {
    let arena = Arena::default();
    let program = nested_label_program(&arena);
    let outcome =
        interpret_scalar_function(&program.unit, program.main, InterpreterLimits::default())
            .expect("a function-scope label is reachable regardless of statement nesting");
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 7,
        })
    );
}

#[test]
fn call_free_goto_reenters_a_nested_while_until_the_guard_changes() {
    let arena = Arena::default();
    let program = call_free_nested_while_goto_program(&arena, NestedWhileGotoKind::RepeatToThree);
    let outcome =
        interpret_scalar_function(&program.unit, program.main, InterpreterLimits::default())
            .expect("goto must enter and re-enter a label nested in a false while");
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 3,
        })
    );
}

#[test]
fn call_free_goto_enters_a_nested_while_and_breaks_from_it() {
    let arena = Arena::default();
    let program = call_free_nested_while_goto_program(&arena, NestedWhileGotoKind::AssignSeven);
    let outcome =
        interpret_scalar_function(&program.unit, program.main, InterpreterLimits::default())
            .expect("goto must enter a label nested in a false while before break exits it");
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 7,
        })
    );
}

#[test]
fn call_free_switch_converts_case_ice_to_the_promoted_selector_type() {
    let arena = Arena::default();
    let program = switch_case_conversion_program(&arena, false);
    let outcome =
        interpret_scalar_function(&program.unit, program.main, InterpreterLimits::default())
            .expect("UINT_MAX must match case -1L after conversion to unsigned int");
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 7,
        })
    );
}

#[test]
fn call_aware_switch_converts_case_ice_to_the_promoted_selector_type() {
    let arena = Arena::default();
    let program = switch_case_conversion_program(&arena, true);
    let outcome =
        interpret_scalar_function(&program.unit, program.main, InterpreterLimits::default())
            .expect("call-aware switch dispatch must use the selector's promoted type");
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 7,
        })
    );
}

#[test]
fn void_helper_call_executes_as_an_expression_statement() {
    let arena = Arena::default();
    let program = void_call_program(&arena);
    let outcome =
        interpret_scalar_function(&program.unit, program.main, InterpreterLimits::default())
            .expect("a direct void call is valid when its result is not consumed");
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 42,
        })
    );
}

#[test]
fn void_calls_execute_in_for_initializer_and_step() {
    let arena = Arena::default();
    let program = void_calls_in_for_clauses_program(&arena);
    let outcome =
        interpret_scalar_function(&program.unit, program.main, InterpreterLimits::default())
            .expect("void calls in both discarded for clauses must execute without a value");
    assert_eq!(
        outcome.steps, 18,
        "the shared fuel count proves that both void helper frames executed"
    );
    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: program.int,
            raw_bits: 1,
        })
    );
}

#[test]
fn unreachable_goto_to_an_absent_label_is_rejected_before_execution() {
    let arena = Arena::default();
    let program = unreachable_missing_label_program(&arena);
    let error =
        interpret_scalar_function(&program.unit, program.main, InterpreterLimits::default())
            .expect_err("function-wide validation must reject an absent label after return");
    assert_eq!(error.code(), "E3007");
    assert_eq!(error.message(), "unknown-interpreter-label: missing");
    assert_eq!(error.span(), Some(span(1_543, 1_556)));
    assert!(matches!(
        error,
        InterpreterError::UnknownLabel { ref spelling, .. } if spelling == "missing"
    ));
}
