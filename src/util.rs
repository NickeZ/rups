// Convert a vector of bytes to a string with hex notation.
#![allow(dead_code)]
pub fn to_hex_string(bytes: Vec<u8>) -> String {
    let strs: Vec<String> = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    strs.join(" ")
}
