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
        res.compile().unwrap();

        let out_dir = std::env::var("OUT_DIR").unwrap();
        let res_o = std::path::Path::new(&out_dir).join("resource.o");
        let res_o_str = res_o.to_str().unwrap().replace('\\', "/");

        // Crucial for GNU linker: pass resource.o directly to link arguments
        println!("cargo:rustc-link-arg={res_o_str}");
    }
}
