//! C-HIR interpreter unit tests.

use super::*;
use crate::{
    IntBase, IntLit, IntSuffix, SpannedName, TypeSpecifier, WrittenDeclarator,
    WrittenDeclaratorLayer,
};
use evocc_common::{Arena, FileId};
use evocc_memory::{AccessLayout, MemoryFault, ObjectId};
use evocc_types::{CanonicalType, IntRank, Qualifiers};

#[test]
fn repeated_evaluation_shares_the_immutable_units_linkage() {
    let unit = CHirUnit::new(vec![], TypeInterner::new(), crate::SymbolTable::new());
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let first = Interpreter::new(&unit, &target, InterpreterLimits::default(), false).unwrap();
    let second = Interpreter::new(&unit, &target, InterpreterLimits::default(), false).unwrap();
    assert!(
        std::ptr::eq(first.linkage, second.linkage),
        "one immutable unit must not rebuild its declaration inventory for every initializer"
    );
}

#[test]
fn substitution_invalidates_only_the_rewritten_units_linkage() {
    let arena = Arena::default();
    let (mut unit, function, _) = scalar_program(&arena, false);
    let scope = unit
        .symbols
        .create_scope(unit.symbols.file_scope(), crate::ScopeKind::Function, None)
        .unwrap();
    unit.symbols.bind_function_scope(scope, function).unwrap();
    unit.symbol_linkage().unwrap();
    let rewritten = crate::inline::substitute(
        &unit,
        &arena,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
    )
    .unwrap();
    assert!(unit.resolved_linkage.get().is_some());
    assert!(rewritten.resolved_linkage.get().is_none());
    let cached = rewritten.symbol_linkage().unwrap();
    let derived = crate::linkage::SymbolLinkage::derive(&rewritten).unwrap();
    assert_eq!(cached.aliases, derived.aliases);
    assert_eq!(cached.weak, derived.weak);
}
fn span(start: u32, end: u32) -> SourceSpan {
    SourceSpan::new(FileId::new(701), start, end, 1, start + 1)
}

fn alloc<'arena, T: 'arena>(arena: &'arena Arena, value: T) -> &'arena T {
    // Safety: these C-HIR nodes contain no Arena handle and their
    // destructors only release owned source-fact buffers.
    unsafe { arena.alloc_shared(value) }
}

fn scalar_program(arena: &Arena, use_task_machine: bool) -> (CHirUnit<'_>, SymbolId, TypeId) {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let function_ty = types.intern(CanonicalType {
        kind: TypeKind::Function {
            return_type: int,
            parameters: FunctionParameters::Prototype {
                types: Vec::new(),
                variadic: false,
            },
        },
        qualifiers: Qualifiers::default(),
    });
    let mut symbols = crate::SymbolTable::new();
    let function = symbols
        .declare(
            symbols.file_scope(),
            "main",
            crate::SymbolData::Function {
                storage: StorageClass::None,
            },
            function_ty,
            span(0, 4),
        )
        .expect("declare test main");

    let value = alloc(
        arena,
        CHirExpr::Lit(
            IntLit {
                value: 7,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span(20, 21),
        ),
    );
    let returned = alloc(arena, CHirStmt::Return(Some(value), span(13, 22)));
    let items = if use_task_machine {
        let name = SpannedName {
            spelling: "done".to_owned(),
            span: span(7, 11),
        };
        vec![
            CHirBlockItem::Statement(alloc(arena, CHirStmt::Goto(name.clone(), span(5, 12)))),
            CHirBlockItem::Statement(alloc(arena, CHirStmt::Label(name, returned, span(13, 22)))),
        ]
    } else {
        vec![CHirBlockItem::Statement(returned)]
    };
    let body = alloc(arena, CHirStmt::Compound(items, span(4, 23), None));
    let declaration = alloc(
        arena,
        CHirDecl::Function(FunctionDecl {
            attributes: Vec::new(),
            symbol: function,
            name: SpannedName {
                spelling: "main".to_owned(),
                span: span(0, 4),
            },
            ty: function_ty,
            return_specifier: TypeSpecifier::Builtin {
                ty: int,
                span: span(0, 3),
            },
            written: WrittenDeclarator {
                layers: vec![WrittenDeclaratorLayer::Function {
                    parameters: None,
                    span: span(3, 5),
                }],
                span: span(0, 5),
            },
            parameters: ParameterSyntax::Prototype {
                parameters: Vec::new(),
                variadic: false,
            },
            storage: StorageClass::None,
            asm_label: None,
            asm_label_span: None,
            inline_spans: Vec::new(),
            gnu_extension_spans: Vec::new(),
            body: Some(body),
            span: span(0, 23),
        }),
    );
    (
        CHirUnit::new(vec![declaration], types, symbols),
        function,
        int,
    )
}

