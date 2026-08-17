use evocc_chir::*;

use evocc_chir::interp::{
    interpret_scalar_function, InterpreterError, InterpreterLimits, ScalarValue,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_types::{
    CanonicalType, FunctionParameters, IntRank, Qualifiers, TypeId, TypeInterner, TypeKind,
};

fn span(start: u32, end: u32) -> SourceSpan {
    SourceSpan::new(FileId::new(97), start, end, 1, start + 1)
}

fn alloc<'arena, T: 'arena>(arena: &'arena Arena, value: T) -> &'arena T {
    // Safety: C-HIR test nodes contain no Arena handle and their destructors
    // only release owned source-fact buffers.
    unsafe { arena.alloc_shared(value) }
}

struct Fixture<'arena> {
    arena: &'arena Arena,
    types: TypeInterner,
    symbols: SymbolTable,
    int: TypeId,
    return_type: TypeId,
    function_ty: TypeId,
    function_symbol: SymbolId,
    function_scope: ScopeId,
}

impl<'arena> Fixture<'arena> {
    fn new(arena: &'arena Arena) -> Self {
        Self::with_void_return(arena, false)
    }

    fn new_void(arena: &'arena Arena) -> Self {
        Self::with_void_return(arena, true)
    }

    fn with_void_return(arena: &'arena Arena, returns_void: bool) -> Self {
        let mut types = TypeInterner::new();
        let int = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let return_type = if returns_void {
            types.intern(CanonicalType {
                kind: TypeKind::Void,
                qualifiers: Qualifiers::default(),
            })
        } else {
            int
        };
        let function_ty = types.intern(CanonicalType {
            kind: TypeKind::Function {
                return_type,
                parameters: FunctionParameters::Prototype {
                    types: Vec::new(),
                    variadic: false,
                },
            },
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = SymbolTable::new();
        let file_scope = symbols.file_scope();
        let function_symbol = symbols
            .declare(
                file_scope,
                "main",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                function_ty,
                span(0, 4),
            )
            .unwrap();
        let function_scope = symbols
            .create_scope(file_scope, ScopeKind::Function, Some(span(5, 40)))
            .unwrap();
        Self {
            arena,
            types,
            symbols,
            int,
            return_type,
            function_ty,
            function_symbol,
            function_scope,
        }
    }

    fn literal(&self, value: u64, at: SourceSpan) -> &'arena CHirExpr<'arena> {
        alloc(
            self.arena,
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

    fn unsigned_literal(&self, value: u64, at: SourceSpan) -> &'arena CHirExpr<'arena> {
        alloc(
            self.arena,
            CHirExpr::Lit(
                IntLit {
                    value,
                    suffix: IntSuffix::U,
                    base: IntBase::Dec,
                },
                at,
            ),
        )
    }

    fn ident(&self, symbol: SymbolId, at: SourceSpan) -> &'arena CHirExpr<'arena> {
        let declaration = self.symbols.symbol(symbol).unwrap();
        alloc(
            self.arena,
            CHirExpr::Ident {
                symbol,
                spelling: declaration.spelling().to_owned(),
                facts: ExprFacts::lvalue(declaration.ty(), true, true).unwrap(),
                span: at,
            },
        )
    }

    fn binary(
        &self,
        op: BinaryOp,
        left: &'arena CHirExpr<'arena>,
        right: &'arena CHirExpr<'arena>,
        at: SourceSpan,
    ) -> &'arena CHirExpr<'arena> {
        alloc(self.arena, CHirExpr::Binary(op, left, right, at))
    }

    fn assignment(
        &self,
        symbol: SymbolId,
        op: AssignmentOp,
        value: &'arena CHirExpr<'arena>,
        at: SourceSpan,
    ) -> &'arena CHirExpr<'arena> {
        let place = ScalarPlace::bind(&self.symbols, &self.types, symbol).unwrap();
        alloc(
            self.arena,
            CHirExpr::Assign {
                op,
                place,
                target: self.ident(symbol, at),
                value,
                facts: ExprFacts::value(place.ty()),
                span: at,
            },
        )
    }

    fn update(&self, symbol: SymbolId, op: UpdateOp, at: SourceSpan) -> &'arena CHirExpr<'arena> {
        let place = ScalarPlace::bind(&self.symbols, &self.types, symbol).unwrap();
        alloc(
            self.arena,
            CHirExpr::Update {
                op,
                place,
                target: self.ident(symbol, at),
                facts: ExprFacts::value(place.ty()),
                span: at,
            },
        )
    }

    fn object(
        &mut self,
        name: &str,
        initializer: Option<&'arena CHirExpr<'arena>>,
        at: SourceSpan,
    ) -> (SymbolId, &'arena CHirDecl<'arena>) {
        let symbol = self
            .symbols
            .declare(
                self.function_scope,
                name,
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                self.int,
                at,
            )
            .unwrap();
        let initializer = initializer.map(|value| {
            ObjectInitializer::new(
                WrittenInitializer::Expression(value),
                InitPlan::new(
                    self.int,
                    vec![InitAction::store_scalar(
                        SubobjectPath::root(),
                        self.int,
                        value,
                        value.span(),
                    )],
                ),
            )
        });
        let declaration = alloc(
            self.arena,
            CHirDecl::Object(ObjectDecl {
                symbol,
                name: SpannedName {
                    spelling: name.to_owned(),
                    span: at,
                },
                ty: self.int,
                specifier: TypeSpecifier::Builtin {
                    ty: self.int,
                    span: at,
                },
                written: WrittenDeclarator {
                    layers: Vec::new(),
                    span: at,
                },
                storage: StorageClass::None,
                initializer,
                span: at,
            }),
        );
        (symbol, declaration)
    }

    fn statement(&self, statement: CHirStmt<'arena>) -> &'arena CHirStmt<'arena> {
        alloc(self.arena, statement)
    }

    fn finish(self, body: &'arena CHirStmt<'arena>) -> (CHirUnit<'arena>, SymbolId) {
        let function = alloc(
            self.arena,
            CHirDecl::Function(FunctionDecl {
                symbol: self.function_symbol,
                name: SpannedName {
                    spelling: "main".to_owned(),
                    span: span(0, 4),
                },
                ty: self.function_ty,
                return_specifier: TypeSpecifier::Builtin {
                    ty: self.return_type,
                    span: span(0, 3),
                },
                written: WrittenDeclarator {
                    layers: Vec::new(),
                    span: span(0, 10),
                },
                parameters: ParameterSyntax::Prototype {
                    parameters: Vec::new(),
                    variadic: false,
                },
                storage: StorageClass::None,
                body: Some(body),
                span: span(0, 40),
            }),
        );
        (
            CHirUnit::new(vec![function], self.types, self.symbols),
            self.function_symbol,
        )
    }
}

#[test]
fn scalar_plan_and_root_compound_assignment_execute_once() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let initial = fixture.literal(7, span(12, 13));
    let (value, declaration) = fixture.object("value", Some(initial), span(6, 14));
    let place = ScalarPlace::bind(&fixture.symbols, &fixture.types, value).unwrap();
    let target = fixture.ident(value, span(16, 21));
    let five = fixture.literal(5, span(25, 26));
    let add = alloc(
        &arena,
        CHirExpr::Assign {
            op: AssignmentOp::Add,
            place,
            target,
            value: five,
            facts: ExprFacts::value(fixture.int),
            span: span(16, 26),
        },
    );
    let mutate = fixture.statement(CHirStmt::Expr(add, span(16, 27)));
    let returned = fixture.ident(value, span(35, 40));
    let return_statement = fixture.statement(CHirStmt::Return(Some(returned), span(28, 41)));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Declaration(declaration),
            CHirBlockItem::Statement(mutate),
            CHirBlockItem::Statement(return_statement),
        ],
        span(5, 42),
    ));
    let int = fixture.int;
    let (unit, function) = fixture.finish(body);

    let first = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    let second = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    assert_eq!(
        first.return_value,
        Some(ScalarValue {
            ty: int,
            raw_bits: 12,
        })
    );
    assert!(first.steps > 0);
    assert_eq!(first.steps, second.steps);
    assert_eq!(
        first
            .return_value
            .unwrap()
            .mathematical_value(unit.types())
            .unwrap(),
        12
    );
}

#[test]
fn uninitialized_reads_fail_at_the_identifier() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let (value, declaration) = fixture.object("empty", None, span(6, 16));
    let read_span = span(24, 29);
    let read = fixture.ident(value, read_span);
    let return_statement = fixture.statement(CHirStmt::Return(Some(read), span(17, 30)));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Declaration(declaration),
            CHirBlockItem::Statement(return_statement),
        ],
        span(5, 31),
    ));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::default()),
        Err(InterpreterError::ReadOfUninitializedObject {
            symbol: value,
            spelling: "empty".to_owned(),
            span: read_span,
        })
    );
}

