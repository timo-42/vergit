//! Reads one version per line on stdin, prints them in PEP 440 order.
//! Used to differentially test ordering against Python's `packaging`.
use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let mut vs: Vec<pep440::Version> = input.lines().map(|l| l.parse().unwrap()).collect();
    vs.sort();
    for v in vs {
        println!("{v}");
    }
}