fn assert_empty_execution_state(outcome: &InterpretOutcome) {
    assert!(outcome.world.volatile_trace().is_empty());
    assert_eq!(
        outcome.memory.config().limits(),
        MemoryLimits {
            max_objects: MAX_MEMORY_OBJECTS,
            max_object_bytes: MAX_MEMORY_OBJECT_BYTES,
            max_total_bytes: MAX_MEMORY_TOTAL_BYTES,
        }
    );
    assert_eq!(
        outcome.world.limits(),
        WorldLimits {
            max_volatile_events: MAX_VOLATILE_EVENTS,
        }
    );
    assert!(matches!(
        outcome.memory.object_info(ObjectId::from_raw(1)),
        Err(MemoryFault::UnknownObject { .. })
    ));
}

#[test]
fn call_free_scalar_execution_keeps_value_and_returns_empty_states() {
    let arena = Arena::default();
    let (unit, function, int) = scalar_program(&arena, false);

    let outcome = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();

    assert_eq!(
        outcome.return_value,
        Some(ScalarValue {
            ty: int,
            raw_bits: 7,
        })
    );
    assert!(outcome.steps > 0);
    assert_empty_execution_state(&outcome);
}

#[test]
fn task_machine_uses_target_aware_states_and_is_deterministic() {
    let arena = Arena::default();
    let (unit, function, int) = scalar_program(&arena, true);
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();

    let first = interpret_scalar_function_with_target(
        &unit,
        function,
        &target,
        InterpreterLimits::default(),
    )
    .unwrap();
    let second = interpret_scalar_function_with_target(
        &unit,
        function,
        &target,
        InterpreterLimits::default(),
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(
        first.return_value,
        Some(ScalarValue {
            ty: int,
            raw_bits: 7,
        })
    );
    assert_eq!(first.memory.config().byte_order(), target.byte_order());
    assert_eq!(
        first.memory.config().pointer_layout().size_bytes,
        target.pointer_layout().size_bytes
    );
    assert_empty_execution_state(&first);
}

#[test]
fn invalid_target_memory_setup_fails_closed_without_a_source_span() {
    let error = InterpreterError::MemorySetup {
        error: MemoryFault::InvalidPointerLayout {
            layout: AccessLayout::new(0, 1),
        },
    };

    assert_eq!(error.code(), "E3007");
    assert_eq!(
        error.message(),
        "interpreter-memory-setup-failed: memory fault: InvalidPointerLayout"
    );
    assert_eq!(error.span(), None);
}

/// Build `int counter; int bump(void) { return counter; } int main(void) { return bump(); }`.
///
/// Only `bump` touches the global, so a non-transitive scan of `main` alone
/// would wrongly conclude that this program needs no memory.
fn transitive_global_program(arena: &Arena) -> (CHirUnit<'_>, SymbolId, SymbolId, SymbolId) {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let function_ty = types.intern(CanonicalType {
        kind: TypeKind::Function {
            return_type: int,
            parameters: FunctionParameters::Prototype {
                types: Vec::new(),
                variadic: false,
            },
        },
        qualifiers: Qualifiers::default(),
    });
    let mut symbols = crate::SymbolTable::new();
    let counter = symbols
        .declare(
            symbols.file_scope(),
            "counter",
            crate::SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(0, 7),
        )
        .expect("declare counter");
    let bump = symbols
        .declare(
            symbols.file_scope(),
            "bump",
            crate::SymbolData::Function {
                storage: StorageClass::None,
            },
            function_ty,
            span(10, 14),
        )
        .expect("declare bump");
    let main = symbols
        .declare(
            symbols.file_scope(),
            "main",
            crate::SymbolData::Function {
                storage: StorageClass::None,
            },
            function_ty,
            span(30, 34),
        )
        .expect("declare main");
    symbols
        .create_scope(
            symbols.file_scope(),
            ScopeKind::Function,
            Some(span(10, 29)),
        )
        .expect("create bump function scope");
    symbols
        .create_scope(
            symbols.file_scope(),
            ScopeKind::Function,
            Some(span(30, 48)),
        )
        .expect("create main function scope");

    let object = alloc(
        arena,
        CHirDecl::Object(ObjectDecl {
            attributes: Vec::new(),
            cleanup: None,
            symbol: counter,
            name: SpannedName {
                spelling: "counter".to_owned(),
                span: span(0, 7),
            },
            ty: int,
            specifier: TypeSpecifier::Builtin {
                ty: int,
                span: span(0, 3),
            },
            written: WrittenDeclarator {
                layers: Vec::new(),
                span: span(0, 7),
            },
            storage: StorageClass::None,
            initializer: None,
            asm_label: None,
            asm_label_span: None,
            gnu_extension_spans: Vec::new(),
            span: span(0, 8),
        }),
    );

    let read_counter = alloc(
        arena,
        CHirExpr::Ident {
            symbol: counter,
            spelling: "counter".to_owned(),
            facts: ExprFacts::lvalue(int, true, true).expect("modifiable global lvalue"),
            span: span(20, 27),
        },
    );
    let bump_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![CHirBlockItem::Statement(alloc(
                arena,
                CHirStmt::Return(Some(read_counter), span(18, 28)),
            ))],
            span(16, 29),
            None,
        ),
    );
    let bump_decl = alloc(
        arena,
        CHirDecl::Function(test_function_decl(
            bump,
            "bump",
            int,
            function_ty,
            Some(bump_body),
            span(10, 29),
        )),
    );

    let callee = alloc(
        arena,
        CHirExpr::Ident {
            symbol: bump,
            spelling: "bump".to_owned(),
            facts: ExprFacts::function_designator(function_ty),
            span: span(40, 44),
        },
    );
    let call = alloc(arena, CHirExpr::Call(callee, Vec::new(), span(40, 46)));
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![CHirBlockItem::Statement(alloc(
                arena,
                CHirStmt::Return(Some(call), span(38, 47)),
            ))],
            span(36, 48),
            None,
        ),
    );
    let main_decl = alloc(
        arena,
        CHirDecl::Function(test_function_decl(
            main,
            "main",
            int,
            function_ty,
            Some(main_body),
            span(30, 48),
        )),
    );

    (
        CHirUnit::new(vec![object, bump_decl, main_decl], types, symbols),
        main,
        bump,
        counter,
    )
}

