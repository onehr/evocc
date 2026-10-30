use super::*;

impl Preprocessor {
    pub fn for_target(target: &str) -> Result<Self, String> {
        let (profile, builtins) = match target {
            AARCH64_UNKNOWN_LINUX_GNU => {
                (AARCH64_UNKNOWN_LINUX_GNU_PREDEFINED, AARCH64_LINUX_BUILTINS)
            }
            "aarch64-apple-darwin" => (AARCH64_DARWIN_PREDEFINED, AARCH64_DARWIN_BUILTINS),
            _ => return Err(format!("unsupported preprocessor target profile: {target}")),
        };
        let mut preprocessor = Self {
            macros: MacroMap::new(),
            macro_stacks: HashMap::new(),
            include_guard_seen: false,
            pragma_once_files: HashSet::new(),
            include_paths: Vec::new(),
            system_include_paths: HashSet::new(),
            current_include_path: None,
            current_system_header: false,
            includers: Vec::new(),
            include_depth: 0,
            forced_includes: Vec::new(),
            macro_prefix_maps: Vec::new(),
            cond_stack: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            standard_includes: true,
            signed_char: false,
            wide_character_bits: 32,
            wide_character_unsigned: true,
            wide_character_refuses_out_of_range_hex: false,
            target_follows_apple_clang: false,
            conditional_depth: 0,
            source_ids: HashMap::new(),
            source_paths: HashMap::new(),
            system_source_files: HashSet::new(),
            user_source_files: HashSet::new(),
            source_contents: HashMap::new(),
            line_controls: HashMap::new(),
            reserved_file_ids: HashSet::new(),
            next_file_id: 1,
            counter: 0,
            builtin_feature_queries: HashSet::new(),
            target_builtins: builtins.lines().map(str::to_owned).collect(),
            target_attributes: HashMap::new(),
            target_features: HashSet::new(),
            availability_target: None,
            poisoned_identifiers: HashSet::new(),
            ignored_pragma_count: 0,
            ignored_pragma_counts: BTreeMap::new(),
            empty_macro_pop_count: 0,
            current_file: String::new(),
        };
        preprocessor.install_predefined_profile(profile.as_bytes())?;
        preprocessor.install_feature_query_macros();
        preprocessor
            .macros
            .define("__DATE__".into(), vec![make_date_token()], Vec::new());
        preprocessor.set_predefined_time("00:00:00");
        Ok(preprocessor)
    }
}
