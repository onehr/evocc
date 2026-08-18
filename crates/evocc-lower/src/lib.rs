//! C-HIR to EvoIR semantic lowering

use evocc_chir::{BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirStmt, CHirUnit};
use evocc_evoir::{Block, BlockArg, BlockId, CmpOp, Function, Inst, Value};
use evocc_types::EvoIRType;

pub fn lower_unit(unit: &CHirUnit<'_>) -> Vec<Function> {
    unit.iter()
        .filter_map(|decl| match decl {
            CHirDecl::Function(function) => function
                .body
                .map(|body| lower_function(&function.name.spelling, body)),
            _ => None,
        })
        .collect()
}

struct LowerContext {
    next_value: u32,
    next_block: u32,
    blocks: Vec<Block>,
    current_block_id: BlockId,
    current_insts: Vec<Inst>,
}

impl LowerContext {
    fn new() -> Self {
        let entry = Block {
            id: 0,
            args: vec![],
            insts: vec![],
            terminator: Inst::Ret(vec![]),
        };
        Self {
            next_value: 0,
            next_block: 1,
            blocks: vec![entry],
            current_block_id: 0,
            current_insts: vec![],
        }
    }

    fn finish(self) -> Function {
        Function {
            name: "lowered".into(),
            params: vec![],
            ret_tys: vec![],
            blocks: self.blocks,
        }
    }

    fn new_value(&mut self) -> u32 {
        let v = self.next_value;
        self.next_value += 1;
        v
    }

    fn emit_inst(&mut self, inst: Inst) {
        self.current_insts.push(inst);
    }

    fn set_terminator(&mut self, term: Inst) {
        let block = &mut self.blocks[self.current_block_id as usize];
        block.insts = std::mem::take(&mut self.current_insts);
        block.terminator = term;
    }

    fn new_block(&mut self) -> BlockId {
        let id = self.next_block;
        self.next_block += 1;
        self.blocks.push(Block {
            id,
            args: vec![],
            insts: vec![],
            terminator: Inst::Ret(vec![]),
        });
        id
    }
}

fn lower_function(name: &str, body: &CHirStmt<'_>) -> Function {
    let mut ctx = LowerContext::new();
    let result = lower_stmt(body, &mut ctx);
    ctx.set_terminator(Inst::Ret(vec![value(result)]));
    let mut func = ctx.finish();
    func.name = name.to_string();
    func.ret_tys = vec![bits32()];
    func
}

fn lower_stmt(stmt: &CHirStmt<'_>, ctx: &mut LowerContext) -> u32 {
    match stmt {
        CHirStmt::Return(Some(expr), _) => lower_expr(expr, ctx),
        CHirStmt::Compound(stmts, _) => {
            let mut last = ctx.new_value();
            ctx.emit_inst(Inst::Const(value(last), 0));
            for item in stmts {
                if let CHirBlockItem::Statement(statement) = item {
                    last = lower_stmt(statement, ctx);
                }
            }
            last
        }
        CHirStmt::Expr(expr, _) => lower_expr(expr, ctx),
        _ => {
            let v = ctx.new_value();
            ctx.emit_inst(Inst::Const(value(v), 0));
            v
        }
    }
}

fn bits32() -> EvoIRType {
    EvoIRType::Bits(32)
}

fn value(id: u32) -> Value {
    Value { id, ty: bits32() }
}

