fn main() {
    println!("cargo:rerun-if-changed=src/app.slint");
    println!("cargo:rerun-if-changed=src/models.slint");
    println!("cargo:rerun-if-changed=src/screens");
    println!("cargo:rerun-if-changed=src/components");
    slint_build::compile("src/app.slint").expect("slint compile failed");
}