#[test]
fn integer_failures_keep_the_full_expression_span() {
    let arena = Arena::default();
    let fixture = Fixture::new(&arena);
    let one = fixture.literal(1, span(12, 13));
    let zero = fixture.literal(0, span(16, 17));
    let expression_span = span(12, 17);
    let division = alloc(
        &arena,
        CHirExpr::Binary(BinaryOp::Div, one, zero, expression_span),
    );
    let body = fixture.statement(CHirStmt::Return(Some(division), span(5, 18)));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::default()),
        Err(InterpreterError::Integer {
            error: integer::IntegerError::DivisionByZero,
            span: expression_span,
        })
    );
}

#[test]
fn loop_fuel_exhaustion_is_anchored_to_the_loop() {
    let arena = Arena::default();
    let fixture = Fixture::new(&arena);
    let loop_span = span(5, 19);
    let empty = fixture.statement(CHirStmt::Compound(Vec::new(), span(14, 17)));
    let body = fixture.statement(CHirStmt::For(
        CHirForInit::Empty,
        None,
        None,
        empty,
        loop_span,
    ));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::new(8)),
        Err(InterpreterError::FuelExhausted {
            limit: 8,
            consumed: 8,
            span: loop_span,
        })
    );
}

#[test]
fn structured_control_uses_private_program_counters_and_switch_fallthrough() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let initial = fixture.literal(1, span(10, 11));
    let (value, value_declaration) = fixture.object("value", Some(initial), span(6, 12));

    let goto_selected = fixture.statement(CHirStmt::Goto(
        SpannedName {
            spelling: "selected".to_owned(),
            span: span(14, 22),
        },
        span(9, 23),
    ));
    let poison = fixture.assignment(
        value,
        AssignmentOp::Simple,
        fixture.literal(99, span(30, 32)),
        span(24, 32),
    );
    let poison = fixture.statement(CHirStmt::Expr(poison, span(24, 33)));
    let label = fixture.statement(CHirStmt::Label(
        SpannedName {
            spelling: "selected".to_owned(),
            span: span(34, 42),
        },
        fixture.statement(CHirStmt::Null(span(42, 43))),
        span(34, 43),
    ));

    let while_condition = fixture.binary(
        BinaryOp::Lt,
        fixture.ident(value, span(45, 50)),
        fixture.literal(3, span(53, 54)),
        span(45, 54),
    );
    let while_increment = fixture.assignment(
        value,
        AssignmentOp::Add,
        fixture.literal(1, span(59, 60)),
        span(55, 60),
    );
    let while_body = fixture.statement(CHirStmt::Expr(while_increment, span(55, 61)));
    let while_statement =
        fixture.statement(CHirStmt::While(while_condition, while_body, span(44, 62)));

    let do_increment = fixture.assignment(
        value,
        AssignmentOp::Add,
        fixture.literal(1, span(69, 70)),
        span(64, 70),
    );
    let do_body = fixture.statement(CHirStmt::Expr(do_increment, span(64, 71)));
    let do_statement = fixture.statement(CHirStmt::DoWhile(
        do_body,
        fixture.literal(0, span(79, 80)),
        span(63, 81),
    ));

    let for_initial = fixture.literal(0, span(88, 89));
    let (index, index_declaration) = fixture.object("index", Some(for_initial), span(82, 90));
    let for_condition = fixture.binary(
        BinaryOp::Lt,
        fixture.ident(index, span(92, 97)),
        fixture.literal(2, span(100, 101)),
        span(92, 101),
    );
    let for_step = fixture.update(index, UpdateOp::PostIncrement, span(103, 110));
    let for_increment = fixture.assignment(
        value,
        AssignmentOp::Add,
        fixture.literal(1, span(116, 117)),
        span(111, 117),
    );
    let for_body = fixture.statement(CHirStmt::Expr(for_increment, span(111, 118)));
    let for_statement = fixture.statement(CHirStmt::For(
        CHirForInit::Declaration(vec![index_declaration]),
        Some(for_condition),
        Some(for_step),
        for_body,
        span(82, 119),
    ));

    let case_one_value = fixture.assignment(
        value,
        AssignmentOp::Simple,
        fixture.literal(99, span(132, 134)),
        span(125, 134),
    );
    let case_one = fixture.statement(CHirStmt::Case(
        fixture.literal(1, span(122, 123)),
        fixture.statement(CHirStmt::Expr(case_one_value, span(125, 135))),
        span(120, 135),
    ));
    let case_two_value = fixture.assignment(
        value,
        AssignmentOp::Add,
        fixture.literal(5, span(146, 147)),
        span(140, 147),
    );
    let case_two = fixture.statement(CHirStmt::Case(
        fixture.literal(2, span(137, 138)),
        fixture.statement(CHirStmt::Expr(case_two_value, span(140, 148))),
        span(136, 148),
    ));
    let case_three_value = fixture.assignment(
        value,
        AssignmentOp::Add,
        fixture.literal(7, span(159, 160)),
        span(153, 160),
    );
    let case_three_body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Statement(
                fixture.statement(CHirStmt::Expr(case_three_value, span(153, 161))),
            ),
            CHirBlockItem::Statement(fixture.statement(CHirStmt::Break(span(162, 168)))),
        ],
        span(152, 169),
    ));
    let case_three = fixture.statement(CHirStmt::Case(
        fixture.literal(3, span(150, 151)),
        case_three_body,
        span(149, 169),
    ));
    let default_value = fixture.assignment(
        value,
        AssignmentOp::Simple,
        fixture.literal(99, span(180, 182)),
        span(173, 182),
    );
    let default = fixture.statement(CHirStmt::Default(
        fixture.statement(CHirStmt::Expr(default_value, span(173, 183))),
        span(170, 183),
    ));
    let switch_body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Statement(case_one),
            CHirBlockItem::Statement(case_two),
            CHirBlockItem::Statement(case_three),
            CHirBlockItem::Statement(default),
        ],
        span(119, 184),
    ));
    let switch_statement = fixture.statement(CHirStmt::Switch(
        fixture.literal(2, span(187, 188)),
        switch_body,
        span(185, 189),
    ));
    let if_condition = fixture.binary(
        BinaryOp::Eq,
        fixture.ident(value, span(191, 196)),
        fixture.literal(6, span(200, 201)),
        span(191, 201),
    );
    let if_statement = fixture.statement(CHirStmt::If(
        if_condition,
        switch_statement,
        None,
        span(190, 202),
    ));
    let returned = fixture.ident(value, span(210, 215));
    let return_statement = fixture.statement(CHirStmt::Return(Some(returned), span(203, 216)));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Declaration(value_declaration),
            CHirBlockItem::Statement(goto_selected),
            CHirBlockItem::Statement(poison),
            CHirBlockItem::Statement(label),
            CHirBlockItem::Statement(while_statement),
            CHirBlockItem::Statement(do_statement),
            CHirBlockItem::Statement(for_statement),
            CHirBlockItem::Statement(if_statement),
            CHirBlockItem::Statement(return_statement),
        ],
        span(5, 217),
    ));
    let (unit, function) = fixture.finish(body);

    let outcome = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    assert_eq!(
        outcome
            .return_value
            .unwrap()
            .mathematical_value(unit.types())
            .unwrap(),
        18
    );
    assert!(outcome.steps > 0);
}

