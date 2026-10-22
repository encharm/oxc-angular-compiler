//! The `ngcheck` command line (not a port): type-checks a project's Angular templates.
//!
//!   ngcheck -p <tsconfig.json> [--tsrs <path>] [--keep]
//!
//! 1. ng-analyze analyzes the project (the tsconfig's files and what they import).
//! 2. Each file with components gets its type-check file (`processor`), written to a sibling of the source root,
//!    `<root>/../.ngcheck-<root name>/`, at the same depths as the sources, so that the relative imports the blocks
//!    copy from the sources resolve, with a tsconfig whose `rootDirs` puts the two trees together.
//! 3. tsrs type-checks that program (`--noEmit`).
//! 4. Diagnostics in type-check files map back to the templates as ngtsc maps them (`crate::ngtsc`); the others are
//!    the program's own TypeScript errors. They print as ngc prints them: `<file>:<line>:<col> - error <code>:
//!    <message>`, paths relative to the working directory.
//!
//! Exit status: 0 with no errors, 1 with errors, 2 when it could not run.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ngtsc::diagnostics::{should_report_diagnostic, translate_diagnostic};
use crate::ngtsc::tcb_util::TcbFile;
use crate::typecheck::ops::codegen::SpanInfo;

/// Where a type-check function's span comments index into (`TcbTemplateSource`, or the component's file)
#[derive(Debug, Clone)]
pub enum TemplateSource {
    /// An external template: spans index into this file
    File(PathBuf),
    /// An inline template given by an expression: diagnostics are reported at this span of the component's file
    Expression { file: PathBuf, span: SpanInfo },
}

/// A diagnostic, located in a source file by JavaScript (UTF-16) string indices
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: Option<PathBuf>,
    pub start: u32,
    pub code: String,
    pub category: Category,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Error,
    Warning,
}

/// What generating one file's type-check block gives
#[derive(Debug, Default)]
pub struct GeneratedFile {
    /// The type-check file's text, or none when the file has nothing to check
    pub tcb: Option<String>,
    /// `TypeCheckId` → where its spans index into (absent: the component's own file)
    pub template_sources: Vec<(String, TemplateSource)>,
    /// Diagnostics found while generating (template parse errors, out-of-band, DOM schema)
    pub diagnostics: Vec<Diagnostic>,
}

struct Args {
    tsconfig: PathBuf,
    tsrs: Option<PathBuf>,
    keep: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let (mut tsconfig, mut tsrs, mut keep) = (None, None, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" | "--project" => tsconfig = args.next().map(PathBuf::from),
            "--tsrs" => tsrs = args.next().map(PathBuf::from),
            "--keep" => keep = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let tsconfig = tsconfig.ok_or("missing -p <tsconfig.json>")?;
    Ok(Args { tsconfig, tsrs, keep })
}

pub fn main() -> i32 {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("ngcheck: {message}");
            }
            eprintln!("usage: ngcheck -p <tsconfig.json> [--tsrs <path>] [--keep]");
            return 2;
        }
    };
    match run(&args) {
        Ok(errors) => i32::from(errors > 0),
        Err(message) => {
            eprintln!("ngcheck: {message}");
            2
        }
    }
}

