//! Source warnings use resolved symbols, not names or optimizer liveness.
use std::collections::{HashMap, HashSet};

use evocc_common::SourceSpan;
use evocc_types::{TargetDataLayout, TypeKind};

use crate::{semantic::ScopeKind, *};

pub const NAMES: [&str; 5] = [
    "unused-variable",
    "unused-parameter",
    "implicit-function-declaration",
    "implicit-int",
    "return-type",
];

#[derive(Clone, Copy, Debug)]
pub struct Finding {
    pub lint: usize,
    pub span: SourceSpan,
}

impl Finding {
    pub fn message(self) -> &'static str {
        [
            "unused variable",
            "unused parameter",
            "function has no declaration",
            "declaration has no type specifier",
            "function does not return the required value",
        ][self.lint]
    }

    pub fn help(self) -> &'static str {
        [
            "remove the variable or use it",
            "use the parameter or mark it __attribute__((unused))",
            "declare the function before its first call",
            "write an explicit type specifier",
            "return a value on each path or change the result type to void",
        ][self.lint]
    }
}

pub fn check(
    unit: &CHirUnit<'_>,
    used: &HashSet<SymbolId>,
    target: &TargetDataLayout,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut nonreturning = HashSet::new();
    inline::for_each_declaration(unit, |declaration| {
        if let CHirDecl::Function(f) = declaration {
            if f.attributes
                .iter()
                .any(|a| a.semantic_name() == Some("noreturn"))
            {
                nonreturning.insert(f.symbol);
            }
        }
    });
    let unused = |symbol, attributes: &[Attribute]| {
        !used.contains(&symbol)
            && !attributes
                .iter()
                .any(|a| matches!(a.semantic_name(), Some("unused" | "used")))
    };
    inline::for_each_declaration(unit, |declaration| match declaration {
        CHirDecl::Object(o)
            if unused(o.symbol, &o.attributes)
                && o.cleanup.is_none()
                && o.storage != StorageClass::Extern
                && unit
                    .symbols()
                    .symbol(o.symbol)
                    .ok()
                    .and_then(|s| unit.symbols().scope(s.scope()).ok())
                    .is_some_and(|s| {
                        s.kind() != ScopeKind::File || o.storage == StorageClass::Static
                    }) =>
        {
            findings.push(Finding {
                lint: 0,
                span: o.name.span,
            });
        }
        CHirDecl::Function(f) if f.body.is_some() => {
            for parameter in f.parameters.parameters() {
                if parameter
                    .symbol
                    .is_some_and(|s| unused(s, &parameter.attributes))
                {
                    findings.push(Finding {
                        lint: 1,
                        span: parameter.name.as_ref().map_or(parameter.span, |n| n.span),
                    });
                }
            }
            if f.name.spelling != "main"
                && matches!(unit.types().get(f.ty).map(|t| &t.kind), Some(TypeKind::Function { return_type, .. })
                    if !matches!(unit.types().get(*return_type).map(|t| &t.kind), Some(TypeKind::Void)))
                && falls_through(f.body.unwrap(), unit, target, &nonreturning)
            {
                findings.push(Finding {
                    lint: 4,
                    span: f.name.span,
                });
            }
        }
        _ => {}
    });
    findings
}

