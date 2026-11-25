// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The sound is ported from REDLINE's engine model
// (src/engine/model.ts, and the engines and loudness trims of src/engine/presets.ts;
// Copyright (c) 2026 JW Dev; MIT; https://github.com/Jwillc/redline at commit ac12cf9). The
// overrun fuel cut when idling down is the one of its src/sim/car.ts. Pro Wave's note
// handling, parameter table and tests are its own.

//! Engine: the sound of a car's engine, simulated: gas in each cylinder, the valves, a
//! throttle and plenum, and pipes that carry pressure waves both ways. What leaves the
//! tailpipes and the intake is the sound.
//!
//! Logic Pro equivalent: none (Sculpture and Alchemy can be made to growl, but nothing models
//! an engine).
//!
//! The note sets the engine's speed so that its firing rate is the note's frequency: an engine
//! of `n` cylinders fires `n / 2` times a turn, so `rpm = Hz * 120 / n` (A3, 220 Hz, is 6,600
//! rpm on an inline four and 3,300 on a V8). The speed is kept between the engine's idle and
//! its redline, so notes below or above those play the idle or the redline: on the Inline-4
//! (850 to 7,200 rpm) that is A0 to about B3. The speed moves to a new note in Response (the
//! time it takes to cover two thirds of the way); the first note starts the engine at its idle
//! speed. The pitch wheel bends the speed. Throttle is how far the throttle opens while a key
//! is down, and Velocity how much the key's velocity takes off it. One engine sounds at a
//! time; the last key held is the one it follows.
//!
//! When the last key goes up, Release says what happens: Stop cuts the fuel and lets the
//! engine spin down until it stands still, then it is silent; Idle closes the throttle and
//! lets it fall back to its idle speed, with the fuel cut while it is more than 900 rpm above
//! it (as REDLINE's car does), and it idles until a new note or All Sound Off. Muffler is how
//! much of the muffler is there (0 is a straight pipe the width of the tailpipe), and Misfire
//! the chance that a cylinder's firing is lost.
//!
//! The engines are REDLINE's eight, with its loudness trims. The original's crank speed comes
//! from its car (inertia, clutch, gears, limiter); here it comes from the note. Its fixed
//! filters of the mechanical noise are worked out for 48 kHz whatever the rate; here they are
//! worked out for the rate in use, so they are the same at every rate.

use super::{stored, NativeInst};
use crate::native_fx::{NativeParam, NativePreset};
use std::f64::consts::PI;

pub const ID: &str = "prowave:engine";
pub const NAME: &str = "Engine";

pub const ENGINE: usize = 0;
pub const THROTTLE: usize = 1;
pub const VELOCITY: usize = 2;
pub const RESPONSE: usize = 3;
pub const RELEASE: usize = 4;
pub const MUFFLER: usize = 5;
pub const MISFIRE: usize = 6;
pub const LEVEL: usize = 7;
pub const COUNT: usize = 8;

/// The engines' names, in REDLINE's order.
pub const ENGINE_NAMES: [&str; 8] = [
    "Inline-4 2.0",
    "V8 5.0 cross-plane",
    "V8 4.5 flat-plane",
    "Inline-6 3.0",
    "Flat-4 2.5 unequal headers",
    "V-twin 1.8 45°",
    "Drift I6 3.2 built",
    "Ford 2.3 SOHC (Pinto)",
];

pub static PARAMS: [NativeParam; COUNT] = [
    NativeParam::choice("Engine", 0, 0, &ENGINE_NAMES),
    NativeParam::knob("Throttle", 0, 100, 60, "%"),
    NativeParam::knob("Velocity", 0, 100, 50, "%"),
    NativeParam::knob("Response", 5, 3000, 200, "ms"),
    NativeParam::choice("Release", 0, 0, &["Stop", "Idle"]),
    NativeParam::knob("Muffler", 0, 100, 100, "%"),
    NativeParam::knob("Misfire", 0, 100, 0, "%"),
    NativeParam::knob("Level", -48, 12, -6, "dB"),
];

/// REDLINE's engines as presets: each picks one.
pub static PRESETS: [NativePreset; 8] = [
    NativePreset {
        name: ENGINE_NAMES[0],
        values: &[(ENGINE, 0.0)],
    },
    NativePreset {
        name: ENGINE_NAMES[1],
        values: &[(ENGINE, 1.0)],
    },
    NativePreset {
        name: ENGINE_NAMES[2],
        values: &[(ENGINE, 2.0)],
    },
    NativePreset {
        name: ENGINE_NAMES[3],
        values: &[(ENGINE, 3.0)],
    },
    NativePreset {
        name: ENGINE_NAMES[4],
        values: &[(ENGINE, 4.0)],
    },
    NativePreset {
        name: ENGINE_NAMES[5],
        values: &[(ENGINE, 5.0)],
    },
    NativePreset {
        name: ENGINE_NAMES[6],
        values: &[(ENGINE, 6.0)],
    },
    NativePreset {
        name: ENGINE_NAMES[7],
        values: &[(ENGINE, 7.0)],
    },
];

pub fn value_text(k: usize, v: i32) -> String {
    match k {
        ENGINE => ENGINE_NAMES
            .get(v.max(0) as usize)
            .copied()
            .unwrap_or("")
            .to_string(),
        THROTTLE | VELOCITY | MUFFLER | MISFIRE => format!("{v}%"),
        RESPONSE => format!("{v}ms"),
        RELEASE => if v != 0 { "Idle" } else { "Stop" }.to_string(),
        LEVEL => format!("{v}dB"),
        _ => String::new(),
    }
}

// ---- The engines (presets.ts) ----

#[derive(Clone, Copy)]
struct Seg {
    len: f64,
    d: f64,
}

const fn seg(len: f64, d: f64) -> Seg {
    Seg { len, d }
}

/// Header pipes that meet in one collector.
struct Group {
    cyls: &'static [usize],
    /// -1 on the car's right, 1 on its left.
    side: f64,
    lens: &'static [f64],
    d: f64,
}

/// The exhaust and the intake runners.
struct Plan {
    groups: &'static [Group],
    /// Each group to its own tailpipe, or all of them merged into one.
    dual: bool,
    merge: Option<Seg>,
    mid: Seg,
    muffler: Seg,
    /// A chamber halfway along the mid pipe.
    resonator: Option<Seg>,
    /// A catalytic converter at the head of the line.
    cat: Option<Seg>,
    tail: Seg,
    runner: Seg,
}

/// The engine's own mechanical and flow noise.
struct Mech {
    valve: f64,
    comb: f64,
    rattle: f64,
    exhaust_hiss: f64,
    intake_hiss: f64,
    /// The iron's ringing: [Hz, Q, gain].
    modes: &'static [[f64; 3]],
}

struct Spec {
    bore: f64,
    stroke: f64,
    rod: f64,
    cr: f64,
    /// The firing order (cylinders from 1).
    order: &'static [usize],
    /// Degrees between firings, when they are not even.
    intervals: Option<&'static [f64]>,
    /// Valves of each kind per cylinder, their diameters (intake, exhaust) and lift.
    valves_n: f64,
    di: f64,
    de: f64,
    lift: f64,
    ivo: f64,
    ivc: f64,
    evo: f64,
    evc: f64,
    throttle_d: f64,
    idle_bypass: f64,
    plenum: f64,
    lambda: f64,
    eff: f64,
    burn: f64,
    /// REDLINE's loudness trims (`TRIM`).
    gain: f64,
    intake_gain: f64,
    block_gain: f64,
    idle: f64,
    redline: f64,
    mech: Option<Mech>,
    plan: Plan,
}

const I4: [usize; 4] = [0, 1, 2, 3];
const BANK_B: [usize; 4] = [4, 5, 6, 7];