#[test]
fn conditional_converts_the_selected_arm_to_the_common_integer_type() {
    let arena = Arena::default();
    let fixture = Fixture::new(&arena);
    let negative_one = alloc(
        &arena,
        CHirExpr::Unary(UnaryOp::Neg, fixture.literal(1, span(12, 13)), span(11, 13)),
    );
    let conditional = alloc(
        &arena,
        CHirExpr::Ternary(
            fixture.literal(1, span(6, 7)),
            negative_one,
            fixture.unsigned_literal(1, span(16, 18)),
            span(6, 18),
        ),
    );
    let comparison = fixture.binary(
        BinaryOp::Lt,
        conditional,
        fixture.literal(0, span(22, 23)),
        span(6, 23),
    );
    let body = fixture.statement(CHirStmt::Return(Some(comparison), span(5, 24)));
    let (unit, function) = fixture.finish(body);

    let outcome = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    assert_eq!(outcome.return_value.unwrap().raw_bits, 0);
}

#[test]
fn sizeof_expression_is_unevaluated_and_uses_the_operand_type() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let (empty, declaration) = fixture.object("empty", None, span(6, 16));
    let sizeof_span = span(24, 36);
    let sizeof = alloc(
        &arena,
        CHirExpr::SizeOfExpr(fixture.ident(empty, span(31, 36)), sizeof_span),
    );
    let returned = fixture.statement(CHirStmt::Return(Some(sizeof), span(17, 37)));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Declaration(declaration),
            CHirBlockItem::Statement(returned),
        ],
        span(5, 38),
    ));
    let (unit, function) = fixture.finish(body);

    let outcome = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    assert_eq!(outcome.return_value.unwrap().raw_bits, 4);
}

