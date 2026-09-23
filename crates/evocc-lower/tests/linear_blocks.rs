use std::collections::BTreeMap;

use evocc_common::{Arena, FileId};
use evocc_evoir::Inst;
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn an_unconditional_single_entry_continuation_needs_no_block_copy() {
    let source = "int f(int *p,int n){for(int j=0;j<n;j++)p[j]=p[j]<p[0]?p[j]:p[0];return p[0];}";
    let arena = Arena::default();
    let file = FileId::new(45120);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module = evocc_lower::lower_unit_with_optimization(
        &unit,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        true,
    )
    .unwrap();
    let function = &module.functions[0];
    let mut entries = BTreeMap::<u32, usize>::new();
    for block in &function.blocks {
        let targets = match &block.terminator {
            Inst::Br(target, _) => vec![*target],
            Inst::CondBr(_, a, _, b, _) => vec![*a, *b],
            Inst::Ret(_) | Inst::Trap(_) => vec![],
            _ => panic!("unexpected control flow in the reduction"),
        };
        for target in targets {
            *entries.entry(target).or_default() += 1;
        }
    }
    let redundant: Vec<_> = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            Inst::Br(target, _)
                if *target != function.blocks[0].id
                    && *target != block.id
                    && entries.get(target) == Some(&1) =>
            {
                Some((block.id, *target))
            }
            _ => None,
        })
        .collect();
    assert!(
        redundant.is_empty(),
        "straight-line continuations still require edge copies: {redundant:?}"
    );
}
