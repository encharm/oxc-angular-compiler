//! `ngcheck`: type-checks a project's Angular templates (see `PORTING.md`).

fn main() {
    std::process::exit(ng_check::ngp::cli::main());
}
