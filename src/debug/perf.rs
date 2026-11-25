//! Performance measurement: `perf_reset` and `perf`, frame-time and GPU
//! probe values, and `vid_fullscreen` for measuring at a display's real size.
//!
//! A measurement window starts at `perf_reset` and ends at `perf <label>`,
//! which prints one line:
//!
//! ```text
//! perf arena: 6.9 ms avg (145 fps), 9.8 ms p99, 41 ms worst, gpu 5.6 ms, cpu 1.2 ms (2.0 p99), 412 frames
//!   gpu: main_opaque_pass_3d 2.1, shadows 1.6, ssao 0.8, ...
//! ```
//!
//! `cpu` is the game's own update (input, gameplay, animation, UI), measured
//! from the start of the frame's `First` schedule to its `Last` schedule. It
//! doesn't include rendering work, which runs alongside on other threads.
//!
//! GPU time per render pass comes from Bevy's `RenderDiagnosticsPlugin`,
//! which uses GPU timestamp queries. It is only added when the game is started
//! with `HARNESS` or `GPU_TIMINGS` set, because the queries cost a little.
//! Without it, `perf` reports frame times only.
//!
//! `PERF_LOG=<file>` writes one line per second of play to that file: frame
//! times, GPU and CPU time, where the camera was, and whether the window was
//! focused or the console open. It is for finding out what a slow moment in
//! normal play was, without typing anything during it.

use bevy::{
    diagnostic::DiagnosticsStore,
    prelude::*,
    render::diagnostic::RenderDiagnosticsPlugin,
    window::{MonitorSelection, WindowMode},
};

use crate::{
    console::{ConsoleAppExt, ConsoleCommand, Print},
    debug::probe::Probe,
};

/// GPU passes listed by `perf`, after the total.
const TOP_PASSES: usize = 8;

pub(super) fn plugin(app: &mut App) {
    // On under the harness and with GPU_TIMINGS set; GPU_TIMINGS=0 turns it off.
    let timed = match std::env::var("GPU_TIMINGS") {
        Ok(v) => v != "0",
        Err(_) => std::env::var_os("HARNESS").is_some(),
    };
    if timed {
        app.add_plugins((
            RenderDiagnosticsPlugin,
            avian3d::prelude::PhysicsDiagnosticsPlugin,
        ));
    }
    if let Some(path) = std::env::var_os("PERF_LOG") {
        match std::fs::File::create(&path) {
            Ok(file) => {
                app.insert_resource(PerfLog {
                    file,
                    started: bevy::platform::time::Instant::now(),
                    second: FrameWindow::default(),
                    since: 0.0,
                });
            }
            Err(e) => warn!("PERF_LOG: can't create {}: {e}", path.display()),
        }
    }
    app.init_resource::<FrameWindow>()
        .init_resource::<UpdateStart>()
        .init_resource::<Seen>()
        .add_systems(First, start_update)
        .add_systems(
            Last,
            (record_frame, write_log.run_if(resource_exists::<PerfLog>)).chain(),
        )
        .add_systems(Update, perf_commands);

    app.console_command("perf_reset", "", "start a new performance measurement")
        .console_command(
            "perf",
            "[label]",
            "print frame and GPU times since perf_reset",
        )
        .console_command(
            "vid_fullscreen",
            "<0|1>",
            "borderless fullscreen on the current display",
        );
}

/// When this frame's update began.
#[derive(Resource, Default)]
struct UpdateStart(Option<bevy::platform::time::Instant>);

fn start_update(mut start: ResMut<UpdateStart>) {
    start.0 = Some(bevy::platform::time::Instant::now());
}

/// Frame times (ms) since the last `perf_reset`.
#[derive(Resource, Default)]
struct FrameWindow {
    frames: Vec<f32>,
    /// CPU time of the game's own update each frame (First to Last), which
    /// doesn't depend on the display or the GPU.
    updates: Vec<f32>,
    /// GPU time per pass, summed over the window: (pass, total ms, samples).
    gpu: Vec<(String, f64, u32)>,
}

/// When each diagnostic was last counted. A pass that stops running (an
/// effect switched off) keeps its last value in the store forever, so only
/// measurements newer than the last one counted are new data.
#[derive(Resource, Default)]
struct Seen(std::collections::HashMap<String, bevy::platform::time::Instant>);