#[test]
fn main_fallthrough_returns_zero_and_void_return_has_no_scalar_value() {
    let arena = Arena::default();
    let fixture = Fixture::new(&arena);
    let body = fixture.statement(CHirStmt::Compound(Vec::new(), span(5, 7)));
    let (unit, function) = fixture.finish(body);
    let outcome = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    assert_eq!(outcome.return_value.unwrap().raw_bits, 0);

    let void_arena = Arena::default();
    let void_fixture = Fixture::new_void(&void_arena);
    let body = void_fixture.statement(CHirStmt::Return(None, span(5, 12)));
    let (unit, function) = void_fixture.finish(body);
    let outcome = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    assert_eq!(outcome.return_value, None);
}

#[test]
fn goto_past_initializer_leaves_the_automatic_object_uninitialized() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let goto = fixture.statement(CHirStmt::Goto(
        SpannedName {
            spelling: "done".to_owned(),
            span: span(11, 15),
        },
        span(6, 16),
    ));
    let initial = fixture.literal(1, span(25, 26));
    let (value, declaration) = fixture.object("value", Some(initial), span(17, 27));
    let label = fixture.statement(CHirStmt::Label(
        SpannedName {
            spelling: "done".to_owned(),
            span: span(28, 32),
        },
        fixture.statement(CHirStmt::Null(span(32, 33))),
        span(28, 33),
    ));
    let read_span = span(41, 46);
    let returned = fixture.statement(CHirStmt::Return(
        Some(fixture.ident(value, read_span)),
        span(34, 47),
    ));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Statement(goto),
            CHirBlockItem::Declaration(declaration),
            CHirBlockItem::Statement(label),
            CHirBlockItem::Statement(returned),
        ],
        span(5, 48),
    ));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::default()),
        Err(InterpreterError::ReadOfUninitializedObject {
            symbol: value,
            spelling: "value".to_owned(),
            span: read_span,
        })
    );
}