static SPECS: [Spec; 8] = [
    Spec {
        bore: 0.086,
        stroke: 0.086,
        rod: 0.145,
        cr: 10.8,
        order: &[1, 3, 4, 2],
        intervals: None,
        valves_n: 2.0,
        di: 0.034,
        de: 0.029,
        lift: 0.0095,
        ivo: 12.0,
        ivc: 52.0,
        evo: 50.0,
        evc: 12.0,
        throttle_d: 0.062,
        idle_bypass: 0.0045,
        plenum: 0.0028,
        lambda: 0.92,
        eff: 0.92,
        burn: 48.0,
        gain: 0.0064,
        intake_gain: 0.1,
        block_gain: 8.1e-05,
        idle: 850.0,
        redline: 7200.0,
        mech: None,
        plan: Plan {
            groups: &[Group {
                cyls: &I4,
                side: -1.0,
                lens: &[0.72, 0.72, 0.72, 0.72],
                d: 0.038,
            }],
            dual: false,
            merge: None,
            mid: seg(2.0, 0.055),
            muffler: seg(0.45, 0.17),
            resonator: None,
            cat: None,
            tail: seg(0.7, 0.055),
            runner: seg(0.32, 0.04),
        },
    },
    Spec {
        bore: 0.093,
        stroke: 0.092,
        rod: 0.155,
        cr: 10.5,
        order: &[1, 5, 4, 8, 6, 3, 7, 2],
        intervals: None,
        valves_n: 1.0,
        di: 0.051,
        de: 0.04,
        lift: 0.0135,
        ivo: 28.0,
        ivc: 68.0,
        evo: 72.0,
        evc: 28.0,
        throttle_d: 0.09,
        idle_bypass: 0.0045,
        plenum: 0.0045,
        lambda: 0.88,
        eff: 0.9,
        burn: 55.0,
        gain: 0.0049,
        intake_gain: 0.183,
        block_gain: 8.7e-05,
        idle: 720.0,
        redline: 6600.0,
        mech: None,
        plan: Plan {
            groups: &[
                Group {
                    cyls: &I4,
                    side: 1.0,
                    lens: &[0.8, 0.78, 0.82, 0.8],
                    d: 0.044,
                },
                Group {
                    cyls: &BANK_B,
                    side: -1.0,
                    lens: &[0.8, 0.82, 0.78, 0.8],
                    d: 0.044,
                },
            ],
            dual: true,
            merge: None,
            mid: seg(1.5, 0.064),
            muffler: seg(0.4, 0.16),
            resonator: None,
            cat: None,
            tail: seg(0.7, 0.064),
            runner: seg(0.42, 0.046),
        },
    },
    Spec {
        bore: 0.094,
        stroke: 0.081,
        rod: 0.15,
        cr: 12.5,
        order: &[1, 5, 3, 7, 4, 8, 2, 6],
        intervals: None,
        valves_n: 2.0,
        di: 0.036,
        de: 0.03,
        lift: 0.0115,
        ivo: 30.0,
        ivc: 62.0,
        evo: 62.0,
        evc: 30.0,
        throttle_d: 0.092,
        idle_bypass: 0.0045,
        plenum: 0.004,
        lambda: 0.88,
        eff: 0.92,
        burn: 46.0,
        gain: 0.00304,
        intake_gain: 0.359,
        block_gain: 9.1e-05,
        idle: 1000.0,
        redline: 9000.0,
        mech: None,
        plan: Plan {
            groups: &[
                Group {
                    cyls: &I4,
                    side: 1.0,
                    lens: &[0.55, 0.55, 0.55, 0.55],
                    d: 0.042,
                },
                Group {
                    cyls: &BANK_B,
                    side: -1.0,
                    lens: &[0.55, 0.55, 0.55, 0.55],
                    d: 0.042,
                },
            ],
            dual: true,
            merge: None,
            mid: seg(1.4, 0.06),
            muffler: seg(0.35, 0.14),
            resonator: None,
            cat: None,
            tail: seg(0.6, 0.06),
            runner: seg(0.26, 0.044),
        },
    },
    Spec {
        bore: 0.084,
        stroke: 0.09,
        rod: 0.145,
        cr: 10.6,
        order: &[1, 5, 3, 6, 2, 4],
        intervals: None,
        valves_n: 2.0,
        di: 0.033,
        de: 0.028,
        lift: 0.0098,
        ivo: 14.0,
        ivc: 50.0,
        evo: 50.0,
        evc: 14.0,
        throttle_d: 0.072,
        idle_bypass: 0.0045,
        plenum: 0.0038,
        lambda: 0.9,
        eff: 0.92,
        burn: 48.0,
        gain: 0.0093,
        intake_gain: 0.212,
        block_gain: 4.3e-05,
        idle: 760.0,
        redline: 7200.0,
        mech: None,
        plan: Plan {
            groups: &[
                Group {
                    cyls: &[0, 1, 2],
                    side: -1.0,
                    lens: &[0.62, 0.62, 0.62],
                    d: 0.04,
                },
                Group {
                    cyls: &[3, 4, 5],
                    side: -1.0,
                    lens: &[0.62, 0.62, 0.62],
                    d: 0.04,
                },
            ],
            dual: false,
            merge: Some(seg(0.35, 0.062)),
            mid: seg(1.6, 0.062),
            muffler: seg(0.42, 0.17),
            resonator: None,
            cat: None,
            tail: seg(0.7, 0.062),
            runner: seg(0.36, 0.04),
        },
    },
    Spec {
        bore: 0.0995,
        stroke: 0.079,
        rod: 0.13,
        cr: 10.0,
        order: &[1, 3, 2, 4],
        intervals: None,
        valves_n: 2.0,
        di: 0.036,
        de: 0.03,
        lift: 0.0098,
        ivo: 12.0,
        ivc: 52.0,
        evo: 52.0,
        evc: 12.0,
        throttle_d: 0.066,
        idle_bypass: 0.0045,
        plenum: 0.0032,
        lambda: 0.9,
        eff: 0.92,
        burn: 50.0,
        gain: 0.00365,
        intake_gain: 0.159,
        block_gain: 0.00015,
        idle: 800.0,
        redline: 6800.0,
        mech: None,
        plan: Plan {
            groups: &[Group {
                cyls: &I4,
                side: -1.0,
                lens: &[0.45, 1.05, 0.5, 1.1],
                d: 0.04,
            }],
            dual: false,
            merge: None,
            mid: seg(1.9, 0.058),
            muffler: seg(0.45, 0.16),
            resonator: None,
            cat: None,
            tail: seg(0.7, 0.058),
            runner: seg(0.35, 0.042),
        },
    },
    Spec {
        bore: 0.1016,
        stroke: 0.1111,
        rod: 0.19,
        cr: 9.7,
        order: &[1, 2],
        intervals: Some(&[315.0, 405.0]),
        valves_n: 2.0,
        di: 0.041,
        de: 0.034,
        lift: 0.0125,
        ivo: 18.0,
        ivc: 50.0,
        evo: 55.0,
        evc: 15.0,
        throttle_d: 0.054,
        idle_bypass: 0.0045,
        plenum: 0.0024,
        lambda: 0.9,
        eff: 0.9,
        burn: 55.0,
        gain: 0.0022,
        intake_gain: 0.31,
        block_gain: 0.00045,
        idle: 900.0,
        redline: 5800.0,
        mech: None,
        plan: Plan {
            groups: &[Group {
                cyls: &[0, 1],
                side: -1.0,
                lens: &[0.75, 0.95],
                d: 0.045,
            }],
            dual: false,
            merge: None,
            mid: seg(1.8, 0.055),
            muffler: seg(0.4, 0.15),
            resonator: None,
            cat: None,
            tail: seg(0.6, 0.055),
            runner: seg(0.3, 0.045),
        },
    },
    Spec {
        bore: 0.087,
        stroke: 0.09,
        rod: 0.143,
        cr: 11.5,
        order: &[1, 5, 3, 6, 2, 4],
        intervals: None,
        valves_n: 2.0,
        di: 0.036,
        de: 0.031,
        lift: 0.0125,
        ivo: 30.0,
        ivc: 64.0,
        evo: 64.0,
        evc: 30.0,
        throttle_d: 0.082,
        idle_bypass: 0.0045,
        plenum: 0.0048,
        lambda: 0.86,
        eff: 0.93,
        burn: 44.0,
        gain: 0.00144,
        intake_gain: 1.5,
        block_gain: 0.0002,
        idle: 950.0,
        redline: 8000.0,
        mech: None,
        plan: Plan {
            groups: &[Group {
                cyls: &[0, 1, 2, 3, 4, 5],
                side: -1.0,
                lens: &[0.74, 0.74, 0.74, 0.74, 0.74, 0.74],
                d: 0.042,
            }],
            dual: false,
            merge: None,
            mid: seg(1.3, 0.072),
            muffler: seg(0.3, 0.13),
            resonator: None,
            cat: None,
            tail: seg(0.5, 0.072),
            runner: seg(0.3, 0.044),
        },
    },
    Spec {
        bore: 0.09604,
        stroke: 0.0794,
        rod: 0.1322,
        cr: 8.4,
        order: &[1, 3, 4, 2],
        intervals: None,
        valves_n: 1.0,
        di: 0.0439,
        de: 0.0381,
        lift: 0.0102,
        ivo: 12.0,
        ivc: 44.0,
        evo: 52.0,
        evc: 10.0,
        throttle_d: 0.04,
        idle_bypass: 0.014,
        plenum: 0.0018,
        lambda: 0.98,
        eff: 0.8,
        burn: 58.0,
        gain: 0.0046,
        intake_gain: 0.15,
        block_gain: 0.000037,
        idle: 750.0,
        redline: 5600.0,
        mech: Some(Mech {
            valve: 12.0,
            comb: 0.00025,
            rattle: 0.8,
            exhaust_hiss: 0.03,
            intake_hiss: 0.45,
            modes: &[
                [190.0, 2.0, 6.0],
                [850.0, 4.0, 1.0],
                [1450.0, 6.0, 0.9],
                [2300.0, 8.0, 0.8],
                [3400.0, 10.0, 0.6],
                [4700.0, 12.0, 0.4],
            ],
        }),
        plan: Plan {
            groups: &[Group {
                cyls: &I4,
                side: -1.0,
                lens: &[0.36, 0.28, 0.26, 0.34],
                d: 0.036,
            }],
            dual: false,
            merge: None,
            mid: seg(1.7, 0.048),
            muffler: seg(0.55, 0.2),
            resonator: Some(seg(0.35, 0.1)),
            cat: Some(seg(0.32, 0.13)),
            tail: seg(0.6, 0.045),
            runner: seg(0.47, 0.037),
        },
    },
];

