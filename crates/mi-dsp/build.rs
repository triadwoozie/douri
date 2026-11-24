// Builds the vendored DSP code and Pro Wave's C interface to it into one static library.
fn main() {
    let sources = [
        "eurorack/stmlib/dsp/atan.cc",
        "eurorack/stmlib/dsp/units.cc",
        "eurorack/stmlib/utils/random.cc",
        "eurorack/rings/dsp/fm_voice.cc",
        "eurorack/rings/dsp/part.cc",
        "eurorack/rings/dsp/resonator.cc",
        "eurorack/rings/dsp/string.cc",
        "eurorack/rings/resources.cc",
        "eurorack/clouds/dsp/correlator.cc",
        "eurorack/clouds/dsp/granular_processor.cc",
        "eurorack/clouds/dsp/mu_law.cc",
        "eurorack/clouds/dsp/pvoc/frame_transformation.cc",
        "eurorack/clouds/dsp/pvoc/phase_vocoder.cc",
        "eurorack/clouds/dsp/pvoc/stft.cc",
        "eurorack/clouds/resources.cc",
        "eurorack/braids/analog_oscillator.cc",
        "eurorack/braids/digital_oscillator.cc",
        "eurorack/braids/macro_oscillator.cc",
        "eurorack/braids/resources.cc",
        "shim/pw_mi.cpp",
    ];
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++14")
        .include("eurorack")
        // shim/clouds/drivers/debug_pin.h stands in for the module's hardware header.
        .include("shim")
        // The code's own switch for building off the module (no linker sections).
        .define("TEST", None)
        .files(sources)
        // The DSP is upstream's code, built as it is: its warnings are not ours to fix.
        .warnings(false)
        .flag_if_supported("/utf-8")
        .flag_if_supported("/EHsc")
        .flag_if_supported("-fvisibility=hidden")
        // The code reads floats as integers through pointer casts (the phase vocoder keeps
        // its phases in the FFT's buffer): the optimiser must not assume it does not.
        .flag_if_supported("-fno-strict-aliasing");
    // Per-sample synthesis: unoptimised, it does not run in real time.
    build.opt_level(3);
    build.compile("pw_mi");
    for f in sources.iter().chain(&["shim/pw_mi.h", "shim/clouds/drivers/debug_pin.h"]) {
        println!("cargo:rerun-if-changed={f}");
    }
    println!("cargo:rerun-if-changed=eurorack");
}