/// The `PERF_LOG` file and the second being measured for it.
#[derive(Resource)]
struct PerfLog {
    file: std::fs::File,
    started: bevy::platform::time::Instant,
    second: FrameWindow,
    /// Seconds of real time in `second` so far.
    since: f32,
}

impl FrameWindow {
    /// Add one frame: its time, its update's CPU time, and fresh GPU samples.
    fn push(&mut self, ms: f32, update: Option<f32>, samples: &[(String, f64)]) {
        self.frames.push(ms);
        self.updates.extend(update);
        for (pass, value) in samples {
            match self.gpu.iter_mut().find(|(name, ..)| name == pass) {
                Some(entry) => {
                    entry.1 += value;
                    entry.2 += 1;
                }
                None => self.gpu.push((pass.clone(), *value, 1)),
            }
        }
    }
}

impl FrameWindow {
    fn summary(&self) -> Option<Summary> {
        if self.frames.is_empty() {
            return None;
        }
        let mut sorted = self.frames.clone();
        sorted.sort_by(f32::total_cmp);
        let avg = sorted.iter().sum::<f32>() / sorted.len() as f32;
        let p99 = sorted[((sorted.len() as f32 * 0.99) as usize).min(sorted.len() - 1)];
        let worst = *sorted.last().unwrap();
        let mut updates = self.updates.clone();
        updates.sort_by(f32::total_cmp);
        let update = (!updates.is_empty()).then(|| {
            let avg = updates.iter().sum::<f32>() / updates.len() as f32;
            let p99 = updates[((updates.len() as f32 * 0.99) as usize).min(updates.len() - 1)];
            (avg, p99)
        });
        let mut passes: Vec<(String, f64)> = self
            .gpu
            .iter()
            .map(|(name, total, n)| (name.clone(), total / f64::from(*n).max(1.0)))
            .collect();
        passes.sort_by(|a, b| b.1.total_cmp(&a.1));
        // Top-level passes only, so nested spans aren't counted twice.
        let gpu = passes
            .iter()
            .filter(|(name, _)| !name.contains('/') && !name.starts_with("physics:"))
            .map(|(_, ms)| ms)
            .sum::<f64>();
        Some(Summary {
            avg,
            p99,
            worst,
            frames: sorted.len(),
            gpu: (!passes.is_empty()).then_some(gpu),
            update,
            passes,
        })
    }
}

struct Summary {
    avg: f32,
    p99: f32,
    worst: f32,
    frames: usize,
    gpu: Option<f64>,
    /// Average and p99 of the update's CPU time.
    update: Option<(f32, f32)>,
    passes: Vec<(String, f64)>,
}

/// Record this frame's time and GPU pass timings.
fn record_frame(
    time: Res<Time<Real>>,
    start: Res<UpdateStart>,
    diagnostics: Option<Res<DiagnosticsStore>>,
    mut seen: ResMut<Seen>,
    mut window: ResMut<FrameWindow>,
    mut log: Option<ResMut<PerfLog>>,
    mut probe: ResMut<Probe>,
) {
    let ms = time.delta_secs() * 1000.0;
    if ms <= 0.0 {
        return;
    }
    let update = start.0.map(|start| start.elapsed().as_secs_f32() * 1000.0);
    let mut samples = Vec::new();
    if let Some(diagnostics) = diagnostics {
        for diagnostic in diagnostics.iter() {
            let path = diagnostic.path().as_str();
            let physics = path.strip_prefix("avian/").map(|p| format!("physics:{p}"));
            let Some(pass) = path
                .strip_prefix("render/")
                .and_then(|p| p.strip_suffix("/elapsed_gpu"))
                .map(str::to_string)
                .or(physics)
            else {
                continue;
            };
            let Some(measurement) = diagnostic.measurement() else {
                continue;
            };
            if seen.0.get(&pass) == Some(&measurement.time) {
                continue;
            }
            seen.0.insert(pass.clone(), measurement.time);
            samples.push((pass, measurement.value));
        }
    }
    window.push(ms, update, &samples);
    if let Some(log) = log.as_mut() {
        log.second.push(ms, update, &samples);
        log.since += ms / 1000.0;
    }
    if let Some(summary) = window.summary() {
        probe.set("frame_ms", format!("{:.2}", summary.avg));
        probe.set("frame_ms_p99", format!("{:.2}", summary.p99));
        if let Some(gpu) = summary.gpu {
            probe.set("gpu_ms", format!("{gpu:.2}"));
        }
        if let Some((avg, _)) = summary.update {
            probe.set("cpu_ms", format!("{avg:.2}"));
        }
    }
}

