//! Ids for tracks and regions that stay the same from one call to the next.
//!
//! The project keeps tracks and regions in lists and tells them apart by their place, which
//! changes when something before them is added, removed or moved. A client needs a name that
//! stays, so this table gives each track an id ("t3") and each region one ("r12") and, before
//! and after every call, matches the project against what it last saw: a track or region that
//! is still there keeps its id wherever it is now; a new one gets a new id; ids of things that
//! are gone are never used again.
//!
//! Matching goes by what a thing is (its name, its kind, where it is, how long it is), in
//! steps from exact to loose, so one change at a time (a move, a resize, a rename, a track put
//! elsewhere) is always followed. What it cannot follow is two things that look the same
//! changing at once, or a thing changed in several ways between two calls by hand in the
//! window: such a track or region comes back with a new id.
use crate::model::{Project, Region, Track, TrackKind};

#[derive(Clone, PartialEq, Debug)]
struct TrackSeen {
    id: u64,
    name: String,
    kind: TrackKind,
    regions: Vec<RegionSeen>,
}

#[derive(Clone, PartialEq, Debug)]
struct RegionSeen {
    id: u64,
    name: String,
    start: u32,
    len: u32,
    audio: bool,
}

impl RegionSeen {
    fn of(id: u64, r: &Region) -> Self {
        Self {
            id,
            name: r.name.clone(),
            start: r.start_bar.to_bits(),
            len: r.len_bars.to_bits(),
            audio: r.is_audio(),
        }
    }
}

#[derive(Default)]
pub struct Ids {
    tracks: Vec<TrackSeen>,
    next_track: u64,
    next_region: u64,
}