fn test_function_decl<'arena>(
    symbol: SymbolId,
    spelling: &str,
    result: TypeId,
    function_ty: TypeId,
    body: Option<&'arena CHirStmt<'arena>>,
    at: SourceSpan,
) -> FunctionDecl<'arena> {
    FunctionDecl {
        attributes: Vec::new(),
        symbol,
        name: SpannedName {
            spelling: spelling.to_owned(),
            span: at,
        },
        ty: function_ty,
        return_specifier: TypeSpecifier::Builtin {
            ty: result,
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
            parameters: Vec::new(),
            variadic: false,
        },
        storage: StorageClass::None,
        asm_label: None,
        asm_label_span: None,
        inline_spans: Vec::new(),
        gnu_extension_spans: Vec::new(),
        body,
        span: at,
    }
}

#[test]
fn reachable_memory_requirements_follow_transitive_calls() {
    let arena = Arena::new(64 * 1024);
    let (unit, main, _bump, counter) = transitive_global_program(&arena);

    let requirements = reachable_memory_requirements(
        &unit,
        main,
        &crate::linkage::SymbolLinkage::default(),
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
    );

    assert!(
        requirements.requires_memory,
        "a global read behind one call level still requires memory"
    );
    assert!(
        requirements.static_objects.contains(&counter),
        "the transitively referenced global must be scheduled for allocation"
    );
}

