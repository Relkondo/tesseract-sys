extern crate bindgen;

use std::env;
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;
use std::io::Write;
#[cfg(windows)]
use vcpkg;

const TESSERACT_VERSION: &str = "5.3.4";
const LIBS_PATH: &str = "resources/libs/";

#[cfg(windows)]
fn find_tesseract_system_lib() -> Vec<String> {
    println!("cargo:rerun-if-env-changed=TESSERACT_INCLUDE_PATHS");
    println!("cargo:rerun-if-env-changed=TESSERACT_LINK_PATHS");
    println!("cargo:rerun-if-env-changed=TESSERACT_LINK_LIBS");

    let vcpkg = || {
        let lib = vcpkg::Config::new().find_package("tesseract").unwrap();

        vec![lib
            .include_paths
            .iter()
            .map(|x| x.to_string_lossy())
            .collect::<String>()]
    };

    let include_paths = env::var("TESSERACT_INCLUDE_PATHS").ok();
    let include_paths = include_paths.as_deref().map(|x| x.split(','));
    let link_paths = env::var("TESSERACT_LINK_PATHS").ok();
    let link_paths = link_paths.as_deref().map(|x| x.split(','));
    let link_libs = env::var("TESSERACT_LINK_LIBS").ok();
    let link_libs = link_libs.as_deref().map(|x| x.split(','));
    if let (Some(include_paths), Some(link_paths), Some(link_libs)) =
        (include_paths, link_paths, link_libs)
    {
        for link_path in link_paths {
            println!("cargo:rustc-link-search={}", link_path)
        }

        for link_lib in link_libs {
            println!("cargo:rustc-link-lib={}", link_lib)
        }

        include_paths.map(|x| x.to_string()).collect::<Vec<_>>()
    } else {
        vcpkg()
    }
}

// we sometimes need additional search paths, which we get using pkg-config
// we can use tesseract installed anywhere on Linux.
// if you change install path(--prefix) to `configure` script.
// set `export PKG_CONFIG_PATH=/path-to-lib/pkgconfig` before.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "freebsd"))]
fn find_tesseract_system_lib() -> Vec<String> {
    let pk = pkg_config::Config::new()
        .atleast_version("4.1")
        .probe("tesseract")
        .unwrap();
    // Tell cargo to tell rustc to link the system proj shared library.
    println!("cargo:rustc-link-search=native={:?}", pk.link_paths[0]);
    println!("cargo:rustc-link-lib=tesseract");

    let mut include_paths = pk.include_paths.clone();
    include_paths
        .iter_mut()
        .map(|x| {
            if !x.ends_with("include") {
                x.pop();
            }
            x
        })
        .map(|x| x.to_string_lossy())
        .map(|x| x.to_string())
        .collect::<Vec<String>>()
}

fn reveal_location() -> std::io::Result<()> {
    // Execute `pwd` command
    let output = Command::new("pwd")
        .output()
        .expect("Failed to execute command");

    // Convert the output to a String
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Get the home directory path
    let home_dir = env::var("HOME").expect("HOME not set");

    // Create a file path in the home directory
    let file_path = format!("{}/location_revealed.txt", home_dir);

    // Open a file in write mode
    let mut file = File::create(file_path)?;

    // Write the command output to the file
    writeln!(file, "{}", path)?;

    Ok(())
}

fn find_bundled_tesseract_lib() -> Vec<String> {
    reveal_location().expect("Failed to reveal location");
    let mut tesseract_dir = LIBS_PATH.to_string();
    tesseract_dir.push_str("tesseract/");
    tesseract_dir.push_str(TESSERACT_VERSION);
    let mut tesseract_lib_dir = tesseract_dir.clone();
    tesseract_lib_dir.push_str("/lib");
    let mut tesseract_include_dir = tesseract_dir.clone();
    tesseract_include_dir.push_str("/include");

    println!("cargo:rustc-link-search=native={}", tesseract_lib_dir);
    println!("cargo:rustc-link-lib=tesseract");

    vec![tesseract_include_dir]
}

#[cfg(all(
    not(windows),
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "freebsd")
))]
fn find_tesseract_system_lib() -> Vec<String> {
    println!("cargo:rustc-link-lib=tesseract");
    vec![]
}

fn capi_bindings(clang_extra_include: &[String]) -> bindgen::Bindings {
    let mut capi_bindings = bindgen::Builder::default()
        .header("wrapper_capi.h")
        .allowlist_function("^Tess.*")
        .blocklist_type("Boxa")
        .blocklist_type("Pix")
        .blocklist_type("Pixa")
        .blocklist_type("_IO_FILE")
        .blocklist_type("_IO_codecvt")
        .blocklist_type("_IO_marker")
        .blocklist_type("_IO_wide_data");

    for inc in clang_extra_include {
        capi_bindings = capi_bindings.clang_arg(format!("-I{}", *inc));
    }

    capi_bindings
        .generate()
        .expect("Unable to generate capi bindings")
}

#[cfg(not(target_os = "macos"))]
fn public_types_bindings(clang_extra_include: &[String]) -> String {
    let mut public_types_bindings = bindgen::Builder::default()
        .header("wrapper_public_types.hpp")
        .rustified_enum("tesseract::OcrEngineMode")
        .rustified_enum("tesseract::Orientation")
        .rustified_enum("tesseract::PageIteratorLevel")
        .rustified_enum("tesseract::PageSegMode")
        .rustified_enum("tesseract::ParagraphJustification")
        .rustified_enum("tesseract::PolyBlockType")
        .rustified_enum("tesseract::TextlineOrder")
        .rustified_enum("tesseract::WritingDirection")
        .blocklist_item("^kPolyBlockNames")
        .blocklist_item("^tesseract::kPolyBlockNames");

    for inc in clang_extra_include {
        public_types_bindings = public_types_bindings.clang_arg(format!("-I{}", *inc));
    }

    public_types_bindings
        .generate()
        .expect("Unable to generate public types bindings")
        .to_string()
        .replace("tesseract_", "")
}

// MacOS clang is incompatible with Bindgen and constexpr
// https://github.com/rust-lang/rust-bindgen/issues/1948
// Hardcode the constants rather than reading them dynamically
#[cfg(target_os = "macos")]
fn public_types_bindings(_clang_extra_include: &[String]) -> &'static str {
    include_str!("src/public_types_bindings_mac.rs")
}

fn main() {
    // Tell cargo to tell rustc to link the system tesseract
    // and leptonica shared libraries.
    let clang_extra_include = find_bundled_tesseract_lib();

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    capi_bindings(&clang_extra_include)
        .write_to_file(out_path.join("capi_bindings.rs"))
        .expect("Couldn't write capi bindings!");
    fs::write(
        out_path.join("public_types_bindings.rs"),
        public_types_bindings(&clang_extra_include),
    )
    .expect("Couldn't write public types bindings!");
}
