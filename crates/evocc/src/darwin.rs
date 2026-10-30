//! The parts of the `aarch64-apple-darwin` target that the driver owns: the
//! predefined macros and the assembler names of declarations.

use std::collections::HashMap;

use evocc_chir::{CHirDecl, CHirUnit};
use evocc_cpp::availability::Version;
use evocc_cpp::Preprocessor;
use evocc_evoir::{Inst, Module};

/// The deployment target that EvoCC assumes when no option and no
/// `MACOSX_DEPLOYMENT_TARGET` names one: macOS 11.0, the first release for
/// arm64. Apple clang infers its default from the SDK and the host instead.
const DEFAULT_DEPLOYMENT: Version = Version::new(11, 0, 0);

/// The `__has_attribute` answers of Apple clang for the attributes that this
/// profile gives Apple clang's meaning (EvoIR 10.1.9).
const ATTRIBUTES: [(&str, i64); 5] = [
    ("availability", 1),
    ("unavailable", 1),
    ("weak_import", 1),
    ("neon_vector_type", 1),
    ("neon_polyvector_type", 1),
];

/// The `__has_feature` names of the availability attribute that Apple clang
/// answers 1 for. Each names a form that this profile reads.
const FEATURES: [&str; 10] = [
    "attribute_availability",
    "attribute_availability_with_message",
    "attribute_availability_app_extension",
    "attribute_availability_with_version_underscores",
    "attribute_availability_tvos",
    "attribute_availability_watchos",
    "attribute_availability_driverkit",
    "attribute_availability_with_strict",
    "attribute_availability_with_replacement",
    "attribute_availability_swift",
];

pub(crate) fn apply_predefined_profile(
    pp: &mut Preprocessor,
    deployment: Version,
) -> Result<(), String> {
    pp.add_target_attributes(ATTRIBUTES);
    pp.add_target_features(FEATURES);
    // Apple guards the rounding declarations with this Armv8 feature macro.
    // Their types are available even when a retained builtin is refused.
    pp.define(
        "__ARM_FEATURE_DIRECTED_ROUNDING".into(),
        crate::decoded_tokens("1")?,
    );
    pp.set_availability_target(deployment);
    let version = deployment.encoded().to_string();
    for name in [
        "__ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__",
        "__ENVIRONMENT_OS_VERSION_MIN_REQUIRED__",
    ] {
        pp.define(name.to_owned(), crate::decoded_tokens(&version)?);
    }
    Ok(())
}

/// The macOS deployment target, selected as Apple clang selects it: the last
/// `-mmacosx-version-min=` or `-mmacos-version-min=` option, then a nonempty
/// `MACOSX_DEPLOYMENT_TARGET`, then EvoCC's default of 11.0. A version is one
/// to three decimal components, each below 100, with a major version of at
/// least 10. A version below 11.0 is kept for the compilation; the platform
/// tools raise the object's minimum to 11.0, as for Apple clang.
pub(crate) fn deployment_target(option: Option<&str>) -> Result<Version, String> {
    let (text, spelling) = match option {
        Some(option) => {
            let text = option.split_once('=').map_or("", |(_, value)| value);
            (text.to_owned(), option.to_owned())
        }
        None => match std::env::var("MACOSX_DEPLOYMENT_TARGET") {
            Ok(text) if !text.is_empty() => {
                let spelling = format!("MACOSX_DEPLOYMENT_TARGET={text}");
                (text, spelling)
            }
            _ => return Ok(DEFAULT_DEPLOYMENT),
        },
    };
    let mut components = Vec::new();
    for part in text.split('.') {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            components.clear();
            break;
        }
        components.push(part.parse::<u32>().unwrap_or(u32::MAX));
    }
    match components.as_slice() {
        [major, rest @ ..]
            if rest.len() <= 2
                && (10..100).contains(major)
                && rest.iter().all(|component| *component < 100) =>
        {
            let minor = rest.first().copied().unwrap_or(0);
            let patch = rest.get(1).copied().unwrap_or(0);
            Ok(Version::new(*major, minor, patch))
        }
        _ => Err(format!("invalid version number in '{spelling}'")),
    }
}

/// Give each symbol that an assembler label names its Darwin spelling.
///
/// Apple's headers write the platform prefix into the label, for example
/// `int close(int) __asm("_close")`. The backend adds the `_` prefix to every
/// symbol, so the module must hold the name without it.
pub(crate) fn apply_assembler_labels(unit: &CHirUnit<'_>, module: &mut Module) {
    let names: HashMap<String, String> = unit
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Function(function) => function.asm_label.as_ref(),
            CHirDecl::Object(object) => object.asm_label.as_ref(),
            _ => None,
        })
        .filter_map(|label| {
            label
                .strip_prefix('_')
                .filter(|name| !name.is_empty())
                .map(|name| (label.clone(), name.to_owned()))
        })
        .collect();
    if names.is_empty() {
        return;
    }
    let rename = |name: &mut String| {
        if let Some(spelling) = names.get(name.as_str()) {
            name.clone_from(spelling);
        }
    };
    for function in &mut module.functions {
        rename(&mut function.name);
        for block in &mut function.blocks {
            for inst in &mut block.insts {
                match inst {
                    Inst::Call { callee, .. } => rename(callee),
                    Inst::FuncAddr { symbol, .. } | Inst::GlobalAddr { symbol, .. } => {
                        rename(symbol)
                    }
                    _ => {}
                }
            }
        }
    }
    for declaration in &mut module.declares {
        rename(&mut declaration.name);
    }
    for alias in &mut module.aliases {
        rename(&mut alias.name);
        rename(&mut alias.target);
    }
    for global in &mut module.globals {
        rename(&mut global.symbol);
        for relocation in &mut global.relocs {
            rename(&mut relocation.symbol);
        }
    }
    for object in &mut module.extern_objects {
        rename(&mut object.symbol);
    }
    for registration in &mut module.lifecycle {
        rename(&mut registration.function);
    }
}
