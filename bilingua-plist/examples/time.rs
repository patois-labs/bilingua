//! `time [N] FILE`: extract and untouched-merge timings (seconds, mean of N runs) and the unit count, for the LocOS
//! speed comparison. The file is read as a `.stringsdict` when its name ends in that, as a `.plist` otherwise.
use bilingua_model::{ExtractOptions, Filter};
use bilingua_plist::{Plist, Stringsdict};
use std::time::Instant;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.pop().expect("a file");
    let n: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(3);
    let bytes = std::fs::read(&path).unwrap();
    let f: Box<dyn Filter> = if path.ends_with(".stringsdict") { Box::new(Stringsdict) } else { Box::new(Plist) };
    let t = Instant::now();
    let mut units = 0;
    for _ in 0..n {
        units = f.extract(&bytes, &ExtractOptions::default()).unwrap().units.len();
    }
    let ex = t.elapsed().as_secs_f64() / n as f64;
    let d = f.extract(&bytes, &ExtractOptions::default()).unwrap();
    let t = Instant::now();
    for _ in 0..n {
        assert_eq!(f.merge(&d).unwrap(), bytes);
    }
    let mg = t.elapsed().as_secs_f64() / n as f64;
    println!("{{\"units\": {units}, \"extract_s\": {ex:.6}, \"merge_s\": {mg:.6}, \"bytes\": {}}}", bytes.len());
}
