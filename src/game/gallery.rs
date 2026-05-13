use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use bevy::prelude::*;
use serde::Serialize;

pub struct GalleryPlugin;

impl Plugin for GalleryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AssetCodexState>();
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub enum GalleryExportMode {
    ManifestOnly,
    ManifestAndScreenshot,
}

impl GalleryExportMode {
    pub fn export_label(self) -> &'static str {
        match self {
            Self::ManifestOnly => "manifest_only",
            Self::ManifestAndScreenshot => "manifest_and_screenshot",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GalleryExportStage {
    Idle,
    ManifestQueued {
        mode: GalleryExportMode,
        queued_frame: u64,
    },
    ScreenshotQueued {
        queued_frame: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GalleryExportQueue {
    pub export_path: PathBuf,
    pub screenshot_path: PathBuf,
    pub pending_stage: GalleryExportStage,
    pub next_export_allowed_at: Option<Instant>,
    pub frame_index: u64,
}

impl GalleryExportQueue {
    pub fn new(export_path: impl Into<PathBuf>, screenshot_path: impl Into<PathBuf>) -> Self {
        Self {
            export_path: export_path.into(),
            screenshot_path: screenshot_path.into(),
            pending_stage: GalleryExportStage::Idle,
            next_export_allowed_at: None,
            frame_index: 0,
        }
    }

    pub fn advance_frame(&mut self) {
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    pub fn queue_export(
        &mut self,
        mode: GalleryExportMode,
        cooldown_seconds: f32,
    ) -> Result<(), f32> {
        let now = Instant::now();
        if let Some(allowed_at) = self.next_export_allowed_at
            && now < allowed_at
        {
            return Err((allowed_at - now).as_secs_f32() * 1000.0);
        }
        self.pending_stage = GalleryExportStage::ManifestQueued {
            mode,
            queued_frame: self.frame_index,
        };
        self.next_export_allowed_at = Some(now + Duration::from_secs_f32(cooldown_seconds));
        Ok(())
    }

    pub fn mark_manifest_exported(&mut self, mode: GalleryExportMode) {
        self.pending_stage = if mode == GalleryExportMode::ManifestAndScreenshot {
            GalleryExportStage::ScreenshotQueued {
                queued_frame: self.frame_index,
            }
        } else {
            GalleryExportStage::Idle
        };
    }

    pub fn reset(&mut self) {
        self.pending_stage = GalleryExportStage::Idle;
    }
}

pub fn prepare_export_path(path: &Path) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetCodexSlot {
    pub slot: String,
    pub material_family: String,
    pub material_id: String,
}

#[derive(Debug, Resource, Clone, Default, PartialEq)]
pub struct AssetCodexState {
    pub visible: bool,
    pub title: String,
    pub subtitle: String,
    pub summary_lines: Vec<String>,
    pub slots: Vec<AssetCodexSlot>,
    pub controls_hint: String,
    pub export_manifest_path: String,
    pub screenshot_path: String,
}

impl AssetCodexState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn panel_body(&self) -> String {
        let mut lines = self.summary_lines.clone();
        if !self.slots.is_empty() {
            lines.push("材质槽：".to_string());
            lines.extend(self.slots.iter().map(|slot| {
                format!(
                    "{} -> {} / {}",
                    slot.slot, slot.material_family, slot.material_id
                )
            }));
        }
        if !self.export_manifest_path.is_empty() {
            lines.push(format!("清单：{}", self.export_manifest_path));
        }
        if !self.screenshot_path.is_empty() {
            lines.push(format!("截图：{}", self.screenshot_path));
        }
        if !self.controls_hint.is_empty() {
            lines.push(format!("操作：{}", self.controls_hint));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::{GalleryExportMode, GalleryExportQueue, GalleryExportStage};

    #[test]
    fn export_queue_transitions_from_manifest_to_screenshot() {
        let mut queue = GalleryExportQueue::new("logs/test.json", "logs/test.png");
        queue
            .queue_export(GalleryExportMode::ManifestAndScreenshot, 0.1)
            .expect("queue should accept first export");
        assert!(matches!(
            queue.pending_stage,
            GalleryExportStage::ManifestQueued {
                mode: GalleryExportMode::ManifestAndScreenshot,
                queued_frame: 0
            }
        ));

        queue.advance_frame();
        queue.mark_manifest_exported(GalleryExportMode::ManifestAndScreenshot);
        assert!(matches!(
            queue.pending_stage,
            GalleryExportStage::ScreenshotQueued { queued_frame: 1 }
        ));

        queue.reset();
        assert!(matches!(queue.pending_stage, GalleryExportStage::Idle));
    }
}
