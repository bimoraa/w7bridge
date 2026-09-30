use std::path::Path;

use super::{padding, rustfmt};

#[test]
fn style_is_idempotent_and_preserves_raw_strings_imports_and_macros() {

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let source =
        "use std::{fs, io};\nfn sample() {\nlet text = r#\"{\n그대로 유지\n}\"#;\nprintln!(\"{}\", text);\n}\n";
    let once = padding(&rustfmt(source, root).unwrap()).unwrap();
    let twice = padding(&rustfmt(&once, root).unwrap()).unwrap();
    assert_eq!(once, twice);
    assert!(once.contains("sample() {\n\n"));
    assert!(once.contains("text);\n\n}"));
    assert!(once.contains("r#\"{\n그대로 유지\n}\"#"));
    assert!(once.starts_with("use std::{fs, io};\n"));
    assert_eq!(padding("fn empty() {}\n").unwrap(), "fn empty() {}\n");

}
