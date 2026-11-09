//! Four built-in demo songs, all originals. Each has a main groove in A and a
//! variation or fill in B; `arrangement` is the bar order the renderer plays.

use crate::pattern::{Grid, Master, Slot, Song, State, VoiceId, VoiceSettings, VOICES};
use VoiceId::*;

pub struct Demo {
    pub file: &'static str,
    pub song: Song,
    pub arrangement: Vec<Slot>,
}

fn vs(tune: f32, decay: f32, level: f32, tone: f32) -> VoiceSettings {
    VoiceSettings {
        tune,
        decay,
        level,
        tone,
        mute: false,
    }
}

fn kit(over: &[(VoiceId, VoiceSettings)]) -> [VoiceSettings; VOICES] {
    let mut k = [VoiceSettings::default(); VOICES];
    for (v, s) in over {
        k[v.index()] = *s;
    }
    k
}

fn boom_bap() -> Demo {
    let a = Grid::from_rows(&[
        (None, ".... x... .... x..."),
        (Some(Kick), "x... ...x x.x. ...."),
        (Some(Snare), ".... x... .... x..."),
        (Some(ClosedHat), "x.x. x.x. x.x. x.xx"),
        (Some(Rim), ".... .... ...x ...."),
    ]);
    let b = Grid::from_rows(&[
        (None, ".... x... .... x..x"),
        (Some(Kick), "x... ...x ..x. .x.."),
        (Some(Snare), ".... x..x .... x..x"),
        (Some(ClosedHat), "x.x. x.x. x.x. ..x."),
        (Some(OpenHat), ".... .... .... x..."),
        (Some(Rim), "..x. .... ...x ...."),
    ]);
    let st = State {
        tempo: 89.0,
        swing: 0.60,
        patterns: [a, b, Grid::default(), Grid::default()],
        voices: kit(&[
            (Kick, vs(-0.1, 0.45, 0.9, 0.55)),
            (Snare, vs(-0.15, 0.45, 0.8, 0.7)),
            (ClosedHat, vs(-0.2, 0.3, 0.5, 0.35)),
            (OpenHat, vs(-0.2, 0.25, 0.45, 0.35)),
            (Rim, vs(0.0, 0.3, 0.55, 0.4)),
        ]),
        master: Master {
            volume: 0.72,
            drive: 0.6,
            wobble: 0.25,
            age: 0.45,
            hiss: 0.2,
            crush: 0.25,
            reverb: 0.12,
            room: 0.2,
        },
    };
    Demo {
        file: "demo-dusty-boom-bap",
        song: Song {
            name: "Dusty Boom-Bap".into(),
            state: st,
        },
        arrangement: vec![Slot::A, Slot::A, Slot::A, Slot::B],
    }
}

fn electro() -> Demo {
    let a = Grid::from_rows(&[
        (None, "x... x... x... x.x."),
        (Some(Kick), "x... ...x ..x. ...."),
        (Some(Clap), ".... x... .... x..."),
        (Some(ClosedHat), "x.xx x.x. x.xx x.x."),
        (Some(Cowbell), "..x. .... ..x. .x.."),
        (Some(Conga), ".... ..x. .... ...."),
    ]);
    let b = Grid::from_rows(&[
        (None, "x... x... x... x.x."),
        (Some(Kick), "x... ...x ..x. ...."),
        (Some(Clap), ".... x... .... x..."),
        (Some(ClosedHat), "x.xx x.x. x.x. ...."),
        (Some(HighTom), ".... .... .... x.x."),
        (Some(MidTom), ".... .... .... .x.x"),
        (Some(LowTom), ".... .... .... ...x"),
        (Some(Cowbell), "..x. .... ..x. ...."),
    ]);
    let st = State {
        tempo: 116.0,
        swing: 0.52,
        patterns: [a, b, Grid::default(), Grid::default()],
        voices: kit(&[
            (Kick, vs(-0.3, 0.8, 0.95, 0.3)),
            (Clap, vs(0.1, 0.45, 0.75, 0.5)),
            (ClosedHat, vs(0.1, 0.2, 0.5, 0.65)),
            (Cowbell, vs(0.0, 0.35, 0.45, 0.5)),
            (Conga, vs(0.2, 0.4, 0.5, 0.4)),
            (LowTom, vs(0.0, 0.5, 0.7, 0.3)),
            (MidTom, vs(0.0, 0.5, 0.7, 0.3)),
            (HighTom, vs(0.0, 0.5, 0.7, 0.3)),
        ]),
        master: Master {
            volume: 0.7,
            drive: 0.3,
            wobble: 0.05,
            age: 0.1,
            hiss: 0.05,
            crush: 0.0,
            reverb: 0.22,
            room: 0.35,
        },
    };
    Demo {
        file: "demo-night-signal-electro",
        song: Song {
            name: "Night Signal Electro".into(),
            state: st,
        },
        arrangement: vec![Slot::A, Slot::A, Slot::A, Slot::B],
    }
}

