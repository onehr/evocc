"""Source limits count code correctly and never grandfather a regression."""

from contextlib import redirect_stdout
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "check-size.py"


class SourceSizeCase(unittest.TestCase):
    def setUp(self):
        self.assertTrue(SCRIPT.is_file(), "the source-size checker is not implemented")
        spec = importlib.util.spec_from_file_location("source_size_check", SCRIPT)
        self.checker = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = self.checker
        spec.loader.exec_module(self.checker)
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)

    def write(self, name, source):
        filename = self.root / name
        filename.parent.mkdir(parents=True, exist_ok=True)
        filename.write_text(source, encoding="utf-8")
        return filename

    def baseline(self, files=None, functions=None):
        return self.write("tools/size-baseline.json", json.dumps({
            "limits": {"file": 1500, "function": 150},
            "files": files or {}, "functions": functions or {},
        }, sort_keys=True) + "\n")

    def run_check(self, *arguments):
        output = io.StringIO()
        with redirect_stdout(output):
            result = self.checker.main(list(arguments), root=self.root)
        return result, output.getvalue()

    @staticmethod
    def rust_function(name, lines):
        return f"fn {name}() {{\n" + "\n" * (lines - 2) + "}\n"


class RustMeasurements(SourceSizeCase):
    def rust(self, source):
        return self.checker.functions_in_rust(source, "crates/demo")

    def test_attributes_and_doc_comments_belong_to_the_function(self):
        source = ('/// First line.\n#[allow(dead_code)]\n'
                  'pub fn sample() {\n    let text = "} fn false_one() {";\n}\n')
        functions = self.rust(source)
        self.assertEqual([(item.key, item.first, item.last, item.lines) for item in functions],
                         [("crates/demo::-::sample", 1, 5, 5)])

    def test_raw_strings_and_byte_strings_do_not_make_functions_or_braces(self):
        source = ('fn sample() {\n'
                  '    let a = r########"} fn phantom() { "########;\n'
                  '    let b = br###"{ fn another() {} }"###;\n'
                  '    let c = b"}\\\"{";\n}\n')
        functions = self.rust(source)
        self.assertEqual([(item.key, item.lines) for item in functions],
                         [("crates/demo::-::sample", 5)])

    def test_character_literals_and_lifetimes_are_distinct(self):
        source = ("fn borrow<'a>(text: &'a str) -> &'a str {\n"
                  "    let closing = '}';\n    let byte = b'{';\n"
                  "    let quote = '\\'';\n    let unicode = '\\u{7b}';\n"
                  "    'label: loop { break 'label; }\n    text\n}\n")
        functions = self.rust(source)
        self.assertEqual([(item.key, item.lines) for item in functions],
                         [("crates/demo::-::borrow", 8)])

    def test_nested_block_comments_do_not_end_a_function(self):
        source = ('fn real() {\n    /* } /* fn phantom() {} */ { */\n'
                  '    // } fn hidden() {\n}\n')
        functions = self.rust(source)
        self.assertEqual([(item.key, item.lines) for item in functions],
                         [("crates/demo::-::real", 4)])

    def test_nested_functions_count_alone_and_in_their_parent(self):
        source = 'fn outer() {\n    fn inner() {}\n    let closure = || { 7 };\n}\n'
        functions = self.rust(source)
        self.assertEqual([(item.key, item.lines) for item in functions],
                         [("crates/demo::-::outer", 4), ("crates/demo::-::inner", 1)])

    def test_trait_declarations_are_not_function_bodies(self):
        source = ('trait Sample {\n    fn declaration(&self);\n    fn body(&self) {}\n}\n'
                  "struct Holder<'a>(&'a str);\nimpl<'a> Holder<'a> {\n"
                  '    fn method(&self) {}\n}\nextern "C" { fn external(); }\n'
                  'type Callback = fn() -> ();\n')
        functions = self.rust(source)
        self.assertEqual([item.key for item in functions],
                         ["crates/demo::Sample::body", "crates/demo::Holder<'a>::method"])

    def test_impl_where_clauses_do_not_change_the_type_key(self):
        source = ('struct Container<T>(T);\n'
                  'impl<T> Container<T> where T: Copy {\n    fn read(&self) {}\n}\n')
        functions = self.rust(source)
        self.assertEqual([item.key for item in functions], ["crates/demo::Container<T>::read"])

    def test_multiline_attributes_and_qualified_signatures_keep_their_start(self):
        source = ('/** A method. */\n#[allow(\n    dead_code\n)]\n'
                  'pub(crate) async unsafe fn sample() {}\n')
        (function,) = self.rust(source)
        self.assertEqual((function.first, function.last), (1, 5))

    def test_const_generics_in_the_signature_are_not_the_body(self):
        source = ('fn sample<const N: usize>() -> [u8; { 1 + 1 }]\n'
                  'where [u8; N]: Copy {\n    [0; 2]\n}\n')
        (function,) = self.rust(source)
        self.assertEqual(function.lines, 4)

    def test_trait_implementations_use_the_self_type_and_nested_functions_are_free(self):
        source = ('impl<T> Sample<T> for Holder<T> where T: Copy {\n'
                  '    fn method() { fn local() {} }\n}\n')
        self.assertEqual([item.key for item in self.rust(source)],
                         ["crates/demo::Holder<T>::method", "crates/demo::-::local"])

    def test_inner_docs_and_function_types_do_not_attach_to_the_next_function(self):
        source = ('//! Module documentation.\ntype Callback = fn();\n'
                  'fn sample(value: impl Fn()) {}\n')
        (function,) = self.rust(source)
        self.assertEqual((function.key, function.first, function.lines),
                         ("crates/demo::-::sample", 3, 1))

    def test_unbounded_raw_hash_counts_and_raw_identifiers(self):
        hashes = "#" * 300
        source = f'fn r#type() {{ let value = r{hashes}" }} "{hashes}; }}\n'
        (function,) = self.rust(source)
        self.assertEqual((function.key, function.lines), ("crates/demo::-::r#type", 1))

    def test_braces_in_c_strings_and_byte_characters_do_not_end_the_body(self):
        source = ('fn sample() {\n    let text = c"} fn hidden() {}";\n'
                  "    let byte = b'\\x7b';\n}\n")
        self.assertEqual([item.lines for item in self.rust(source)], [4])

    def test_const_expressions_do_not_change_impl_header_nesting(self):
        source = ('impl<const N: usize> Sample<{ if 1 < 2 { 1 } else { 2 } }> '
                  'for Holder<N> where [u8; N]: Copy {\n    fn method() {}\n}\n')
        self.assertEqual([item.key for item in self.rust(source)],
                         ["crates/demo::Holder<N>::method"])

    def test_file_moves_keep_keys_and_repetitions_have_ordered_suffixes(self):
        self.write("crates/demo/src/a.rs", "fn same() {}\n")
        second = self.write("crates/demo/src/z.rs", "fn same() {}\n")
        first = self.checker.measure(self.root)
        self.assertEqual(first["functions"],
                         {"crates/demo::-::same": 1, "crates/demo::-::same#2": 1})
        second.rename(second.with_name("zz.rs"))
        self.assertEqual(self.checker.measure(self.root)["functions"], first["functions"])