#[test]
fn parenthesized_unary_overflow_is_anchored_to_operator_and_operand() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let maximum = fixture.literal(2_147_483_647, span(10, 20));
    let negative_maximum = alloc(&arena, CHirExpr::Unary(UnaryOp::Neg, maximum, span(9, 20)));
    let minimum = fixture.binary(
        BinaryOp::Sub,
        negative_maximum,
        fixture.literal(1, span(23, 24)),
        span(9, 24),
    );
    let (value, declaration) = fixture.object("value", Some(minimum), span(6, 25));
    let operand = fixture.ident(value, span(40, 45));
    let inner = alloc(&arena, CHirExpr::Unary(UnaryOp::Neg, operand, span(38, 46)));
    let outer = alloc(&arena, CHirExpr::Unary(UnaryOp::Plus, inner, span(37, 46)));
    let returned = fixture.statement(CHirStmt::Return(Some(outer), span(30, 47)));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Declaration(declaration),
            CHirBlockItem::Statement(returned),
        ],
        span(5, 48),
    ));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::default()),
        Err(InterpreterError::Integer {
            error: integer::IntegerError::SignedOverflow,
            span: span(39, 45),
        })
    );
}

#[test]
fn forged_object_identity_fails_before_execution() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let (symbol, declaration) = fixture.object("actual", None, span(6, 17));
    let CHirDecl::Object(mut forged) = (*declaration).clone() else {
        unreachable!()
    };
    forged.name.spelling = "forged".to_owned();
    let declaration = alloc(&arena, CHirDecl::Object(forged));
    let body = fixture.statement(CHirStmt::Compound(
        vec![CHirBlockItem::Declaration(declaration)],
        span(5, 18),
    ));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::default()),
        Err(InterpreterError::MalformedObjectDeclaration {
            symbol,
            span: span(6, 17),
        })
    );
}