fn run(args: &Args) -> Result<usize, String> {
    let tsconfig = std::fs::canonicalize(&args.tsconfig).map_err(|e| format!("{}: {e}", args.tsconfig.display()))?;
    let generated = generate_project(&tsconfig)?;

    // The type-check files, beside the sources
    let root = common_root(generated.iter().map(|(path, _)| path.as_path()))
        .unwrap_or_else(|| tsconfig.parent().unwrap().to_path_buf());
    let out = sibling_out_dir(&root);
    let _ = std::fs::remove_dir_all(&out);
    let mut sources_by_tcb: Vec<(PathBuf, PathBuf, GeneratedFile)> = vec![];
    let mut diagnostics: Vec<Diagnostic> = vec![];
    for (path, mut file) in generated {
        diagnostics.append(&mut file.diagnostics);
        let Some(tcb) = &file.tcb else { continue };
        let relative = path.strip_prefix(&root).map_err(|_| format!("{} is outside {}", path.display(), root.display()))?;
        let tcb_path = out.join(relative).with_extension("ngtypecheck.ts");
        std::fs::create_dir_all(tcb_path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&tcb_path, tcb).map_err(|e| e.to_string())?;
        sources_by_tcb.push((tcb_path, path, file));
    }
    let program_tsconfig = write_tsconfig(&tsconfig, &root, &out)?;

    // tsrs, and the mapping back
    let tsrs = args.tsrs.clone().map_or_else(find_tsrs, Ok)?;
    let output = Command::new(&tsrs)
        .args(["--noEmit", "--pretty", "false", "-p"])
        .arg(&program_tsconfig)
        .output()
        .map_err(|e| format!("{}: {e}", tsrs.display()))?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr);
    for d in parse_tsc_output(&text) {
        let Some((tcb_path, component, file)) =
            sources_by_tcb.iter().find(|(tcb_path, ..)| d.file.as_deref() == Some(tcb_path.as_path()))
        else {
            diagnostics.push(Diagnostic {
                file: d.file.clone(),
                start: utf16_offset_of(d.file.as_deref(), d.line, d.col),
                code: format!("TS{}", d.code),
                category: d.category,
                message: d.message,
            });
            continue;
        };
        let text = std::fs::read_to_string(tcb_path).map_err(|e| e.to_string())?;
        let start = byte_offset(&text, d.line, d.col);
        if !should_report_diagnostic(d.code, &text, start) {
            continue;
        }
        let Some(location) = translate_diagnostic(&TcbFile::new(text), start) else { continue };
        let (file, start) = match file.template_sources.iter().find(|(id, _)| *id == location.id) {
            Some((_, TemplateSource::File(template))) => (template.clone(), location.span.start),
            Some((_, TemplateSource::Expression { file, span })) => (file.clone(), span.start),
            None => (component.clone(), location.span.start),
        };
        diagnostics.push(Diagnostic {
            file: Some(file),
            start,
            code: format!("TS{}", d.code),
            category: d.category,
            message: d.message,
        });
    }
    if !args.keep {
        let _ = std::fs::remove_dir_all(&out);
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let mut errors = 0;
    for d in &diagnostics {
        let category = match d.category {
            Category::Error => {
                errors += 1;
                "error"
            }
            Category::Warning => "warning",
        };
        match &d.file {
            Some(file) => {
                let (line, col) = line_col(file, d.start);
                let shown = file.strip_prefix(&cwd).unwrap_or(file);
                println!("{}:{line}:{col} - {category} {}: {}", shown.display(), d.code, d.message);
            }
            None => println!("{category} {}: {}", d.code, d.message),
        }
    }
    println!("\n{errors} error{}", if errors == 1 { "" } else { "s" });
    Ok(errors)
}

/// Analyzes the project with ng-analyze and generates each file's type-check block (`processor`)
fn generate_project(tsconfig: &Path) -> Result<Vec<(PathBuf, GeneratedFile)>, String> {
    let (analyzer, files) = analyze(tsconfig)?;
    let mut generated = vec![];
    for result in &files {
        let path = PathBuf::from(&result.file_path);
        // the analysis' own diagnostics (unsupported decorators, unresolvable metadata, ...)
        let mut file = generate_file(&analyzer, result)?;
        for d in &result.diagnostics {
            let file_path = d.file_path.as_ref().map_or_else(|| path.clone(), PathBuf::from);
            // ng-analyze's spans are JavaScript string (UTF-16) indices already
            let start = d.span.as_ref().map_or(0, |span| span.start);
            file.diagnostics.push(Diagnostic {
                file: Some(file_path),
                start,
                code: format!("NG{}", d.code),
                category: if d.category == 1 { Category::Error } else { Category::Warning },
                message: d.message_text.clone(),
            });
        }
        generated.push((path, file));
    }
    Ok(generated)
}

/// ng-analyze's analysis of the project: every file it compiles, with its metadata
fn analyze(tsconfig: &Path) -> Result<(ng_analyze::Analyzer, Vec<ng_analyze::AnalysisResult>), String> {
    use futures::StreamExt;
    let options = ng_analyze::AnalyzerOptions {
        tsconfig_path: tsconfig.to_string_lossy().into_owned(),
        optimize: Some(true),
        ..Default::default()
    };
    let analyzer = ng_analyze::Analyzer::new_core(options)?;
    let mut chunks =
        analyzer.analyze_optimized_core(ng_analyze::compiler::analyzer::get_global_pool().clone())?;
    let mut files = vec![];
    while let Some(chunk) = futures::executor::block_on(chunks.next()) {
        files.extend(chunk?.files);
    }
    Ok((analyzer, files))
}