/// A pipe of the network as the plan lays it out.
#[derive(Clone, Copy)]
struct PipeSpec {
    len: f64,
    d: f64,
    t: f64,
    loss: f64,
    muffler: bool,
}

const fn pipe(len: f64, d: f64, t: f64, loss: f64) -> PipeSpec {
    PipeSpec {
        len,
        d,
        t,
        loss,
        muffler: false,
    }
}

/// The pipe network: pipes, each cylinder's header and runner, the junctions (pipe, end) and
/// the open tailpipe ends with their pan.
#[derive(Default)]
struct Net {
    pipes: Vec<PipeSpec>,
    exh: Vec<usize>,
    run: Vec<usize>,
    joins: Vec<Vec<(usize, u8)>>,
    opens: Vec<(usize, f64)>,
    tail_d: f64,
}

impl Net {
    fn add(&mut self, p: PipeSpec) -> usize {
        self.pipes.push(p);
        self.pipes.len() - 1
    }

    /// Pipes in a row from a set of upstream ends; returns the last pipe.
    fn chain(&mut self, from: Vec<(usize, u8)>, segs: &[PipeSpec]) -> usize {
        let mut ends = from;
        let mut last = 0;
        for s in segs {
            last = self.add(*s);
            let mut join = ends;
            join.push((last, 0));
            self.joins.push(join);
            ends = vec![(last, 1)];
        }
        last
    }

    /// `build` of presets.ts, without the drawing.
    fn build(spec: &Spec) -> Net {
        let plan = &spec.plan;
        let n = spec.order.len();
        let mut net = Net {
            exh: vec![0; n],
            run: vec![0; n],
            tail_d: plan.tail.d,
            ..Net::default()
        };
        let mut group_ends: Vec<(usize, f64)> = Vec::new();
        for g in plan.groups {
            let mut heads = Vec::new();
            for (k, ci) in g.cyls.iter().enumerate() {
                let id = net.add(pipe(g.lens[k], g.d, 950.0, 0.05));
                net.exh[*ci] = id;
                heads.push((id, 1));
                let r = net.add(pipe(plan.runner.len, plan.runner.d, 310.0, 0.03));
                net.run[*ci] = r;
            }
            let prim = net.chain(heads, &[pipe(0.25, g.d * 1.5, 880.0, 0.05)]);
            group_ends.push((prim, g.side));
        }
        let line = |net: &mut Net, from: Vec<(usize, u8)>, x: f64| {
            let mut segs: Vec<PipeSpec> = Vec::new();
            if let Some(c) = plan.cat {
                segs.push(pipe(c.len, c.d, 820.0, 0.4));
            }
            if let Some(r) = plan.resonator {
                segs.push(pipe(plan.mid.len / 2.0, plan.mid.d, 770.0, 0.05));
                segs.push(pipe(r.len, r.d, 720.0, 0.2));
                segs.push(pipe(plan.mid.len / 2.0, plan.mid.d, 700.0, 0.05));
            } else {
                segs.push(pipe(plan.mid.len, plan.mid.d, 760.0, 0.05));
            }
            segs.push(PipeSpec {
                muffler: true,
                ..pipe(plan.muffler.len, plan.muffler.d, 650.0, 0.35)
            });
            segs.push(pipe(plan.tail.len, plan.tail.d, 600.0, 0.05));
            let last = net.chain(from, &segs);
            net.opens.push((last, (x * 2.4).clamp(-1.0, 1.0)));
        };
        if plan.dual {
            for (p, side) in &group_ends {
                line(&mut net, vec![(*p, 1)], 0.33 * side);
            }
        } else if group_ends.len() > 1 {
            let m = plan.merge.unwrap_or(seg(0.3, plan.mid.d));
            let ends = group_ends.iter().map(|(p, _)| (*p, 1)).collect();
            let merged = net.chain(ends, &[pipe(m.len, m.d, 820.0, 0.05)]);
            line(&mut net, vec![(merged, 1)], -0.3);
        } else if let Some((p, side)) = group_ends.first() {
            line(&mut net, vec![(*p, 1)], 0.33 * side);
        }
        net
    }
}

/// `phasesFor`: cylinder k of the firing order fires at idx * 720 / n degrees (or after the
/// given intervals).
fn phases_for(order: &[usize], intervals: Option<&[f64]>) -> Vec<f64> {
    let n = order.len();
    let mut ph = vec![0.0; n];
    let mut a = 0.0;
    for (idx, cyl) in order.iter().enumerate() {
        ph[cyl - 1] = a;
        a += intervals.map_or(720.0 / n as f64, |iv| iv[idx]);
    }
    ph
}

