// Embeds the application icon (assets/app/icon.ico at the repository root) into the Windows executables, where
// Explorer, the taskbar and shortcuts show it.
fn main() {
    println!("cargo:rerun-if-changed=../../assets/app/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/app/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=application icon not embedded: {e}");
        }
    }
}
