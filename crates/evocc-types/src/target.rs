use crate::TargetDataLayout;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Isa {
    Aarch64,
}

impl Isa {
    pub const fn memory_barrier(self, acquire_only: bool) -> &'static str {
        match (self, acquire_only) {
            (Self::Aarch64, true) => "dmb ishld",
            (Self::Aarch64, false) => "dmb ish",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatingSystem {
    Darwin,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectFormat {
    MachO,
    Elf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetPlatform {
    Aarch64UnknownLinuxGnu,
    Aarch64AppleDarwin,
}

impl TargetPlatform {
    pub fn parse(triple: &str) -> Option<Self> {
        match triple {
            "aarch64-unknown-linux-gnu" => Some(Self::Aarch64UnknownLinuxGnu),
            "aarch64-apple-darwin" => Some(Self::Aarch64AppleDarwin),
            _ => None,
        }
    }

    pub const fn isa(self) -> Isa {
        match self {
            Self::Aarch64UnknownLinuxGnu | Self::Aarch64AppleDarwin => Isa::Aarch64,
        }
    }

    pub const fn operating_system(self) -> OperatingSystem {
        match self {
            Self::Aarch64UnknownLinuxGnu => OperatingSystem::Linux,
            Self::Aarch64AppleDarwin => OperatingSystem::Darwin,
        }
    }

    pub const fn object_format(self) -> ObjectFormat {
        match self {
            Self::Aarch64UnknownLinuxGnu => ObjectFormat::Elf,
            Self::Aarch64AppleDarwin => ObjectFormat::MachO,
        }
    }

    pub const fn layout(self) -> TargetDataLayout {
        match self {
            Self::Aarch64UnknownLinuxGnu => TargetDataLayout::aarch64_unknown_linux_gnu(),
            Self::Aarch64AppleDarwin => TargetDataLayout::aarch64_apple_darwin(),
        }
    }

    pub const fn evoir_layout_key(self) -> &'static str {
        match self {
            Self::Aarch64UnknownLinuxGnu => {
                "aarch64-unknown-linux-gnu/evoir-layout-v4-hfa-natural-alignment\n"
            }
            Self::Aarch64AppleDarwin => {
                "aarch64-apple-darwin/evoir-layout-v1-long-double-binary64\n"
            }
        }
    }
}

impl TargetDataLayout {
    pub const fn platform(&self) -> TargetPlatform {
        self.platform
    }

    pub const fn isa(&self) -> Isa {
        self.platform.isa()
    }

    pub const fn anonymous_arguments_on_stack(&self) -> bool {
        self.anonymous_on_stack
    }

    pub const fn va_list_is_pointer(&self) -> bool {
        self.pointer_va_list
    }

    pub const fn with_variadic_abi(
        mut self,
        pointer_va_list: bool,
        anonymous_on_stack: bool,
    ) -> Self {
        self.pointer_va_list = pointer_va_list;
        self.anonymous_on_stack = anonymous_on_stack;
        self
    }
}
