use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bevy::prelude::*;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};

use super::config::AppConfig;

const MAX_SLOW_FRAME_PHASES: usize = 5;

#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq)]
pub struct PerformanceSessionId(pub u128);

impl PerformanceSessionId {
    pub fn new() -> Self {
        let value = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self(value)
    }
}

impl Default for PerformanceSessionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct LatestPerformanceFrame(pub u64);

#[derive(Debug, Resource, Default)]
pub struct MainScheduleTiming {
    started_at: Option<Instant>,
    latest_ms: f32,
}

impl MainScheduleTiming {
    pub fn latest_ms(&self) -> f32 {
        self.latest_ms
    }
}

#[derive(Debug, Resource, Default, Clone, Copy, PartialEq, Eq)]
struct RenderPerformanceFrame {
    session_id: u128,
    frame: u64,
}

#[derive(Debug, Resource, Default)]
struct RenderScheduleTiming {
    started_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PerformancePhase {
    Assets,
    Director,
    Ecology,
    Environment,
    Intent,
    Journey,
    Landmarks,
    MaterialGallery,
    MaterialGeneration,
    MaterialUpload,
    Places,
    PhysicsBroadPhase,
    PhysicsColliderStreaming,
    PhysicsNarrowPhase,
    PhysicsQuery,
    PhysicsSolver,
    Presentation,
    Regions,
    Player,
    Signs,
    Ui,
    Village,
    WorldCollision,
    WorldImpostor,
    WorldStreaming,
    WorldVisibility,
}

impl PerformancePhase {
    pub const COUNT: usize = 26;

    pub const ALL: [Self; Self::COUNT] = [
        Self::Assets,
        Self::Director,
        Self::Ecology,
        Self::Environment,
        Self::Intent,
        Self::Journey,
        Self::Landmarks,
        Self::MaterialGallery,
        Self::MaterialGeneration,
        Self::MaterialUpload,
        Self::Places,
        Self::PhysicsBroadPhase,
        Self::PhysicsColliderStreaming,
        Self::PhysicsNarrowPhase,
        Self::PhysicsQuery,
        Self::PhysicsSolver,
        Self::Presentation,
        Self::Regions,
        Self::Player,
        Self::Signs,
        Self::Ui,
        Self::Village,
        Self::WorldCollision,
        Self::WorldImpostor,
        Self::WorldStreaming,
        Self::WorldVisibility,
    ];

    const fn index(self) -> usize {
        self as usize
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assets => "assets",
            Self::Director => "director",
            Self::Ecology => "ecology",
            Self::Environment => "environment",
            Self::Intent => "intent",
            Self::Journey => "journey",
            Self::Landmarks => "landmarks",
            Self::MaterialGallery => "material_gallery",
            Self::MaterialGeneration => "material_generation",
            Self::MaterialUpload => "material_upload",
            Self::Places => "places",
            Self::PhysicsBroadPhase => "physics_broad_phase",
            Self::PhysicsColliderStreaming => "physics_collider_streaming",
            Self::PhysicsNarrowPhase => "physics_narrow_phase",
            Self::PhysicsQuery => "physics_query",
            Self::PhysicsSolver => "physics_solver",
            Self::Presentation => "presentation",
            Self::Regions => "regions",
            Self::Player => "player",
            Self::Signs => "signs",
            Self::Ui => "ui",
            Self::Village => "village",
            Self::WorldCollision => "world_collision",
            Self::WorldImpostor => "world_impostor",
            Self::WorldStreaming => "world_streaming",
            Self::WorldVisibility => "world_visibility",
        }
    }
}

#[derive(Debug, Resource)]
pub struct FramePerformance {
    current_frame_samples_ns: [AtomicU64; PerformancePhase::COUNT],
    active_phases: Mutex<HashMap<PerformancePhase, Instant>>,
    summary: Mutex<FramePerformanceSummary>,
}

#[derive(Debug, Default)]
struct FramePerformanceSummary {
    frame_count: u64,
    last_frame_ms: f32,
    moving_average_ms: f32,
    phase_totals: HashMap<PerformancePhase, PhaseAggregate>,
    previous_frame_samples: HashMap<PerformancePhase, f32>,
}

