//! Explicit target-library boundary. Core EvoIR never knows fenv_t's layout.
//! The two runtimes share the AArch64 layout, but not raise/update behavior.
use super::*;
use evocc_types::floating::{FpEnvironment, FpRounding, FpStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenvRuntime {
    Aarch64Glibc,
    Aarch64Musl,
}

impl FenvRuntime {
    fn raised(self, flags: FpStatus) -> FpStatus {
        if self == Self::Aarch64Glibc
            && flags.bits() & (FpStatus::OVERFLOW.bits() | FpStatus::UNDERFLOW.bits()) != 0
        {
            flags.union(FpStatus::INEXACT)
        } else {
            flags
        }
    }
}

fn control(rounding: FpRounding) -> u32 {
    match rounding {
        FpRounding::NearestEven => 0,
        FpRounding::Upward => 1 << 22,
        FpRounding::Downward => 2 << 22,
        FpRounding::TowardZero => 3 << 22,
    }
}

fn rounding(control: u32) -> Option<FpRounding> {
    Some(match control {
        0 => FpRounding::NearestEven,
        0x400000 => FpRounding::Upward,
        0x800000 => FpRounding::Downward,
        0xc00000 => FpRounding::TowardZero,
        _ => return None,
    })
}

fn mask(bits: u64) -> FpStatus {
    FpStatus::from_bits((bits & 31) as u8).expect("five-bit exception mask")
}

impl WorldState {
    /// Pure numeric FP is admitted only in its fixed-RNE environment. Exact
    /// representation operations do not need this check. Sticky flags alone
    /// do not make the fixed environment incompatible.
    pub fn check_fixed_fp_environment(&self) -> Result<(), MemoryFault> {
        if self.fp_environment.rounding == FpRounding::NearestEven {
            Ok(())
        } else {
            Err(MemoryFault::FloatingEnvironment {
                control: control(self.fp_environment.rounding),
                status: u32::from(self.fp_environment.flags.bits()),
            })
        }
    }
}

impl MemoryState {
    fn load_fp_environment(&self, address: Pointer) -> Result<FpEnvironment, MemoryFault> {
        // FE_DFL_ENV is a runtime sentinel, not a fabricated memory object.
        if self.pointer_address(PointerValue::object(address))? == u64::MAX {
            return Ok(FpEnvironment::default());
        }
        let bytes = self.load_bytes(address, AccessLayout::new(8, 4))?;
        let control = u32::from_le_bytes(bytes.bytes()[..4].try_into().unwrap());
        let status = u32::from_le_bytes(bytes.bytes()[4..].try_into().unwrap());
        let error = || MemoryFault::FloatingEnvironment { control, status };
        if status & !31 != 0 {
            return Err(error());
        }
        Ok(FpEnvironment {
            rounding: rounding(control).ok_or_else(error)?,
            flags: mask(u64::from(status)),
        })
    }

    fn store_fp_environment(
        &self,
        address: Pointer,
        env: FpEnvironment,
    ) -> Result<Self, MemoryFault> {
        let bytes = [
            control(env.rounding).to_le_bytes(),
            u32::from(env.flags.bits()).to_le_bytes(),
        ]
        .concat();
        self.store_bytes(
            address,
            AccessLayout::new(8, 4),
            &ByteSequence::try_from_bytes(bytes)?,
        )
    }

    /// Execute a selected target runtime model, returning both explicit state
    /// snapshots. No host fenv, ambient table, or unmodeled callback is used.
    pub fn call_library_with_world(
        &self,
        world: &WorldState,
        runtime: FenvRuntime,
        name: &str,
        arguments: &[LibraryValue],
    ) -> Result<(Self, WorldState, LibraryValue), MemoryFault> {
        use LibraryValue::{Integer, Pointer as Address};
        let fenv_name = matches!(
            name,
            "fegetround"
                | "fesetround"
                | "fetestexcept"
                | "feclearexcept"
                | "feraiseexcept"
                | "fegetexceptflag"
                | "fesetexceptflag"
                | "fegetenv"
                | "fesetenv"
                | "feholdexcept"
                | "feupdateenv"
        );
        if fenv_name
            && (self.config.byte_order != ByteOrder::Little
                || self.config.pointer_layout != AccessLayout::new(8, 8))
        {
            return Err(MemoryFault::ExternalFunction { name: name.into() });
        }
        let mut env = world.fp_environment();
        let mut memory = self.clone();
        let result = match (name, arguments) {
            ("fegetround", []) => u64::from(control(env.rounding)),
            ("fesetround", [Integer(value)]) => match rounding(*value as u32) {
                Some(rounding) => {
                    env.rounding = rounding;
                    0
                }
                None => 1,
            },
            ("fetestexcept", [Integer(value)]) => u64::from(env.flags.bits() & mask(*value).bits()),
            ("feclearexcept", [Integer(value)]) => {
                env = env.cleared(mask(*value));
                0
            }
            ("feraiseexcept", [Integer(value)]) => {
                env = env.raised(runtime.raised(mask(*value)));
                0
            }
            ("fegetexceptflag", [Address(destination), Integer(value)]) => {
                memory = self.store_integer(
                    *destination,
                    AccessLayout::new(4, 4),
                    u128::from(env.flags.bits() & mask(*value).bits()),
                )?;
                0
            }
            ("fesetexceptflag", [Address(source), Integer(value)]) => {
                let flags = self.load_integer(*source, AccessLayout::new(4, 4))?;
                env = env
                    .cleared(mask(*value))
                    .raised(mask(flags as u64 & *value));
                0
            }
            ("fegetenv" | "feholdexcept", [Address(destination)]) => {
                memory = self.store_fp_environment(*destination, env)?;
                if name == "feholdexcept" {
                    env = env.cleared(FpStatus::ALL);
                }
                0
            }
            ("fesetenv" | "feupdateenv", [Address(source)]) => {
                let restored = self.load_fp_environment(*source)?;
                env = if name == "feupdateenv" {
                    restored.raised(runtime.raised(env.flags))
                } else {
                    restored
                };
                0
            }
            _ => {
                let (memory, result) = self.call_library(name, arguments)?;
                return Ok((memory, world.clone(), result));
            }
        };
        Ok((memory, world.with_fp_environment(env), Integer(result)))
    }
}