// ---- The model (model.ts) ----

const R: f64 = 287.0;
const GAMMA: f64 = 1.35;
const CV: f64 = R / (GAMMA - 1.0);
const CP: f64 = GAMMA * CV;
const P0: f64 = 101325.0;
const T0: f64 = 300.0;
const LHV: f64 = 44e6;
const AFR: f64 = 14.7;
const PSI_N: usize = 4096;

/// Flow through an orifice: mdot = A * p_up / sqrt(R T_up) * psi(p_down / p_up), tabulated.
fn psi_table() -> Vec<f64> {
    let prc = (2.0 / (GAMMA + 1.0)).powf(GAMMA / (GAMMA - 1.0));
    let psi_c = GAMMA.sqrt() * (2.0 / (GAMMA + 1.0)).powf((GAMMA + 1.0) / (2.0 * (GAMMA - 1.0)));
    (0..=PSI_N + 1)
        .map(|i| {
            let pr = (i as f64 / PSI_N as f64).min(1.0);
            if pr <= prc {
                psi_c
            } else {
                ((2.0 * GAMMA / (GAMMA - 1.0))
                    * (pr.powf(2.0 / GAMMA) - pr.powf((GAMMA + 1.0) / GAMMA)))
                .max(0.0)
                .sqrt()
            }
        })
        .collect()
}

#[inline]
fn psi(t: &[f64], pr: f64) -> f64 {
    // Not a number, or not above 0: the first entry (as `!(pr > 0)` in the original).
    if pr.is_nan() || pr <= 0.0 {
        return t[0];
    }
    let x = pr * PSI_N as f64;
    let i = x as usize;
    if i >= PSI_N {
        return 0.0;
    }
    t[i] + (t[i + 1] - t[i]) * (x - i as f64)
}

/// Signed mass flow from side 1 to side 2. `inv` = 1 / sqrt(R T) of each side.
#[inline]
fn flow(t: &[f64], p1: f64, inv1: f64, p2: f64, inv2: f64, a: f64) -> f64 {
    let p1 = if p1 < 1000.0 { 1000.0 } else { p1 };
    let p2 = if p2 < 1000.0 { 1000.0 } else { p2 };
    if p1 >= p2 {
        a * p1 * inv1 * psi(t, p2 / p1)
    } else {
        -a * p2 * inv2 * psi(t, p1 / p2)
    }
}

/// Flow `m` from a volume (`pc`) into a pipe end whose pressure is `ps + k m`: Illinois
/// regula falsi.
fn valve_flow(t: &[f64], pc: f64, invc: f64, ps: f64, k: f64, invp: f64, a_: f64) -> f64 {
    if a_ <= 0.0 {
        return 0.0;
    }
    let f0 = flow(t, pc, invc, ps, invp, a_);
    if f0 == 0.0 {
        return 0.0;
    }
    let (mut a, mut ga, mut b) = (0.0, -f0, f0);
    let mut gb = b - flow(t, pc, invc, ps + k * b, invp, a_);
    if gb == 0.0 {
        return b;
    }
    let mut side = 0;
    for _ in 0..10 {
        let m = (a * gb - b * ga) / (gb - ga);
        let gm = m - flow(t, pc, invc, ps + k * m, invp, a_);
        if gm.abs() < 1e-9 {
            return m;
        }
        if gm * gb > 0.0 {
            b = m;
            gb = gm;
            if side == -1 {
                ga *= 0.5;
            }
            side = -1;
        } else {
            a = m;
            ga = gm;
            if side == 1 {
                gb *= 0.5;
            }
            side = 1;
        }
    }
    (a * gb - b * ga) / (gb - ga)
}

/// A pipe of fixed gas temperature: a right-going and a left-going delay line. End 0 is
/// upstream.
struct Pipe {
    r: Vec<f64>,
    l: Vec<f64>,
    i: usize,
    area: f64,
    base_area: f64,
    c: f64,
    rho_c: f64,
    inv: f64,
    t: f64,
    g: f64,
    alpha: f64,
    s0: f64,
    s1: f64,
    d0: f64,
    d1: f64,
    arr0: f64,
    arr1: f64,
    out0: f64,
    out1: f64,
    muffler: bool,
}

impl Pipe {
    fn new(s: &PipeSpec, fs: f64) -> Pipe {
        let c = (GAMMA * R * s.t).sqrt();
        let rho = P0 / (R * s.t);
        let area = PI * s.d * s.d / 4.0;
        let n = ((s.len / c * fs).round() as usize).max(2);
        // Loss per pass grows with length and thinner pipes; the low-pass stands in for wall
        // friction.
        let fc = (fs * 0.45).min(9000.0 * (s.d / 0.05).sqrt() / s.len.max(0.3));
        Pipe {
            r: vec![0.0; n],
            l: vec![0.0; n],
            i: 0,
            area,
            base_area: area,
            c,
            rho_c: rho * c,
            inv: 1.0 / (R * s.t).sqrt(),
            t: s.t,
            g: (-s.loss * s.len / (s.d / 0.05).sqrt()).exp(),
            alpha: 1.0 - (-2.0 * PI * fc.max(2500.0) / fs).exp(),
            s0: 0.0,
            s1: 0.0,
            d0: 0.0,
            d1: 0.0,
            arr0: 0.0,
            arr1: 0.0,
            out0: 0.0,
            out1: 0.0,
            muffler: s.muffler,
        }
    }

    fn clear(&mut self) {
        self.r.fill(0.0);
        self.l.fill(0.0);
        self.i = 0;
        (self.s0, self.s1, self.d0, self.d1) = (0.0, 0.0, 0.0, 0.0);
        (self.arr0, self.arr1, self.out0, self.out1) = (0.0, 0.0, 0.0, 0.0);
    }

    #[inline]
    fn read(&mut self) {
        // Low-pass and damping act on the swing of each wave only; its mean, which carries
        // the steady flow, passes untouched, or the pipe would make or lose gas.
        self.s1 += self.alpha * (self.r[self.i] - self.s1);
        self.s0 += self.alpha * (self.l[self.i] - self.s0);
        self.d1 += 0.0004 * (self.s1 - self.d1);
        self.d0 += 0.0004 * (self.s0 - self.d0);
        self.arr1 = self.d1 + self.g * (self.s1 - self.d1);
        self.arr0 = self.d0 + self.g * (self.s0 - self.d0);
        self.out0 = self.arr0;
        self.out1 = self.arr1;
    }

    #[inline]
    fn write(&mut self) {
        self.r[self.i] = self.out0;
        self.l[self.i] = self.out1;
        self.i = if self.i + 1 == self.r.len() {
            0
        } else {
            self.i + 1
        };
    }
}

#[derive(Clone, Copy, Default)]
struct Cylinder {
    phase: f64,
    m: f64,
    mf: f64,
    e: f64,
    p: f64,
    t: f64,
    v: f64,
    q: f64,
    burn: f64,
    burn_dur: f64,
    last_local: f64,
}

