//! VIVAD_LIFE_AB=1 prices the market in one run, with the camera held still: each step below takes
//! its turn between two turns of the plain market, so whatever else the Mac is doing falls on both
//! alike, and the difference of each step from the plain turns round it is printed. Nothing else can
//! price a thing on a machine that is busy building, where two runs differ by more than the market
//! costs. Run it on a tour view held long enough: VIVAD_TOUR_ONLY=karol_bagh VIVAD_TOUR_SETTLE=9000.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::camera::visibility::VisibilityRange;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::batch::probe::Tagged;

/// Stops the people's pages turning.
pub static FROZEN: AtomicBool = AtomicBool::new(false);

/// What a step switches off.
#[derive(Clone, Copy, Default)]
struct Step {
    name: &'static str,
    hide: &'static [&'static str],
    abrupt: bool,
    culled: bool,
    no_cast: bool,
    frozen: bool,
}

const OVER: &str = "life over";
const NEAR: &str = "life goods";
const FAR: &str = "life far";
const PEOPLE: &str = "life";
const STEPS: [Step; 11] = [
    Step { name: "all on", hide: &[], abrupt: false, culled: false, no_cast: false, frozen: false },
    Step { name: "no goods near", hide: &[NEAR], abrupt: false, culled: false, no_cast: false, frozen: false },
    Step { name: "no goods far", hide: &[FAR], abrupt: false, culled: false, no_cast: false, frozen: false },
    Step { name: "no overhead", hide: &[OVER], abrupt: false, culled: false, no_cast: false, frozen: false },
    Step { name: "no goods", hide: &[NEAR, FAR, OVER], abrupt: false, culled: false, no_cast: false, frozen: false },
    Step { name: "no people", hide: &[PEOPLE], abrupt: false, culled: false, no_cast: false, frozen: false },
    Step { name: "no life", hide: &[NEAR, FAR, OVER, PEOPLE], abrupt: false, culled: false, no_cast: false, frozen: false },
    Step { name: "abrupt ranges", hide: &[], abrupt: true, culled: false, no_cast: false, frozen: false },
    Step { name: "backs culled", hide: &[], abrupt: false, culled: true, no_cast: false, frozen: false },
    Step { name: "no life shadows", hide: &[], abrupt: false, culled: false, no_cast: true, frozen: false },
    Step { name: "pages still", hide: &[], abrupt: false, culled: false, no_cast: false, frozen: true },
];
/// Frames of a step, the first few of which are the change settling.
const FRAMES: usize = 18;
const SETTLING: usize = 8;
/// Seconds in before the first step: the people are baked by then.
const FROM: f32 = 22.0;

#[derive(Default)]
pub struct Run {
    /// The step showing: every other turn is the first, the plain market.
    step: usize,
    next: usize,
    frame: usize,
    rounds: usize,
    frames: Vec<f32>,
    /// The last two plain turns and the step between them.
    before: f32,
    tried: Option<(usize, f32)>,
    gaps: HashMap<usize, Vec<f32>>,
    ranges: HashMap<Entity, VisibilityRange>,
}

#[allow(clippy::type_complexity)]
pub fn step(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut run: Local<Run>,
    mut q: Query<(Entity, &Tagged, &mut Visibility, &mut VisibilityRange, &MeshMaterial3d<StandardMaterial>, Has<NotShadowCaster>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if time.elapsed_secs() < FROM {
        return;
    }
    let run = &mut *run;
    let now = STEPS[run.step];
    run.frame += 1;
    if run.frame > SETTLING {
        run.frames.push(time.delta_secs() * 1000.0);
    }
    if run.frame >= FRAMES {
        run.frame = 0;
        // The quickest frames but one: a busy Mac only ever adds to a frame, so these are what
        // this step costs, and not who else had the machine.
        run.frames.sort_by(f32::total_cmp);
        let median = run.frames[1];
        run.frames.clear();
        if run.step == 0 {
            if let Some((k, at)) = run.tried.take() {
                run.gaps.entry(k).or_default().push(at - (run.before + median) / 2.0);
            }
            run.before = median;
            run.next = run.next % (STEPS.len() - 1) + 1;
            run.step = run.next;
            if run.next == 1 {
                run.rounds += 1;
                let rows: Vec<String> = (1..STEPS.len())
                    .map(|k| {
                        let g = run.gaps.entry(k).or_default();
                        g.sort_by(f32::total_cmp);
                        format!("{} {:+.2}", STEPS[k].name, g.get(g.len() / 2).copied().unwrap_or(0.0))
                    })
                    .collect();
                println!("[life-ab] round {} frame {median:.2} ms: {}", run.rounds, rows.join(", "));
            }
        } else {
            run.tried = Some((run.step, median));
            run.step = 0;
        }
    }
    let want = STEPS[run.step];
    FROZEN.store(want.frozen, Ordering::Relaxed);
    for (e, tag, mut vis, mut range, material, none) in &mut q {
        if !tag.0.starts_with("life") || tag.0 == "life steam" {
            continue;
        }
        let goal = if want.hide.contains(&tag.0) { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != goal {
            *vis = goal;
        }
        let saved = run.ranges.entry(e).or_insert_with(|| range.clone());
        let goal = if want.abrupt { VisibilityRange { start_margin: saved.start_margin.start..saved.start_margin.start, end_margin: saved.end_margin.end..saved.end_margin.end, use_aabb: false } } else { saved.clone() };
        if *range != goal {
            *range = goal;
        }
        if want.no_cast != now.no_cast || (want.no_cast && !none) {
            if want.no_cast {
                commands.entity(e).insert(NotShadowCaster);
            } else if tag.0 == OVER {
                commands.entity(e).remove::<NotShadowCaster>();
            }
        }
        if want.culled != now.culled
            && tag.0 != PEOPLE
            && let Some(mut m) = materials.get_mut(&material.0)
            && (m.double_sided == want.culled)
        {
            m.double_sided = !want.culled;
            m.cull_mode = want.culled.then_some(bevy::render::render_resource::Face::Back);
        }
    }
}
