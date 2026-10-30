//! Parser builtin profiles.

use super::*;

pub(super) fn legacy_atomic(
    name: &str,
) -> Option<(
    Option<evocc_chir::AtomicRmwOp>,
    evocc_chir::AtomicRmwReturn,
    evocc_chir::AtomicOrder,
)> {
    use evocc_chir::{AtomicOrder as O, AtomicRmwOp as R, AtomicRmwReturn as V};
    let name = name.strip_prefix("__sync_")?;
    let result = match name {
        "bool_compare_and_swap" => (Some(R::CompareExchange), V::Success, O::SeqCst),
        "val_compare_and_swap" => (Some(R::CompareExchange), V::Old, O::SeqCst),
        "lock_test_and_set" => (Some(R::Exchange), V::Old, O::Acquire),
        "lock_release" => (None, V::Old, O::Release),
        _ => {
            let (operation, result) = if let Some(op) = name.strip_prefix("fetch_and_") {
                (op, V::Old)
            } else {
                (name.strip_suffix("_and_fetch")?, V::New)
            };
            let operation = R::from_mnemonic(operation)?;
            if matches!(operation, R::Exchange | R::CompareExchange) {
                return None;
            }
            (Some(operation), result, O::SeqCst)
        }
    };
    Some(result)
}

pub(super) fn gnu_atomic(name: &str) -> Option<GnuAtomic> {
    use evocc_chir::{AtomicCopyOp as C, AtomicRmwOp as R, AtomicRmwReturn as V};
    let name = name.strip_prefix("__atomic_")?;
    Some(match name {
        "load" => GnuAtomic::Generic(C::Load),
        "store" => GnuAtomic::Generic(C::Store),
        "exchange" => GnuAtomic::Generic(C::Exchange),
        "compare_exchange" => GnuAtomic::Generic(C::CompareExchange),
        "exchange_n" => GnuAtomic::Update(R::Exchange, V::Old),
        "compare_exchange_n" => GnuAtomic::CompareExchange,
        "test_and_set" => GnuAtomic::TestAndSet,
        "clear" => GnuAtomic::Clear,
        "always_lock_free" => GnuAtomic::AlwaysLockFree,
        "is_lock_free" => GnuAtomic::IsLockFree,
        _ => {
            let (operation, result) = if let Some(op) = name.strip_prefix("fetch_") {
                (op, V::Old)
            } else {
                (name.strip_suffix("_fetch")?, V::New)
            };
            let operation = R::from_mnemonic(operation)
                .filter(|op| !matches!(op, R::Exchange | R::CompareExchange))?;
            GnuAtomic::Update(operation, result)
        }
    })
}

pub(super) fn clang_atomic(name: &str) -> Option<ClangAtomic> {
    use evocc_chir::AtomicRmwOp as R;
    Some(match name.strip_prefix("__c11_atomic_")? {
        "init" => ClangAtomic::Init,
        "load" => ClangAtomic::Load,
        "store" => ClangAtomic::Store,
        "exchange" => ClangAtomic::Exchange,
        "compare_exchange_strong" | "compare_exchange_weak" => ClangAtomic::CompareExchange,
        "fetch_add" => ClangAtomic::Fetch(R::Add),
        "fetch_sub" => ClangAtomic::Fetch(R::Sub),
        "fetch_and" => ClangAtomic::Fetch(R::And),
        "fetch_or" => ClangAtomic::Fetch(R::Or),
        "fetch_xor" => ClangAtomic::Fetch(R::Xor),
        "is_lock_free" => ClangAtomic::IsLockFree,
        _ => return None,
    })
}

/// The order a compare-exchange carries in EvoIR: its success order,
/// strengthened to acq_rel when a release success pairs with an acquiring
/// failure. Each half of an admitted pair is at most as strong as this order.
pub(super) fn merged_exchange_order(
    success: evocc_chir::AtomicOrder,
    failure: evocc_chir::AtomicOrder,
) -> evocc_chir::AtomicOrder {
    use evocc_chir::AtomicOrder as O;
    if success == O::Release && matches!(failure, O::Consume | O::Acquire) {
        O::AcqRel
    } else {
        success
    }
}

/// The order of a Clang compare-exchange. Apple clang admits a failure order
/// stronger than the success order. Such a failure order is consume or
/// acquire after a relaxed or consume success, or seq_cst, so it is at least
/// as strong as both halves; the other pairs merge as the GNU ones do.
pub(super) fn merged_clang_exchange_order(
    success: evocc_chir::AtomicOrder,
    failure: evocc_chir::AtomicOrder,
) -> evocc_chir::AtomicOrder {
    if failure as u8 > success as u8 {
        failure
    } else {
        merged_exchange_order(success, failure)
    }
}
