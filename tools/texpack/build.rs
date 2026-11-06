// The ISPC texture encoder (intel_tex_2) is C++: link its runtime.
fn main() {
    println!("cargo:rustc-link-lib=stdc++");
}