#[test]
fn unreachable_globals_are_not_scheduled_for_allocation() {
    let arena = Arena::new(64 * 1024);
    let (unit, _main, bump, counter) = transitive_global_program(&arena);

    // Entering at `bump` still reaches `counter`; entering nowhere else must not
    // invent objects, so the manifest stays exactly one object wide.
    let requirements = reachable_memory_requirements(
        &unit,
        bump,
        &crate::linkage::SymbolLinkage::default(),
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
    );

    assert!(requirements.requires_memory);
    assert_eq!(
        requirements
            .static_objects
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![counter]
    );
}

#[test]
fn deeply_nested_static_initializer_executes_without_recursive_stack_growth() {
    let arena = Arena::new(64 * 1024);
    let (unit, main) = deep_static_initializer_program(&arena, 512);

    let outcome = interpret_scalar_function_with_target(
        &unit,
        main,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        InterpreterLimits::default(),
    )
    .expect("a defined constant initializer is independent of host stack depth");

    assert_eq!(
        outcome
            .return_value
            .unwrap()
            .mathematical_value(unit.types())
            .unwrap(),
        1
    );
}

#[test]
fn static_initializer_iteration_retains_the_execution_budget() {
    let arena = Arena::new(64 * 1024);
    let (unit, main) = deep_static_initializer_program(&arena, 512);
    let error = interpret_scalar_function_with_target(
        &unit,
        main,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        InterpreterLimits::new(32),
    )
    .expect_err("constant initialization consumes the same explicit fuel budget");
    assert!(
        matches!(error, InterpreterError::FuelExhausted { .. }),
        "{error:?}"
    );
}

#[test]
fn odd_negations_in_static_initializers_preserve_the_value() {
    let arena = Arena::new(64 * 1024);
    let negations = 63;
    let (unit, main) = deep_static_initializer_program(&arena, negations);

    let outcome = interpret_scalar_function_with_target(
        &unit,
        main,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        InterpreterLimits::new(1_000_000),
    )
    .expect("a defined static initializer must be evaluated");

    let value = outcome
        .return_value
        .expect("main returns the initialized global")
        .mathematical_value(unit.types())
        .expect("the global is a plain int");
    // An even number of negations leaves the literal 1 unchanged.
    assert_eq!(value, if negations.is_multiple_of(2) { 1 } else { -1 });
}

