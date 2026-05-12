use bevy::prelude::*;

use crate::game::flow::AppScreen;

pub struct NotebookPlugin;

impl Plugin for NotebookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::InGame), initialize_notebook);
        app.add_systems(OnExit(AppScreen::InGame), cleanup_notebook);
    }
}

#[derive(Debug, Resource, Clone, PartialEq, Default)]
pub struct NotebookState {
    pub entries: Vec<NotebookEntry>,
    pub unread_count: usize,
    next_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotebookEntry {
    pub id: u64,
    pub kind: NotebookEntryKind,
    pub at_seconds: f32,
    pub location: Option<String>,
    pub source: NotebookSource,
    pub title: String,
    pub body: String,
    pub tags: Vec<NotebookTag>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum NotebookEntryKind {
    Dream,
    Person,
    Place,
    Sign,
    JourneyEcho,
    Observation,
    PlayerNote,
}

impl NotebookEntryKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dream => "梦境",
            Self::Person => "人物",
            Self::Place => "地点",
            Self::Sign => "征兆",
            Self::JourneyEcho => "回响",
            Self::Observation => "观察",
            Self::PlayerNote => "笔记",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum NotebookSource {
    Dream,
    Dialogue,
    PlaceArrival,
    Sign,
    Perception,
    Journey,
    Observation,
    Manual,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum NotebookTag {
    Village,
    Sea,
    Flock,
    Merchant,
    Shepherd,
    Dream,
    Pyramid,
    Desert,
    Omen,
    Perception,
    Memory,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotebookRecord {
    pub kind: NotebookEntryKind,
    pub at_seconds: f32,
    pub location: Option<String>,
    pub source: NotebookSource,
    pub title: String,
    pub body: String,
    pub tags: Vec<NotebookTag>,
}

impl NotebookState {
    pub fn record(&mut self, record: NotebookRecord) -> Option<u64> {
        if self.is_duplicate(&record) {
            return None;
        }

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.entries.push(NotebookEntry {
            id,
            kind: record.kind,
            at_seconds: record.at_seconds,
            location: record.location,
            source: record.source,
            title: record.title,
            body: record.body,
            tags: record.tags,
        });
        self.entries
            .sort_by(|a, b| a.at_seconds.total_cmp(&b.at_seconds).then(a.id.cmp(&b.id)));
        self.unread_count = self.unread_count.saturating_add(1);
        Some(id)
    }

    pub fn mark_all_read(&mut self) {
        self.unread_count = 0;
    }

    pub fn latest(&self) -> Option<&NotebookEntry> {
        self.entries.last()
    }

    pub fn recent_lines(&self, limit: usize) -> Vec<String> {
        let start = self.entries.len().saturating_sub(limit);
        self.entries[start..]
            .iter()
            .map(format_notebook_entry_line)
            .collect()
    }

    fn is_duplicate(&self, record: &NotebookRecord) -> bool {
        self.entries.iter().rev().take(8).any(|entry| {
            entry.kind == record.kind
                && entry.title == record.title
                && entry.source == record.source
                && (entry.at_seconds - record.at_seconds).abs() < 4.0
        })
    }
}

pub fn record_notebook_entry(
    notebook: Option<&mut NotebookState>,
    record: NotebookRecord,
) -> Option<u64> {
    let notebook = notebook?;
    let id = notebook.record(record)?;
    if let Some(entry) = notebook.entries.iter().find(|entry| entry.id == id) {
        tracing::info!(
            target: "dao_game::notebook::record",
            entry_id = entry.id,
            kind = entry.kind.label(),
            source = ?entry.source,
            title = %entry.title,
            "notebook entry recorded"
        );
    }
    Some(id)
}

pub fn format_notebook_entry_line(entry: &NotebookEntry) -> String {
    let total_seconds = entry.at_seconds.max(0.0).floor() as u32;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!(
        "{minutes:02}:{seconds:02} [{}] {}",
        entry.kind.label(),
        entry.title
    )
}

pub fn dream_record(at_seconds: f32) -> NotebookRecord {
    NotebookRecord {
        kind: NotebookEntryKind::Dream,
        at_seconds,
        location: Some("村庄".to_string()),
        source: NotebookSource::Dream,
        title: "沙暴中的金字塔".to_string(),
        body: "梦里有远方沙漠、埋住天光的风沙，以及一座巨大金字塔。醒来后，海风仍像从梦里吹来。"
            .to_string(),
        tags: vec![
            NotebookTag::Village,
            NotebookTag::Dream,
            NotebookTag::Desert,
            NotebookTag::Pyramid,
        ],
    }
}

fn initialize_notebook(mut commands: Commands) {
    commands.insert_resource(NotebookState::default());
}

fn cleanup_notebook(mut commands: Commands) {
    commands.remove_resource::<NotebookState>();
}

#[cfg(test)]
mod tests {
    use super::{
        NotebookEntryKind, NotebookSource, NotebookState, NotebookTag, dream_record,
        format_notebook_entry_line,
    };

    #[test]
    fn notebook_records_and_formats_entries() {
        let mut notebook = NotebookState::default();
        let id = notebook.record(dream_record(75.2));

        assert_eq!(id, Some(0));
        assert_eq!(notebook.unread_count, 1);
        assert_eq!(
            format_notebook_entry_line(notebook.latest().expect("entry")),
            "01:15 [梦境] 沙暴中的金字塔"
        );
    }

    #[test]
    fn notebook_deduplicates_recent_equivalent_records() {
        let mut notebook = NotebookState::default();
        assert_eq!(notebook.record(dream_record(10.0)), Some(0));
        assert_eq!(notebook.record(dream_record(12.0)), None);
        assert_eq!(notebook.entries.len(), 1);
    }

    #[test]
    fn notebook_keeps_same_title_when_time_is_far_apart() {
        let mut notebook = NotebookState::default();
        assert_eq!(notebook.record(dream_record(10.0)), Some(0));
        assert_eq!(notebook.record(dream_record(20.0)), Some(1));
        assert_eq!(notebook.entries.len(), 2);
    }

    #[test]
    fn player_note_kind_is_available_without_task_language() {
        let mut notebook = NotebookState::default();
        let id = notebook.record(super::NotebookRecord {
            kind: NotebookEntryKind::PlayerNote,
            at_seconds: 3.0,
            location: None,
            source: NotebookSource::Manual,
            title: "海边的风".to_string(),
            body: "只是记下风的方向。".to_string(),
            tags: vec![NotebookTag::Sea],
        });

        assert_eq!(id, Some(0));
        assert!(notebook.latest().expect("entry").body.contains("风"));
    }
}
