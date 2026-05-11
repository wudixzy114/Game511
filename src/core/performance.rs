use std::time::Duration;

use bevy::prelude::*;

use super::config::AppConfig;

#[derive(Debug, Resource)]
pub struct FramePerformance {
    frame_count: u64,
    last_frame_ms: f32,
    moving_average_ms: f32,
}

impl Default for FramePerformance {
    fn default() -> Self {
        Self {
            frame_count: 0,
            last_frame_ms: 0.0,
            moving_average_ms: 0.0,
        }
    }
}

impl FramePerformance {
    fn update(&mut self, frame_duration: Duration) -> FrameSnapshot {
        self.frame_count += 1;
        let frame_ms = frame_duration.as_secs_f32() * 1000.0;
        self.last_frame_ms = frame_ms;
        if self.frame_count == 1 {
            self.moving_average_ms = frame_ms;
        } else {
            self.moving_average_ms = self.moving_average_ms * 0.9 + frame_ms * 0.1;
        }

        FrameSnapshot {
            frame_count: self.frame_count,
            frame_ms,
            moving_average_ms: self.moving_average_ms,
        }
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    pub fn last_frame_ms(&self) -> f32 {
        self.last_frame_ms
    }

    pub fn moving_average_ms(&self) -> f32 {
        self.moving_average_ms
    }
}

#[derive(Debug, Clone, Copy, Message)]
pub struct PerformanceAlert {
    pub frame_ms: f32,
    pub budget_ms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameSnapshot {
    pub frame_count: u64,
    pub frame_ms: f32,
    pub moving_average_ms: f32,
}

pub fn track_frame_timing(
    time: Res<Time>,
    config: Res<AppConfig>,
    mut performance: ResMut<FramePerformance>,
    mut alerts: MessageWriter<PerformanceAlert>,
) {
    let snapshot = performance.update(time.delta());
    if snapshot
        .frame_count
        .is_multiple_of(u64::from(config.frame_log_interval))
    {
        tracing::info!(
            target: "dao_game::performance::frame",
            frame = snapshot.frame_count,
            frame_ms = snapshot.frame_ms,
            average_ms = snapshot.moving_average_ms,
            target_fps = config.quality.target_fps,
            "frame timing sample"
        );
    }

    if snapshot.frame_ms > config.quality.frame_time_budget_ms {
        tracing::warn!(
            target: "dao_game::performance::budget",
            frame = snapshot.frame_count,
            frame_ms = snapshot.frame_ms,
            budget_ms = config.quality.frame_time_budget_ms,
            "frame budget exceeded"
        );
        alerts.write(PerformanceAlert {
            frame_ms: snapshot.frame_ms,
            budget_ms: config.quality.frame_time_budget_ms,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::FramePerformance;

    #[test]
    fn moving_average_updates_with_new_frame() {
        let mut performance = FramePerformance::default();
        let first = performance.update(Duration::from_millis(16));
        let second = performance.update(Duration::from_millis(24));

        assert_eq!(first.frame_count, 1);
        assert_eq!(second.frame_count, 2);
        assert!(second.moving_average_ms > first.moving_average_ms);
    }
}
