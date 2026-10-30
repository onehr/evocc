use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn pin_call<I, C, A>(
    target: &A,
    insts: &mut Vec<I>,
    args: &[VirtualReg<C>],
    results: &[VirtualReg<C>],
    next: &mut u32,
    pins: &mut BTreeMap<u32, u8>,
    outgoing_slots: &mut u32,
    indirect_result: bool,
) -> Result<CallCopies<C>, String>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    A: AllocationTarget<C>,
{
    // Section 6 (revision 0.18): with the marker, the call's last argument is
    // the destination of a record result and uses its indirect-result location.
    let indirect = indirect_result.then(|| args.len().saturating_sub(1));
    let mut moved = Vec::new();
    let mut preparation = Vec::new();
    let mut stack_classes = Vec::new();
    let mut cursor = ArgumentCursor::new(target.abi().argument_banks);
    for (position, arg) in args.iter().enumerate() {
        if indirect == Some(position) {
            let outgoing = fresh(next, arg.class);
            pins.insert(outgoing.id, target.abi().require_indirect_result()?);
            preparation.push((I::copy(outgoing, *arg), false));
            moved.push(outgoing);
            continue;
        }
        // Stack slots are assigned in argument order at increasing addresses
        // from the caller's stack pointer at the call.
        let location = place(&mut cursor, arg.class);
        if let Err(slot) = location {
            preparation.push((I::outgoing_arg(*arg, slot), true));
            stack_classes.push(arg.class);
            *outgoing_slots = (*outgoing_slots).max(cursor.stack_words());
            continue;
        }
        let outgoing = fresh(next, arg.class);
        pins.insert(outgoing.id, location.unwrap());
        preparation.push((I::copy(outgoing, *arg), false));
        moved.push(outgoing);
    }
    let pinned: BTreeSet<_> = moved.iter().map(|reg| pins[&reg.id]).collect();
    let saturated = stack_classes.iter().any(|class| {
        let candidates = target.candidates(*class, false);
        !candidates.is_empty() && candidates.iter().all(|number| pinned.contains(number))
    });
    if saturated {
        // A late outgoing reload needs a register before the call consumes
        // its fixed operands. Store the already-evaluated stack arguments
        // before occupying the entire pool with argument pins. Both internal
        // orders and all ABI offsets remain unchanged.
        let (stores, copies): (Vec<_>, Vec<_>) =
            preparation.into_iter().partition(|(_, stack)| *stack);
        insts.extend(stores.into_iter().chain(copies).map(|(inst, _)| inst));
    } else {
        insts.extend(preparation.into_iter().map(|(inst, _)| inst));
    }
    let mut outputs = Vec::new();
    let mut cursor = ArgumentCursor::new(target.abi().result_banks);
    for result in results {
        let incoming = fresh(next, result.class);
        pins.insert(
            incoming.id,
            target
                .abi()
                .result_register(place(&mut cursor, result.class))?,
        );
        outputs.push((incoming, *result));
    }
    Ok((moved, outputs))
}