class PythonMeasurements(SourceSizeCase):
    def test_decorators_async_functions_and_nested_functions(self):
        source = ('@decorate\nasync def outer():\n    @decorate\n'
                  '    def inner():\n        return 1\n    return inner\n')
        functions = self.checker.functions_in_python(source, "tools/demo.py")
        self.assertEqual([(item.key, item.first, item.last, item.lines) for item in functions],
                         [("tools/demo.py::-::outer", 1, 6, 6),
                          ("tools/demo.py::-::inner", 3, 5, 3)])

    def test_scope_and_unterminated_last_lines_are_counted(self):
        self.write("crates/demo/src/lib.rs", "fn code() {}")
        self.write("crates/demo/tests/outside.rs", "\n" * 2001)
        self.write("lean/outside.lean", "\n" * 2001)
        self.write("tools/example.py", "def example():\n    return 1")
        measured = self.checker.measure(self.root)
        self.assertEqual(measured["files"],
                         {"crates/demo/src/lib.rs": 1, "tools/example.py": 2})
        self.assertEqual(measured["functions"]["tools/example.py::-::example"], 2)

    def test_methods_in_different_classes_have_distinct_keys(self):
        source = 'class First:\n    def call(self): pass\nclass Second:\n    def call(self): pass\n'
        functions = self.checker.functions_in_python(source, "tools/demo.py")
        self.assertEqual([item.key for item in functions],
                         ["tools/demo.py::First::call", "tools/demo.py::Second::call"])

    def test_empty_files_and_crlf_have_physical_line_counts(self):
        self.write("tools/empty.py", "")
        self.write("crates/demo/src/lib.rs", "fn sample() {\r\n}\r\n")
        measured = self.checker.measure(self.root)
        self.assertEqual(measured["files"], {"crates/demo/src/lib.rs": 2, "tools/empty.py": 0})
        self.assertEqual(measured["functions"], {"crates/demo::-::sample": 2})


