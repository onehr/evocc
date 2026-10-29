//! Lowering conditional values.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_ternary(
        &mut self,
        condition: &'a CHirExpr<'a>,
        then_expression: &'a CHirExpr<'a>,
        else_expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let known_truth =
            self.known_expression_truth(condition, &[then_expression, else_expression]);
        let predicate = self.lower_condition(condition, state)?;
        if let Some(value) = self.reuse_compared_selection(
            condition,
            then_expression,
            else_expression,
            &predicate,
            state,
        )? {
            return Ok(value);
        }
        let keys = self.expression_join_keys(state, &[then_expression, else_expression]);
        let mut then_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let mut else_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let then_arguments = self.edge_values(state, &keys)?;
        let else_arguments = self.edge_values(state, &keys)?;
        self.terminate(
            state,
            conditional_branch(
                predicate,
                known_truth,
                then_state.block,
                then_arguments,
                else_state.block,
                else_arguments,
            ),
        )?;
        let then_value = self.lower_expr(then_expression, &mut then_state)?;
        let else_value = self.lower_expr(else_expression, &mut else_state)?;
        let (then_value, else_value, common) =
            self.common_branch_values(then_value, else_value, &mut then_state, &mut else_state)?;
        let temporary = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        then_state.env.insert(
            temporary,
            BoundValue {
                value: then_value.value,
                integer: Some(common),
            },
        );
        else_state.env.insert(
            temporary,
            BoundValue {
                value: else_value.value,
                integer: Some(common),
            },
        );
        let mut merge_keys = keys;
        merge_keys.push(temporary);
        let merge = self.new_block_from(&then_state, &merge_keys, BlockRole::Ordinary)?;
        self.branch(&then_state, merge.block, &merge_keys)?;
        self.branch(&else_state, merge.block, &merge_keys)?;
        *state = merge;
        let result = state
            .env
            .remove(&temporary)
            .ok_or(LowerError::InvalidControlFlow {
                span: condition.span(),
                reason: "ternary merge lost its result",
            })?;
        Ok(ExprValue {
            value: result.value,
            integer: result.integer.ok_or(LowerError::InvalidControlFlow {
                span: condition.span(),
                reason: "ternary result has no integer type",
            })?,
        })
    }

    pub(super) fn lower_elvis(
        &mut self,
        operand: &'a CHirExpr<'a>,
        else_expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let known_truth = self.known_expression_truth(operand, &[else_expression]);
        // Retain the one evaluated operand as an explicit edge value. The
        // true successor consumes that value directly; it never lowers the
        // source expression a second time.
        let operand_value = self.lower_expr(operand, state)?;
        let retained = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        state.env.insert(
            retained,
            BoundValue {
                value: operand_value.value.clone(),
                integer: Some(operand_value.integer),
            },
        );
        let predicate = self.lower_evaluated_condition(operand, operand_value, state)?;
        let mut keys = self.expression_join_keys(state, &[else_expression]);
        // Normalize the retained value exactly where an explicit scalar local
        // initialized from the same SSA value would appear. `keys_for`
        // otherwise groups all temporaries after locals, which would make the
        // GNU shorthand observably different in Section 9 text despite
        // carrying the same edge values.
        keys.retain(|key| *key != retained);
        let retained_id = state
            .env
            .get(&retained)
            .ok_or(LowerError::InvalidControlFlow {
                span: operand.span(),
                reason: "elvis lowering lost its retained operand",
            })?
            .value
            .id;
        let insertion = keys
            .iter()
            .position(|key| {
                state
                    .env
                    .get(key)
                    .is_none_or(|bound| bound.value.id > retained_id)
            })
            .unwrap_or(keys.len());
        keys.insert(insertion, retained);
        let mut then_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let mut else_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let then_arguments = self.edge_values(state, &keys)?;
        let else_arguments = self.edge_values(state, &keys)?;
        self.terminate(
            state,
            conditional_branch(
                predicate,
                known_truth,
                then_state.block,
                then_arguments,
                else_state.block,
                else_arguments,
            ),
        )?;
        let retained_value =
            then_state
                .env
                .get(&retained)
                .cloned()
                .ok_or(LowerError::InvalidControlFlow {
                    span: operand.span(),
                    reason: "elvis true edge lost its retained operand",
                })?;
        let then_value = ExprValue {
            value: retained_value.value,
            integer: retained_value
                .integer
                .ok_or(LowerError::InvalidControlFlow {
                    span: operand.span(),
                    reason: "elvis retained operand has no integer type",
                })?,
        };
        let else_value = self.lower_expr(else_expression, &mut else_state)?;
        let (then_value, else_value, common) =
            self.common_branch_values(then_value, else_value, &mut then_state, &mut else_state)?;
        let result_key = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        then_state.env.insert(
            result_key,
            BoundValue {
                value: then_value.value,
                integer: Some(common),
            },
        );
        else_state.env.insert(
            result_key,
            BoundValue {
                value: else_value.value,
                integer: Some(common),
            },
        );
        let mut merge_keys = keys;
        merge_keys.push(result_key);
        let mut merge = self.new_block_from(&then_state, &merge_keys, BlockRole::Ordinary)?;
        self.branch(&then_state, merge.block, &merge_keys)?;
        self.branch(&else_state, merge.block, &merge_keys)?;
        let result = merge
            .env
            .remove(&result_key)
            .ok_or(LowerError::InvalidControlFlow {
                span: operand.span(),
                reason: "elvis merge lost its result",
            })?;
        merge.env.remove(&retained);
        *state = merge;
        Ok(ExprValue {
            value: result.value,
            integer: result.integer.ok_or(LowerError::InvalidControlFlow {
                span: operand.span(),
                reason: "elvis result has no integer type",
            })?,
        })
    }
}
