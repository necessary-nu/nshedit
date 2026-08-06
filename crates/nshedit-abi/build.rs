//! Stamps a SONAME onto the shared library.
//!
//! A shared object's SONAME is copied verbatim into every program that links
//! it, as `DT_NEEDED`, and the loader then searches for a file with exactly
//! that name. Without one the linker falls back to recording the path it was
//! handed, so the binary breaks the moment the library moves — and no compat
//! symlink can help, because nothing is looking for one.
//!
//! Cargo names the artifact `libnshedit.so`, which is a *link* name, not a
//! runtime one. `packaging/install.sh` lays out the real chain:
//!
//! ```text
//! libnshedit.so.0.0.0     the object
//! libnshedit.so.0    ->   the object      the SONAME, what DT_NEEDED records
//! libnshedit.so      ->   libnshedit.so.0 the -lnshedit link name
//! libedit.so.0       ->   libnshedit.so.0 libedit's own soname
//! libedit.so.2       ->   libnshedit.so.0 Debian's patched soname
//! ```
//!
//! See `abi-soname` in `plan/main.styx` for why the name is ours rather than
//! libedit's, and why `libreadline.so.8` is not in that list.
//!
//! # This is not a probe
//!
//! `plan/decisions/no-c-ffi.md` forbids a `build.rs` that hunts for a library
//! — no `pkg-config`, no path search, no compiling a test program to see what
//! links. This one emits a constant string decided at author time and reads
//! nothing but two Cargo environment variables. It cannot fail, and it cannot
//! make the build depend on what happens to be installed.

fn main() {
    // Only the cdylib gets this. `rustc-link-arg-cdylib` is scoped to that
    // artifact, so the staticlib, the tests and any example link unchanged —
    // a SONAME on an executable is meaningless at best.
    println!("cargo::rerun-if-changed=build.rs");

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match os.as_str() {
        // Mach-O calls it the install name and wants the full path a consumer
        // will find it under, so `@rpath` defers that to the consumer's own
        // rpath rather than baking in a prefix this crate cannot know.
        "macos" | "ios" | "tvos" | "watchos" | "visionos" => {
            println!("cargo::rustc-link-arg-cdylib=-Wl,-install_name,@rpath/libnshedit.0.dylib");
        }
        // Everything else we target is ELF, where every linker in use spells
        // this `-soname`.
        "windows" => {}
        _ => {
            println!("cargo::rustc-link-arg-cdylib=-Wl,-soname,libnshedit.so.0");
        }
    }
}