#[test]
fn static_integer_machine_does_not_execute_calls_or_dead_operands() {
    let arena = Arena::default();
    let (unit, function, int) = scalar_program(&arena, false);
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let callee = alloc(
        &arena,
        CHirExpr::Ident {
            symbol: function,
            spelling: "main".to_owned(),
            facts: ExprFacts::function_designator(unit.symbols().symbol(function).unwrap().ty()),
            span: span(0, 4),
        },
    );
    let call = alloc(&arena, CHirExpr::Call(callee, Vec::new(), span(0, 6)));
    let zero = alloc(
        &arena,
        CHirExpr::Lit(
            IntLit {
                value: 0,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span(7, 8),
        ),
    );
    let comma = alloc(&arena, CHirExpr::Comma(call, zero, span(0, 8)));
    for expression in [call, comma] {
        assert!(matches!(
            interpret_static_integer_expression(&unit, expression, &target),
            Err(InterpreterError::UnsupportedExpression {
                feature: "nonconstant expression in static integer initializer",
                ..
            })
        ));
    }
    let selected = alloc(&arena, CHirExpr::Ternary(zero, call, zero, span(0, 8)));
    assert_eq!(
        interpret_static_integer_expression(&unit, selected, &target).unwrap(),
        CInteger::int(0)
    );
    let size = alloc(&arena, CHirExpr::SizeOfExpr(call, int, span(0, 8)));
    assert_eq!(
        interpret_static_integer_expression(&unit, size, &target).unwrap(),
        CInteger::new(4, CIntegerType::ULONG)
    );
}

/// Build `int deep = -(-(-...1...)); int main(void) { return deep; }` with
/// `depth` nested negations.
fn deep_static_initializer_program(arena: &Arena, depth: usize) -> (CHirUnit<'_>, SymbolId) {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let function_ty = types.intern(CanonicalType {
        kind: TypeKind::Function {
            return_type: int,
            parameters: FunctionParameters::Prototype {
                types: Vec::new(),
                variadic: false,
            },
        },
        qualifiers: Qualifiers::default(),
    });
    let mut symbols = crate::SymbolTable::new();
    let deep = symbols
        .declare(
            symbols.file_scope(),
            "deep",
            crate::SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(0, 4),
        )
        .expect("declare deep");
    let main = symbols
        .declare(
            symbols.file_scope(),
            "main",
            crate::SymbolData::Function {
                storage: StorageClass::None,
            },
            function_ty,
            span(30, 34),
        )
        .expect("declare main");
    symbols
        .create_scope(
            symbols.file_scope(),
            ScopeKind::Function,
            Some(span(30, 46)),
        )
        .expect("create main function scope");

    let mut value: &CHirExpr<'_> = alloc(
        arena,
        CHirExpr::Lit(
            IntLit {
                value: 1,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span(10, 11),
        ),
    );
    for _ in 0..depth {
        value = alloc(arena, CHirExpr::Unary(UnaryOp::Neg, value, span(10, 11)));
    }

    let plan = crate::InitPlan::new(
        int,
        vec![crate::InitAction::store_scalar(
            crate::SubobjectPath::root(),
            int,
            value,
            span(10, 11),
        )],
    );
    let object = alloc(
        arena,
        CHirDecl::Object(ObjectDecl {
            attributes: Vec::new(),
            cleanup: None,
            symbol: deep,
            name: SpannedName {
                spelling: "deep".to_owned(),
                span: span(0, 4),
            },
            ty: int,
            specifier: TypeSpecifier::Builtin {
                ty: int,
                span: span(0, 3),
            },
            written: WrittenDeclarator {
                layers: Vec::new(),
                span: span(0, 4),
            },
            storage: StorageClass::None,
            initializer: Some(crate::ObjectInitializer::new(
                crate::WrittenInitializer::Expression(value),
                plan,
            )),
            asm_label: None,
            asm_label_span: None,
            gnu_extension_spans: Vec::new(),
            span: span(0, 12),
        }),
    );

    let read_deep = alloc(
        arena,
        CHirExpr::Ident {
            symbol: deep,
            spelling: "deep".to_owned(),
            facts: ExprFacts::lvalue(int, true, true).expect("modifiable global lvalue"),
            span: span(40, 44),
        },
    );
    let main_body = alloc(
        arena,
        CHirStmt::Compound(
            vec![CHirBlockItem::Statement(alloc(
                arena,
                CHirStmt::Return(Some(read_deep), span(38, 45)),
            ))],
            span(36, 46),
            None,
        ),
    );
    let main_decl = alloc(
        arena,
        CHirDecl::Function(test_function_decl(
            main,
            "main",
            int,
            function_ty,
            Some(main_body),
            span(30, 46),
        )),
    );

    (CHirUnit::new(vec![object, main_decl], types, symbols), main)
}
