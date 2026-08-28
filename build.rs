fn main() {
    #[cfg(windows)]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let icon_path = std::path::Path::new(&manifest_dir).join("assets").join("icon.ico");
        let icon_str = icon_path.to_str().unwrap().replace('\\', "/");

        let mut res = winres::WindowsResource::new();
        res.set_icon(&icon_str);
        res.set("ProductName", "DubRust");
        res.set("FileDescription", "DubRust — Dubbing and Voice-Over Studio");
        // The icon is cosmetic: never fail the whole build because of it.
        if let Err(err) = res.compile() {
            println!("cargo:warning=skipping Windows resources: {err}");
            return;
        }

        let out_dir = std::env::var("OUT_DIR").unwrap();
        let res_o = std::path::Path::new(&out_dir).join("resource.o");

        // Only the GNU toolchain needs resource.o passed to the linker by hand.
        // With MSVC winres emits its own link directives and resource.o does not
        // exist, so passing it would fail linking with LNK1181.
        if res_o.exists() {
            let res_o_str = res_o.to_str().unwrap().replace('\\', "/");
            println!("cargo:rustc-link-arg={res_o_str}");
        }
    }
}
