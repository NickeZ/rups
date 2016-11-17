#[test]
fn it_works() {
    use std::process::Command;

    let output = Command::new("python")
        .arg("tests/connection_tests.py")
        .output()
        .unwrap_or_else(|e| { panic!("failed to execute process: {}", e) });

    let s = match String::from_utf8(output.stdout) {
        Ok(v) => v,
        Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
    };

    println!("result: {}", s); //must run "cargo test -- --nocapture" to see output
}