#[derive(Clone, Copy, Default)]
struct Open {
    pipe: usize,
    pan: f64,
    s: f64,
    u: f64,
    hp: f64,
    hpx: f64,
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn pass(low: bool, f: f64, q: f64, fs: f64) -> Biquad {
        let w0 = 2.0 * PI * f / fs;
        let (al, c) = (w0.sin() / (2.0 * q), w0.cos());
        let a0 = 1.0 + al;
        let b = if low {
            [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0]
        } else {
            [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0]
        };
        Biquad {
            b0: b[0] / a0,
            b1: b[1] / a0,
            b2: b[2] / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - al) / a0,
            ..Biquad::default()
        }
    }

    /// A band-pass of constant skirt gain (the iron's modes).
    fn band(f: f64, q: f64, fs: f64) -> Biquad {
        let w0 = 2.0 * PI * f / fs;
        let al = w0.sin() / (2.0 * q);
        let a0 = 1.0 + al;
        Biquad {
            b0: al / a0,
            b2: -al / a0,
            a1: -2.0 * w0.cos() / a0,
            a2: (1.0 - al) / a0,
            ..Biquad::default()
        }
    }

    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        (self.x2, self.x1, self.y2, self.y1) = (self.x1, x, self.y1, y);
        y
    }

    fn clear(&mut self) {
        (self.x1, self.x2, self.y1, self.y2) = (0.0, 0.0, 0.0, 0.0);
    }
}

/// One engine: `EngineSim`. Its memory is allocated in `new`; `process` does not allocate.
struct EngineSim {
    spec: &'static Spec,
    fs: f64,
    dt: f64,
    psi: Vec<f64>,
    pipes: Vec<Pipe>,
    cyl: Vec<Cylinder>,
    exh: Vec<usize>,
    run: Vec<usize>,
    joins: Vec<Vec<(usize, u8)>>,
    opens: Vec<Open>,
    r: f64,
    rod: f64,
    ap: f64,
    vc: f64,
    theta: f64,
    omega: f64,
    throttle: f64,
    fuel_cut: bool,
    muffler: f64,
    tail_d: f64,
    /// 0 to 1: the chance a firing is lost.
    misfire: f64,
    plenum_m: f64,
    pool_b: f64,
    runner_m: f64,
    plenum_v: f64,
    th_a: f64,
    idle_a: f64,
    inv0: f64,
    intake_u: f64,
    intake_hp: f64,
    intake_x: f64,
    block_hp: f64,
    block_x: f64,
    rng: u32,
    modes: Vec<(Biquad, f64)>,
    shape: [Biquad; 3],
    prev_lift: Vec<f64>,
    noise_state: u32,
    iv_a: f64,
    ev_a: f64,
}

impl EngineSim {
    fn new(spec: &'static Spec, fs: f64) -> EngineSim {
        let net = Net::build(spec);
        let pipes: Vec<Pipe> = net.pipes.iter().map(|p| Pipe::new(p, fs)).collect();
        let runner_m = net
            .run
            .iter()
            .map(|i| {
                let p = &pipes[*i];
                (P0 / (R * p.t)) * p.area * net.pipes[*i].len
            })
            .sum();
        let n = spec.order.len();
        let r = spec.stroke / 2.0;
        let ap = PI * spec.bore * spec.bore / 4.0;
        let vd = ap * spec.stroke;
        let th_a = PI * spec.throttle_d * spec.throttle_d / 4.0;
        let modes = spec.mech.as_ref().map_or(Vec::new(), |m| {
            m.modes
                .iter()
                .map(|[f, q, g]| (Biquad::band(*f, *q, fs), *g))
                .collect()
        });
        let mut sim = EngineSim {
            spec,
            fs,
            dt: 1.0 / fs,
            psi: psi_table(),
            pipes,
            cyl: phases_for(spec.order, spec.intervals)
                .into_iter()
                .map(|phase| Cylinder {
                    phase,
                    ..Cylinder::default()
                })
                .collect(),
            exh: net.exh,
            run: net.run,
            joins: net.joins,
            opens: net
                .opens
                .iter()
                .map(|(pipe, pan)| Open {
                    pipe: *pipe,
                    pan: *pan,
                    ..Open::default()
                })
                .collect(),
            r,
            rod: spec.rod,
            ap,
            vc: vd / (spec.cr - 1.0),
            theta: 0.0,
            omega: 0.0,
            throttle: 0.0,
            fuel_cut: false,
            muffler: 1.0,
            tail_d: net.tail_d,
            misfire: 0.0,
            plenum_m: 0.0,
            pool_b: 0.0,
            runner_m,
            plenum_v: spec.plenum,
            th_a,
            idle_a: th_a * spec.idle_bypass,
            inv0: 1.0 / (R * T0).sqrt(),
            intake_u: 0.0,
            intake_hp: 0.0,
            intake_x: 0.0,
            block_hp: 0.0,
            block_x: 0.0,
            rng: 12345,
            modes,
            shape: [
                Biquad::pass(false, 650.0, 0.7, fs),
                Biquad::pass(true, 4200.0, 0.7, fs),
                Biquad::pass(true, 5500.0, 0.7, fs),
            ],
            prev_lift: vec![0.0; n * 2],
            noise_state: 0x9e37_79b9,
            iv_a: spec.valves_n * PI * spec.di * spec.di / 4.0,
            ev_a: spec.valves_n * PI * spec.de * spec.de / 4.0,
        };
        sim.reset();
        sim
    }

    /// The engine as it is made: stopped, every cylinder at ambient, the plenum at idle vacuum.
    fn reset(&mut self) {
        for p in &mut self.pipes {
            p.clear();
        }
        for i in 0..self.cyl.len() {
            let v = self.volume(self.local(self.cyl[i].phase, 0.0));
            let m = P0 * v / (R * T0);
            let c = &mut self.cyl[i];
            *c = Cylinder {
                phase: c.phase,
                m,
                mf: m,
                e: m * CV * T0,
                p: P0,
                t: T0,
                v,
                q: 0.0,
                burn: -1.0,
                burn_dur: 50.0,
                last_local: 0.0,
            };
        }
        for o in &mut self.opens {
            (o.s, o.u, o.hp, o.hpx) = (0.0, 0.0, 0.0, 0.0);
        }
        for (m, _) in &mut self.modes {
            m.clear();
        }
        for f in &mut self.shape {
            f.clear();
        }
        self.prev_lift.fill(0.0);
        self.theta = 0.0;
        self.omega = 0.0;
        // Start at idle vacuum, as a warm engine would be.
        self.plenum_m = 0.4 * P0 * self.plenum_v / (R * T0);
        self.pool_b = 0.0;
        (self.intake_u, self.intake_hp, self.intake_x) = (0.0, 0.0, 0.0);
        (self.block_hp, self.block_x) = (0.0, 0.0);
        self.rng = 12345;
        self.noise_state = 0x9e37_79b9;
    }

    fn noise(&mut self) -> f64 {
        let mut x = self.noise_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise_state = x;
        f64::from(x) / 2_147_483_648.0 - 1.0
    }