/// One file's type-check block and the diagnostics found making it (`processor`)
fn generate_file(
    _analyzer: &ng_analyze::Analyzer,
    _result: &ng_analyze::AnalysisResult,
) -> Result<GeneratedFile, String> {
    // wired to `crate::ngp::processor` once that port lands
    Ok(GeneratedFile::default())
}

/// tsrs from `@codecharm/tsrs` (its install links it to `bin/tsrs`) in the nearest `node_modules`, else `tsrs` on the
/// `PATH`
fn find_tsrs() -> Result<PathBuf, String> {
    let exe = if cfg!(windows) { "tsrs.exe" } else { "tsrs" };
    let mut dir = std::env::current_dir().ok();
    while let Some(d) = dir {
        let candidate = d.join("node_modules/@codecharm/tsrs/bin").join(exe);
        if candidate.exists() {
            return Ok(candidate);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Ok(PathBuf::from(exe))
}

/// The deepest directory containing every path
fn common_root<'p>(paths: impl Iterator<Item = &'p Path>) -> Option<PathBuf> {
    let mut root: Option<PathBuf> = None;
    for path in paths {
        let dir = path.parent()?;
        root = Some(match root {
            None => dir.to_path_buf(),
            Some(r) => r.ancestors().find(|a| dir.starts_with(a))?.to_path_buf(),
        });
    }
    root
}

fn sibling_out_dir(root: &Path) -> PathBuf {
    let name = root.file_name().map_or_else(|| "root".into(), |n| n.to_string_lossy().into_owned());
    root.parent().unwrap_or(root).join(format!(".ngcheck-{name}"))
}

/// The program's tsconfig, in `out`: the project's options with paths relative to `out`, its files, and the
/// type-check files; `rootDirs` lets the blocks' `./x.component` imports reach the sources in `root`
fn write_tsconfig(tsconfig: &Path, root: &Path, out: &Path) -> Result<PathBuf, String> {
    let dir = tsconfig.parent().unwrap();
    let relative = |p: &Path| path_relative(out, p).to_string_lossy().replace('\\', "/");
    let config = serde_json::json!({
        "extends": relative(tsconfig),
        "compilerOptions": {
            "noEmit": true,
            "rootDirs": [relative(root), "."],
            "sourceMap": false,
            "inlineSources": false,
        },
        // the project's own files come from the tsconfig it extends; add the type-check files
        "include": tsconfig_includes(tsconfig, dir, out)?,
    });
    let path = out.join("tsconfig.json");
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The project tsconfig's `include` patterns, rebased onto `out`, plus the type-check files (`extends` keeps
/// `files` and `exclude` relative to the file that sets them, but this file's own `include` replaces the project's)
fn tsconfig_includes(tsconfig: &Path, dir: &Path, out: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(tsconfig).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&strip_json_comments(&text))
        .map_err(|e| format!("{}: {e}", tsconfig.display()))?;
    let mut include: Vec<String> = json["include"]
        .as_array()
        .map(|patterns| {
            patterns
                .iter()
                .filter_map(|p| p.as_str())
                .map(|p| path_relative(out, &dir.join(p)).to_string_lossy().replace('\\', "/"))
                .collect()
        })
        .unwrap_or_default();
    include.push("./**/*.ngtypecheck.ts".into());
    Ok(include)
}

/// JSON with `//` and `/* */` comments (tsconfig files), without them
fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                out.extend(chars.next());
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut last = ' ';
            for c in chars.by_ref() {
                if last == '*' && c == '/' {
                    break;
                }
                last = c;
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `to` relative to the directory `from`
fn path_relative(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to_components).take_while(|(a, b)| a == b).count();
    let mut path = PathBuf::new();
    for _ in common..from.len() {
        path.push("..");
    }
    for c in &to_components[common..] {
        path.push(c);
    }
    path
}

/// A diagnostic as `tsc --pretty false` prints it
#[derive(Debug, Clone)]
struct TscDiagnostic {
    file: Option<PathBuf>,
    line: u32,
    col: u32,
    code: u32,
    category: Category,
    message: String,
}

/// `file(line,col): error TSn: message`, continuation lines indented; or `error TSn: message`
fn parse_tsc_output(text: &str) -> Vec<TscDiagnostic> {
    let mut out: Vec<TscDiagnostic> = vec![];
    for line in text.lines() {
        if let Some(d) = parse_tsc_line(line) {
            out.push(d);
        } else if line.starts_with(' ') && !out.is_empty() {
            let last = out.last_mut().unwrap();
            last.message.push('\n');
            last.message.push_str(line);
        }
    }
    out
}

fn parse_tsc_line(line: &str) -> Option<TscDiagnostic> {
    let (location, rest) = match line.find("): ") {
        Some(i) if line[..i].contains('(') => (Some(&line[..=i]), &line[i + 3..]),
        _ => (None, line),
    };
    let (category, rest) = if let Some(r) = rest.strip_prefix("error TS") {
        (Category::Error, r)
    } else if let Some(r) = rest.strip_prefix("warning TS") {
        (Category::Warning, r)
    } else {
        return None;
    };
    let (code, message) = rest.split_once(": ")?;
    let code = code.parse().ok()?;
    let (file, line_no, col) = match location {
        Some(loc) => {
            let open = loc.rfind('(')?;
            let (l, c) = loc[open + 1..loc.len() - 1].split_once(',')?;
            let file = PathBuf::from(&loc[..open]);
            let file = std::fs::canonicalize(&file).unwrap_or(file);
            (Some(file), l.parse().ok()?, c.parse().ok()?)
        }
        None => (None, 0, 0),
    };
    Some(TscDiagnostic { file, line: line_no, col, code, category, message: message.to_string() })
}

/// The byte offset of a 1-based line and 1-based UTF-16 column in `text`
fn byte_offset(text: &str, line: u32, col: u32) -> usize {
    let line_start: usize =
        text.split_inclusive('\n').take(line.saturating_sub(1) as usize).map(str::len).sum();
    let mut units = 0;
    for (i, c) in text[line_start..].char_indices() {
        if units >= col.saturating_sub(1) as usize || c == '\n' {
            return line_start + i;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// The UTF-16 index of a 1-based line and column in a file
fn utf16_offset_of(file: Option<&Path>, line: u32, col: u32) -> u32 {
    let Some(text) = file.and_then(|f| std::fs::read_to_string(f).ok()) else { return 0 };
    let byte = byte_offset(&text, line, col);
    text[..byte].encode_utf16().count() as u32
}

/// The 1-based line and UTF-16 column of a UTF-16 index in a file
fn line_col(file: &Path, start: u32) -> (u32, u32) {
    let Ok(text) = std::fs::read_to_string(file) else { return (0, 0) };
    let (mut line, mut col, mut units) = (1, 1, 0u32);
    for c in text.chars() {
        if units >= start {
            break;
        }
        units += c.len_utf16() as u32;
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += c.len_utf16() as u32;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tsc_lines() {
        let d = parse_tsc_line("src/a.ts(12,5): error TS2339: Property 'x' does not exist on type 'Y'.").unwrap();
        assert_eq!((d.line, d.col, d.code, d.category), (12, 5, 2339, Category::Error));
        assert_eq!(d.message, "Property 'x' does not exist on type 'Y'.");
        let d = parse_tsc_line("error TS5023: Unknown compiler option 'x'.").unwrap();
        assert!(d.file.is_none());
        assert!(parse_tsc_line("Found 3 errors.").is_none());
    }

    #[test]
    fn byte_offsets_count_utf16_columns() {
        let text = "ab\nłx = 1\n";
        assert_eq!(byte_offset(text, 2, 1), 3);
        // `ł` is two bytes and one UTF-16 unit
        assert_eq!(byte_offset(text, 2, 2), 5);
    }

    #[test]
    fn relative_paths() {
        assert_eq!(path_relative(Path::new("/a/.ngcheck-src"), Path::new("/a/src/tsconfig.json")), PathBuf::from("../src/tsconfig.json"));
        assert_eq!(common_root([Path::new("/a/src/x/y.ts"), Path::new("/a/src/z.ts")].into_iter()), Some(PathBuf::from("/a/src")));
    }

    #[test]
    fn json_comments() {
        assert_eq!(strip_json_comments("{\"a\": \"//x\", // c\n/* d */\"b\": 1}"), "{\"a\": \"//x\", \n\"b\": 1}");
    }
}
