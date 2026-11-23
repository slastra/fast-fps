//! Performance measurement: `perf_reset` and `perf`, frame-time and GPU
//! probe values, and `vid_fullscreen` for measuring at a display's real size.
//!
//! A measurement window starts at `perf_reset` and ends at `perf <label>`,
//! which prints one line:
//!
//! ```text
//! perf arena: 6.9 ms avg (145 fps), 9.8 ms p99, 41 ms worst, gpu 5.6 ms, 412 frames
//!   gpu: main_opaque_pass_3d 2.1, shadows 1.6, ssao 0.8, ...
//! ```
//!
//! GPU time per render pass comes from Bevy's `RenderDiagnosticsPlugin`,
//! which uses GPU timestamp queries. It is only added when the game is started
//! with `HARNESS` or `GPU_TIMINGS` set, because the queries cost a little.
//! Without it, `perf` reports frame times only.

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
    let timed = std::env::var_os("HARNESS").is_some() || std::env::var_os("GPU_TIMINGS").is_some();
    if timed {
        app.add_plugins(RenderDiagnosticsPlugin);
    }
    app.init_resource::<FrameWindow>()
        .add_systems(Last, record_frame)
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

/// Frame times (ms) since the last `perf_reset`.
#[derive(Resource, Default)]
struct FrameWindow {
    frames: Vec<f32>,
    /// GPU time per pass, summed over the window: (pass, total ms, samples).
    gpu: Vec<(String, f64, u32)>,
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
        let mut passes: Vec<(String, f64)> = self
            .gpu
            .iter()
            .map(|(name, total, n)| (name.clone(), total / f64::from(*n).max(1.0)))
            .collect();
        passes.sort_by(|a, b| b.1.total_cmp(&a.1));
        // Top-level passes only, so nested spans aren't counted twice.
        let gpu = passes
            .iter()
            .filter(|(name, _)| !name.contains('/'))
            .map(|(_, ms)| ms)
            .sum::<f64>();
        Some(Summary {
            avg,
            p99,
            worst,
            frames: sorted.len(),
            gpu: (!passes.is_empty()).then_some(gpu),
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
    passes: Vec<(String, f64)>,
}

/// Record this frame's time and GPU pass timings.
fn record_frame(
    time: Res<Time<Real>>,
    diagnostics: Option<Res<DiagnosticsStore>>,
    mut window: ResMut<FrameWindow>,
    mut probe: ResMut<Probe>,
) {
    let ms = time.delta_secs() * 1000.0;
    if ms <= 0.0 {
        return;
    }
    window.frames.push(ms);
    if let Some(diagnostics) = diagnostics {
        for diagnostic in diagnostics.iter() {
            let path = diagnostic.path().as_str();
            let Some(pass) = path
                .strip_prefix("render/")
                .and_then(|p| p.strip_suffix("/elapsed_gpu"))
            else {
                continue;
            };
            let Some(value) = diagnostic.value() else {
                continue;
            };
            match window.gpu.iter_mut().find(|(name, ..)| name == pass) {
                Some(entry) => {
                    entry.1 += value;
                    entry.2 += 1;
                }
                None => window.gpu.push((pass.to_string(), value, 1)),
            }
        }
    }
    if let Some(summary) = window.summary() {
        probe.set("frame_ms", format!("{:.2}", summary.avg));
        probe.set("frame_ms_p99", format!("{:.2}", summary.p99));
        if let Some(gpu) = summary.gpu {
            probe.set("gpu_ms", format!("{gpu:.2}"));
        }
    }
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
                print.write(Print(format!(
                    "perf {label}: {:.2} ms avg ({:.0} fps), {:.2} ms p99, {:.1} ms worst, {gpu}, {} frames",
                    s.avg,
                    1000.0 / s.avg,
                    s.p99,
                    s.worst,
                    s.frames
                )));
                if !s.passes.is_empty() {
                    let top: Vec<String> = s
                        .passes
                        .iter()
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
