use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn activation_addresses_lower_and_have_stable_nondereferenceable_identity() {
    for (source, diagnostic) in [
        ("int main(void){return __builtin_return_address(0)!=__builtin_return_address(0) || __builtin_frame_address(0)!=__builtin_frame_address(0);}", None),
        ("unsigned long f(void){return (unsigned long)__builtin_frame_address(0);} int main(void){return f()==f();}", None),
        ("int main(void){return *(int *)__builtin_return_address(0);}", Some("E3013")),
        ("int main(void){return *(int *)__builtin_frame_address(0);}", Some("E3013")),
        ("int unused(void){return __builtin_frame_address(1)==0;}int main(void){if(0)return unused();return 0;}", None),
        ("int main(void){return __builtin_frame_address(0x100000000UL)==0 || __builtin_return_address(0x100000000UL)==0;}", None),
    ] {
        let arena = Arena::default();
        let file = FileId::new(45020);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let text = evocc_evoir::render_module(&module);
        assert_eq!(evocc_evoir::render_module(&evocc_evoir::parse_section9_module(text.as_bytes()).unwrap()), text);
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let args = main.params.iter().map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0)).collect::<Vec<_>>();
        let ir = evocc_evoir::interpret_module(&module, "main", &args, evocc_evoir::InterpreterLimits::new(10000));
        let symbol = unit.iter().find_map(|d| match d { CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol), _ => None }).unwrap();
        let chir = interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(10000));
        if let Some(code) = diagnostic {
            assert_eq!(ir.unwrap_err().code(), code, "{source}");
            assert_eq!(chir.unwrap_err().code(), code, "{source}");
        } else {
            assert_eq!(ir.unwrap().return_values[0].raw_bits, 0, "{source}");
            assert_eq!(chir.unwrap().return_value.unwrap().raw_bits, 0, "{source}");
        }
    }
}

#[test]
fn ancestor_frames_lower_but_require_native_execution() {
    for (argument, depth) in [("1", 1), ("2", 2), ("16", 16), ("0x100000001UL", 1)] {
        let source = format!("int main(void){{return (__builtin_frame_address({argument}))==0;}}");
        let arena = Arena::default();
        let file = FileId::new(45021);
        let mut lexer = Lexer::new(file, &source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, &source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .expect("ancestor frame has native lowering");
        let text = evocc_evoir::render_module(&module);
        assert!(
            text.contains(&format!("frame_address depth={depth} : ptr<generic>")),
            "{text}"
        );
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let args = main
            .params
            .iter()
            .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
            .collect::<Vec<_>>();
        assert_eq!(
            evocc_evoir::interpret_module(
                &module,
                "main",
                &args,
                evocc_evoir::InterpreterLimits::new(1000)
            )
            .unwrap_err()
            .code(),
            "E3031"
        );
        let symbol = unit
            .symbols()
            .resolve(unit.symbols().file_scope(), "main")
            .unwrap()
            .unwrap();
        assert_eq!(
            interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(1000))
                .unwrap_err()
                .code(),
            "E3031"
        );
    }
}
