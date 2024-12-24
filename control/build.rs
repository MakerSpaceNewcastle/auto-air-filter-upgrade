use std::process::Command;

fn main() {
    set_git_version();
}

fn set_git_version() {
    let version = Command::new("git")
        .arg("describe")
        .arg("--always")
        .arg("--dirty=-modified")
        .output()
        .unwrap()
        .stdout;
    let version = String::from_utf8(version).unwrap();
    println!("cargo::rustc-env=VERSION={version}",);
}
