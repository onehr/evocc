use evocc_mir::generic::abi::{AbiFacts, ArgumentCursor};
use evocc_mir::generic::register::{NamedRegisterView, RegisterBank, RegisterList};
use evocc_mir::isa::aarch64::abi::LINUX;

#[test]
fn arguments_and_results_keep_independent_noncontiguous_orders() {
    let arguments = [RegisterList(&[11, 7, 2, 5]), RegisterList(&[91, 85])];
    let results = [RegisterList(&[5, 2]), RegisterList(&[85])];
    let mut cursor = ArgumentCursor::new(&arguments);
    for number in [11, 7, 2, 5] {
        assert_eq!(cursor.place(0, 1), Ok(number));
    }
    assert_eq!(cursor.place(0, 1), Err(0));
    assert_eq!(cursor.place(1, 2), Ok(91));
    assert_eq!(cursor.place(1, 2), Ok(85));
    assert_eq!(cursor.place(1, 2), Err(2));
    let mut cursor = ArgumentCursor::new(&results);
    assert_eq!(cursor.place(0, 1), Ok(5));
    assert_eq!(cursor.place(0, 1), Ok(2));
    assert_eq!(cursor.place(0, 1), Err(0));
    assert_eq!(arguments[0].position(2), Some(2));
    assert_eq!(results[0].position(2), Some(1));
    assert_eq!(arguments[0].position(0), None);
    assert_eq!(arguments[0].nth(4), None);
}

#[test]
fn empty_and_maximum_identity_lists_do_not_wrap_the_cursor() {
    const REGISTERS: [u8; 256] = {
        let mut values = [0; 256];
        let mut index = 0;
        while index < values.len() {
            values[index] = index as u8;
            index += 1;
        }
        values
    };
    let banks = [RegisterList(&[]), RegisterList(&REGISTERS)];
    let mut cursor = ArgumentCursor::new(&banks);
    assert_eq!(cursor.place(0, 2), Err(0));
    for number in REGISTERS {
        assert_eq!(cursor.place(1, 1), Ok(number));
    }
    assert_eq!(cursor.used(1), 256);
    assert_eq!(cursor.place(1, 1), Err(2));
    assert_eq!(banks[0].count(), 0);
    assert_eq!(banks[1].count(), 256);
    assert_eq!(banks[1].position(255), Some(255));
}

#[test]
fn register_views_can_use_names_instead_of_number_suffixes() {
    let view = NamedRegisterView {
        bank: RegisterBank {
            first: 254,
            count: 2,
        },
        bits: 64,
        names: &["left", "right"],
    };
    assert_eq!(view.spell(254), "left");
    assert_eq!(view.spell(255), "right");
}

#[test]
fn optional_roles_do_not_consume_argument_positions() {
    let facts = AbiFacts {
        indirect_result: None,
        result_overflow: None,
        ..LINUX.facts
    };
    let mut cursor = ArgumentCursor::new(facts.argument_banks);
    assert_eq!(cursor.place(0, 1), Ok(0));
    assert!(facts.require_indirect_result().is_err());
}

#[test]
fn target_predicates_and_abi_accessors_keep_the_two_platforms_distinct() {
    for (target, elf, darwin) in [
        (evocc_mir::Target::Linux, true, false),
        (evocc_mir::Target::Darwin, false, true),
    ] {
        assert_eq!(target.isa(), evocc_mir::Isa::Aarch64);
        assert_eq!(target.is_elf(), elf);
        assert_eq!(target.is_linux(), elf);
        assert_eq!(target.is_darwin(), darwin);
        assert_eq!(*target.abi_facts(), target.aarch64_abi().facts);
    }
}

struct ListedTarget {
    facts: AbiFacts,
    registers: evocc_mir::isa::aarch64::allocation::Aarch64Allocation,
}

impl ListedTarget {
    fn new() -> Self {
        Self {
            facts: AbiFacts {
                argument_banks: &[RegisterList(&[7, 1, 9, 4]), RegisterList(&[64, 65])],
                result_banks: &[RegisterList(&[4, 1]), RegisterList(&[65])],
                indirect_result: None,
                result_overflow: None,
                ..LINUX.facts
            },
            registers: evocc_mir::isa::aarch64::allocation::Aarch64Allocation::for_target(
                evocc_mir::Target::Linux,
            ),
        }
    }
}

