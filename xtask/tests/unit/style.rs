use std::path::Path;

use super::{horizontal_signatures, padding, rustfmt};

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

#[test]
fn explicit_horizontal_signature_survives_the_full_pipeline() {

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let source = concat!(
        "pub(super) fn execute( store: &FileStore, path: &str, data: Option<&[u8]>, expected: Option<&str>, ) ",
        "-> Result<Value, FileError> {\n\n",
        "    Ok(json!({\"sha256\":store.write(path,data,expected)?}))\n\n}\n",
    );
    let formatted = rustfmt(source, root).unwrap();
    assert_ne!(source, formatted);
    let once = padding(&horizontal_signatures(source, &formatted).unwrap()).unwrap();
    let twice = padding(&horizontal_signatures(&once, &rustfmt(&once, root).unwrap()).unwrap()).unwrap();
    assert_eq!(source, once);
    assert_eq!(once, twice);

}

#[test]
fn horizontal_signature_restoration_leaves_other_signatures_and_literals_to_rustfmt() {

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let source = concat!(
        "fn first( value: &str, ) { println!(\"{}\", value); }\n",
        "fn ordinary(value : &str) {}\n",
        "fn last( value: &str, ) { let text = r#\"( stays unchanged )\"#; }\n",
        "fn 한글( value: &str, ) {}\n",
        "fn wide( first_parameter: &FileStore, second_parameter: &FileStore, third_parameter: &FileStore, ",
        "fourth_parameter: &FileStore, ) -> Result<Value, FileError> {}\n",
        "fn commented(\n    value: &str, // 이 주석은 parameter에 붙어 있어.\n) {}\n",
    );
    let formatted = rustfmt(source, root).unwrap();
    let restored = horizontal_signatures(source, &formatted).unwrap();
    assert!(restored.contains("fn first( value: &str, )"));
    assert!(restored.contains("fn ordinary(value: &str)"));
    assert!(restored.contains("fn last( value: &str, )"));
    assert!(restored.contains("fn 한글( value: &str, )"));
    assert!(restored.contains(concat!(
        "fn wide( first_parameter: &FileStore, second_parameter: &FileStore, third_parameter: &FileStore, ",
        "fourth_parameter: &FileStore, ) -> Result<Value, FileError>",
    )));
    assert!(restored.contains("value: &str, // 이 주석은 parameter에 붙어 있어.\n"));
    assert!(restored.contains("r#\"( stays unchanged )\"#"));
    let twice = horizontal_signatures(&restored, &rustfmt(&restored, root).unwrap()).unwrap();
    assert_eq!(restored, twice);

}