    fn rand(&mut self) -> f64 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(self.rng) / 4_294_967_296.0
    }

    /// Degrees after this cylinder's firing TDC, 0 to 720.
    fn local(&self, phase: f64, theta: f64) -> f64 {
        let d = (theta * 180.0 / PI - phase) % 720.0;
        if d < 0.0 {
            d + 720.0
        } else {
            d
        }
    }

    fn volume(&self, local_deg: f64) -> f64 {
        let a = local_deg * PI / 180.0;
        let (s, c) = (a.sin(), a.cos());
        let x =
            self.r * (1.0 - c) + self.rod - (self.rod * self.rod - self.r * self.r * s * s).sqrt();
        self.vc + self.ap * x
    }

    fn lift(local_deg: f64, open: f64, close: f64) -> f64 {
        let mut d = local_deg - open;
        if d < 0.0 {
            d += 720.0;
        }
        let span = close - open;
        if d >= span {
            return 0.0;
        }
        let s = (PI * d / span).sin();
        s * s
    }

    fn set_muffler(&mut self, f: f64) {
        self.muffler = f;
        let a = PI * self.tail_d * self.tail_d / 4.0;
        for p in &mut self.pipes {
            if p.muffler {
                p.area = a + (p.base_area - a) * f;
            }
        }
    }

    /// Runs `left.len()` samples; the crank speed slews from the current one to
    /// `omega_target` (rad/s) over them.
    fn process(&mut self, left: &mut [f32], right: &mut [f32], omega_target: f64) {
        let spec = self.spec;
        let n = left.len().min(right.len());
        let dt = self.dt;
        let w0 = self.omega;
        let evo = 180.0 - spec.evo;
        let evc = 360.0 + spec.evc;
        let ivo = 360.0 - spec.ivo;
        let ivc = 540.0 + spec.ivc;
        let lift_e = (spec.lift * 4.0 / spec.de).min(1.0);
        let lift_i = (spec.lift * 4.0 / spec.di).min(1.0);
        let cd = 0.62;
        let th_area =
            self.idle_a + self.th_a * (1.0 - (self.throttle.clamp(0.0, 1.0) * PI / 2.0).cos());
        let ncyl = self.cyl.len();
        for k in 0..n {
            let w = w0 + (omega_target - w0) * (k + 1) as f64 / n as f64;
            self.omega = w;
            let dth = w * dt;
            self.theta += dth;
            if self.theta >= 4.0 * PI {
                self.theta -= 4.0 * PI;
            }
            for p in &mut self.pipes {
                p.read();
            }
            let pp = self.plenum_m * R * T0 / self.plenum_v;
            let mut clicks = 0.0;
            let pool_m = self.plenum_m + self.runner_m * pp / P0;
            let rpm = w * 60.0 / (2.0 * PI);
            let adv = 8.0 + 22.0 * (rpm / 5500.0).min(1.0);
            let mut block = 0.0;
            for ci in 0..ncyl {
                let mut c = self.cyl[ci];
                let loc = self.local(c.phase, self.theta);
                let vn = self.volume(loc);
                let dv = vn - c.v;
                // valves
                let le = Self::lift(loc, evo, evc) * lift_e;
                let li = Self::lift(loc, ivo, ivc) * lift_i;
                if let Some(mech) = &spec.mech {
                    // A valve seating: a click, harder the faster the engine turns.
                    let (pe, pi) = (self.prev_lift[ci * 2], self.prev_lift[ci * 2 + 1]);
                    let speed = (w.abs() / 100.0).min(3.0);
                    if pe > 0.0 && le == 0.0 {
                        clicks += mech.valve * (0.6 + 0.8 * self.rand()) * speed;
                    }
                    if pi > 0.0 && li == 0.0 {
                        clicks += mech.valve * (0.6 + 0.8 * self.rand()) * speed;
                    }
                    self.prev_lift[ci * 2] = le;
                    self.prev_lift[ci * 2 + 1] = li;
                }
                let ae = le.min(1.0) * self.ev_a * cd;
                let ai = li.min(1.0) * self.iv_a * cd;
                let invc = 1.0 / (R * c.t).sqrt();
                let (ei, ri) = (self.exh[ci], self.run[ci]);
                let m_e = if ae > 0.0 {
                    let ep = &self.pipes[ei];
                    valve_flow(
                        &self.psi,
                        c.p,
                        invc,
                        P0 + 2.0 * ep.arr0,
                        ep.c / ep.area,
                        ep.inv,
                        ae,
                    )
                } else {
                    0.0
                };
                let m_i = if ai > 0.0 {
                    let rp = &self.pipes[ri];
                    valve_flow(
                        &self.psi,
                        c.p,
                        invc,
                        P0 + 2.0 * rp.arr1,
                        rp.c / rp.area,
                        rp.inv,
                        ai,
                    )
                } else {
                    0.0
                };
                let (ep_t, rp_t) = (self.pipes[ei].t, self.pipes[ri].t);
                {
                    let ep = &mut self.pipes[ei];
                    ep.out0 = ep.arr0 + ep.c * m_e / ep.area;
                }
                {
                    let rp = &mut self.pipes[ri];
                    rp.out1 = rp.arr1 + rp.c * m_i / rp.area;
                }
                // mass and energy
                let (mut dm, mut de, mut dmf) = (0.0, 0.0, 0.0);
                let frac = if c.m > 0.0 { c.mf / c.m } else { 0.0 };
                if m_e >= 0.0 {
                    dm -= m_e * dt;
                    de -= m_e * dt * CP * c.t;
                    dmf -= m_e * dt * frac;
                } else {
                    dm -= m_e * dt;
                    de -= m_e * dt * CP * ep_t;
                }
                // Gas pushed back into the intake carries its burned share; what comes in
                // again is only as fresh as the intake is.
                if m_i >= 0.0 {
                    dm -= m_i * dt;
                    de -= m_i * dt * CP * c.t;
                    dmf -= m_i * dt * frac;
                    self.pool_b = pool_m.min(self.pool_b + m_i * dt * (1.0 - frac));
                } else {
                    dm -= m_i * dt;
                    de -= m_i * dt * CP * rp_t;
                    let share = self.pool_b / pool_m;
                    dmf -= m_i * dt * (1.0 - share);
                    self.pool_b = (self.pool_b + m_i * dt * share).max(0.0);
                }
                // spark
                let spark = 720.0 - adv;
                let mut crossed = false;
                if dth > 0.0 {
                    let prev = c.last_local;
                    crossed = if prev < spark {
                        loc >= spark || loc < prev
                    } else {
                        false
                    };
                }
                c.last_local = loc;
                if crossed && c.burn < 0.0 {
                    let lost = self.misfire > 0.0 && self.rand() < self.misfire;
                    if !self.fuel_cut && !lost && c.mf > 0.0 {
                        let air = c.mf.min(c.m);
                        c.q = air / (AFR * spec.lambda)
                            * LHV
                            * spec.eff
                            * (1.0 + 0.06 * (self.rand() - 0.5));
                        c.burn = 0.0;
                        c.burn_dur = spec.burn * (1.0 + 0.1 * (self.rand() - 0.5));
                    }
                }
                if c.burn >= 0.0 {
                    let d0 = c.burn / c.burn_dur;
                    c.burn += dth * 180.0 / PI;
                    let d1 = (c.burn / c.burn_dur).min(1.0);
                    let x0 = 1.0 - (-5.0 * d0 * d0 * d0).exp();
                    let x1 = 1.0 - (-5.0 * d1 * d1 * d1).exp();
                    de += c.q * (x1 - x0);
                    if d1 >= 1.0 {
                        c.burn = -1.0;
                        dmf -= c.mf.min(c.q / (LHV * spec.eff) * AFR * spec.lambda);
                    }
                }
                // wall heat
                let area = 2.0 * self.ap + PI * spec.bore * (vn / self.ap);
                let h = 250.0 + 0.6 * w.abs() * spec.stroke / PI * 200.0;
                de -= h * area * (c.t - 450.0) * dt;
                // piston work
                de -= c.p * dv;
                c.m = (c.m + dm).max(1e-7);
                c.mf = c.m.min((c.mf + dmf).max(0.0));
                c.e = (c.m * CV * 250.0).max(c.e + de);
                c.v = vn;
                c.t = c.e / (c.m * CV);
                let pn = c.m * R * c.t / vn;
                block += pn - c.p;
                c.p = pn;
                self.cyl[ci] = c;
            }

            // plenum and throttle
            let mut m_pl = 0.0;
            for &ri in &self.run {
                let rp = &mut self.pipes[ri];
                rp.out0 = (pp - P0) - rp.arr0;
                m_pl += rp.area / rp.c * (rp.out0 - rp.arr0);
            }
            let m_th = flow(&self.psi, P0, self.inv0, pp, self.inv0, th_area * 0.8);
            self.plenum_m = (self.plenum_m + (m_th - m_pl) * dt).max(1e-6);

            // junctions
            for join in &self.joins {
                let (mut sy, mut sa) = (0.0, 0.0);
                for &(pi, side) in join {
                    let p = &self.pipes[pi];
                    let y = p.area / p.rho_c;
                    sy += y;
                    sa += y * if side == 0 { p.arr0 } else { p.arr1 };
                }
                let pj = 2.0 * sa / sy;
                for &(pi, side) in join {
                    let p = &mut self.pipes[pi];
                    if side == 0 {
                        p.out0 = pj - p.arr0;
                    } else {
                        p.out1 = pj - p.arr1;
                    }
                }
            }
            // open ends radiate
            let (mut l, mut r) = (0.0, 0.0);
            for o in &mut self.opens {
                let p = &mut self.pipes[o.pipe];
                o.s += 0.18 * (p.arr1 - o.s);
                p.out1 = -o.s * 0.97;
                let u = (p.arr1 - p.out1) * p.area / p.rho_c;
                let du = (u - o.u) * self.fs;
                o.u = u;
                o.hp = 0.995 * (o.hp + du - o.hpx);
                o.hpx = du;
                l += o.hp * (1.0 - o.pan) * 0.5 + o.hp * 0.25;
                r += o.hp * (1.0 + o.pan) * 0.5 + o.hp * 0.25;
            }
            for p in &mut self.pipes {
                p.write();
            }

            // intake mouth and block
            let du = (m_th - self.intake_u) * self.fs;
            self.intake_u = m_th;
            self.intake_hp = 0.995 * (self.intake_hp + du - self.intake_x);
            self.intake_x = du;
            self.block_hp = 0.97 * (self.block_hp + block - self.block_x);
            self.block_x = block;
            let iv = self.intake_hp * spec.intake_gain;
            let bl = self.block_hp * spec.block_gain;
            let mut mech_out = 0.0;
            if let Some(mech) = &spec.mech {
                // The iron rings: valve clicks and the combustion's pressure rise, through the
                // head and block's modes.
                let drive = clicks + mech.comb * self.block_hp;
                for (m, g) in &mut self.modes {
                    mech_out += m.run(drive) * *g;
                }
                // Broadband: rattle with speed (pulsing a little with each firing), hiss with
                // the flow.
                let krpm = w.abs() * 60.0 / (2.0 * PI) / 1000.0;
                let pulse = 1.0 + 0.15 * (self.theta * ncyl as f64 / 2.0).cos();
                let tail: f64 = self.opens.iter().map(|o| o.u * o.u).sum();
                let vac = (1.0 - pp / P0).max(0.0);
                let amp = mech.rattle * krpm.powf(1.4) * pulse
                    + mech.exhaust_hiss * tail * 1e4
                    + mech.intake_hiss * m_th.abs() * 100.0 * (vac + 0.05).sqrt();
                let mut v = self.noise() * amp;
                for f in &mut self.shape {
                    v = f.run(v);
                }
                mech_out += v;
            }
            let s = spec.gain;
            left[k] = ((l + iv + bl + mech_out) * s).tanh() as f32;
            right[k] = ((r + iv + bl + mech_out) * s).tanh() as f32;
        }
    }
}