#[test]
fn forged_scalar_place_spelling_fails_closed() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let (symbol, declaration) = fixture.object(
        "actual",
        Some(fixture.literal(1, span(13, 14))),
        span(6, 15),
    );
    let place = ScalarPlace::bind(&fixture.symbols, &fixture.types, symbol).unwrap();
    let target_span = span(17, 23);
    let target = alloc(
        &arena,
        CHirExpr::Ident {
            symbol,
            spelling: "forged".to_owned(),
            facts: ExprFacts::lvalue(fixture.int, true, true).unwrap(),
            span: target_span,
        },
    );
    let assignment = alloc(
        &arena,
        CHirExpr::Assign {
            op: AssignmentOp::Simple,
            place,
            target,
            value: fixture.literal(2, span(26, 27)),
            facts: ExprFacts::value(fixture.int),
            span: span(17, 27),
        },
    );
    let mutation = fixture.statement(CHirStmt::Expr(assignment, span(17, 28)));
    let returned = fixture.statement(CHirStmt::Return(
        Some(fixture.ident(symbol, span(36, 42))),
        span(29, 43),
    ));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Declaration(declaration),
            CHirBlockItem::Statement(mutation),
            CHirBlockItem::Statement(returned),
        ],
        span(5, 44),
    ));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::default()),
        Err(InterpreterError::InvalidScalarPlace {
            symbol,
            span: target_span,
        })
    );
}