impl evocc_mir::generic::allocation::AllocationTarget<evocc_mir::RegClass> for ListedTarget {
    fn bank(&self, class: evocc_mir::RegClass) -> RegisterBank {
        self.registers.bank(class)
    }
    fn candidates(&self, class: evocc_mir::RegClass, across_call: bool) -> &'static [u8] {
        self.registers.candidates(class, across_call)
    }
    fn abi(&self) -> &AbiFacts {
        &self.facts
    }
    fn cycle_scratch(&self, class: evocc_mir::RegClass) -> u8 {
        self.registers.cycle_scratch(class)
    }
    fn flags_assignment_tag(&self) -> u8 {
        self.registers.flags_assignment_tag()
    }
    fn fixed_assembly_registers(&self) -> &'static [u8] {
        self.registers.fixed_assembly_registers()
    }
    fn clobber_classes(&self) -> &'static [evocc_mir::RegClass] {
        self.registers.clobber_classes()
    }
    fn fixed_binding_spelling(&self, number: u8) -> String {
        self.registers.fixed_binding_spelling(number)
    }
}

fn reg(id: u32) -> evocc_mir::VReg {
    evocc_mir::VReg {
        id,
        class: evocc_mir::RegClass::Gpr64,
    }
}

#[test]
fn allocation_and_verification_use_list_positions_not_numeric_ranges() {
    use evocc_mir::generic::machine::MachInst;
    use evocc_mir::{BinOp, MBlock, MFunction, MInst, MTerm};
    let target = ListedTarget::new();
    let input = MFunction {
        name: "sum".into(),
        blocks: vec![MBlock {
            id: 0,
            args: vec![reg(0), reg(1), reg(2)],
            insts: vec![
                MInst::Binary {
                    op: BinOp::Add,
                    width: evocc_mir::Width::X,
                    dst: reg(3),
                    lhs: reg(0),
                    rhs: reg(1),
                },
                MInst::Binary {
                    op: BinOp::Add,
                    width: evocc_mir::Width::X,
                    dst: reg(4),
                    lhs: reg(3),
                    rhs: reg(2),
                },
            ],
            term: MTerm::Ret {
                values: vec![reg(4)],
            },
        }],
    };
    let mut allocated = evocc_mir::generic::allocation::allocate(&target, &input).unwrap();
    evocc_mir::generic::allocverify::verify_allocation(&target, &allocated).unwrap();
    let mut defined = std::collections::BTreeSet::new();
    let mut incoming = None;
    for inst in &allocated.blocks[0].insts {
        for value in inst.uses() {
            if !defined.contains(&value.id) && allocated.assignment[&value.id] == 7 {
                incoming = Some(value.id);
            }
        }
        defined.extend(inst.defs().iter().map(|value| value.id));
    }
    allocated.assignment.insert(incoming.unwrap(), 4);
    assert!(evocc_mir::generic::allocverify::verify_allocation(&target, &allocated).is_err());
}

#[test]
fn unavailable_indirect_and_overflow_roles_return_errors() {
    use evocc_mir::{MBlock, MFunction, MInst, MTerm};
    let target = ListedTarget::new();
    for (args, insts, message) in [
        (vec![reg(0)], vec![MInst::IndirectResult], "indirect-result"),
        (
            Vec::new(),
            vec![MInst::Bl {
                symbol: "callee".into(),
                args: Vec::new(),
                results: vec![reg(0), reg(1), reg(2)],
                fp_mode: None,
                returns_twice: false,
            }],
            "overflow-result",
        ),
    ] {
        let input = MFunction {
            name: "caller".into(),
            blocks: vec![MBlock {
                id: 0,
                args,
                insts,
                term: MTerm::Ret { values: Vec::new() },
            }],
        };
        let error = evocc_mir::generic::allocation::allocate(&target, &input).unwrap_err();
        assert!(error.contains(message), "{error}");
    }
}