// ---- The instrument ----

/// Frames between updates of the crank speed's target.
const CHUNK: usize = 32;
/// Time constant of the spin-down when the fuel is cut and the engine stops, in seconds.
const STOP_SECONDS: f64 = 0.3;
/// Below this crank speed (rad/s, about 20 rpm) a stopping engine fades out and is silent.
const STOPPED_OMEGA: f64 = 2.0;
/// Seconds of that fade.
const FADE_SECONDS: f64 = 0.03;
/// Above idle by this much (rpm) the fuel is cut while idling down (REDLINE's overrun cut).
const OVERRUN_RPM: f64 = 900.0;

#[derive(Clone, Copy, PartialEq)]
enum State {
    Off,
    /// Following the held key.
    Driven,
    /// No key held: falling to idle, or stopping.
    Released,
}

/// The instrument: REDLINE's eight engines, made at once, one of them sounding.
pub struct EngineSound {
    rate: f64,
    applied: [i32; COUNT],
    sims: Vec<EngineSim>,
    current: usize,
    state: State,
    /// The keys held, the last pressed at the top.
    held: [u8; 128],
    depth: usize,
    velocity: u8,
    bend: f64,
    /// The fade of a stopping engine: 1 until it stands still, then down to 0.
    fade: f64,
}

/// rad/s for `rpm`.
fn omega_of(rpm: f64) -> f64 {
    rpm * PI / 30.0
}

/// The crank speed (rpm) at which an engine of `cylinders` fires at the frequency of MIDI
/// note `note`, kept between `idle` and `redline`.
pub fn rpm_for_note(note: f64, cylinders: usize, idle: f64, redline: f64) -> f64 {
    let hz = 440.0 * ((note - 69.0) / 12.0).exp2();
    (hz * 120.0 / cylinders.max(1) as f64).clamp(idle, redline)
}

impl EngineSound {
    pub fn new(rate: u32, values: &[f32]) -> Self {
        let fs = f64::from(rate.max(1));
        let mut e = EngineSound {
            rate: fs,
            applied: stored(&PARAMS, values),
            sims: SPECS.iter().map(|s| EngineSim::new(s, fs)).collect(),
            current: 0,
            state: State::Off,
            held: [0; 128],
            depth: 0,
            velocity: 100,
            bend: 0.0,
            fade: 1.0,
        };
        e.current = e.engine_index();
        e.apply_sim_settings();
        e
    }

    fn engine_index(&self) -> usize {
        (self.applied[ENGINE].max(0) as usize).min(self.sims.len() - 1)
    }

    fn apply_sim_settings(&mut self) {
        let muffler = f64::from(self.applied[MUFFLER]) / 100.0;
        let misfire = f64::from(self.applied[MISFIRE]) / 100.0;
        for s in &mut self.sims {
            if s.muffler != muffler {
                s.set_muffler(muffler);
            }
            s.misfire = misfire;
        }
    }

    fn top(&self) -> Option<u8> {
        self.depth.checked_sub(1).map(|i| self.held[i])
    }

    fn remove_held(&mut self, note: u8) {
        if let Some(i) = self.held[..self.depth].iter().position(|n| *n == note) {
            self.held.copy_within(i + 1..self.depth, i);
            self.depth -= 1;
        }
    }

    fn stop_now(&mut self) {
        self.state = State::Off;
        self.depth = 0;
        self.fade = 1.0;
        let i = self.current;
        self.sims[i].reset();
    }

    /// The target crank speed (rad/s), throttle and fuel cut for the next chunk, and the time
    /// constant (seconds) of the move to that speed.
    fn controls(&self) -> (f64, f64, bool, f64) {
        let spec = self.sims[self.current].spec;
        let response = f64::from(self.applied[RESPONSE]) / 1000.0;
        match (self.state, self.top()) {
            (State::Driven, Some(note)) => {
                let rpm = rpm_for_note(
                    f64::from(note) + self.bend,
                    spec.order.len(),
                    spec.idle,
                    spec.redline,
                );
                let amount = f64::from(self.applied[VELOCITY]) / 100.0;
                let vel = f64::from(self.velocity) / 127.0;
                let throttle =
                    f64::from(self.applied[THROTTLE]) / 100.0 * (1.0 - amount + amount * vel);
                (omega_of(rpm), throttle, false, response)
            }
            _ if self.applied[RELEASE] != 0 => {
                let rpm = self.sims[self.current].omega * 30.0 / PI;
                let overrun = rpm > spec.idle + OVERRUN_RPM;
                (omega_of(spec.idle), 0.0, overrun, response)
            }
            _ => (0.0, 0.0, true, STOP_SECONDS),
        }
    }
}