fn lower_expr(expr: &CHirExpr<'_>, ctx: &mut LowerContext) -> u32 {
    match expr {
        CHirExpr::Lit(lit, _) => {
            let v = ctx.new_value();
            ctx.emit_inst(Inst::Const(value(v), lit.value));
            v
        }
        CHirExpr::Binary(op, lhs, rhs, _) => {
            let l = lower_expr(lhs, ctx);
            let r = lower_expr(rhs, ctx);
            let v = ctx.new_value();
            let inst = match op {
                BinaryOp::Add => Inst::Add(value(v), value(l), value(r)),
                BinaryOp::Sub => Inst::Sub(value(v), value(l), value(r)),
                BinaryOp::Mul => Inst::Mul(value(v), value(l), value(r)),
                BinaryOp::Div => Inst::SDiv(value(v), value(l), value(r)),
                BinaryOp::Lt => Inst::ICmp(value(v), CmpOp::SLt, value(l), value(r)),
                BinaryOp::Eq => Inst::ICmp(value(v), CmpOp::Eq, value(l), value(r)),
                _ => Inst::Const(value(v), 0),
            };
            ctx.emit_inst(inst);
            v
        }
        CHirExpr::Ternary(cond, t, f, _) => {
            let cv = lower_expr(cond, ctx);
            let then_block = ctx.new_block();
            let else_block = ctx.new_block();
            let merge_block = ctx.new_block();
            ctx.set_terminator(Inst::CondBr(
                value(cv),
                then_block,
                vec![],
                else_block,
                vec![],
            ));
            ctx.current_block_id = then_block;
            let tv = lower_expr(t, ctx);
            ctx.set_terminator(Inst::Br(merge_block, vec![value(tv)]));
            ctx.current_block_id = else_block;
            let fv = lower_expr(f, ctx);
            ctx.set_terminator(Inst::Br(merge_block, vec![value(fv)]));
            ctx.current_block_id = merge_block;
            let merge_value = ctx.new_value();
            let merge = &mut ctx.blocks[merge_block as usize];
            merge.args.push(BlockArg {
                name: "ternary".into(),
                value: value(merge_value),
            });
            let result = ctx.new_value();
            ctx.emit_inst(Inst::Const(value(result), 0));
            result
        }
        CHirExpr::Unary(_op, inner, _) => {
            let v = lower_expr(inner, ctx);
            let zero = ctx.new_value();
            ctx.emit_inst(Inst::Const(value(zero), 0));
            let r = ctx.new_value();
            ctx.emit_inst(Inst::Sub(value(r), value(zero), value(v)));
            r
        }
        CHirExpr::Ident { .. } => {
            let v = ctx.new_value();
            ctx.emit_inst(Inst::Const(value(v), 0));
            v
        }
        _ => {
            let v = ctx.new_value();
            ctx.emit_inst(Inst::Const(value(v), 0));
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_chir::eval_expr;
    use evocc_chir::{
        BinaryOp, CHirDecl, CHirExpr, CHirStmt, CHirUnit, FunctionDecl, IntBase, IntLit, IntSuffix,
        ParameterSyntax, SpannedName, StorageClass, SymbolData, SymbolTable, TypeSpecifier,
        WrittenDeclarator, WrittenDeclaratorLayer,
    };
    use evocc_common::{Arena, FileId, SourceSpan};
    use evocc_types::{
        CanonicalType, FunctionParameters, IntRank, Qualifiers, TypeInterner, TypeKind,
    };

    fn int_lit(v: i64) -> CHirExpr<'static> {
        CHirExpr::Lit(
            IntLit {
                value: v as u64,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            test_span(),
        )
    }

    fn test_span() -> SourceSpan {
        SourceSpan::new(FileId::new(0), 0, 0, 0, 0)
    }

    fn arena_alloc<'arena, T: 'arena>(arena: &'arena Arena, value: T) -> &'arena T {
        // Safety: this test allocates only C-HIR nodes. They contain no Arena
        // handle and their transitive destructors only release owned buffers.
        unsafe { arena.alloc_shared(value) }
    }

    #[test]
    fn chir_to_evoir_lowering_preserves_semantics() {
        let arena = Arena::default();
        let one = arena_alloc(&arena, int_lit(1));
        let two = arena_alloc(&arena, int_lit(2));
        let three = arena_alloc(&arena, int_lit(3));
        let product = arena_alloc(
            &arena,
            CHirExpr::Binary(BinaryOp::Mul, two, three, test_span()),
        );
        let expression = arena_alloc(
            &arena,
            CHirExpr::Binary(BinaryOp::Add, one, product, test_span()),
        );
        let body = arena_alloc(&arena, CHirStmt::Return(Some(expression), test_span()));

        let mut types = TypeInterner::new();
        let int_type = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let function_type = types.intern(CanonicalType {
            kind: TypeKind::Function {
                return_type: int_type,
                parameters: FunctionParameters::Prototype {
                    types: Vec::new(),
                    variadic: false,
                },
            },
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = SymbolTable::new();
        let function_symbol = symbols
            .declare(
                symbols.file_scope(),
                "f",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                function_type,
                test_span(),
            )
            .unwrap();
        let func_decl = arena_alloc(
            &arena,
            CHirDecl::Function(FunctionDecl {
                symbol: function_symbol,
                name: SpannedName {
                    spelling: "f".into(),
                    span: test_span(),
                },
                ty: function_type,
                return_specifier: TypeSpecifier::Builtin {
                    ty: int_type,
                    span: test_span(),
                },
                written: WrittenDeclarator {
                    layers: vec![WrittenDeclaratorLayer::Function { span: test_span() }],
                    span: test_span(),
                },
                parameters: ParameterSyntax::Prototype {
                    parameters: Vec::new(),
                    variadic: false,
                },
                storage: StorageClass::None,
                inline_spans: Vec::new(),
                gnu_extension_spans: Vec::new(),
                body: Some(body),
                span: test_span(),
            }),
        );
        let unit = CHirUnit::new(vec![func_decl], types, symbols);

        let functions = lower_unit(&unit);
        assert_eq!(functions.len(), 1);
        let evoir_func = &functions[0];

        assert!(evocc_evoir::validate_ssa(evoir_func).is_ok());

        let outcome = evocc_evoir::eval_function(
            evoir_func,
            &[],
            evocc_evoir::InterpreterLimits::new(100_000),
        )
        .expect("lowered EvoIR executes");
        let results = outcome.return_values;
        assert_eq!(results, vec![evocc_evoir::RuntimeValue::bits(32, 7)]);

        let mut vars = std::collections::HashMap::new();
        let chir_result = eval_expr(expression, &mut vars);
        assert_eq!(chir_result, 7);
        assert_eq!(results[0].raw_bits, chir_result as u64);
    }
}
