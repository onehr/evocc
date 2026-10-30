//! C-HIR interpreter varargs.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn schedule_varargs(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let CHirExpr::BuiltinVarArgs {
            op,
            first,
            second,
            requested_ty,
            span,
            ..
        } = expression
        else {
            return Err(InterpreterError::VarArgs {
                span: expression.span(),
            });
        };
        tasks.push(MachineTask::ApplyVarArgs {
            op: *op,
            requested_ty: *requested_ty,
            span: *span,
        });
        if *op == crate::VarArgsBuiltinOp::Copy {
            let source = second.ok_or(InterpreterError::VarArgs { span: *span })?;
            tasks.push(MachineTask::EvalExpression {
                expression: source,
                allow_root_mutation: true,
            });
        }
        tasks.push(MachineTask::EvalExpression {
            expression: first,
            allow_root_mutation: true,
        });
        Ok(())
    }

    pub(super) fn apply_varargs(
        &mut self,
        op: crate::VarArgsBuiltinOp,
        requested_ty: Option<TypeId>,
        span: SourceSpan,
        values: &mut Vec<CValue>,
    ) -> Result<(), InterpreterError> {
        use crate::VarArgsBuiltinOp;
        let fault = |error| InterpreterError::Memory { error, span };
        let address = |value| match value {
            CValue::Aggregate { address, .. } => Ok(address),
            _ => Err(fault(MemoryFault::InvalidArgumentList)),
        };
        let source = if op == VarArgsBuiltinOp::Copy {
            Some(address(pop_machine_value(values, span)?)?)
        } else {
            None
        };
        let list = address(pop_machine_value(values, span)?)?;
        if let Some(source) = source {
            self.memory = self
                .memory
                .copy_bytes(
                    list,
                    source,
                    AccessLayout::from_type_layout(self.target.va_list_layout()),
                )
                .map_err(fault)?;
            return Ok(());
        }
        let identity = match op {
            VarArgsBuiltinOp::Start => {
                let arguments = self
                    .memory_frame
                    .as_ref()
                    .ok_or_else(|| fault(MemoryFault::InvalidArgumentList))?
                    .anonymous
                    .clone();
                Some(self.argument_lists.start(arguments))
            }
            VarArgsBuiltinOp::End => None,
            VarArgsBuiltinOp::Arg => {
                let requested = requested_ty.ok_or(InterpreterError::VarArgs { span })?;
                if crate::half_interface_refused(self.unit.types(), &self.target, requested, true) {
                    return Err(InterpreterError::FloatingCallInterface { span });
                }
                let target = self.value_type(requested, span)?;
                if matches!(target, ValueType::Aggregate(ty) if !crate::is_integer_variadic_record(self.unit.types(), &self.target, ty))
                {
                    return Err(InterpreterError::VarArgs { span });
                }
                let identity = self.memory.argument_list(list).map_err(fault)?;
                let value = *self.argument_lists.read(identity).map_err(fault)?;
                let matches = match (value, target) {
                    (CValue::Integer(value), ValueType::Integer(ty)) => {
                        value.ty().bits() == ty.bits()
                    }
                    (CValue::Floating(value), ValueType::Floating(kind)) => value.kind() == kind,
                    (CValue::Pointer { .. }, ValueType::Pointer(ty)) => !matches!(
                        self.unit.types().get(ty.pointee()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    ),
                    (CValue::FunctionPointer { .. }, ValueType::Pointer(ty)) => matches!(
                        self.unit.types().get(ty.pointee()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    ),
                    (CValue::Aggregate { ty, .. }, ValueType::Aggregate(expected)) => {
                        self.same_aggregate_type(ty, expected)
                    }
                    _ => false,
                };
                if !matches {
                    return Err(fault(MemoryFault::InvalidArgumentList));
                }
                values.push(self.cast_runtime_value(value, target, span)?);
                Some(self.argument_lists.advance(identity).map_err(fault)?)
            }
            VarArgsBuiltinOp::Copy => unreachable!("copy returned above"),
        };
        self.memory = self
            .memory
            .with_argument_list(list, identity)
            .map_err(fault)?;
        Ok(())
    }
}