impl NativeInst for EngineSound {
    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else {
            return;
        };
        self.applied[index] = p.stored(value);
        match index {
            ENGINE => {
                let next = self.engine_index();
                if next != self.current {
                    // The new engine at the old one's speed, as REDLINE's worklet does.
                    let omega = self.sims[self.current].omega;
                    self.sims[next].reset();
                    self.sims[next].omega = omega;
                    self.current = next;
                }
            }
            MUFFLER | MISFIRE => self.apply_sim_settings(),
            _ => {}
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.applied.get(index).map_or(0.0, |v| *v as f32)
    }

    fn note_on(&mut self, note: u8, velocity: u8) {
        self.remove_held(note);
        if self.depth < self.held.len() {
            self.held[self.depth] = note;
            self.depth += 1;
        }
        self.velocity = velocity;
        if self.state == State::Off {
            // The engine starts at its idle speed.
            let i = self.current;
            self.sims[i].reset();
            self.sims[i].omega = omega_of(self.sims[i].spec.idle);
        }
        self.fade = 1.0;
        self.state = State::Driven;
    }

    fn note_off(&mut self, note: u8) {
        self.remove_held(note);
        if self.depth == 0 && self.state == State::Driven {
            self.state = State::Released;
        }
    }

    fn all_notes_off(&mut self) {
        self.depth = 0;
        if self.state == State::Driven {
            self.state = State::Released;
        }
    }

    fn all_sound_off(&mut self) {
        self.stop_now();
    }

    fn bend(&mut self, semitones: f32) {
        self.bend = f64::from(semitones);
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.state == State::Off {
            left.fill(0.0);
            right.fill(0.0);
            return;
        }
        let gain = 10f64.powf(f64::from(self.applied[LEVEL]) / 20.0);
        let fade_step = 1.0 / (FADE_SECONDS * self.rate).max(1.0);
        let n = left.len().min(right.len());
        let mut at = 0;
        while at < n {
            let end = (at + CHUNK).min(n);
            let (target, throttle, cut, tau) = self.controls();
            let sim = &mut self.sims[self.current];
            sim.throttle = throttle;
            sim.fuel_cut = cut;
            let step = 1.0 - (-((end - at) as f64) / (tau * self.rate).max(1.0)).exp();
            let next = sim.omega + (target - sim.omega) * step;
            sim.process(&mut left[at..end], &mut right[at..end], next);
            let stopping = self.state == State::Released && self.applied[RELEASE] == 0;
            for (l, r) in left[at..end].iter_mut().zip(right[at..end].iter_mut()) {
                if stopping && sim.omega < STOPPED_OMEGA {
                    self.fade = (self.fade - fade_step).max(0.0);
                }
                let g = (gain * self.fade) as f32;
                *l *= g;
                *r *= g;
            }
            at = end;
            if self.fade <= 0.0 {
                left[at..n].fill(0.0);
                right[at..n].fill(0.0);
                self.stop_now();
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::open_native_instrument;
    use super::super::test_notes::{peak, pitch, play, sounds_and_falls_silent};
    use super::*;

    #[test]
    fn engine_does_not_allocate() {
        // A stopping engine takes up to about two seconds to stand still.
        sounds_and_falls_silent(ID, 48000, &[], 2.5);
        sounds_and_falls_silent(ID, 44100, &[], 2.5);
        // Every engine sounds, finite, without allocation.
        for k in 0..ENGINE_NAMES.len() {
            let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
            values[ENGINE] = k as f32;
            sounds_and_falls_silent(ID, 48000, &values, 2.5);
        }
    }

    #[test]
    fn the_note_sets_the_firing_rate() {
        // An inline four fires twice a turn: C3 (130.8 Hz) is 3,924 rpm.
        assert!((rpm_for_note(48.0, 4, 850.0, 7200.0) - 3924.0).abs() < 1.0);
        // A V8: A3 (220 Hz) is 3,300 rpm.
        assert!((rpm_for_note(57.0, 8, 720.0, 6600.0) - 3300.0).abs() < 0.5);
        // Below idle and above the redline: kept there.
        assert_eq!(rpm_for_note(0.0, 4, 850.0, 7200.0), 850.0);
        assert_eq!(rpm_for_note(127.0, 4, 850.0, 7200.0), 7200.0);
        // Heard: the Inline-4, its speed settled, sounds at the note's frequency.
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[RESPONSE] = 5.0;
        values[THROTTLE] = 100.0;
        for (note, hz) in [(43u8, 97.999), (48, 130.813), (55, 195.998)] {
            let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
                panic!("does not open")
            };
            let (l, r) = play(&mut inst, 48000, 256, &[(0, 0x90, note, 127)]);
            let mono: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a + b).collect();
            let got = pitch(&mono[24000..], 48000.0, 60.0, 400.0);
            assert!(
                (got / hz - 1.0).abs() < 0.03,
                "note {note}: {got} Hz, not {hz}"
            );
        }
    }

    /// Release Idle keeps the engine running after the key; Stop lets it stand still.
    #[test]
    fn release_idles_or_stops() {
        let mut values: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
        values[RELEASE] = 1.0;
        let Some(mut inst) = open_native_instrument(ID, 48000, &values) else {
            panic!("does not open")
        };
        let _ = play(
            &mut inst,
            48000,
            256,
            &[(0, 0x90, 45, 100), (12000, 0x80, 45, 0)],
        );
        let (l, _) = play(&mut inst, 48000, 256, &[]);
        assert!(peak(&l[24000..]) > 0.005, "idling: {}", peak(&l[24000..]));
        // All Sound Off silences it at once.
        let (l, _) = play(&mut inst, 4800, 256, &[(0, 0xb0, 120, 0)]);
        assert_eq!(peak(&l[256..]), 0.0);
    }

    /// A second key moves the engine to it; letting it go goes back to the first.
    #[test]
    fn the_last_key_held_is_followed() {
        let mut e = EngineSound::new(48000, &[]);
        e.note_on(40, 100);
        e.note_on(52, 100);
        assert_eq!(e.top(), Some(52));
        e.note_off(52);
        assert_eq!(e.top(), Some(40));
        assert!(e.state == State::Driven);
        e.note_off(40);
        assert!(e.state == State::Released);
    }

    #[test]
    fn every_engine_is_a_preset() {
        assert_eq!(PRESETS.len(), SPECS.len());
        for (k, p) in PRESETS.iter().enumerate() {
            let mut v: Vec<f32> = PARAMS.iter().map(|p| p.default).collect();
            p.apply(&mut v);
            assert_eq!(v[ENGINE], k as f32);
            assert_eq!(value_text(ENGINE, k as i32), p.name);
        }
        let info = super::super::native_instrument_info(ID).map(|i| i.presets.len());
        assert_eq!(info, Some(8));
    }

    /// The firing order: an even four fires every 180 degrees, the V-twin at 315 then 405.
    #[test]
    fn the_phases_follow_the_firing_order() {
        assert_eq!(
            phases_for(&[1, 3, 4, 2], None),
            vec![0.0, 540.0, 180.0, 360.0]
        );
        assert_eq!(phases_for(&[1, 2], Some(&[315.0, 405.0])), vec![0.0, 315.0]);
        // The network: one header and one runner for each cylinder, and one tailpipe for a
        // single exhaust, two for a dual one.
        for (k, s) in SPECS.iter().enumerate() {
            let net = Net::build(s);
            assert_eq!(net.exh.len(), s.order.len(), "{k}");
            assert_eq!(net.opens.len(), if s.plan.dual { 2 } else { 1 }, "{k}");
            assert_eq!(
                net.pipes.iter().filter(|p| p.muffler).count(),
                net.opens.len()
            );
        }
    }
}