#[derive(Debug)]
struct FrameUpdate {
    snapshot: FrameSnapshot,
    previous_frame_samples: HashMap<PerformancePhase, f32>,
    phase_totals: HashMap<PerformancePhase, PhaseAggregate>,
}

impl FrameUpdate {
    fn previous_frame_phase_ms(&self, phase: PerformancePhase) -> f32 {
        self.previous_frame_samples
            .get(&phase)
            .copied()
            .unwrap_or(0.0)
    }
}

impl Default for FramePerformance {
    fn default() -> Self {
        Self {
            current_frame_samples_ns: std::array::from_fn(|_| AtomicU64::new(0)),
            active_phases: Mutex::new(HashMap::new()),
            summary: Mutex::new(FramePerformanceSummary::default()),
        }
    }
}

impl FramePerformance {
    fn update(&self, frame_duration: Duration) -> FrameUpdate {
        let frame_ms = frame_duration.as_secs_f32() * 1000.0;
        let current_frame_samples = self.take_current_frame_samples();
        let mut summary = lock_ignoring_poison(&self.summary);
        summary.frame_count += 1;
        summary.last_frame_ms = frame_ms;
        if summary.frame_count == 1 {
            summary.moving_average_ms = frame_ms;
        } else {
            summary.moving_average_ms = summary.moving_average_ms * 0.9 + frame_ms * 0.1;
        }

        summary.previous_frame_samples.clear();
        summary.previous_frame_samples.extend(current_frame_samples);

        let phase_breakdown = top_phase_breakdown(&summary.previous_frame_samples);
        let phase_samples = summary.previous_frame_samples.clone();
        for (phase, phase_ms) in &phase_samples {
            let aggregate = summary.phase_totals.entry(*phase).or_default();
            aggregate.samples += 1;
            aggregate.total_ms += *phase_ms;
            aggregate.max_ms = aggregate.max_ms.max(*phase_ms);
        }

        FrameUpdate {
            snapshot: FrameSnapshot {
                frame_count: summary.frame_count,
                frame_ms,
                moving_average_ms: summary.moving_average_ms,
                phase_breakdown,
            },
            previous_frame_samples: summary.previous_frame_samples.clone(),
            phase_totals: summary.phase_totals.clone(),
        }
    }

    fn take_current_frame_samples(&self) -> HashMap<PerformancePhase, f32> {
        let mut samples = HashMap::new();
        for phase in PerformancePhase::ALL {
            let elapsed_ns =
                self.current_frame_samples_ns[phase.index()].swap(0, Ordering::Relaxed);
            if elapsed_ns == 0 {
                continue;
            }
            samples.insert(phase, elapsed_ns as f32 / 1_000_000.0);
        }
        samples
    }

    pub fn begin_phase(&self, phase: PerformancePhase) {
        lock_ignoring_poison(&self.active_phases).insert(phase, Instant::now());
    }

    pub fn end_phase(&self, phase: PerformancePhase) -> Option<f32> {
        let started_at = lock_ignoring_poison(&self.active_phases).remove(&phase)?;
        Some(self.record_phase_duration(phase, started_at.elapsed()))
    }

    pub fn record_phase_duration(&self, phase: PerformancePhase, elapsed: Duration) -> f32 {
        let elapsed_ms = elapsed.as_secs_f32() * 1000.0;
        let elapsed_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        self.current_frame_samples_ns[phase.index()].fetch_add(elapsed_ns, Ordering::Relaxed);
        elapsed_ms
    }

    pub fn frame_count(&self) -> u64 {
        lock_ignoring_poison(&self.summary).frame_count
    }

    pub fn last_frame_ms(&self) -> f32 {
        lock_ignoring_poison(&self.summary).last_frame_ms
    }

    pub fn moving_average_ms(&self) -> f32 {
        lock_ignoring_poison(&self.summary).moving_average_ms
    }