#[test]
fn return_update_is_a_full_expression_root() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let (value, declaration) =
        fixture.object("value", Some(fixture.literal(1, span(13, 14))), span(6, 15));
    let update = fixture.update(value, UpdateOp::PreIncrement, span(23, 30));
    let returned = fixture.statement(CHirStmt::Return(Some(update), span(16, 31)));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Declaration(declaration),
            CHirBlockItem::Statement(returned),
        ],
        span(5, 32),
    ));
    let (unit, function) = fixture.finish(body);

    let outcome = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    assert_eq!(outcome.return_value.unwrap().raw_bits, 2);
}

#[test]
fn if_assignment_is_a_full_expression_root() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let (value, declaration) =
        fixture.object("value", Some(fixture.literal(0, span(13, 14))), span(6, 15));
    let condition = fixture.assignment(
        value,
        AssignmentOp::Simple,
        fixture.literal(1, span(24, 25)),
        span(17, 25),
    );
    let then_return = fixture.statement(CHirStmt::Return(
        Some(fixture.ident(value, span(34, 39))),
        span(27, 40),
    ));
    let else_return = fixture.statement(CHirStmt::Return(
        Some(fixture.literal(99, span(48, 50))),
        span(41, 51),
    ));
    let conditional = fixture.statement(CHirStmt::If(
        condition,
        then_return,
        Some(else_return),
        span(16, 51),
    ));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Declaration(declaration),
            CHirBlockItem::Statement(conditional),
        ],
        span(5, 52),
    ));
    let (unit, function) = fixture.finish(body);

    let outcome = interpret_scalar_function(&unit, function, InterpreterLimits::default()).unwrap();
    assert_eq!(outcome.return_value.unwrap().raw_bits, 1);
}

#[test]
fn malformed_initializer_plan_in_a_dead_branch_fails_prevalidation() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let initial = fixture.literal(7, span(24, 25));
    let (symbol, declaration) = fixture.object("hidden", Some(initial), span(17, 26));
    let CHirDecl::Object(mut malformed) = (*declaration).clone() else {
        unreachable!()
    };
    malformed.initializer = Some(ObjectInitializer::new(
        WrittenInitializer::Expression(initial),
        InitPlan::new(fixture.int, Vec::new()),
    ));
    let malformed = alloc(&arena, CHirDecl::Object(malformed));
    let dead_body = fixture.statement(CHirStmt::Compound(
        vec![CHirBlockItem::Declaration(malformed)],
        span(16, 27),
    ));
    let dead_branch = fixture.statement(CHirStmt::If(
        fixture.literal(0, span(9, 10)),
        dead_body,
        None,
        span(6, 27),
    ));
    let returned = fixture.statement(CHirStmt::Return(
        Some(fixture.literal(0, span(35, 36))),
        span(28, 37),
    ));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Statement(dead_branch),
            CHirBlockItem::Statement(returned),
        ],
        span(5, 38),
    ));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::default()),
        Err(InterpreterError::InvalidInitializerPlan {
            symbol,
            span: span(24, 25),
        })
    );
}

#[test]
fn inner_compound_slot_is_inactive_after_scope_exit() {
    let arena = Arena::default();
    let mut fixture = Fixture::new(&arena);
    let (hidden, declaration) = fixture.object(
        "hidden",
        Some(fixture.literal(7, span(18, 19))),
        span(6, 20),
    );
    let inner = fixture.statement(CHirStmt::Compound(
        vec![CHirBlockItem::Declaration(declaration)],
        span(5, 21),
    ));
    let read_span = span(29, 35);
    let returned = fixture.statement(CHirStmt::Return(
        Some(fixture.ident(hidden, read_span)),
        span(22, 36),
    ));
    let body = fixture.statement(CHirStmt::Compound(
        vec![
            CHirBlockItem::Statement(inner),
            CHirBlockItem::Statement(returned),
        ],
        span(4, 37),
    ));
    let (unit, function) = fixture.finish(body);

    assert_eq!(
        interpret_scalar_function(&unit, function, InterpreterLimits::default()),
        Err(InterpreterError::OutOfScopeObject {
            symbol: hidden,
            spelling: "hidden".to_owned(),
            span: read_span,
        })
    );
}