/// Each second, write that second's numbers to the `PERF_LOG` file.
fn write_log(
    mut log: ResMut<PerfLog>,
    console: Res<crate::console::Console>,
    camera: Query<&GlobalTransform, With<crate::render::WorldCamera>>,
    screen: Single<&Window>,
) {
    use std::io::Write;
    if log.since < 1.0 {
        return;
    }
    let Some(s) = log.second.summary() else {
        return;
    };
    let gpu = s.gpu.map_or("-".to_string(), |g| format!("{g:.2}"));
    let (cpu, cpu_p99) = s.update.unwrap_or((0.0, 0.0));
    let at = camera.iter().next().map_or("-".to_string(), |t| {
        let p = t.translation();
        format!("{:.0} {:.1} {:.0}", p.x, p.y, p.z)
    });
    let mut flags = Vec::new();
    if !screen.focused {
        flags.push("unfocused");
    }
    if console.open {
        flags.push("console");
    }
    let line = format!(
        "{:7.1}s  {:4.0} fps  avg {:5.2}  p99 {:6.2}  worst {:6.1}  gpu {gpu:>5}  cpu {cpu:5.2} ({cpu_p99:5.2})  {}x{} {:?}  at {at}  {}\n",
        log.started.elapsed().as_secs_f32(),
        1000.0 / s.avg,
        s.avg,
        s.p99,
        s.worst,
        screen.physical_width(),
        screen.physical_height(),
        screen.mode,
        flags.join(" "),
    );
    let _ = log.file.write_all(line.as_bytes());
    log.second = FrameWindow::default();
    log.since = 0.0;
}

fn perf_commands(
    mut cmds: MessageReader<ConsoleCommand>,
    mut window: ResMut<FrameWindow>,
    mut print: MessageWriter<Print>,
    mut screen: Single<&mut Window>,
) {
    for cmd in cmds.read() {
        match cmd.name.as_str() {
            "perf_reset" => *window = FrameWindow::default(),
            "perf" => {
                let label = cmd.arg(0).unwrap_or("now");
                let Some(s) = window.summary() else {
                    print.write(Print::error("perf: no frames measured yet"));
                    continue;
                };
                let gpu = s
                    .gpu
                    .map_or("gpu n/a".to_string(), |g| format!("gpu {g:.2} ms"));
                let (cpu, cpu_p99) = s.update.unwrap_or((0.0, 0.0));
                print.write(Print(format!(
                    "perf {label}: {:.2} ms avg ({:.0} fps), {:.2} ms p99, {:.1} ms worst, {gpu}, cpu {cpu:.2} ms ({cpu_p99:.2} p99), {} frames",
                    s.avg,
                    1000.0 / s.avg,
                    s.p99,
                    s.worst,
                    s.frames
                )));
                let physics: Vec<String> = s
                    .passes
                    .iter()
                    .filter(|(name, _)| name.starts_with("physics:"))
                    .map(|(name, v)| format!("{} {v:.2}", &name["physics:".len()..]))
                    .collect();
                if !physics.is_empty() {
                    print.write(Print(format!("  physics: {}", physics.join(", "))));
                }
                if !s.passes.is_empty() {
                    let top: Vec<String> = s
                        .passes
                        .iter()
                        .filter(|(name, _)| !name.starts_with("physics:"))
                        .take(TOP_PASSES)
                        .map(|(name, ms)| format!("{name} {ms:.2}"))
                        .collect();
                    print.write(Print(format!("  gpu: {}", top.join(", "))));
                }
            }
            "vid_fullscreen" => {
                screen.mode = match cmd.arg(0) {
                    Some("1") => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
                    Some("0") => WindowMode::Windowed,
                    _ => {
                        print.write(Print::error("usage: vid_fullscreen <0|1>"));
                        continue;
                    }
                };
            }
            _ => {}
        }
    }
}