fn west_coast() -> Demo {
    let a = Grid::from_rows(&[
        (None, ".... x... .... x..."),
        (Some(Kick), "x... ..x. x... ..x."),
        (Some(Snare), ".... x... .... x..."),
        (Some(Clap), ".... x... .... x..."),
        (Some(ClosedHat), "x... x.x. x... x.x."),
        (Some(OpenHat), "..x. .... ..x. ...."),
        (Some(Rim), ".... ...x .... ...."),
    ]);
    let b = Grid::from_rows(&[
        (None, ".... x... .... x..."),
        (Some(Kick), "x... ..x. x..x ..x."),
        (Some(Snare), ".... x... .... x.x."),
        (Some(Clap), ".... x... .... x..."),
        (Some(ClosedHat), "x... x.x. x... x..."),
        (Some(OpenHat), "..x. .... ..x. ..x."),
        (Some(Conga), ".x.. .... .x.. ...."),
    ]);
    let st = State {
        tempo: 92.0,
        swing: 0.64,
        patterns: [a, b, Grid::default(), Grid::default()],
        voices: kit(&[
            (Kick, vs(-0.35, 0.8, 0.95, 0.35)),
            (Snare, vs(0.1, 0.35, 0.55, 0.45)),
            (Clap, vs(-0.1, 0.55, 0.75, 0.6)),
            (ClosedHat, vs(0.0, 0.25, 0.42, 0.45)),
            (OpenHat, vs(0.0, 0.35, 0.4, 0.45)),
            (Rim, vs(0.1, 0.25, 0.45, 0.3)),
            (Conga, vs(-0.2, 0.45, 0.45, 0.3)),
        ]),
        master: Master {
            volume: 0.7,
            drive: 0.45,
            wobble: 0.15,
            age: 0.3,
            hiss: 0.1,
            crush: 0.0,
            reverb: 0.3,
            room: 0.45,
        },
    };
    Demo {
        file: "demo-low-sun-slow-roll",
        song: Song {
            name: "Low Sun Slow Roll".into(),
            state: st,
        },
        arrangement: vec![Slot::A, Slot::A, Slot::A, Slot::B],
    }
}

fn four_floor() -> Demo {
    let a = Grid::from_rows(&[
        (None, "x... x... x... x..."),
        (Some(Kick), "x... x... x... x..."),
        (Some(Clap), ".... x... .... x..."),
        (Some(ClosedHat), "x... x... x... x..."),
        (Some(OpenHat), "..x. ..x. ..x. ..x."),
        (Some(Rim), "...x .... ...x ..x."),
        (Some(Conga), ".x.. ...x .x.. ...."),
    ]);
    let b = Grid::from_rows(&[
        (None, "x... x... x... x.xx"),
        (Some(Kick), "x... x... x... x..."),
        (Some(Clap), ".... x... .... x..."),
        (Some(Snare), ".... .... ..x. x.xx"),
        (Some(ClosedHat), "x... x... x... x..."),
        (Some(OpenHat), "..x. ..x. ..x. ...."),
        (Some(Cowbell), ".... .... .... ..x."),
        (Some(Conga), ".x.. ...x .x.. ...."),
    ]);
    let st = State {
        tempo: 124.0,
        swing: 0.50,
        patterns: [a, b, Grid::default(), Grid::default()],
        voices: kit(&[
            (Kick, vs(0.0, 0.4, 0.95, 0.45)),
            (Clap, vs(0.0, 0.4, 0.7, 0.4)),
            (Snare, vs(0.2, 0.2, 0.5, 0.8)),
            (ClosedHat, vs(0.0, 0.15, 0.35, 0.6)),
            (OpenHat, vs(0.0, 0.2, 0.5, 0.6)),
            (Rim, vs(0.2, 0.2, 0.4, 0.5)),
            (Cowbell, vs(0.1, 0.3, 0.4, 0.5)),
            (Conga, vs(0.3, 0.35, 0.45, 0.4)),
        ]),
        master: Master {
            volume: 0.7,
            drive: 0.35,
            wobble: 0.05,
            age: 0.1,
            hiss: 0.05,
            crush: 0.0,
            reverb: 0.18,
            room: 0.3,
        },
    };
    Demo {
        file: "demo-warehouse-four",
        song: Song {
            name: "Warehouse Four".into(),
            state: st,
        },
        arrangement: vec![Slot::A, Slot::A, Slot::A, Slot::B],
    }
}

pub fn all() -> Vec<Demo> {
    vec![boom_bap(), electro(), west_coast(), four_floor()]
}

/// By file stem or name, case-insensitive; a unique prefix or substring of either works.
pub fn find(name: &str) -> Option<Demo> {
    let want = name.to_lowercase();
    let mut all = all();
    let norm = |d: &Demo| (d.file.to_string(), d.song.name.to_lowercase());
    if let Some(i) = all.iter().position(|d| {
        let (f, n) = norm(d);
        f == want || n == want
    }) {
        return Some(all.swap_remove(i));
    }
    let hits: Vec<usize> = (0..all.len())
        .filter(|&i| {
            let (f, n) = norm(&all[i]);
            f.contains(&want) || n.contains(&want)
        })
        .collect();
    if hits.len() == 1 {
        Some(all.swap_remove(hits[0]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demos_are_valid_and_survive_json() {
        for d in all() {
            let back = Song::from_json(&d.song.to_json()).unwrap();
            assert_eq!(back, d.song, "{}", d.file);
            assert!(!d.song.state.patterns[0].is_empty());
        }
    }

    #[test]
    fn find_by_name_or_fragment() {
        assert_eq!(find("boom").unwrap().file, "demo-dusty-boom-bap");
        assert_eq!(find("Warehouse Four").unwrap().file, "demo-warehouse-four");
        assert_eq!(find("electro").unwrap().file, "demo-night-signal-electro");
        assert!(find("demo").is_none(), "ambiguous");
    }
}
