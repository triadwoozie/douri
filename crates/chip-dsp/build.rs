// Builds the vendored cores and Pro Wave's C interface to them into one static library.
fn main() {
    let sources = [
        "vendor/msfa/dx7note.cc",
        "vendor/msfa/env.cc",
        "vendor/msfa/exp2.cc",
        "vendor/msfa/fm_core.cc",
        "vendor/msfa/fm_op_kernel.cc",
        "vendor/msfa/freqlut.cc",
        "vendor/msfa/lfo.cc",
        "vendor/msfa/patch.cc",
        "vendor/msfa/pitchenv.cc",
        "vendor/msfa/sin.cc",
        "vendor/ymfm/ymfm_adpcm.cpp",
        "vendor/ymfm/ymfm_opl.cpp",
        "vendor/ymfm/ymfm_opn.cpp",
        "vendor/ymfm/ymfm_pcm.cpp",
        "vendor/ymfm/ymfm_ssg.cpp",
        "vendor/tetorica_sega_psg/segapsg.cpp",
        "vendor/yamaha-smaf-player/ma_fm_core.cpp",
        "vendor/yamaha-smaf-player/ma_player.cpp",
        "vendor/yamaha-smaf-player/smaf_file.cpp",
        "vendor/yamaha-smaf-player/smaf_voice.cpp",
        "shim/pw_fm6.cpp",
        "shim/pw_smaf.cpp",
        "shim/pw_chips.cpp",
    ];
    let mut build = cc::Build::new();
    build
        .cpp(true)
        // The SMAF player is written in C++20; the rest builds as it too.
        .std("c++20")
        .include("vendor/msfa")
        .include("vendor/ymfm")
        .include("vendor/chips-m6581")
        .include("vendor/tetorica_sega_psg")
        .include("vendor/yamaha-smaf-player")
        .include("shim")
        // The FM core's portable path: its NEON path needs an assembly file that is not here.
        .define("MSFA_FORCE_SCALAR", None)
        .files(sources)
        // The cores are upstream's code, built as they are: their warnings are not ours to fix.
        .warnings(false)
        .flag_if_supported("/utf-8")
        .flag_if_supported("/EHsc")
        .flag_if_supported("-fvisibility=hidden");
    // Per-sample synthesis: unoptimised, it does not run in real time.
    build.opt_level(3);
    build.compile("pw_chip");
    for f in sources.iter().chain(&["shim/pw_chip.h"]) {
        println!("cargo:rerun-if-changed={f}");
    }
    println!("cargo:rerun-if-changed=vendor");
}
