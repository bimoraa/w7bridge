/*! rustfmt와 AST 기반 body padding을 하나의 pipeline으로 적용한다. */

use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

use proc_macro2::{LineColumn, Span, extra::DelimSpan};
use syn::{
    Block, ExprMatch, ExprStruct, FieldsNamed, ItemEnum, ItemImpl, ItemMod, ItemTrait, Signature,
    spanned::Spanned,
    visit::{self, Visit},
};

#[derive(Default)]
struct Boundaries {

    after: BTreeSet<usize>,
    before: BTreeSet<usize>,

}

impl Boundaries {

    fn add(&mut self, span: DelimSpan) {

        let open = span.open().start().line;
        let close = span.close().start().line;

        if close > open + 1 {

            self.after.insert(open);
            self.before.insert(close);

        }

    }

}

impl<'ast> Visit<'ast> for Boundaries {

    fn visit_block(&mut self, node: &'ast Block) {

        if !node.stmts.is_empty() {

            self.add(node.brace_token.span);

        }
        visit::visit_block(self, node);

    }

    fn visit_fields_named(&mut self, node: &'ast FieldsNamed) {

        if !node.named.is_empty() {

            self.add(node.brace_token.span);

        }
        visit::visit_fields_named(self, node);

    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {

        if !node.variants.is_empty() {

            self.add(node.brace_token.span);

        }
        visit::visit_item_enum(self, node);

    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {

        if !node.items.is_empty() {

            self.add(node.brace_token.span);

        }
        visit::visit_item_impl(self, node);

    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {

        if !node.items.is_empty() {

            self.add(node.brace_token.span);

        }
        visit::visit_item_trait(self, node);

    }

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {

        if let Some((brace, items)) = &node.content
            && !items.is_empty()
        {

            self.add(brace.span);

        }

        visit::visit_item_mod(self, node);

    }

    fn visit_expr_struct(&mut self, node: &'ast ExprStruct) {

        if !node.fields.is_empty() || node.rest.is_some() {

            self.add(node.brace_token.span);

        }
        visit::visit_expr_struct(self, node);

    }

    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {

        if !node.arms.is_empty() {

            self.add(node.brace_token.span);

        }
        visit::visit_expr_match(self, node);

    }

}

fn padding(source: &str) -> Result<String, syn::Error> {

    let mut boundaries = Boundaries::default();
    boundaries.visit_file(&syn::parse_file(source)?);
    let lines: Vec<_> = source.lines().collect();
    let mut result = String::new();

    for (index, line) in lines.iter().enumerate() {

        if boundaries.before.contains(&(index + 1)) && !result.ends_with("\n\n") {

            result.push('\n');

        }

        result.push_str(line);
        result.push('\n');

        if boundaries.after.contains(&(index + 1)) && lines.get(index + 1).is_some_and(|next| !next.trim().is_empty()) {

            result.push('\n');

        }

    }

    Ok(result)

}

fn rustfmt(source: &str, root: &Path) -> Result<String, String> {

    // stdin으로 읽혀서 파일 경로 header가 AST 입력에 섞이지 않게 해.
    let mut child = Command::new("rustfmt")
        .args(["--emit", "stdout", "--config-path"])
        .arg(root.join("rustfmt.toml"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|_| "rustfmt를 실행할 수 없습니다")?;
    let mut input = child.stdin.take().ok_or("rustfmt 입력을 연결할 수 없습니다")?;
    input.write_all(source.as_bytes()).map_err(|_| "rustfmt에 소스를 전달할 수 없습니다")?;
    drop(input);
    let output = child.wait_with_output().map_err(|_| "rustfmt 종료를 확인할 수 없습니다")?;

    if !output.status.success() {

        return Err("rustfmt 검사가 실패했습니다".into());

    }

    String::from_utf8(output.stdout).map_err(|_| "rustfmt 출력이 UTF-8이 아닙니다".into())

}

#[derive(Default)]
struct Signatures {

    spans: Vec<(Span, DelimSpan)>,

}

impl<'ast> Visit<'ast> for Signatures {

    fn visit_signature( &mut self, node: &'ast Signature, ) {

        self.spans.push((node.span(), node.paren_token.span));
        visit::visit_signature(self, node);

    }

}

fn source_offset( source: &str, position: LineColumn, ) -> usize {

    let mut lines = source.split_inclusive('\n');
    let start = lines.by_ref().take(position.line - 1).map(str::len).sum::<usize>();
    let line = lines.next().unwrap_or_default();

    // span의 문자 열을 UTF-8 byte 위치로 바꿔서 한글 identifier도 보존해.
    start + line.char_indices().nth(position.column).map_or(line.len(), |(index, _)| index)

}

fn horizontal_signatures( original: &str, formatted: &str, ) -> Result<String, syn::Error> {

    let mut before = Signatures::default();
    let mut after = Signatures::default();
    before.visit_file(&syn::parse_file(original)?);
    after.visit_file(&syn::parse_file(formatted)?);
    let mut result = formatted.to_owned();

    // 명시적으로 가로로 쓴 signature만 복원해. body와 나머지 구문은 rustfmt가 맡아.
    for ((old, params), (new, _)) in before.spans.iter().zip(&after.spans).rev() {

        if old.start().line != old.end().line {

            continue;

        }

        let old_start = source_offset(original, old.start());
        let old_end = source_offset(original, old.end());
        let signature = &original[old_start..old_end];
        let params_start = source_offset(original, params.open().end());
        let params_end = source_offset(original, params.close().start());
        let parameters = &original[params_start..params_end];

        if !parameters.starts_with(' ') || !parameters.ends_with(' ') {

            continue;

        }

        let new_start = source_offset(formatted, new.start());
        let new_end = source_offset(formatted, new.end());
        result.replace_range(new_start..new_end, signature);

    }

    Ok(result)

}

fn files(directory: &Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {

    for entry in fs::read_dir(directory)? {

        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;

        if kind.is_dir() {

            files(&path, paths)?;

        } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "rs") {

            paths.push(path);

        }

    }

    Ok(())

}

fn run() -> Result<bool, String> {

    let args: Vec<_> = std::env::args().skip(1).collect();
    let check = match args.as_slice() {

        [] => false,
        [arg] if arg == "--check" => true,
        _ => return Err("사용법: cargo run --locked -p xtask -- [--check]".into()),

    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().ok_or("프로젝트 루트를 찾을 수 없습니다")?;
    let mut paths = Vec::new();

    for directory in ["src", "tests", "xtask/src", "xtask/tests"] {

        files(&root.join(directory), &mut paths).map_err(|_| "Rust 파일 목록을 읽을 수 없습니다")?;

    }

    let desktop_source = root.join("frontend/src-tauri/src");

    if desktop_source.is_dir() {

        files(&desktop_source, &mut paths).map_err(|_| "desktop Rust 파일 목록을 읽을 수 없습니다")?;

    }

    let desktop_build = root.join("frontend/src-tauri/build.rs");

    if desktop_build.is_file() {

        paths.push(desktop_build);

    }

    paths.sort();
    let mut changed = false;

    for path in paths {

        let original = fs::read_to_string(&path).map_err(|_| "Rust 파일을 읽을 수 없습니다")?;
        let formatted = horizontal_signatures(&original, &rustfmt(&original, root)?)
            .and_then(|source| padding(&source))
            .map_err(|_| format!("Rust AST를 해석할 수 없습니다: {}", path.display()))?;

        if original != formatted {

            changed = true;

            if check {

                eprintln!("형식 수정 필요: {}", path.display());

            } else {

                fs::write(&path, formatted).map_err(|_| "Rust 파일을 저장할 수 없습니다")?;

            }

        }

    }

    Ok(check && changed)

}

fn main() -> ExitCode {

    match run() {

        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::FAILURE,
        Err(message) => {

            eprintln!("오류: {message}");
            ExitCode::FAILURE

        }

    }

}

#[cfg(test)]
#[path = "../tests/unit/style.rs"]
mod tests;
