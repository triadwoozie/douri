// Builds the vendored Novox engine core and Pro Wave's C interface to it into one static library.
fn main() {
    let sources = [
        "engine/src/lexicon.cpp",
        "engine/src/phonemes.cpp",
        "engine/src/planner.cpp",
        "engine/src/score.cpp",
        "engine/src/synth.cpp",
        "engine/src/voice.cpp",
        "shim/pw_novox.cpp",
    ];
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include("engine/src")
        .include("shim")
        .files(sources)
        // The engine is upstream's code, built as it is: its warnings are not ours to fix.
        .warnings(false)
        // Sources are UTF-8 (MSVC reads them in the system code page otherwise).
        .flag_if_supported("/utf-8")
        .flag_if_supported("/EHsc")
        .flag_if_supported("-fvisibility=hidden");
    // The synthesis is per sample: unoptimised, a phrase takes several times as long.
    build.opt_level(3);
    build.compile("novox");
    for f in sources.iter().chain(&["shim/pw_novox.h"]) {
        println!("cargo:rerun-if-changed={f}");
    }
    println!("cargo:rerun-if-changed=engine/src");
}
