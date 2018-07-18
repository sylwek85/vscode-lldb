extern crate cpp_build;

fn main() {
    cpp_build::Config::new()
        .include("/usr/lib/llvm-6.0/include")
        .build("src/lldb.rs");
    println!("cargo:rustc-link-search={}", "/usr/lib/llvm-6.0/lib");
    println!("cargo:rustc-link-lib={}", "lldb-6.0");
}
