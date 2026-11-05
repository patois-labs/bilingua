//! Developer tool (not shipped): `dump FILE` prints the units and project facts as JSON, `gen PATH MB` writes a
//! large generated project file, `time FILE` times extract and merge.

use bilingua_model::*;
use bilingua_sdlproj::{project_info, Sdlproj};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opts = ExtractOptions { granularity: Granularity::Paragraph, ..Default::default() };
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            match Sdlproj.extract(&bytes, &opts) {
                Ok(d) => {
                    let i = project_info(&bytes).expect("info");
                    println!(
                        "{}",
                        serde_json::json!({"units": d.units.len(), "source": i.source_lang, "targets": i.target_langs, "guid": i.package_guid, "type": i.package_type, "name": i.name})
                    );
                }
                Err(e) => println!("{}", serde_json::json!({"error": e.to_string()})),
            }
        }
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<PackageProject Guid=\"7d1c2a40-5b6e-4f70-8a91-0b2c3d4e5f60\" PackageGuid=\"c0ffee00-1234-4abc-9def-001122334455\" PackageType=\"ProjectPackage\" Name=\"big\">\r\n  <LanguageDirections>\r\n    <LanguageDirection TargetLanguageCode=\"de-DE\" SourceLanguageCode=\"en-US\" />\r\n    <LanguageDirection TargetLanguageCode=\"ja-JP\" SourceLanguageCode=\"en-US\" />\r\n  </LanguageDirections>\r\n  <ProjectFiles>\r\n");
            let mut i = 0usize;
            while s.len() < mb * 1_000_000 {
                s.push_str(&format!("    <ProjectFile Guid=\"99999999-0000-4000-8000-{i:012}\" Name=\"file{i}.docx\" Path=\"\" Role=\"Translatable\" FilterDefinitionId=\"WordprocessingML v. 2\">\r\n      <LanguageFiles>\r\n        <LanguageFile Guid=\"a{i}\" Path=\"de-DE\\\" LanguageCode=\"de-DE\"><FileVersions><FileVersion VersionNumber=\"1\" FileName=\"file{i}.docx.sdlxliff\" PhysicalPath=\"de-DE\\file{i}.docx.sdlxliff\" /></FileVersions></LanguageFile>\r\n      </LanguageFiles>\r\n    </ProjectFile>\r\n"));
                i += 1;
            }
            s.push_str("  </ProjectFiles>\r\n</PackageProject>\r\n");
            std::fs::write(&args[2], s).expect("write");
        }
        Some("time") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            let t = Instant::now();
            let d = Sdlproj.extract(&bytes, &opts).expect("extract");
            let t_extract = t.elapsed();
            let t = Instant::now();
            let same = Sdlproj.merge(&d).expect("merge") == bytes;
            let t_merge = t.elapsed();
            println!("{{\"bytes\": {}, \"units\": {}, \"extract_s\": {:.4}, \"merge_untouched_s\": {:.4}, \"identical\": {}}}", bytes.len(), d.units.len(), t_extract.as_secs_f64(), t_merge.as_secs_f64(), same);
        }
        _ => eprintln!("usage: sdlproj_dump dump FILE | gen PATH MB | time FILE"),
    }
}
