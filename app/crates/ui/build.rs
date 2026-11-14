fn main() {
    slint_build::compile("ui/app.slint").expect("no se pudo compilar ui/app.slint");
    // Icono del ejecutable (Explorador, barra de tareas).
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        println!("cargo:rerun-if-changed=recursos/app.ico");
        winresource::WindowsResource::new().set_icon("recursos/app.ico").set("ProductName", "OpenSUA").set("FileDescription", "OpenSUA").compile().expect("no se pudo incrustar el icono");
    }
}