impl Ids {
    /// Matches the table to the project as it is now.
    pub fn reconcile(&mut self, p: &Project) {
        let old = std::mem::take(&mut self.tracks);
        let mut used = vec![false; old.len()];
        let same_count = old.len() == p.tracks.len();
        let mut picked: Vec<Option<usize>> = vec![None; p.tracks.len()];
        let fits = |o: &TrackSeen, t: &Track| o.name == t.name && o.kind == t.kind;
        // In its place, as it was.
        for (i, t) in p.tracks.iter().enumerate() {
            if old.get(i).is_some_and(|o| fits(o, t)) {
                picked[i] = Some(i);
                used[i] = true;
            }
        }
        // Somewhere else (tracks were added, removed or put in another order).
        for (i, t) in p.tracks.iter().enumerate() {
            if picked[i].is_none() {
                if let Some(k) = (0..old.len()).find(|&k| !used[k] && fits(&old[k], t)) {
                    picked[i] = Some(k);
                    used[k] = true;
                }
            }
        }
        // In its place under another name.
        if same_count {
            for (i, t) in p.tracks.iter().enumerate() {
                if picked[i].is_none() && !used[i] && old[i].kind == t.kind {
                    picked[i] = Some(i);
                    used[i] = true;
                }
            }
        }
        // The regions of the tracks that are gone can still be found on other tracks.
        let mut pool: Vec<(u64, RegionSeen, bool)> = Vec::new();
        let mut tracks: Vec<TrackSeen> = Vec::with_capacity(p.tracks.len());
        for (i, t) in p.tracks.iter().enumerate() {
            let (id, regions) = match picked[i] {
                Some(k) => (old[k].id, old[k].regions.clone()),
                None => {
                    self.next_track += 1;
                    (self.next_track, Vec::new())
                }
            };
            for r in regions {
                pool.push((id, r, false));
            }
            tracks.push(TrackSeen {
                id,
                name: t.name.clone(),
                kind: t.kind,
                regions: Vec::new(),
            });
        }
        for (k, o) in old.iter().enumerate() {
            if !used[k] {
                for r in &o.regions {
                    pool.push((0, r.clone(), false));
                }
            }
        }
        // Regions: exact first, then one thing changed, then moved to another track.
        let total: usize = p.tracks.iter().map(|t| t.regions.len()).sum();
        let mut found: Vec<Vec<Option<u64>>> = p
            .tracks
            .iter()
            .map(|t| vec![None; t.regions.len()])
            .collect();
        type Rule = fn(&RegionSeen, &RegionSeen) -> bool;
        let rules: [(bool, Rule); 6] = [
            (true, |a, b| {
                a.name == b.name && a.start == b.start && a.len == b.len
            }),
            (true, |a, b| a.name == b.name && a.len == b.len),
            (true, |a, b| a.name == b.name && a.start == b.start),
            (true, |a, b| a.start == b.start && a.len == b.len),
            (false, |a, b| a.name == b.name && a.len == b.len),
            // Moved and resized at once (Fit to Grid does this to a region it fits).
            (true, |a, b| a.name == b.name),
        ];
        // A very large project is matched exactly only (the looser steps compare every
        // region with every other).
        let steps = if total.saturating_mul(pool.len()) > 4_000_000 {
            1
        } else {
            rules.len()
        };
        for (same_track, rule) in rules.iter().take(steps) {
            for (ti, t) in p.tracks.iter().enumerate() {
                for (ri, r) in t.regions.iter().enumerate() {
                    if found[ti][ri].is_some() {
                        continue;
                    }
                    let now = RegionSeen::of(0, r);
                    let hit = pool.iter_mut().find(|(track, seen, taken)| {
                        !*taken
                            && seen.audio == now.audio
                            && (!*same_track || *track == tracks[ti].id)
                            && rule(seen, &now)
                    });
                    if let Some((_, seen, taken)) = hit {
                        *taken = true;
                        found[ti][ri] = Some(seen.id);
                    }
                }
            }
        }
        // What is left: a track with one region not found yet and one old region not taken
        // has the same region, whatever changed about it.
        if steps == rules.len() {
            for (ti, t) in p.tracks.iter().enumerate() {
                let open: Vec<usize> = (0..t.regions.len())
                    .filter(|ri| found[ti][*ri].is_none())
                    .collect();
                let [ri] = open[..] else {
                    continue;
                };
                let audio = t.regions[ri].is_audio();
                let mut left = pool.iter_mut().filter(|(track, seen, taken)| {
                    !*taken && *track == tracks[ti].id && seen.audio == audio
                });
                if let (Some((_, seen, taken)), None) = (left.next(), left.next()) {
                    *taken = true;
                    found[ti][ri] = Some(seen.id);
                }
            }
        }
        for (ti, t) in p.tracks.iter().enumerate() {
            for (ri, r) in t.regions.iter().enumerate() {
                let id = found[ti][ri].unwrap_or_else(|| {
                    self.next_region += 1;
                    self.next_region
                });
                tracks[ti].regions.push(RegionSeen::of(id, r));
            }
        }
        self.tracks = tracks;
    }

    /// The id of the track at `index` ("t3").
    pub fn track_id(&self, index: usize) -> String {
        self.tracks
            .get(index)
            .map_or_else(|| "t0".to_string(), |t| format!("t{}", t.id))
    }

    /// Where the track with this id is now.
    pub fn track_index(&self, id: &str) -> Option<usize> {
        let n: u64 = id.strip_prefix('t')?.parse().ok()?;
        self.tracks.iter().position(|t| t.id == n)
    }

    /// The id of region `region` of track `track` ("r12").
    pub fn region_id(&self, track: usize, region: usize) -> String {
        self.tracks
            .get(track)
            .and_then(|t| t.regions.get(region))
            .map_or_else(|| "r0".to_string(), |r| format!("r{}", r.id))
    }

    /// Where the region with this id is now: (track, region).
    pub fn region_at(&self, id: &str) -> Option<(usize, usize)> {
        let n: u64 = id.strip_prefix('r')?.parse().ok()?;
        self.tracks
            .iter()
            .enumerate()
            .find_map(|(ti, t)| t.regions.iter().position(|r| r.id == n).map(|ri| (ti, ri)))
    }

