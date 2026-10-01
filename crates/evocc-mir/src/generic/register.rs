//! Target-independent descriptions of physical register banks and their views.

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
