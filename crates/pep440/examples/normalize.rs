//! Reads one version per line on stdin, prints its normal form or `<INVALID>`.
//! Used to differentially test against Python's `packaging`.
use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    for line in input.lines() {
        match line.parse::<pep440::Version>() {
            Ok(v) => println!("{v}"),
            Err(_) => println!("<INVALID>"),
        }
    }
}