/// Build only control edges. Return values and executed expressions are unchanged.
/// A reachable exit reports a warning, not a proof of undefined execution.
fn falls_through(
    body: &CHirStmt<'_>,
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    nonreturning: &HashSet<SymbolId>,
) -> bool {
    struct Flow<'u, 'a> {
        unit: &'u CHirUnit<'a>,
        target: &'u TargetDataLayout,
        nonreturning: &'u HashSet<SymbolId>,
        edges: Vec<Vec<usize>>,
        labels: HashMap<String, usize>,
        jumps: Vec<(usize, String)>,
        indirect_jumps: Vec<usize>,
    }
    impl Flow<'_, '_> {
        fn node(&mut self, successors: Vec<usize>) -> usize {
            let id = self.edges.len();
            self.edges.push(successors);
            id
        }
        fn branch(&mut self, id: usize, condition: Option<&CHirExpr<'_>>, yes: usize, no: usize) {
            let known = condition.map_or(Some(true), |e| {
                constant::eval_executable_integer_constant(
                    e,
                    self.unit.types(),
                    self.unit.symbols(),
                    self.target,
                )
                .map(|v| v.truth())
            });
            self.edges[id] = match known {
                Some(true) => vec![yes],
                Some(false) => vec![no],
                None => vec![yes, no],
            };
        }
        fn stmt(
            &mut self,
            s: &CHirStmt<'_>,
            next: usize,
            brk: usize,
            cont: usize,
            cases: &mut Vec<(usize, bool)>,
        ) -> usize {
            match s {
                CHirStmt::Return(..) | CHirStmt::Expr(CHirExpr::BuiltinUnreachable { .. }, _) => {
                    self.node(vec![])
                }
                CHirStmt::Expr(CHirExpr::Call(CHirExpr::Ident { symbol, .. }, _, _), _)
                    if self.nonreturning.contains(symbol) =>
                {
                    self.node(vec![])
                }
                CHirStmt::Compound(items, ..) => {
                    items.iter().rev().fold(next, |next, item| match item {
                        CHirBlockItem::Statement(s) => self.stmt(s, next, brk, cont, cases),
                        _ => next,
                    })
                }
                CHirStmt::If(c, a, b, _) => {
                    let a = self.stmt(a, next, brk, cont, cases);
                    let b = b.map_or(next, |b| self.stmt(b, next, brk, cont, cases));
                    let id = self.node(vec![]);
                    self.branch(id, Some(c), a, b);
                    id
                }
                CHirStmt::While(c, body, _) | CHirStmt::DoWhile(body, c, _) => {
                    let id = self.node(vec![]);
                    let body = self.stmt(body, id, next, id, cases);
                    self.branch(id, Some(c), body, next);
                    if matches!(s, CHirStmt::DoWhile(..)) {
                        body
                    } else {
                        id
                    }
                }
                CHirStmt::For(_, c, _, body, _) => {
                    let id = self.node(vec![]);
                    let body = self.stmt(body, id, next, id, cases);
                    self.branch(id, *c, body, next);
                    id
                }
                CHirStmt::Break(_) => self.node(vec![brk]),
                CHirStmt::Continue(_) => self.node(vec![cont]),
                CHirStmt::Label(name, body, _) => {
                    let id = self.stmt(body, next, brk, cont, cases);
                    self.labels.insert(name.spelling.clone(), id);
                    id
                }
                CHirStmt::Goto(name, _) => {
                    let id = self.node(vec![]);
                    self.jumps.push((id, name.spelling.clone()));
                    id
                }
                CHirStmt::Switch(_, body, _) => {
                    let mut nested = Vec::new();
                    self.stmt(body, next, next, cont, &mut nested);
                    let mut edges: Vec<_> = nested.iter().map(|c| c.0).collect();
                    if !nested.iter().any(|c| c.1) {
                        edges.push(next);
                    }
                    self.node(edges)
                }
                CHirStmt::Case(_, body, _)
                | CHirStmt::CaseRange(_, _, body, _)
                | CHirStmt::Default(body, _) => {
                    let id = self.stmt(body, next, brk, cont, cases);
                    cases.push((id, matches!(s, CHirStmt::Default(..))));
                    id
                }
                CHirStmt::ComputedGoto(target, _) => {
                    // A GNU indirect jump transfers to a function-local label,
                    // not to the closing brace. Resolve its possible labels
                    // after visiting the whole body. Keep an unknown exit when
                    // evaluating the operand itself can contain control flow.
                    if inline::expression_has_statements(target) {
                        return self.node(vec![0]);
                    }
                    let id = self.node(vec![]);
                    self.indirect_jumps.push(id);
                    id
                }
                CHirStmt::Asm(a) => {
                    let id = self.node(vec![next]);
                    self.jumps
                        .extend(a.labels.iter().map(|label| (id, label.spelling.clone())));
                    id
                }
                _ => next,
            }
        }
    }
    let mut graph = Flow {
        unit,
        target,
        nonreturning,
        edges: vec![vec![]],
        labels: HashMap::new(),
        jumps: Vec::new(),
        indirect_jumps: Vec::new(),
    };
    let entry = graph.stmt(body, 0, 0, 0, &mut Vec::new());
    for (id, name) in graph.jumps {
        graph.edges[id].push(*graph.labels.get(&name).unwrap_or(&0));
    }
    if !graph.indirect_jumps.is_empty() {
        // Over-approximate every address-taken label in this function. A label
        // inside unmodeled expression control flow retains the unknown exit;
        // no target information is not evidence that a jump cannot return.
        let mut targets: Vec<_> = inline::addressed_label_names(body)
            .iter()
            .map(|name| *graph.labels.get(name).unwrap_or(&0))
            .collect();
        if targets.is_empty() {
            targets.push(0);
        }
        // Share the target set so many jumps and labels do not create a
        // quadratic number of edges in a large threaded interpreter.
        let dispatch = graph.edges.len();
        graph.edges.push(targets);
        for id in graph.indirect_jumps {
            graph.edges[id].push(dispatch);
        }
    }
    let mut seen = HashSet::new();
    let mut work = vec![entry];
    while let Some(id) = work.pop() {
        if id == 0 {
            return true;
        }
        if seen.insert(id) {
            work.extend(&graph.edges[id]);
        }
    }
    false
}
