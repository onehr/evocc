pub use evocc_types::Isa;
use evocc_types::{ObjectFormat, OperatingSystem, TargetPlatform};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    Darwin,
    Linux,
}

impl Target {
    pub const fn platform(self) -> TargetPlatform {
        match self {
            Self::Darwin => TargetPlatform::Aarch64AppleDarwin,
            Self::Linux => TargetPlatform::Aarch64UnknownLinuxGnu,
        }
    }

    pub const fn isa(self) -> Isa {
        self.platform().isa()
    }

    pub const fn is_elf(self) -> bool {
        matches!(self.platform().object_format(), ObjectFormat::Elf)
    }

    pub const fn is_darwin(self) -> bool {
        matches!(self.platform().operating_system(), OperatingSystem::Darwin)
    }

    pub const fn is_linux(self) -> bool {
        matches!(self.platform().operating_system(), OperatingSystem::Linux)
    }
}
