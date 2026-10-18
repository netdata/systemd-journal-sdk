fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if matches!(target.as_str(), "linux" | "macos" | "freebsd") {
        println!("cargo:rerun-if-changed=native_unit_pattern.c");
        println!("cargo:rerun-if-changed=../../../../go/cmd/journalctl/unit_pattern_compat.h");
        cc::Build::new()
            .file("native_unit_pattern.c")
            .compile("journal_unit_pattern");
    }
    pkg_config::Config::new()
        .atleast_version("10.42")
        .probe("libpcre2-8")
        .expect("journalctl requires PCRE2-8 development files; the SDK has no PCRE2 dependency");
}