    /// Every track id, in order (for the events).
    pub fn track_ids(&self) -> Vec<String> {
        (0..self.tracks.len()).map(|i| self.track_id(i)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Region, RegionParams};

    fn region(name: &str, start: f32, len: f32) -> Region {
        Region {
            name: name.into(),
            start_bar: start,
            len_bars: len,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            audio: None,
            missing: None,
            params: RegionParams::default(),
            generated: None,
        }
    }

    fn project() -> Project {
        let mut p = Project::empty();
        p.add_track(TrackKind::Instrument);
        p.add_track(TrackKind::Audio);
        p.tracks[0].regions.push(region("Verse", 1.0, 4.0));
        p.tracks[0].regions.push(region("Verse", 5.0, 4.0));
        p.tracks[1].regions.push(region("Bass", 1.0, 8.0));
        p
    }

    #[test]
    fn ids_stay_through_moves_resizes_renames_and_reordering() {
        let mut p = project();
        let mut ids = Ids::default();
        ids.reconcile(&p);
        let (t0, t1, t2) = (ids.track_id(0), ids.track_id(1), ids.track_id(2));
        let (a, b, c) = (
            ids.region_id(0, 0),
            ids.region_id(0, 1),
            ids.region_id(1, 0),
        );
        assert_eq!([a.as_str(), b.as_str(), c.as_str()], ["r1", "r2", "r3"]);
        assert_ne!(t0, t1);

        // One of two regions with the same name and length moves: the other one keeps its id.
        p.tracks[0].regions[1].start_bar = 9.0;
        ids.reconcile(&p);
        assert_eq!(
            (ids.region_id(0, 0), ids.region_id(0, 1)),
            (a.clone(), b.clone())
        );

        // A resize, then a rename.
        p.tracks[1].regions[0].len_bars = 2.0;
        ids.reconcile(&p);
        p.tracks[1].regions[0].name = "Sub".into();
        ids.reconcile(&p);
        assert_eq!(ids.region_id(1, 0), c);

        // A region goes to another track.
        let moved = p.tracks[0].regions.remove(0);
        p.tracks[1].regions.push(moved);
        ids.reconcile(&p);
        assert_eq!(ids.region_at(&a), Some((1, 1)));
        assert_eq!(ids.region_at(&b), Some((0, 0)));

        // Moved and resized at once, then renamed, moved and resized at once (the only
        // region of its track that changed).
        p.tracks[1].regions[0].start_bar = 3.0;
        p.tracks[1].regions[0].len_bars = 7.0;
        ids.reconcile(&p);
        assert_eq!(ids.region_id(1, 0), c);
        let r = &mut p.tracks[1].regions[0];
        (r.name, r.start_bar, r.len_bars) = ("Low".into(), 1.0, 2.0);
        ids.reconcile(&p);
        assert_eq!(ids.region_id(1, 0), c);

        // The tracks change places, and one is renamed.
        p.tracks.swap(0, 2);
        ids.reconcile(&p);
        assert_eq!(
            (ids.track_index(&t0), ids.track_index(&t2)),
            (Some(2), Some(0))
        );
        p.tracks[1].name = "Lead".into();
        ids.reconcile(&p);
        assert_eq!(ids.track_index(&t1), Some(1));
        assert_eq!(ids.region_at(&c), Some((1, 0)));

        // A removed track's id is gone for good; a new track gets a new one.
        p.tracks.remove(0);
        ids.reconcile(&p);
        assert_eq!(ids.track_index(&t2), None);
        let n = p.add_track(TrackKind::Audio);
        ids.reconcile(&p);
        let new = ids.track_id(n);
        assert!(![t0, t1, t2].contains(&new), "{new} was used before");
        assert_eq!(ids.region_at("r999"), None);
        assert_eq!(ids.track_index("nonsense"), None);
    }
}