    pub fn previous_frame_phase_ms(&self, phase: PerformancePhase) -> f32 {
        lock_ignoring_poison(&self.summary)
            .previous_frame_samples
            .get(&phase)
            .copied()
            .unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PhaseAggregate {
    samples: u64,
    total_ms: f32,
    max_ms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseBreakdown {
    pub phase: PerformancePhase,
    pub frame_ms: f32,
}

#[derive(Debug, Clone, Copy, Message)]
pub struct PerformanceAlert {
    pub frame_ms: f32,
    pub budget_ms: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameSnapshot {
    pub frame_count: u64,
    pub frame_ms: f32,
    pub moving_average_ms: f32,
    pub phase_breakdown: Vec<PhaseBreakdown>,
}

#[derive(Debug, Clone, Resource)]
pub struct PerformanceSessionReport {
    budget_ms: f32,
    total_frames: u64,
    over_budget_frames: u64,
    worst_frame_ms: f32,
    total_frame_ms: f32,
    total_over_budget_frame_ms: f32,
    phase_totals: HashMap<PerformancePhase, PhaseAggregate>,
}

impl PerformanceSessionReport {
    pub fn new(budget_ms: f32) -> Self {
        Self {
            budget_ms,
            total_frames: 0,
            over_budget_frames: 0,
            worst_frame_ms: 0.0,
            total_frame_ms: 0.0,
            total_over_budget_frame_ms: 0.0,
            phase_totals: HashMap::new(),
        }
    }
}

pub fn announce_performance_session_start(
    config: Res<AppConfig>,
    session_id: Res<PerformanceSessionId>,
) {
    tracing::info!(
        target: "dao_game::performance::session_start",
        session_id = session_id.0,
        budget_ms = config.quality.frame_time_budget_ms,
        target_fps = config.quality.target_fps,
        "performance session started"
    );
}

pub fn begin_main_schedule_timing(mut timing: ResMut<MainScheduleTiming>) {
    timing.started_at = Some(Instant::now());
}

pub fn end_main_schedule_timing(mut timing: ResMut<MainScheduleTiming>) {
    let Some(started_at) = timing.started_at.take() else {
        return;
    };
    timing.latest_ms = started_at.elapsed().as_secs_f32() * 1000.0;
}

#[allow(clippy::too_many_arguments)]
pub fn track_frame_timing(
    time: Res<Time>,
    config: Res<AppConfig>,
    session_id: Res<PerformanceSessionId>,
    main_schedule: Res<MainScheduleTiming>,
    mut latest_frame: ResMut<LatestPerformanceFrame>,
    performance: Res<FramePerformance>,
    mut report: ResMut<PerformanceSessionReport>,
    mut alerts: MessageWriter<PerformanceAlert>,
) {
    let update = performance.update(time.delta());
    let snapshot = update.snapshot.clone();
    latest_frame.0 = snapshot.frame_count;
    report.total_frames += 1;
    report.worst_frame_ms = report.worst_frame_ms.max(snapshot.frame_ms);
    report.total_frame_ms += snapshot.frame_ms;
    if snapshot.frame_ms > config.quality.frame_time_budget_ms {
        report.over_budget_frames += 1;
        report.total_over_budget_frame_ms += snapshot.frame_ms;
    }
    report.phase_totals = update.phase_totals.clone();

    if should_log_frame(snapshot.frame_count, config.performance_detail_interval) {
        let environment_ms = update.previous_frame_phase_ms(PerformancePhase::Environment);
        let assets_ms = update.previous_frame_phase_ms(PerformancePhase::Assets);
        let director_ms = update.previous_frame_phase_ms(PerformancePhase::Director);
        let ecology_ms = update.previous_frame_phase_ms(PerformancePhase::Ecology);
        let landmarks_ms = update.previous_frame_phase_ms(PerformancePhase::Landmarks);
        let intent_ms = update.previous_frame_phase_ms(PerformancePhase::Intent);
        let journey_ms = update.previous_frame_phase_ms(PerformancePhase::Journey);
        let material_gallery_ms =
            update.previous_frame_phase_ms(PerformancePhase::MaterialGallery);
        let material_generation_ms =
            update.previous_frame_phase_ms(PerformancePhase::MaterialGeneration);
        let material_upload_ms =
            update.previous_frame_phase_ms(PerformancePhase::MaterialUpload);
        let places_ms = update.previous_frame_phase_ms(PerformancePhase::Places);
        let physics_broad_phase_ms =
            update.previous_frame_phase_ms(PerformancePhase::PhysicsBroadPhase);
        let physics_collider_streaming_ms =
            update.previous_frame_phase_ms(PerformancePhase::PhysicsColliderStreaming);
        let physics_narrow_phase_ms =
            update.previous_frame_phase_ms(PerformancePhase::PhysicsNarrowPhase);
        let physics_query_ms = update.previous_frame_phase_ms(PerformancePhase::PhysicsQuery);
        let physics_solver_ms = update.previous_frame_phase_ms(PerformancePhase::PhysicsSolver);
        let presentation_ms = update.previous_frame_phase_ms(PerformancePhase::Presentation);
        let regions_ms = update.previous_frame_phase_ms(PerformancePhase::Regions);
        let player_ms = update.previous_frame_phase_ms(PerformancePhase::Player);
        let signs_ms = update.previous_frame_phase_ms(PerformancePhase::Signs);
        let ui_ms = update.previous_frame_phase_ms(PerformancePhase::Ui);
        let village_ms = update.previous_frame_phase_ms(PerformancePhase::Village);
        let world_collision_ms =
            update.previous_frame_phase_ms(PerformancePhase::WorldCollision);
        let world_impostor_ms = update.previous_frame_phase_ms(PerformancePhase::WorldImpostor);
        let world_streaming_ms =
            update.previous_frame_phase_ms(PerformancePhase::WorldStreaming);
        let world_visibility_ms =
            update.previous_frame_phase_ms(PerformancePhase::WorldVisibility);
        let profiled_phase_ms = PerformancePhase::ALL
            .iter()
            .map(|phase| update.previous_frame_phase_ms(*phase))
            .sum::<f32>();
        tracing::trace!(
            target: "dao_game::performance::frame_detail",
            session_id = session_id.0,
            frame = snapshot.frame_count,
            frame_ms = snapshot.frame_ms,
            average_ms = snapshot.moving_average_ms,
            budget_ms = config.quality.frame_time_budget_ms,
            budget_delta_ms = snapshot.frame_ms - config.quality.frame_time_budget_ms,
            main_schedule_ms = main_schedule.latest_ms(),
            profiled_phase_ms = profiled_phase_ms,
            assets_ms = assets_ms,
            director_ms = director_ms,
            ecology_ms = ecology_ms,
            environment_ms = environment_ms,
            intent_ms = intent_ms,
            journey_ms = journey_ms,
            landmarks_ms = landmarks_ms,
            material_gallery_ms = material_gallery_ms,
            material_generation_ms = material_generation_ms,
            material_upload_ms = material_upload_ms,
            places_ms = places_ms,
            physics_broad_phase_ms = physics_broad_phase_ms,
            physics_collider_streaming_ms = physics_collider_streaming_ms,
            physics_narrow_phase_ms = physics_narrow_phase_ms,
            physics_query_ms = physics_query_ms,
            physics_solver_ms = physics_solver_ms,
            presentation_ms = presentation_ms,
            regions_ms = regions_ms,
            player_ms = player_ms,
            signs_ms = signs_ms,
            ui_ms = ui_ms,
            village_ms = village_ms,
            world_collision_ms = world_collision_ms,
            world_impostor_ms = world_impostor_ms,
            world_streaming_ms = world_streaming_ms,
            world_visibility_ms = world_visibility_ms,
            "frame detail sample"
        );
    }

    if should_log_frame(snapshot.frame_count, config.frame_log_interval) {
        let slowest_phase = snapshot.phase_breakdown.first().copied();
        tracing::info!(
            target: "dao_game::performance::frame",
            session_id = session_id.0,
            frame = snapshot.frame_count,
            frame_ms = snapshot.frame_ms,
            average_ms = snapshot.moving_average_ms,
            target_fps = config.quality.target_fps,
            slowest_phase = slowest_phase.map(|phase| phase.phase.as_str()),
            slowest_phase_ms = slowest_phase.map(|phase| phase.frame_ms),
            "frame timing sample"
        );
    }

    if snapshot.frame_ms > config.quality.frame_time_budget_ms {
        let top1 = snapshot.phase_breakdown.first().copied();
        let top2 = snapshot.phase_breakdown.get(1).copied();
        let top3 = snapshot.phase_breakdown.get(2).copied();
        let top4 = snapshot.phase_breakdown.get(3).copied();
        let top5 = snapshot.phase_breakdown.get(4).copied();
        tracing::warn!(
            target: "dao_game::performance::budget",
            session_id = session_id.0,
            frame = snapshot.frame_count,
            frame_ms = snapshot.frame_ms,
            budget_ms = config.quality.frame_time_budget_ms,
            top_phase_1_name = top1.map(|phase| phase.phase.as_str()),
            top_phase_1_ms = top1.map(|phase| phase.frame_ms),
            top_phase_2_name = top2.map(|phase| phase.phase.as_str()),
            top_phase_2_ms = top2.map(|phase| phase.frame_ms),
            top_phase_3_name = top3.map(|phase| phase.phase.as_str()),
            top_phase_3_ms = top3.map(|phase| phase.frame_ms),
            top_phase_4_name = top4.map(|phase| phase.phase.as_str()),
            top_phase_4_ms = top4.map(|phase| phase.frame_ms),
            top_phase_5_name = top5.map(|phase| phase.phase.as_str()),
            top_phase_5_ms = top5.map(|phase| phase.frame_ms),
            "frame budget exceeded"
        );
        alerts.write(PerformanceAlert {
            frame_ms: snapshot.frame_ms,
            budget_ms: config.quality.frame_time_budget_ms,
        });
    }
}

fn extract_render_performance_frame(
    mut commands: Commands,
    session_id: Extract<Res<PerformanceSessionId>>,
    frame: Extract<Res<LatestPerformanceFrame>>,
) {
    commands.insert_resource(RenderPerformanceFrame {
        session_id: session_id.0,
        frame: frame.0,
    });
}

fn begin_render_schedule_timing(mut timing: ResMut<RenderScheduleTiming>) {
    timing.started_at = Some(Instant::now());
}

fn end_render_schedule_timing(
    frame: Option<Res<RenderPerformanceFrame>>,
    mut timing: ResMut<RenderScheduleTiming>,
) {
    let Some(frame) = frame else {
        return;
    };
    let Some(started_at) = timing.started_at.take() else {
        return;
    };
    tracing::trace!(
        target: "dao_game::performance::render_detail",
        session_id = frame.session_id,
        frame = frame.frame,
        render_schedule_ms = started_at.elapsed().as_secs_f32() * 1000.0,
        "render schedule sample"
    );
}

pub fn install_render_schedule_timing(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<RenderScheduleTiming>()
        .add_systems(ExtractSchedule, extract_render_performance_frame)
        .add_systems(
            Render,
            (
                begin_render_schedule_timing.in_set(RenderSystems::ExtractCommands),
                end_render_schedule_timing.in_set(RenderSystems::PostCleanup),
            ),
        );
}

fn should_log_frame(frame_count: u64, interval: u32) -> bool {
    interval > 0 && frame_count.is_multiple_of(u64::from(interval))
}

fn lock_ignoring_poison<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn report_performance_session_summary(
    exit: MessageReader<AppExit>,
    session_id: Res<PerformanceSessionId>,
    report: Res<PerformanceSessionReport>,
) {
    if exit.is_empty() || report.total_frames == 0 {
        return;
    }

    let average_frame_ms = if report.total_frames > 0 {
        report
            .phase_totals
            .values()
            .map(|aggregate| aggregate.total_ms)
            .sum::<f32>()
            / report.total_frames as f32
    } else {
        0.0
    };
    let average_over_budget_frame_ms = if report.over_budget_frames > 0 {
        report.total_over_budget_frame_ms / report.over_budget_frames as f32
    } else {
        0.0
    };
    let hot_phases = top_phase_summary(&report.phase_totals, report.total_frames);
    let hot1 = hot_phases.first().copied();
    let hot2 = hot_phases.get(1).copied();
    let hot3 = hot_phases.get(2).copied();
    let hot4 = hot_phases.get(3).copied();
    let hot5 = hot_phases.get(4).copied();
    tracing::info!(
        target: "dao_game::performance::session",
        session_id = session_id.0,
        frames = report.total_frames,
        over_budget_frames = report.over_budget_frames,
        budget_ms = report.budget_ms,
        worst_frame_ms = report.worst_frame_ms,
        average_frame_ms = report.total_frame_ms / report.total_frames as f32,
        average_over_budget_frame_ms = average_over_budget_frame_ms,
        average_profiled_phase_ms = average_frame_ms,
        hot_phase_1_name = hot1.map(|phase| phase.0),
        hot_phase_1_avg_ms = hot1.map(|phase| phase.1),
        hot_phase_1_max_ms = hot1.map(|phase| phase.2),
        hot_phase_2_name = hot2.map(|phase| phase.0),
        hot_phase_2_avg_ms = hot2.map(|phase| phase.1),
        hot_phase_2_max_ms = hot2.map(|phase| phase.2),
        hot_phase_3_name = hot3.map(|phase| phase.0),
        hot_phase_3_avg_ms = hot3.map(|phase| phase.1),
        hot_phase_3_max_ms = hot3.map(|phase| phase.2),
        hot_phase_4_name = hot4.map(|phase| phase.0),
        hot_phase_4_avg_ms = hot4.map(|phase| phase.1),
        hot_phase_4_max_ms = hot4.map(|phase| phase.2),
        hot_phase_5_name = hot5.map(|phase| phase.0),
        hot_phase_5_avg_ms = hot5.map(|phase| phase.1),
        hot_phase_5_max_ms = hot5.map(|phase| phase.2),
        "performance session summary"
    );
}

fn top_phase_breakdown(samples: &HashMap<PerformancePhase, f32>) -> Vec<PhaseBreakdown> {
    let mut phases: Vec<PhaseBreakdown> = samples
        .iter()
        .map(|(phase, frame_ms)| PhaseBreakdown {
            phase: *phase,
            frame_ms: *frame_ms,
        })
        .collect();
    phases.sort_by(|left, right| right.frame_ms.total_cmp(&left.frame_ms));
    phases.truncate(MAX_SLOW_FRAME_PHASES);
    phases
}

fn top_phase_summary(
    totals: &HashMap<PerformancePhase, PhaseAggregate>,
    total_frames: u64,
) -> Vec<(&'static str, f32, f32)> {
    let mut phases: Vec<(&'static str, f32, f32)> = totals
        .iter()
        .map(|(phase, aggregate)| {
            let avg_ms = if total_frames > 0 {
                aggregate.total_ms / total_frames as f32
            } else {
                0.0
            };
            (phase.as_str(), avg_ms, aggregate.max_ms)
        })
        .collect();
    phases.sort_by(|left, right| right.1.total_cmp(&left.1));
    phases.truncate(MAX_SLOW_FRAME_PHASES);
    phases
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{FramePerformance, PerformancePhase, should_log_frame, top_phase_breakdown};

    #[test]
    fn moving_average_updates_with_new_frame() {
        let performance = FramePerformance::default();
        let first = performance.update(Duration::from_millis(16)).snapshot;
        let second = performance.update(Duration::from_millis(24)).snapshot;

        assert_eq!(first.frame_count, 1);
        assert_eq!(second.frame_count, 2);
        assert!(second.moving_average_ms > first.moving_average_ms);
    }

    #[test]
    fn phase_samples_roll_into_previous_frame_snapshot() {
        let performance = FramePerformance::default();
        performance
            .record_phase_duration(PerformancePhase::WorldStreaming, Duration::from_millis(5));
        performance.record_phase_duration(PerformancePhase::Player, Duration::from_millis(2));

        let snapshot = performance.update(Duration::from_millis(16)).snapshot;

        assert_eq!(snapshot.phase_breakdown.len(), 2);
        assert_eq!(
            snapshot.phase_breakdown[0].phase,
            PerformancePhase::WorldStreaming
        );
        assert!(snapshot.phase_breakdown[0].frame_ms >= 5.0);
    }

    #[test]
    fn top_phase_breakdown_orders_largest_phase_first() {
        let phases = top_phase_breakdown(&std::collections::HashMap::from([
            (PerformancePhase::Ui, 1.0),
            (PerformancePhase::WorldCollision, 9.0),
            (PerformancePhase::Player, 3.0),
        ]));

        assert_eq!(
            phases.first().map(|phase| phase.phase),
            Some(PerformancePhase::WorldCollision)
        );
    }

    #[test]
    fn frame_logging_interval_zero_disables_samples() {
        assert!(!should_log_frame(10, 0));
        assert!(should_log_frame(10, 5));
        assert!(!should_log_frame(11, 5));
    }
}
