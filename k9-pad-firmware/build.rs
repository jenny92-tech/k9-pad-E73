// INPUT:  cc, const_gen, xz2, memory.x, vial.json, src/wououi/csrc/*.c
// OUTPUT: libwououi.a, config_generated.rs, linker configuration
// POS:    Cargo 构建脚本：交叉编译 WouoUI C 库 + 生成 vial 配置
//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.
//!
//! The build script also sets the linker flags to tell it which link script to use.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::{env, fs};

use const_gen::*;
use xz2::read::XzEncoder;

fn main() {
    // Generate vial config at the root of project
    println!("cargo:rerun-if-changed=vial.json");
    generate_vial_config();

    // Compile WouoUI C library
    compile_wououi();

    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying `memory.x`
    // here, we ensure the build script is only re-run when
    // `memory.x` is changed.
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rerun-if-changed=keyboard.toml");

    // Specify linker arguments.

    // `--nmagic` is required if memory section addresses are not aligned to 0x10000,
    // for example the FLASH and RAM sections in your `memory.x`.
    // See https://github.com/rust-embedded/cortex-m-quickstart/pull/95
    println!("cargo:rustc-link-arg=--nmagic");

    // Set the linker script to the one provided by cortex-m-rt.
    println!("cargo:rustc-link-arg=-Tlink.x");

    // Set the extra linker script from defmt
    println!("cargo:rustc-link-arg=-Tdefmt.x");

    // Use flip-link overflow check: https://github.com/knurling-rs/flip-link
    println!("cargo:rustc-linker=flip-link");
}

fn compile_wououi() {
    let target = env::var("TARGET").unwrap();

    // Only compile for ARM targets
    if !target.contains("thumbv7em") {
        return;
    }

    println!("cargo:rerun-if-changed=src/wououi/csrc/");

    let mut build = cc::Build::new();

    // Configure for ARM Cortex-M4 bare-metal
    build
        .compiler("arm-none-eabi-gcc")
        .target("thumbv7em-none-eabihf")
        .opt_level(2)
        .flag("-mcpu=cortex-m4")
        .flag("-mthumb")
        .flag("-mfloat-abi=hard")
        .flag("-mfpu=fpv4-sp-d16")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .flag("-fno-exceptions")
        .flag("-fno-unwind-tables")
        .flag("-fno-asynchronous-unwind-tables")
        .flag("-ffreestanding")
        .flag("-nostdinc")
        .flag("-w") // Suppress warnings from third-party C library
        .define("WOUOUI_EMBEDDED", None)
        .include("src/wououi/csrc")
        .file("src/wououi/csrc/WouoUI.c")
        .file("src/wououi/csrc/WouoUI_anim.c")
        .file("src/wououi/csrc/WouoUI_graph.c")
        .file("src/wououi/csrc/WouoUI_page.c")
        .file("src/wououi/csrc/WouoUI_win.c")
        .file("src/wououi/csrc/WouoUI_msg.c")
        .file("src/wououi/csrc/WouoUI_font.c")
        .file("src/wououi/csrc/WouoUI_port.c")
        .file("src/wououi/csrc/WouoUI_k9pad.c");

    build.compile("wououi");

    // cc-rs compile() should handle link-lib automatically, but we also
    // explicitly emit for clarity
    let out_dir = env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search=native={}", out_dir);

    // Link the library with whole-archive to ensure all symbols are included
    // rust-lld uses different syntax than GNU ld
    println!("cargo:rustc-link-arg=--whole-archive");
    println!("cargo:rustc-link-arg={}/libwououi.a", out_dir);
    println!("cargo:rustc-link-arg=--no-whole-archive");
}

fn generate_vial_config() {
    // Generated vial config file
    let out_file = Path::new(&env::var_os("OUT_DIR").unwrap()).join("config_generated.rs");

    let p = Path::new("vial.json");
    let mut content = String::new();
    match File::open(p) {
        Ok(mut file) => {
            file.read_to_string(&mut content).expect("Cannot read vial.json");
        }
        Err(e) => println!("Cannot find vial.json {:?}: {}", p, e),
    };

    let vial_cfg = json::stringify(json::parse(&content).unwrap());
    let mut keyboard_def_compressed: Vec<u8> = Vec::new();
    XzEncoder::new(vial_cfg.as_bytes(), 6)
        .read_to_end(&mut keyboard_def_compressed)
        .unwrap();

    let keyboard_id: Vec<u8> = vec![0xB9, 0xBC, 0x09, 0xB2, 0x9D, 0x37, 0x4C, 0xEA];
    let const_declarations = [
        const_declaration!(pub VIAL_KEYBOARD_DEF = keyboard_def_compressed),
        const_declaration!(pub VIAL_KEYBOARD_ID = keyboard_id),
    ]
    .map(|s| "#[allow(clippy::redundant_static_lifetimes)]\n".to_owned() + s.as_str())
    .join("\n");
    fs::write(out_file, const_declarations).unwrap();
}
