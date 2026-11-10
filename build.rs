//! The commit the program is built from, for people: a network game's
//! guests see it beside a difference (`git describe --always --dirty`,
//! `unknown` built without git); and on Windows the program's icon.
fn main() {
    let commit = std::process::Command::new("git")
        .args(["describe", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=VOXCADE_COMMIT={commit}");
    for path in [".git/HEAD", ".git/index", ".git/refs"] {
        println!("cargo:rerun-if-changed={path}");
    }
    // On Windows the program carries its icon (dist/voxcade.ico,
    // tools/make_icon.py). If the resource compiler is missing, the build
    // goes on without it.
    println!("cargo:rerun-if-changed=dist/voxcade.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("dist/voxcade.ico");
        if let Err(e) = resource.compile() {
            println!("cargo:warning=no icon in the program: {e}");
        }
    }
}