class RatchetUpdates(SourceSizeCase):
    def test_update_lowers_old_file_and_function_sizes(self):
        source = self.rust_function("large", 160) + "\n" * 1540
        self.write("crates/demo/src/lib.rs", source)
        baseline = self.baseline({"crates/demo/src/lib.rs": 1800},
                                 {"crates/demo::-::large": 180})
        result, _ = self.run_check("--update")
        self.assertEqual(result, 0)
        record = json.loads(baseline.read_text())
        self.assertEqual(record["files"], {"crates/demo/src/lib.rs": 1700})
        self.assertEqual(record["functions"], {"crates/demo::-::large": 160})
        self.assertEqual(self.run_check()[0], 0)

    def test_update_removes_missing_and_now_small_items(self):
        self.write("crates/demo/src/lib.rs", "fn smaller() {}\n")
        baseline = self.baseline({"crates/demo/src/lib.rs": 1800, "crates/demo/src/gone.rs": 1600},
                                 {"crates/demo::-::smaller": 170, "crates/demo::-::gone": 200})
        self.assertEqual(self.run_check("--update")[0], 0)
        record = json.loads(baseline.read_text())
        self.assertEqual(record["files"], {})
        self.assertEqual(record["functions"], {})

    def test_update_refuses_growth_and_leaves_the_file_unchanged(self):
        self.write("crates/demo/src/lib.rs", "\n" * 1601)
        baseline = self.baseline({"crates/demo/src/lib.rs": 1600})
        original = baseline.read_bytes()
        for arguments in [(), ("--update",)]:
            result, output = self.run_check(*arguments)
            self.assertEqual(result, 1)
            self.assertIn("crates/demo/src/lib.rs", output)
            self.assertIn("1601", output)
            self.assertIn("1600", output)
            self.assertEqual(baseline.read_bytes(), original)

    def test_update_never_adds_an_oversized_file_or_function(self):
        baseline = self.baseline()
        original = baseline.read_bytes()
        for source, expected in [("\n" * 1501, "1500"),
                                 (self.rust_function("new_large", 151), "150")]:
            self.write("crates/demo/src/lib.rs", source)
            result, output = self.run_check("--update")
            self.assertEqual(result, 1)
            self.assertIn(expected, output)
            self.assertEqual(baseline.read_bytes(), original)

    def test_report_includes_measured_items_and_totals(self):
        self.write("crates/demo/src/lib.rs", self.rust_function("large", 160))
        self.baseline(functions={"crates/demo::-::large": 160})
        result, output = self.run_check("--report")
        self.assertEqual(result, 0)
        self.assertIn("crates/demo/src/lib.rs", output)
        self.assertIn("crates/demo::-::large", output)
        self.assertIn("160", output)

    def test_update_is_atomic_when_a_reduction_also_contains_a_new_violation(self):
        self.write("crates/demo/src/lib.rs", "\n" * 1600)
        self.write("crates/demo/src/new.rs", self.rust_function("too_large", 151))
        baseline = self.baseline({"crates/demo/src/lib.rs": 1700})
        original = baseline.read_bytes()
        self.assertEqual(self.run_check("--update")[0], 1)
        self.assertEqual(baseline.read_bytes(), original)

    def test_update_does_not_edit_an_already_current_baseline(self):
        self.write("crates/demo/src/lib.rs", "\n" * 1600)
        baseline = self.baseline({"crates/demo/src/lib.rs": 1600})
        original = baseline.read_bytes()
        self.assertEqual(self.run_check("--update")[0], 0)
        self.assertEqual(baseline.read_bytes(), original)


class BaselineInitialization(SourceSizeCase):
    def git(self, *arguments):
        return subprocess.run(["git", "-C", str(self.root), *arguments],
                              capture_output=True, check=True, text=True)

    def commit_sources(self):
        self.git("init", "-q")
        self.git("add", "crates")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                 "commit", "-qm", "Add the fixture")

    def test_initial_update_uses_committed_sources_not_new_code(self):
        self.write("crates/demo/src/lib.rs", "\n" * 1600)
        self.commit_sources()
        self.write("crates/demo/src/new.rs", self.rust_function("new_large", 151))
        self.assertEqual(self.run_check("--update")[0], 1)
        self.assertFalse((self.root / "tools/size-baseline.json").exists())
        self.write("crates/demo/src/new.rs", "fn small() {}\n")
        self.assertEqual(self.run_check("--update")[0], 0)
        record = json.loads((self.root / "tools/size-baseline.json").read_text())
        self.assertEqual(record["files"], {"crates/demo/src/lib.rs": 1600})

    def test_initial_update_refuses_growth_in_committed_sources(self):
        self.write("crates/demo/src/lib.rs", "\n" * 1600)
        self.commit_sources()
        self.write("crates/demo/src/lib.rs", "\n" * 1601)
        self.assertEqual(self.run_check("--update")[0], 1)
        self.assertFalse((self.root / "tools/size-baseline.json").exists())

    def test_a_deleted_committed_baseline_cannot_be_reinitialized(self):
        self.write("crates/demo/src/lib.rs", "fn small() {}\n")
        self.commit_sources()
        baseline = self.baseline()
        self.git("add", "tools/size-baseline.json")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                 "commit", "-qm", "Add the baseline")
        baseline.unlink()
        self.assertEqual(self.run_check("--update")[0], 1)
        self.assertFalse(baseline.exists())


class GateWiring(SourceSizeCase):
    def test_both_gate_modes_stop_before_cargo_on_a_size_failure(self):
        gate = SCRIPT.with_name("gate-changed.sh").read_text()
        self.write("tools/gate-changed.sh", gate)
        self.write("tools/check-size.py", 'print("size limit rejected"); raise SystemExit(1)\n')
        for mode in ("--full-workspace", "HEAD"):
            with self.subTest(mode=mode):
                result = subprocess.run(["sh", "tools/gate-changed.sh", mode], cwd=self.root,
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 1)
                self.assertEqual(result.stdout, "size limit rejected\n")
                self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()
