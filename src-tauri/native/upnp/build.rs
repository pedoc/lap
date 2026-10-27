use std::{env, path::PathBuf};

pub fn build() {
    println!("cargo:rerun-if-changed=native/upnp");
    println!("cargo:rerun-if-changed=third_party/libupnp");
    println!("cargo:rerun-if-changed=third_party/pthreads4w");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let destination = cmake::Config::new(root.join("native/upnp"))
        .profile("Release")
        .define("CMAKE_POLICY_VERSION_MINIMUM", "3.13")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        .define("CMAKE_INSTALL_LIBDIR", "lib")
        .build();
    println!(
        "cargo:rustc-link-search=native={}",
        destination.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=lap_upnp");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=static=libupnps");
        println!("cargo:rustc-link-lib=static=ixmls");
        println!("cargo:rustc-link-lib=static=pthreadVC3");
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=iphlpapi");
    } else {
        println!("cargo:rustc-link-lib=static=upnp");
        println!("cargo:rustc-link-lib=static=ixml");
        println!("cargo:rustc-link-lib=pthread");
    }
}
