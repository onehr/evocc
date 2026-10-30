use super::{DARWIN_TARGET, SUPPORTED_TARGET};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DriverTarget {
    Aarch64UnknownLinuxGnu,
    Aarch64AppleDarwin,
}

impl DriverTarget {
    pub(super) fn triple(self) -> &'static str {
        match self {
            Self::Aarch64UnknownLinuxGnu => SUPPORTED_TARGET,
            Self::Aarch64AppleDarwin => DARWIN_TARGET,
        }
    }

    /// The target of the machine that runs the driver. The cc actions use it
    /// when the command line names no target.
    pub(super) fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Aarch64AppleDarwin
        } else {
            Self::Aarch64UnknownLinuxGnu
        }
    }

    pub(super) fn is_darwin(self) -> bool {
        self == Self::Aarch64AppleDarwin
    }

    pub(super) fn is_linux(self) -> bool {
        matches!(
            self.platform().operating_system(),
            evocc_types::OperatingSystem::Linux
        )
    }

    pub(super) fn platform(self) -> evocc_types::TargetPlatform {
        match self {
            Self::Aarch64UnknownLinuxGnu => evocc_types::TargetPlatform::Aarch64UnknownLinuxGnu,
            Self::Aarch64AppleDarwin => evocc_types::TargetPlatform::Aarch64AppleDarwin,
        }
    }
}
