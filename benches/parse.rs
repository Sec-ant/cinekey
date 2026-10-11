use cinekey::{ExpectedKind, ParseContext, parse, parse_with_context};
use std::{hint::black_box, time::Instant};

fn main() {
    let cases = [
        "Slow.Horses.S04E01.Identity.Theft.2160p.ATVP.WEB-DL.DDP5.1.DV.HDR.H.265-NTb.mkv",
        "Fear.Street.Part.3.1666.2021.1080p.WEB-DL.mkv",
        "SUPERNATURAL - S08 E07 - A LITTLE SLICE OF KEVIN 720P DD5.1 x264 MMI.mkv",
        "[SweetSub&VCB-Studio] Made in Abyss Retsujitsu no Ougonkyou [NCED02][Ma10p_1080p][x265_flac].mkv",
        "Watashi.wo.Hanasanaide.E03.2016.Amazon.WEB-DL.1080p.H264.DDP-HDCTV.mkv",
        "2001.A.Space.Odyssey.1968.1080p.BluRay.x264-GROUP.mkv",
    ];
    let context = ParseContext {
        expected_kind: Some(ExpectedKind::Series),
        ancestors: vec!["A.Miss.Mallard.Mystery.2000.1080p.WEBRip.AVC.AAC"],
    };
    let iterations = 1_000_000usize;
    let started = Instant::now();
    for i in 0..iterations {
        let input = cases[i % cases.len()];
        let result = if i % 2 == 0 {
            parse(black_box(input))
        } else {
            parse_with_context(black_box(input), &context)
        };
        black_box(result);
    }
    let elapsed = started.elapsed();
    println!(
        "{iterations} parses in {:.3}s ({:.0} parses/s, {:.1} ns/parse; {} representative names)",
        elapsed.as_secs_f64(),
        iterations as f64 / elapsed.as_secs_f64(),
        elapsed.as_nanos() as f64 / iterations as f64,
        cases.len(),
    );
}
