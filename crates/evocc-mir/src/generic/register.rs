//! Target-independent descriptions of physical register banks and their views.

/// Properties needed by the shared SSA transport and demand algorithms.
pub trait RegisterClass: Copy + Ord {
    fn is_flags(self) -> bool;
}

/// A virtual identity and the target-defined class of the value it transports.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtualReg<C> {
    pub id: u32,
    pub class: C,
}

impl<C: std::fmt::Debug> std::fmt::Debug for VirtualReg<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VReg")
            .field("id", &self.id)
            .field("class", &self.class)
            .finish()
    }
}

impl<C> VirtualReg<C> {
    #[must_use]
    pub const fn new(id: u32, class: C) -> Self {
        Self { id, class }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterBank {
    pub first: u8,
    pub count: u8,
}

impl RegisterBank {
    #[must_use]
    pub const fn contains(self, number: u8) -> bool {
        number >= self.first && number - self.first < self.count
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterList(pub &'static [u8]);

impl RegisterList {
    #[must_use]
    pub const fn count(self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn nth(self, index: usize) -> Option<u8> {
        self.0.get(index).copied()
    }

    #[must_use]
    pub fn position(self, number: u8) -> Option<usize> {
        self.0.iter().position(|candidate| *candidate == number)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedRegisterView {
    pub bank: RegisterBank,
    pub bits: u16,
    pub names: &'static [&'static str],
}

impl NamedRegisterView {
    #[must_use]
    pub fn spell(self, number: u8) -> String {
        self.names[usize::from(number - self.bank.first)].to_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterView {
    pub bank: RegisterBank,
    pub bits: u16,
    pub prefix: &'static str,
}

impl RegisterView {
    #[must_use]
    pub fn spell(self, number: u8) -> String {
        format!("{}{}", self.prefix, number - self.bank.first)
    }
}
