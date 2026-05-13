use bevy::{
    app::AppExit, camera::ClearColorConfig, ecs::hierarchy::ChildSpawnerCommands,
    input::keyboard::KeyCode, prelude::*, ui::IsDefaultUiCamera,
};
use std::{fs, path::PathBuf};

use crate::core::performance::{FramePerformance, PerformancePhase};
use crate::game::{
    director::DirectorState,
    ecology::EcologyState,
    flow::{AppScreen, InGameState, PendingSessionLaunch, SessionMode},
    gallery::AssetCodexState,
    intent::{IntentState, PerceptionState, intent_debug_line, perception_label},
    journey::{JourneyStage, JourneyState, StoryArcStage, format_journey_memory_line},
    landmarks::LandmarkState,
    notebook::{NotebookEntry, NotebookEntryKind, NotebookState, format_notebook_entry_line},
    places::{MeaningfulPlaces, PlaceKind, planar_distance},
    player::{CameraMode, FirstPersonState},
    regions::{RegionGraphState, RegionMilestoneKind},
    signs::{OmenGuidancePhase, OmenKind, SignState},
    village::{HerdingPhase, VillageState},
    world::{BiomeKind, WandererPrototype, WorldCycle, WorldMap},
};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiFontHandle>();
        app.init_resource::<UiModeState>();
        app.add_systems(Startup, spawn_ui_camera);
        app.add_systems(OnEnter(AppScreen::MainMenu), spawn_main_menu);
        app.add_systems(OnEnter(AppScreen::InGame), (reset_ui_mode, spawn_hud));
        app.add_systems(OnEnter(InGameState::Paused), spawn_pause_menu);
        app.add_systems(
            Update,
            (
                process_pending_session_launch.run_if(in_state(AppScreen::MainMenu)),
                handle_ui_mode_input.run_if(in_state(AppScreen::InGame)),
                update_button_interactions,
                handle_button_actions,
                toggle_pause_with_escape.run_if(in_state(AppScreen::InGame)),
                (
                    update_hud_control_text,
                    update_hud_stats_text,
                    update_hud_omen_text,
                    update_hud_context_text,
                    update_hud_compass_text,
                    update_asset_codex_panel,
                    update_notebook_overlay,
                )
                    .run_if(in_state(AppScreen::InGame)),
                update_crosshair_visibility.run_if(in_state(AppScreen::InGame)),
            ),
        );
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonTone {
    Primary,
    Secondary,
    Danger,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum UiButtonAction {
    StartExploration,
    StartPresentation,
    StartMaterialGallery,
    Resume,
    Restart,
    ReturnToMenu,
    Quit,
}

#[derive(Component)]
struct HudStatsText;

#[derive(Component)]
struct HudDevPanel;

#[derive(Component)]
struct HudControlPanel;

#[derive(Component)]
struct HudControlText;

#[derive(Component)]
struct HudOmenText;

#[derive(Component)]
struct HudOmenContainer;

#[derive(Component)]
struct HudJourneyText;

#[derive(Component)]
struct HudInteractionText;

#[derive(Component)]
struct HudNotebookText;

#[derive(Component)]
struct HudReturnPathText;

#[derive(Component)]
struct HudCompassPanel;

#[derive(Component)]
struct HudCompassText;

#[derive(Component)]
struct AssetCodexPanel;

#[derive(Component)]
struct AssetCodexTitleText;

#[derive(Component)]
struct AssetCodexBodyText;

#[derive(Component)]
struct Crosshair;

#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq)]
pub struct UiModeState {
    pub hud_mode: HudMode,
    pub notebook_open: bool,
    pub notebook_category: NotebookEntryKind,
    pub compass_open: bool,
    pub return_paths_open: bool,
}

impl Default for UiModeState {
    fn default() -> Self {
        Self {
            hud_mode: HudMode::Formal,
            notebook_open: false,
            notebook_category: NotebookEntryKind::Dream,
            compass_open: true,
            return_paths_open: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudMode {
    Formal,
    Development,
}

impl HudMode {
    fn label(self) -> &'static str {
        match self {
            Self::Formal => "正式 HUD",
            Self::Development => "开发 HUD",
        }
    }
}

fn reset_ui_mode(mut ui_mode: ResMut<UiModeState>) {
    *ui_mode = UiModeState::default();
}

#[derive(Component)]
struct NotebookOverlay;

#[derive(Component)]
struct NotebookTitleText;

#[derive(Component)]
struct NotebookBodyText;

#[derive(Resource)]
pub(crate) struct UiFontHandle(pub(crate) Handle<Font>);

impl FromWorld for UiFontHandle {
    fn from_world(world: &mut World) -> Self {
        let mut font_assets = world.resource_mut::<Assets<Font>>();
        Self(load_ui_font_handle(&mut font_assets))
    }
}

type ButtonInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Interaction,
        &'static ButtonTone,
        &'static mut BackgroundColor,
    ),
    (Changed<Interaction>, With<Button>),
>;

type ButtonActionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static UiButtonAction),
    (Changed<Interaction>, With<Button>),
>;

type HudResources<'w> = (
    Res<'w, SessionMode>,
    Option<Res<'w, WorldCycle>>,
    Option<Res<'w, SignState>>,
    Option<Res<'w, WorldMap>>,
    Option<Res<'w, JourneyState>>,
    Option<Res<'w, IntentState>>,
    Option<Res<'w, PerceptionState>>,
    Option<Res<'w, FirstPersonState>>,
    Option<Res<'w, RegionGraphState>>,
    Option<Res<'w, LandmarkState>>,
    Option<Res<'w, EcologyState>>,
    Option<Res<'w, DirectorState>>,
    Option<Res<'w, MeaningfulPlaces>>,
);

type CompassResources<'w> = (
    Option<Res<'w, WorldMap>>,
    Option<Res<'w, MeaningfulPlaces>>,
    Option<Res<'w, RegionGraphState>>,
    Option<Res<'w, FirstPersonState>>,
);

type HudContextResources<'w> = (
    Option<Res<'w, VillageState>>,
    Option<Res<'w, NotebookState>>,
    Option<Res<'w, PerceptionState>>,
    Res<'w, UiModeState>,
    Option<Res<'w, RegionGraphState>>,
);

type HudContextQueries<'w, 's> = (
    Query<'w, 's, &'static mut Text, With<HudInteractionText>>,
    Query<'w, 's, &'static mut Text, (With<HudNotebookText>, Without<HudInteractionText>)>,
    Query<
        'w,
        's,
        &'static mut Text,
        (
            With<HudReturnPathText>,
            Without<HudInteractionText>,
            Without<HudNotebookText>,
        ),
    >,
);

const TEXT_PRIMARY: Color = Color::srgb(0.92, 0.9, 0.85);
const TEXT_MUTED: Color = Color::srgb(0.72, 0.71, 0.67);
const TEXT_ACCENT: Color = Color::srgb(0.84, 0.74, 0.56);
const MENU_BACKGROUND: Color = Color::srgb(0.11, 0.11, 0.1);
const PANEL_BACKGROUND: Color = Color::srgba(0.17, 0.17, 0.15, 0.96);
const PANEL_BORDER: Color = Color::srgb(0.38, 0.33, 0.25);
const OVERLAY_BACKGROUND: Color = Color::srgba(0.03, 0.04, 0.04, 0.7);

fn load_ui_font_handle(font_assets: &mut Assets<Font>) -> Handle<Font> {
    let Some(path) = find_cjk_font_path() else {
        tracing::warn!("未找到可用中文 UI 字体，回退到 Bevy 默认字体");
        return Handle::default();
    };

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "读取中文 UI 字体失败，回退到 Bevy 默认字体"
            );
            return Handle::default();
        }
    };

    match Font::try_from_bytes(bytes) {
        Ok(font) => {
            tracing::info!("使用中文 UI 字体：{}", path.display());
            font_assets.add(font)
        }
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = ?error,
                "解析中文 UI 字体失败，回退到 Bevy 默认字体"
            );
            Handle::default()
        }
    }
}

fn ui_text_font(font: &Handle<Font>, font_size: f32) -> TextFont {
    TextFont {
        font: font.clone(),
        font_size,
        ..Default::default()
    }
}

fn find_cjk_font_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DAO_UI_FONT_PATH")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return Some(path);
    }

    [
        r"C:\Windows\Fonts\NotoSansSC-VF.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsunb.ttf",
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simsun.ttc",
        r"/System/Library/Fonts/PingFang.ttc",
        r"/System/Library/Fonts/STHeiti Light.ttc",
        r"/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        r"/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        r"/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

fn spawn_ui_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("UiCamera"),
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..Default::default()
        },
        IsDefaultUiCamera,
    ));
}

fn process_pending_session_launch(
    mut pending_launch: ResMut<PendingSessionLaunch>,
    mut session_mode: ResMut<SessionMode>,
    mut next_screen: ResMut<NextState<AppScreen>>,
) {
    let Some(mode) = pending_launch.0.take() else {
        return;
    };

    *session_mode = mode;
    next_screen.set(AppScreen::InGame);
}

fn spawn_main_menu(
    mut commands: Commands,
    pending_launch: Res<PendingSessionLaunch>,
    ui_font: Res<UiFontHandle>,
) {
    if pending_launch.0.is_some() {
        return;
    }
    let font = ui_font.0.clone();

    commands
        .spawn((
            Name::new("MainMenuRoot"),
            DespawnOnExit(AppScreen::MainMenu),
            Node {
                width: percent(100),
                height: percent(100),
                padding: UiRect::axes(px(56), px(40)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Stretch,
                ..Default::default()
            },
            BackgroundColor(MENU_BACKGROUND),
        ))
        .with_children(|parent| {
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    width: percent(58),
                    max_width: px(760),
                    height: percent(100),
                    ..Default::default()
                })
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("道"),
                        ui_text_font(&font, 72.0),
                        TextColor(TEXT_PRIMARY),
                        Node {
                            margin: UiRect::bottom(px(18)),
                            ..Default::default()
                        },
                    ));
                    parent.spawn((
                        Text::new(
                            "没有任务面板，只有世界、征兆与自己的方向。先把生命周期打通，再把叙事与系统继续往里生长。",
                        ),
                        ui_text_font(&font, 22.0),
                        TextColor(TEXT_MUTED),
                        Node {
                            max_width: px(640),
                            margin: UiRect::bottom(px(28)),
                            ..Default::default()
                        },
                    ));

                    parent
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: px(14),
                                margin: UiRect::top(px(8)),
                                ..Default::default()
                            },
                            BackgroundColor(Color::NONE),
                        ))
                        .with_children(|parent| {
                            spawn_action_button(
                                parent,
                                &font,
                                "进入世界",
                                "第一人称探索当前原型",
                                UiButtonAction::StartExploration,
                                ButtonTone::Primary,
                            );
                            spawn_action_button(
                                parent,
                                &font,
                                "展示场景",
                                "自动巡游现有环境与征兆系统",
                                UiButtonAction::StartPresentation,
                                ButtonTone::Secondary,
                            );
                            spawn_action_button(
                                parent,
                                &font,
                                "材质陈列馆 / 图鉴",
                                "审查程序化材质族、物体样本与导出信息",
                                UiButtonAction::StartMaterialGallery,
                                ButtonTone::Secondary,
                            );
                            spawn_action_button(
                                parent,
                                &font,
                                "退出",
                                "关闭程序",
                                UiButtonAction::Quit,
                                ButtonTone::Danger,
                            );
                        });
                });

            parent
                .spawn((
                    Node {
                        width: px(420),
                        align_self: AlignSelf::Center,
                        padding: UiRect::all(px(28)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(8)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(16),
                        ..Default::default()
                    },
                    BackgroundColor(PANEL_BACKGROUND),
                    BorderColor::all(PANEL_BORDER),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("当前流程"),
                        ui_text_font(&font, 26.0),
                        TextColor(TEXT_ACCENT),
                    ));
                    for line in [
                        "主菜单 -> 进入游戏",
                        "Esc 打开暂停界面",
                        "暂停界面可继续、重开、返回主菜单、退出",
                        "HUD 会显示时间、地貌与征兆状态",
                    ] {
                        parent.spawn((
                            Text::new(line),
                            ui_text_font(&font, 18.0),
                            TextColor(TEXT_MUTED),
                        ));
                    }
                });
        });
}

fn spawn_hud(mut commands: Commands, session_mode: Res<SessionMode>, ui_font: Res<UiFontHandle>) {
    let font = ui_font.0.clone();
    commands
        .spawn((
            Name::new("HudRoot"),
            DespawnOnExit(AppScreen::InGame),
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..Default::default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(24),
                    top: px(24),
                    padding: UiRect::all(px(18)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    width: px(360),
                    ..Default::default()
                },
                BackgroundColor(Color::srgba(0.08, 0.09, 0.08, 0.72)),
                BorderColor::all(Color::srgba(0.32, 0.35, 0.28, 0.85)),
                Visibility::Hidden,
                HudDevPanel,
                children![
                    (
                        Text::new(format!("开发 HUD / {}模式", session_mode.label())),
                        ui_text_font(&font, 21.0),
                        TextColor(TEXT_ACCENT),
                    ),
                    (
                        Text::new("世界状态载入中"),
                        ui_text_font(&font, 16.0),
                        TextColor(TEXT_PRIMARY),
                        HudStatsText,
                    )
                ],
            ));

            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(24),
                    top: px(24),
                    padding: UiRect::all(px(16)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    width: px(320),
                    ..Default::default()
                },
                BackgroundColor(Color::srgba(0.08, 0.09, 0.08, 0.66)),
                BorderColor::all(Color::srgba(0.28, 0.34, 0.37, 0.8)),
                Visibility::Hidden,
                HudControlPanel,
                children![(
                    Text::new(control_hint(*session_mode)),
                    ui_text_font(&font, 16.0),
                    TextColor(TEXT_MUTED),
                    HudControlText,
                )],
            ));

            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(24),
                    top: px(24),
                    padding: UiRect::axes(px(14), px(12)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    width: px(260),
                    ..Default::default()
                },
                BackgroundColor(Color::srgba(0.06, 0.07, 0.07, 0.5)),
                BorderColor::all(Color::srgba(0.32, 0.36, 0.34, 0.72)),
                Visibility::Hidden,
                HudCompassPanel,
                children![(
                    Text::new("方位感正在形成"),
                    ui_text_font(&font, 14.0),
                    TextColor(TEXT_MUTED),
                    HudCompassText,
                )],
            ));

            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(24),
                    bottom: px(28),
                    padding: UiRect::axes(px(16), px(12)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    width: px(420),
                    ..Default::default()
                },
                BackgroundColor(Color::srgba(0.08, 0.09, 0.08, 0.58)),
                BorderColor::all(Color::srgba(0.35, 0.32, 0.24, 0.78)),
                children![(
                    Text::new("初入世界"),
                    ui_text_font(&font, 17.0),
                    TextColor(TEXT_PRIMARY),
                    HudJourneyText,
                )],
            ));

            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(24),
                    bottom: px(156),
                    padding: UiRect::axes(px(16), px(14)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    width: px(520),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    ..Default::default()
                },
                BackgroundColor(Color::srgba(0.08, 0.09, 0.08, 0.7)),
                BorderColor::all(Color::srgba(0.42, 0.36, 0.24, 0.86)),
                Visibility::Hidden,
                AssetCodexPanel,
                children![
                    (
                        Text::new("AssetCodex"),
                        ui_text_font(&font, 19.0),
                        TextColor(TEXT_ACCENT),
                        AssetCodexTitleText,
                    ),
                    (
                        Text::new("图鉴样本载入中"),
                        ui_text_font(&font, 12.5),
                        TextColor(TEXT_MUTED),
                        AssetCodexBodyText,
                    )
                ],
            ));

            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(24),
                    bottom: px(28),
                    padding: UiRect::axes(px(16), px(12)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    width: px(360),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..Default::default()
                },
                BackgroundColor(Color::srgba(0.08, 0.09, 0.08, 0.54)),
                BorderColor::all(Color::srgba(0.35, 0.32, 0.24, 0.72)),
                children![
                    (
                        Text::new(""),
                        ui_text_font(&font, 16.0),
                        TextColor(TEXT_PRIMARY),
                        HudInteractionText,
                    ),
                    (
                        Text::new(""),
                        ui_text_font(&font, 14.0),
                        TextColor(TEXT_MUTED),
                        HudNotebookText,
                    ),
                    (
                        Text::new(""),
                        ui_text_font(&font, 14.0),
                        TextColor(TEXT_ACCENT),
                        HudReturnPathText,
                    )
                ],
            ));

            parent.spawn((
                Node {
                    width: px(24),
                    height: px(24),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..Default::default()
                },
                Crosshair,
                children![(
                    Text::new("+"),
                    ui_text_font(&font, 20.0),
                    TextColor(Color::srgb(0.93, 0.92, 0.88)),
                )],
            ));

            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(28),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..Default::default()
                },
                children![(
                    Node {
                        padding: UiRect::axes(px(18), px(12)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(8)),
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgba(0.12, 0.13, 0.12, 0.76)),
                    BorderColor::all(Color::srgba(0.45, 0.38, 0.26, 0.85)),
                    Visibility::Hidden,
                    HudOmenContainer,
                    children![(
                        Text::new(""),
                        ui_text_font(&font, 18.0),
                        TextColor(TEXT_ACCENT),
                        HudOmenText,
                    )]
                )],
            ));

            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(600),
                    max_width: percent(88),
                    max_height: percent(78),
                    padding: UiRect::all(px(24)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(14),
                    ..Default::default()
                },
                BackgroundColor(Color::srgba(0.08, 0.09, 0.08, 0.88)),
                BorderColor::all(Color::srgba(0.46, 0.39, 0.27, 0.9)),
                Visibility::Hidden,
                NotebookOverlay,
                children![
                    (
                        Text::new("记事本"),
                        ui_text_font(&font, 26.0),
                        TextColor(TEXT_ACCENT),
                        NotebookTitleText,
                    ),
                    (
                        Text::new(""),
                        ui_text_font(&font, 16.0),
                        TextColor(TEXT_PRIMARY),
                        NotebookBodyText,
                    )
                ],
            ));
        });
}

fn spawn_pause_menu(
    mut commands: Commands,
    session_mode: Res<SessionMode>,
    journey: Option<Res<JourneyState>>,
    notebook: Option<Res<NotebookState>>,
    ui_font: Res<UiFontHandle>,
) {
    let font = ui_font.0.clone();
    commands
        .spawn((
            Name::new("PauseMenuRoot"),
            DespawnOnExit(AppScreen::InGame),
            DespawnOnExit(InGameState::Paused),
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..Default::default()
            },
            BackgroundColor(OVERLAY_BACKGROUND),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: px(440),
                        padding: UiRect::all(px(28)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(8)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(16),
                        ..Default::default()
                    },
                    BackgroundColor(PANEL_BACKGROUND),
                    BorderColor::all(PANEL_BORDER),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("已暂停"),
                        ui_text_font(&font, 34.0),
                        TextColor(TEXT_PRIMARY),
                    ));
                    parent.spawn((
                        Text::new(format!("当前会话：{}模式", session_mode.label())),
                        ui_text_font(&font, 18.0),
                        TextColor(TEXT_MUTED),
                        Node {
                            margin: UiRect::bottom(px(4)),
                            ..Default::default()
                        },
                    ));
                    parent
                        .spawn((
                            Node {
                                padding: UiRect::all(px(14)),
                                border: UiRect::all(px(1)),
                                border_radius: BorderRadius::all(px(8)),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8),
                                ..Default::default()
                            },
                            BackgroundColor(Color::srgba(0.11, 0.12, 0.1, 0.78)),
                            BorderColor::all(Color::srgba(0.38, 0.33, 0.25, 0.8)),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new("回响"),
                                ui_text_font(&font, 20.0),
                                TextColor(TEXT_ACCENT),
                            ));
                            parent.spawn((
                                Text::new(pause_echo_text(journey.as_deref())),
                                ui_text_font(&font, 15.0),
                                TextColor(TEXT_MUTED),
                            ));
                        });
                    parent
                        .spawn((
                            Node {
                                padding: UiRect::all(px(14)),
                                border: UiRect::all(px(1)),
                                border_radius: BorderRadius::all(px(8)),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8),
                                ..Default::default()
                            },
                            BackgroundColor(Color::srgba(0.11, 0.12, 0.1, 0.72)),
                            BorderColor::all(Color::srgba(0.34, 0.34, 0.3, 0.76)),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new("记事本"),
                                ui_text_font(&font, 20.0),
                                TextColor(TEXT_ACCENT),
                            ));
                            parent.spawn((
                                Text::new(notebook_pause_text(notebook.as_deref())),
                                ui_text_font(&font, 15.0),
                                TextColor(TEXT_MUTED),
                            ));
                        });
                    spawn_action_button(
                        parent,
                        &font,
                        "继续",
                        "回到当前会话",
                        UiButtonAction::Resume,
                        ButtonTone::Primary,
                    );
                    spawn_action_button(
                        parent,
                        &font,
                        "重新开始",
                        "按当前模式重建世界会话",
                        UiButtonAction::Restart,
                        ButtonTone::Secondary,
                    );
                    spawn_action_button(
                        parent,
                        &font,
                        "返回主菜单",
                        "结束当前会话并回到标题界面",
                        UiButtonAction::ReturnToMenu,
                        ButtonTone::Secondary,
                    );
                    spawn_action_button(
                        parent,
                        &font,
                        "退出",
                        "关闭程序",
                        UiButtonAction::Quit,
                        ButtonTone::Danger,
                    );
                });
        });
}

fn spawn_action_button(
    parent: &mut ChildSpawnerCommands<'_>,
    font: &Handle<Font>,
    title: &'static str,
    subtitle: &'static str,
    action: UiButtonAction,
    tone: ButtonTone,
) {
    parent.spawn((
        Button,
        Node {
            width: percent(100),
            min_height: px(76),
            padding: UiRect::axes(px(18), px(14)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(8)),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..Default::default()
        },
        BackgroundColor(button_color(tone, ButtonVisualState::Normal)),
        BorderColor::all(button_border_color(tone)),
        action,
        tone,
        children![
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    ..Default::default()
                },
                children![
                    (
                        Text::new(title),
                        ui_text_font(font, 22.0),
                        TextColor(TEXT_PRIMARY),
                    ),
                    (
                        Text::new(subtitle),
                        ui_text_font(font, 14.0),
                        TextColor(TEXT_MUTED),
                    )
                ]
            ),
            (
                Text::new(">"),
                ui_text_font(font, 20.0),
                TextColor(TEXT_ACCENT),
            )
        ],
    ));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonVisualState {
    Normal,
    Hovered,
    Pressed,
}

fn update_button_interactions(mut query: ButtonInteractionQuery<'_, '_>) {
    for (interaction, tone, mut background) in &mut query {
        let visual = match *interaction {
            Interaction::Pressed => ButtonVisualState::Pressed,
            Interaction::Hovered => ButtonVisualState::Hovered,
            Interaction::None => ButtonVisualState::Normal,
        };
        background.0 = button_color(*tone, visual);
    }
}

fn handle_button_actions(
    interactions: ButtonActionQuery<'_, '_>,
    mut next_screen: ResMut<NextState<AppScreen>>,
    mut next_in_game: Option<ResMut<NextState<InGameState>>>,
    mut pending_launch: ResMut<PendingSessionLaunch>,
    mut active_session_mode: ResMut<SessionMode>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match action {
            UiButtonAction::StartExploration => {
                pending_launch.0 = None;
                *active_session_mode = SessionMode::Exploration;
                next_screen.set(AppScreen::InGame);
            }
            UiButtonAction::StartPresentation => {
                pending_launch.0 = None;
                *active_session_mode = SessionMode::Presentation;
                next_screen.set(AppScreen::InGame);
            }
            UiButtonAction::StartMaterialGallery => {
                pending_launch.0 = None;
                *active_session_mode = SessionMode::MaterialGallery;
                next_screen.set(AppScreen::InGame);
            }
            UiButtonAction::Resume => {
                if let Some(next_in_game) = next_in_game.as_deref_mut() {
                    next_in_game.set(InGameState::Running);
                }
            }
            UiButtonAction::Restart => {
                pending_launch.0 = Some(*active_session_mode);
                next_screen.set(AppScreen::MainMenu);
            }
            UiButtonAction::ReturnToMenu => {
                pending_launch.0 = None;
                next_screen.set(AppScreen::MainMenu);
            }
            UiButtonAction::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}

fn handle_ui_mode_input(
    session_mode: Res<SessionMode>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ui_mode: ResMut<UiModeState>,
    mut notebook: Option<ResMut<NotebookState>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        ui_mode.hud_mode = match ui_mode.hud_mode {
            HudMode::Formal => HudMode::Development,
            HudMode::Development => HudMode::Formal,
        };
    }

    if *session_mode != SessionMode::Exploration {
        return;
    }

    if keys.just_pressed(KeyCode::KeyM) {
        ui_mode.compass_open = !ui_mode.compass_open;
    }

    if keys.just_pressed(KeyCode::KeyR) {
        ui_mode.return_paths_open = !ui_mode.return_paths_open;
    }

    if keys.just_pressed(KeyCode::KeyN) {
        ui_mode.notebook_open = !ui_mode.notebook_open;
        if ui_mode.notebook_open
            && let Some(notebook) = notebook.as_deref_mut()
        {
            notebook.mark_all_read();
        }
    }

    if !ui_mode.notebook_open {
        return;
    }

    if keys.just_pressed(KeyCode::Tab) || keys.just_pressed(KeyCode::ArrowRight) {
        ui_mode.notebook_category = shift_notebook_category(ui_mode.notebook_category, 1);
    } else if keys.just_pressed(KeyCode::ArrowLeft) {
        ui_mode.notebook_category = shift_notebook_category(ui_mode.notebook_category, -1);
    }
}

fn toggle_pause_with_escape(
    keys: Res<ButtonInput<KeyCode>>,
    current_state: Res<State<InGameState>>,
    mut ui_mode: ResMut<UiModeState>,
    mut next_state: ResMut<NextState<InGameState>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }

    if ui_mode.notebook_open {
        ui_mode.notebook_open = false;
        return;
    }

    next_state.set(match current_state.get() {
        InGameState::Running => InGameState::Paused,
        InGameState::Paused => InGameState::Running,
    });
}

fn update_hud_control_text(
    performance: Res<FramePerformance>,
    session_mode: Res<SessionMode>,
    ui_mode: Res<UiModeState>,
    mut controls_query: Query<&mut Text, With<HudControlText>>,
    mut panel_query: Query<&mut Visibility, With<HudControlPanel>>,
) {
    let started_at = std::time::Instant::now();
    let Some(mut controls_text) = controls_query.iter_mut().next() else {
        return;
    };
    let detail_hint = match *session_mode {
        SessionMode::MaterialGallery => {
            format!(
                ",/. 家族  -/= 样本  O 导出对象图鉴  Shift+O 连带截图  E 导出材质馆  F3 {}",
                ui_mode.hud_mode.label()
            )
        }
        _ => format!(
            "F 交谈/观察  E 感知  M 方位感  N 记事本  Tab 分类  G 通过边界  F3 {}",
            ui_mode.hud_mode.label()
        ),
    };
    controls_text.0 = format!("{}\n{}", control_hint(*session_mode), detail_hint);
    for mut visibility in &mut panel_query {
        *visibility = if ui_mode.hud_mode == HudMode::Development {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    performance.record_phase_duration(PerformancePhase::Ui, started_at.elapsed());
}

fn update_hud_stats_text(
    performance: Res<FramePerformance>,
    ui_mode: Res<UiModeState>,
    resources: HudResources<'_>,
    wanderer_query: Query<&Transform, With<WandererPrototype>>,
    mut stats_query: Query<&mut Text, With<HudStatsText>>,
    mut panel_query: Query<&mut Visibility, With<HudDevPanel>>,
) {
    let started_at = std::time::Instant::now();
    let (
        _,
        cycle,
        signs,
        world_map,
        journey,
        intent,
        perception,
        camera_state,
        regions,
        landmarks,
        ecology,
        director,
        places,
    ) = resources;
    for mut visibility in &mut panel_query {
        *visibility = if ui_mode.hud_mode == HudMode::Development {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    let status_line = if let (Some(cycle), Some(world_map), Some(transform)) = (
        cycle.as_deref(),
        world_map.as_deref(),
        wanderer_query.iter().next(),
    ) {
        let biome = world_map
            .sample_biome(transform.translation.x, transform.translation.z)
            .map(biome_label)
            .unwrap_or("未知");
        format!(
            "时辰：{}  地貌：{}  坐标：{:.0}, {:.0}",
            time_of_day_label(cycle.normalized_time),
            biome,
            transform.translation.x,
            transform.translation.z,
        )
    } else {
        "世界状态载入中".to_string()
    };

    let resonance_line = signs
        .as_deref()
        .map(|signs| {
            format!(
                "共鸣：{:.0}%  平静：{:.0}%  征兆：{}  强度：{:.0}%",
                signs.resonance * 100.0,
                signs.calm * 100.0,
                omen_label(signs.current_omen),
                signs.omen_intensity * 100.0,
            )
        })
        .unwrap_or_else(|| "共鸣：--  平静：--  征兆：未感知".to_string());
    let sign_detail_line = signs
        .as_deref()
        .map(|signs| {
            format!(
                "征兆拆分：基础 {:.0}%  感知 {:.0}%  衰退 {:.0}%",
                signs.base_omen_intensity * 100.0,
                signs.perception_omen_intensity * 100.0,
                signs.omen_decay * 100.0,
            )
        })
        .unwrap_or_else(|| "征兆拆分：--".to_string());
    let journey_line = journey
        .as_deref()
        .map(|journey| {
            format!(
                "旅程：{} / {} / {}  距离：{}",
                journey.stage.label(),
                journey.story_stage.label(),
                journey.dream.phase.label(),
                journey
                    .last_distance_to_target
                    .map(|distance| format!("{distance:.0}m"))
                    .unwrap_or_else(|| "--".to_string())
            )
        })
        .unwrap_or_else(|| "旅程：未开始  距离：--".to_string());
    let intent_line = intent_debug_line(intent.as_deref(), perception.as_deref());
    let camera_line = camera_state
        .as_deref()
        .map(|state| {
            format!(
                "视角：{}  第三人称距离：{:.1}",
                state.camera_mode.label(),
                state.third_person_distance
            )
        })
        .unwrap_or_else(|| "视角：未初始化".to_string());
    let region_line = regions
        .as_deref()
        .map(|regions| {
            let current = regions
                .region(regions.current_region)
                .map(|region| region.kind.label())
                .unwrap_or("未知区域");
            let gate = regions
                .nearest_gate
                .map(|gate| {
                    format!(
                        "边界 {:.0}m {}",
                        gate.distance,
                        if gate.open { "可通过" } else { "有征兆" }
                    )
                })
                .unwrap_or_else(|| "无近处边界".to_string());
            format!("区域：{current}  {gate}")
        })
        .unwrap_or_else(|| "区域：未初始化".to_string());
    let pyramid_line = landmarks
        .as_deref()
        .map(|landmarks| {
            format!(
                "金字塔：{}  沙暴 {:.0}%  轮廓 {:.0}%",
                landmarks
                    .pyramid_signal
                    .distance
                    .map(|distance| format!("{distance:.0}m"))
                    .unwrap_or_else(|| "--".to_string()),
                landmarks.pyramid_signal.sandstorm_strength * 100.0,
                landmarks.pyramid_signal.silhouette_strength * 100.0,
            )
        })
        .unwrap_or_else(|| "金字塔：未初始化".to_string());
    let ecology_line = ecology
        .as_deref()
        .and_then(|ecology| {
            ecology
                .latest_signal
                .map(|signal| format!("生态征兆：{signal:?}"))
        })
        .unwrap_or_else(|| "生态征兆：平静".to_string());
    let director_line = director
        .as_deref()
        .and_then(|director| {
            director.last_validation.as_ref().map(|validation| {
                format!(
                    "导演建议：{:?}  采纳 {} 拒绝 {}",
                    director.request_status,
                    validation.accepted.len(),
                    validation.rejected.len()
                )
            })
        })
        .unwrap_or_else(|| "导演建议：待定".to_string());
    let place_line = places
        .as_deref()
        .and_then(|places| {
            places.nearest_place().map(|place| {
                format!(
                    "近处地点：{} {}",
                    place.kind.label(),
                    places
                        .nearest_distance
                        .map(|distance| format!("{distance:.0}m"))
                        .unwrap_or_else(|| "--".to_string())
                )
            })
        })
        .unwrap_or_else(|| "近处地点：未理解".to_string());

    let Some(mut stats_text) = stats_query.iter_mut().next() else {
        return;
    };
    stats_text.0 = format!(
        "{status_line}\n{resonance_line}\n{sign_detail_line}\n{journey_line}\n{intent_line}\n{camera_line}\n{region_line}\n{place_line}\n{pyramid_line}\n{ecology_line}\n{director_line}"
    );
    performance.record_phase_duration(PerformancePhase::Ui, started_at.elapsed());
}

fn update_hud_omen_text(
    performance: Res<FramePerformance>,
    signs: Option<Res<SignState>>,
    journey: Option<Res<JourneyState>>,
    perception: Option<Res<PerceptionState>>,
    mut omen_text_query: Query<&mut Text, With<HudOmenText>>,
    mut omen_container_query: Query<&mut Visibility, With<HudOmenContainer>>,
    mut journey_text_query: Query<&mut Text, (With<HudJourneyText>, Without<HudOmenText>)>,
) {
    let started_at = std::time::Instant::now();
    let Some(mut visibility) = omen_container_query.iter_mut().next() else {
        return;
    };
    let Some(mut omen_text) = omen_text_query.iter_mut().next() else {
        return;
    };
    if let Some(mut journey_text) = journey_text_query.iter_mut().next()
        && let Some(journey) = journey.as_deref()
    {
        journey_text.0 = formal_journey_hint(journey, signs.as_deref(), perception.as_deref());
    }
    if let Some(signs) = signs
        .as_deref()
        .filter(|signs| signs.omen_intensity > 0.05 || signs.response_intensity > 0.02)
    {
        omen_text.0 = formal_omen_hint(signs);
        *visibility = Visibility::Visible;
    } else {
        omen_text.0.clear();
        *visibility = Visibility::Hidden;
    }
    performance.record_phase_duration(PerformancePhase::Ui, started_at.elapsed());
}

fn update_hud_context_text(resources: HudContextResources<'_>, queries: HudContextQueries<'_, '_>) {
    let (village, notebook, perception, ui_mode, regions) = resources;
    let (mut interaction_query, mut notebook_query, mut return_path_query) = queries;
    if let Some(mut interaction_text) = interaction_query.iter_mut().next() {
        let herding = village.as_deref().and_then(herding_status_hint);
        let interaction = village
            .as_deref()
            .and_then(|village| village.interaction_prompt.as_deref())
            .map(|prompt| format!("{prompt}  F"));
        let perception = perception
            .as_deref()
            .map(|perception| format!("{}  E", perception_label(perception)));
        let gate = regions.as_deref().map(gate_status_hint);
        interaction_text.0 = [herding, interaction, perception, gate]
            .into_iter()
            .flatten()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("    ");
    }

    if let Some(mut notebook_text) = notebook_query.iter_mut().next() {
        if ui_mode.notebook_open {
            notebook_text.0 = format!("记事本已打开 [{}]", ui_mode.notebook_category.label());
            return;
        }
        notebook_text.0 = notebook
            .as_deref()
            .and_then(|notebook| {
                notebook.latest().map(|entry| {
                    if notebook.unread_count > 0 {
                        format!("记事本有新记录：{}", entry.title)
                    } else {
                        format_notebook_entry_line(entry)
                    }
                })
            })
            .unwrap_or_default();
    }

    if let Some(mut return_path_text) = return_path_query.iter_mut().next() {
        return_path_text.0 = return_path_hint(notebook.as_deref(), &ui_mode);
    }
}

fn update_hud_compass_text(
    ui_mode: Res<UiModeState>,
    resources: CompassResources<'_>,
    wanderer_query: Query<&Transform, With<WandererPrototype>>,
    mut compass_query: Query<&mut Text, With<HudCompassText>>,
    mut compass_panel_query: Query<&mut Visibility, With<HudCompassPanel>>,
) {
    let (world_map, places, regions, camera_state) = resources;
    if let Some(mut compass_visibility) = compass_panel_query.iter_mut().next() {
        *compass_visibility = if ui_mode.compass_open
            && !ui_mode.notebook_open
            && ui_mode.hud_mode == HudMode::Formal
        {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Some(mut compass_text) = compass_query.iter_mut().next() {
        compass_text.0 = compass_text_for_context(
            wanderer_query.iter().next(),
            world_map.as_deref(),
            camera_state.as_deref(),
            places.as_deref(),
            regions.as_deref(),
        );
    }
}

fn update_crosshair_visibility(
    session_mode: Res<SessionMode>,
    in_game_state: Res<State<InGameState>>,
    ui_mode: Res<UiModeState>,
    camera_state: Option<Res<FirstPersonState>>,
    mut query: Query<&mut Visibility, With<Crosshair>>,
) {
    let visible = *session_mode == SessionMode::Exploration
        && *in_game_state.get() == InGameState::Running
        && !ui_mode.notebook_open
        && camera_state
            .as_deref()
            .is_none_or(|state| state.camera_mode == CameraMode::FirstPerson);
    for mut visibility in &mut query {
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn update_notebook_overlay(
    ui_mode: Res<UiModeState>,
    notebook: Option<Res<NotebookState>>,
    mut overlay_query: Query<&mut Visibility, With<NotebookOverlay>>,
    mut title_query: Query<&mut Text, (With<NotebookTitleText>, Without<NotebookBodyText>)>,
    mut body_query: Query<&mut Text, (With<NotebookBodyText>, Without<NotebookTitleText>)>,
) {
    let Some(mut overlay_visibility) = overlay_query.iter_mut().next() else {
        return;
    };
    let Some(mut title_text) = title_query.iter_mut().next() else {
        return;
    };
    let Some(mut body_text) = body_query.iter_mut().next() else {
        return;
    };

    if !ui_mode.notebook_open {
        *overlay_visibility = Visibility::Hidden;
        body_text.0.clear();
        return;
    }

    *overlay_visibility = Visibility::Visible;
    title_text.0 = format!("记事本 / {}", ui_mode.notebook_category.label());
    body_text.0 = notebook_overlay_text(notebook.as_deref(), ui_mode.notebook_category);
}

fn update_asset_codex_panel(
    session_mode: Res<SessionMode>,
    codex: Res<AssetCodexState>,
    mut panel_query: Query<&mut Visibility, With<AssetCodexPanel>>,
    mut title_query: Query<&mut Text, (With<AssetCodexTitleText>, Without<AssetCodexBodyText>)>,
    mut body_query: Query<&mut Text, (With<AssetCodexBodyText>, Without<AssetCodexTitleText>)>,
) {
    let Some(mut panel_visibility) = panel_query.iter_mut().next() else {
        return;
    };
    let Some(mut title_text) = title_query.iter_mut().next() else {
        return;
    };
    let Some(mut body_text) = body_query.iter_mut().next() else {
        return;
    };

    let visible = *session_mode == SessionMode::MaterialGallery && codex.visible;
    *panel_visibility = if visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    if !visible {
        return;
    }

    title_text.0 = if codex.subtitle.is_empty() {
        codex.title.clone()
    } else {
        format!("{} / {}", codex.title, codex.subtitle)
    };
    body_text.0 = codex.panel_body();
}

fn button_color(tone: ButtonTone, visual: ButtonVisualState) -> Color {
    match (tone, visual) {
        (ButtonTone::Primary, ButtonVisualState::Normal) => Color::srgb(0.23, 0.34, 0.26),
        (ButtonTone::Primary, ButtonVisualState::Hovered) => Color::srgb(0.29, 0.41, 0.31),
        (ButtonTone::Primary, ButtonVisualState::Pressed) => Color::srgb(0.35, 0.47, 0.36),
        (ButtonTone::Secondary, ButtonVisualState::Normal) => Color::srgb(0.2, 0.21, 0.2),
        (ButtonTone::Secondary, ButtonVisualState::Hovered) => Color::srgb(0.26, 0.27, 0.25),
        (ButtonTone::Secondary, ButtonVisualState::Pressed) => Color::srgb(0.31, 0.31, 0.29),
        (ButtonTone::Danger, ButtonVisualState::Normal) => Color::srgb(0.29, 0.18, 0.17),
        (ButtonTone::Danger, ButtonVisualState::Hovered) => Color::srgb(0.36, 0.22, 0.2),
        (ButtonTone::Danger, ButtonVisualState::Pressed) => Color::srgb(0.43, 0.26, 0.23),
    }
}

fn button_border_color(tone: ButtonTone) -> Color {
    match tone {
        ButtonTone::Primary => Color::srgb(0.43, 0.54, 0.39),
        ButtonTone::Secondary => Color::srgb(0.36, 0.34, 0.29),
        ButtonTone::Danger => Color::srgb(0.56, 0.34, 0.3),
    }
}

fn control_hint(session_mode: SessionMode) -> &'static str {
    match session_mode {
        SessionMode::Exploration => "WASD 移动  Shift 疾走  Space 跳跃  V 视角  Esc 暂停",
        SessionMode::Presentation => "自动巡游展示场景  Esc 暂停",
        SessionMode::MaterialGallery => {
            "材质陈列馆/图鉴  WASD/鼠标 移动  Space/Ctrl 升降  Shift 加速  1-4 光照  [ ] 分类  ,/. 家族  -/= 样本  E/O 导出"
        }
    }
}

fn time_of_day_label(normalized_time: f32) -> &'static str {
    match normalized_time.rem_euclid(1.0) {
        value if value < 0.18 => "黎明",
        value if value < 0.36 => "白昼",
        value if value < 0.58 => "午后",
        value if value < 0.76 => "黄昏",
        _ => "深夜",
    }
}

fn biome_label(biome: BiomeKind) -> &'static str {
    match biome {
        BiomeKind::Water => "水域",
        BiomeKind::Meadow => "草甸",
        BiomeKind::Grove => "林地",
        BiomeKind::Steppe => "旷野",
        BiomeKind::Ridge => "山脊",
        BiomeKind::DesertSand => "沙丘",
        BiomeKind::Gobi => "戈壁",
        BiomeKind::Oasis => "绿洲",
    }
}

fn compass_text_for_context(
    player: Option<&Transform>,
    world_map: Option<&WorldMap>,
    camera_state: Option<&FirstPersonState>,
    places: Option<&MeaningfulPlaces>,
    regions: Option<&RegionGraphState>,
) -> String {
    let Some(player) = player else {
        return "方位感正在形成".to_string();
    };
    let position = player.translation;
    let facing = camera_state
        .map(|state| facing_label_from_yaw(state.yaw))
        .unwrap_or_else(|| {
            let forward = player.forward();
            facing_label_from_direction(Vec3::new(forward.x, forward.y, forward.z))
        });
    let terrain = world_map
        .map(|world_map| nearby_terrain_line(world_map, position))
        .unwrap_or_else(|| "地貌：未知".to_string());
    let place_line = places
        .and_then(|places| nearest_memory_marker(position, places))
        .unwrap_or_else(|| "附近还没有清晰的记忆标记。".to_string());
    let gate_line = regions
        .and_then(|regions| regions.nearest_gate)
        .map(|gate| {
            if gate.open {
                format!("边界在{}，雾已经让出路。", distance_band(gate.distance))
            } else {
                format!("{}有边界气息。", distance_band(gate.distance))
            }
        })
        .unwrap_or_else(|| "边界仍在雾外。".to_string());
    format!("朝向：{facing}\n{terrain}\n{place_line}\n{gate_line}")
}

fn nearby_terrain_line(world_map: &WorldMap, position: Vec3) -> String {
    let label_at = |offset: Vec3| {
        world_map
            .sample_biome(position.x + offset.x, position.z + offset.z)
            .map(biome_label)
            .unwrap_or("未知")
    };
    let step = 18.0;
    format!(
        "地貌：脚下{} 北{} 东{} 南{} 西{}",
        label_at(Vec3::ZERO),
        label_at(Vec3::new(0.0, 0.0, -step)),
        label_at(Vec3::new(step, 0.0, 0.0)),
        label_at(Vec3::new(0.0, 0.0, step)),
        label_at(Vec3::new(-step, 0.0, 0.0)),
    )
}

fn nearest_memory_marker(position: Vec3, places: &MeaningfulPlaces) -> Option<String> {
    let place = places.nearest_place()?;
    let distance = places
        .nearest_distance
        .unwrap_or_else(|| planar_distance(position, place.position));
    if distance > 130.0 {
        return None;
    }
    Some(format!(
        "{}在{}的{}。",
        place.kind.label(),
        direction_label_between(position, place.position),
        distance_band(distance)
    ))
}

fn distance_band(distance: f32) -> &'static str {
    match distance.max(0.0) {
        value if value < 18.0 => "身边",
        value if value < 48.0 => "近处",
        value if value < 95.0 => "远处",
        _ => "天边",
    }
}

fn facing_label_from_yaw(yaw: f32) -> &'static str {
    let direction = Quat::from_rotation_y(yaw) * -Vec3::Z;
    facing_label_from_direction(direction)
}

fn direction_label_between(from: Vec3, to: Vec3) -> &'static str {
    facing_label_from_direction(to - from)
}

fn facing_label_from_direction(direction: Vec3) -> &'static str {
    let flat = Vec3::new(direction.x, 0.0, direction.z).normalize_or_zero();
    if flat == Vec3::ZERO {
        return "未定";
    }
    let angle = flat.x.atan2(-flat.z).rem_euclid(std::f32::consts::TAU);
    let octant = ((angle / (std::f32::consts::TAU / 8.0)).round() as usize) % 8;
    ["北", "东北", "东", "东南", "南", "西南", "西", "西北"][octant]
}

fn omen_label(omen: Option<OmenKind>) -> &'static str {
    match omen {
        Some(OmenKind::DawnLight) => "曙光",
        Some(OmenKind::GroveWhisper) => "林语",
        Some(OmenKind::SummitCall) => "山鸣",
        Some(OmenKind::StillWater) => "止水",
        None => "未感知",
    }
}

fn place_label(place: Option<PlaceKind>) -> &'static str {
    place.map(PlaceKind::label).unwrap_or("未名之地")
}

fn guidance_phase_label(phase: OmenGuidancePhase) -> &'static str {
    match phase {
        OmenGuidancePhase::Dormant => "风声尚远",
        OmenGuidancePhase::Far => "远处有微光",
        OmenGuidancePhase::DrawingNear => "征兆渐近",
        OmenGuidancePhase::Arrived => "此地正安静等待",
        OmenGuidancePhase::Responding => "世界正在回应",
    }
}

fn formal_omen_hint(signs: &SignState) -> String {
    let place = place_label(signs.target_place_kind);
    let distance = signs
        .target_distance
        .map(|distance| format!(" {distance:.0}m"))
        .unwrap_or_default();
    format!(
        "{}：{}{}",
        omen_label(signs.current_omen),
        guidance_phase_label(signs.guidance_phase),
        if signs.guidance_phase == OmenGuidancePhase::Responding {
            format!("，{place}有了回声")
        } else if distance.is_empty() {
            String::new()
        } else {
            format!("，{place}{distance}")
        }
    )
}

fn formal_journey_hint(
    journey: &JourneyState,
    signs: Option<&SignState>,
    perception: Option<&PerceptionState>,
) -> String {
    if journey.dream.phase == crate::game::journey::DreamPhase::InDream {
        return "沙暴遮住天空，金字塔的轮廓在远处浮现。".to_string();
    }
    if journey.dream.phase == crate::game::journey::DreamPhase::Afterglow {
        if perception.is_some_and(|perception| perception.active) {
            return "梦中的金色斜面短暂清晰。".to_string();
        }
        match journey.story_stage {
            StoryArcStage::FarBankOutpost => {
                return "雾后已有歇脚地，车辙和摊声把城镇带近了一点。".to_string();
            }
            StoryArcStage::TownPreparation => {
                return "买卖、旅费和陌生人的判断，开始成为路的一部分。".to_string();
            }
            StoryArcStage::FirstLoss => {
                return "失去没有把路截断，它把另一种理解留在路口。".to_string();
            }
            StoryArcStage::DesertDeparture => {
                return "草线已经断在砂砾前，沙漠的方向更清楚了。".to_string();
            }
            _ => {}
        }
        if let Some(cue) = journey.afterglow.cue {
            return format!("{}：{}", cue.label(), cue.hint());
        }
        if journey.afterglow.unanswered_seconds > 18.0 || journey.dream.echo_strength < 0.22 {
            return "梦醒后的回声正在变轻，风仍在等你重新看向村外。".to_string();
        }
        return "梦醒后，风仍带着沙的气味。".to_string();
    }
    if matches!(
        journey.stage,
        JourneyStage::WorldResponded | JourneyStage::EchoSettled
    ) && let Some(memory) = journey.memories.last()
    {
        return memory.text.clone();
    }
    if journey.interaction.near_target {
        return match journey.interaction.completed_kind {
            Some(kind) => format!("你已{}，此地开始回应。", kind.label()),
            None => "近处安静下来。".to_string(),
        };
    }
    signs
        .and_then(|signs| signs.target_place_kind)
        .map(|kind| format!("{}在远处留下气息。", kind.label()))
        .unwrap_or_else(|| journey.story_stage.label().to_string())
}

fn pause_echo_text(journey: Option<&JourneyState>) -> String {
    let Some(journey) = journey else {
        return "还没有留下回响。".to_string();
    };
    if journey.memories.is_empty() && journey.triggered_omens.is_empty() {
        return "还没有留下回响。".to_string();
    }

    let mut lines: Vec<String> = journey
        .memories
        .iter()
        .rev()
        .take(5)
        .map(format_journey_memory_line)
        .collect();
    if lines.is_empty() {
        lines.push(format!(
            "{}曾经显现。",
            omen_label(journey.triggered_omens.last().map(|memory| memory.omen))
        ));
    }
    lines.reverse();
    lines.join("\n")
}

fn notebook_pause_text(notebook: Option<&NotebookState>) -> String {
    let Some(notebook) = notebook else {
        return "记事本还没有记录。".to_string();
    };
    let lines = notebook.recent_lines(6);
    if lines.is_empty() {
        "记事本还没有记录。".to_string()
    } else {
        lines.join("\n")
    }
}

fn shift_notebook_category(current: NotebookEntryKind, delta: i32) -> NotebookEntryKind {
    let categories = NotebookEntryKind::ALL;
    let index = categories
        .iter()
        .position(|kind| *kind == current)
        .unwrap_or(0) as i32;
    let next = (index + delta).rem_euclid(categories.len() as i32) as usize;
    categories[next]
}

fn notebook_overlay_text(notebook: Option<&NotebookState>, category: NotebookEntryKind) -> String {
    let Some(notebook) = notebook else {
        return "记事本还没有记录。".to_string();
    };
    let entries: Vec<&NotebookEntry> = notebook
        .entries
        .iter()
        .filter(|entry| entry.kind == category)
        .rev()
        .take(6)
        .collect();
    if entries.is_empty() {
        return format!("这一页还没有{}记录。", category.label());
    }

    entries
        .into_iter()
        .rev()
        .map(|entry| {
            format!(
                "{}\n{}\n{}",
                format_notebook_entry_line(entry),
                entry.body,
                entry
                    .location
                    .as_deref()
                    .map(|location| format!("地点：{location}"))
                    .unwrap_or_else(|| "地点：未注明".to_string())
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn return_path_hint(notebook: Option<&NotebookState>, ui_mode: &UiModeState) -> String {
    if ui_mode.notebook_open {
        return String::new();
    }
    let Some(notebook) = notebook else {
        return "归路  R".to_string();
    };
    let mut places = notebook
        .entries
        .iter()
        .rev()
        .filter(|entry| entry.kind == NotebookEntryKind::Place)
        .take(3)
        .map(|entry| entry.title.as_str())
        .collect::<Vec<_>>();
    places.dedup();
    if !ui_mode.return_paths_open {
        return if places.is_empty() {
            "归路  R".to_string()
        } else {
            format!("归路  R  {}", places[0])
        };
    }
    if places.is_empty() {
        "归路仍在记忆之外".to_string()
    } else {
        format!("熟悉的归路：{}", places.join(" / "))
    }
}

fn herding_status_hint(village: &VillageState) -> Option<String> {
    match village.herding.phase {
        HerdingPhase::Prompted if village.herding.task_available => {
            Some("羊群看向草地  F".to_string())
        }
        HerdingPhase::FollowingToGrass => Some("羊群跟着你的步子".to_string()),
        HerdingPhase::GrazingAtPatch => Some("羊群正在吃草，风慢下来".to_string()),
        HerdingPhase::ReturningToPen => Some("羊群记得回圈的路".to_string()),
        HerdingPhase::Completed if village.herding.first_task_completed => {
            Some("羊群已经安顿，村外的风更清楚".to_string())
        }
        _ => None,
    }
}

fn gate_status_hint(regions: &RegionGraphState) -> String {
    if let Some(crossing) = regions.crossing.as_ref() {
        return format!("正在穿过{}", crossing.gate_kind.label());
    }
    if let Some(hint) = regions.outpost.as_ref().and_then(|outpost| {
        regions
            .milestones
            .next_hint(regions.current_region, outpost.discovered)
    }) {
        return hint.to_string();
    }
    if regions.milestones.town_edge.discovered && !regions.milestones.loss_crossroad.discovered {
        return RegionMilestoneKind::LossCrossroad.hint().to_string();
    }
    if regions.milestones.loss_crossroad.discovered && !regions.milestones.desert_road.discovered {
        return RegionMilestoneKind::DesertRoad.hint().to_string();
    }
    regions
        .nearest_gate
        .map(|gate| {
            if gate.open {
                "雾河让出浅处  G".to_string()
            } else {
                "雾里有旧水声".to_string()
            }
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use bevy::{
        prelude::{App, AppExtStates, State, Update},
        state::app::StatesPlugin,
    };

    use crate::game::{
        flow::{AppScreen, PendingSessionLaunch, SessionMode},
        notebook::{NotebookEntryKind, NotebookSource, NotebookState},
        regions::{
            GateProximity, RegionBiomeBias, RegionBoundaryKind, RegionGraphState, RegionId,
            RegionJourneyMilestones, RegionKind, RegionMilestoneKind, RegionMilestoneState,
            RegionProfile, RegionWeatherBias, TransitionCondition, TransitionGate,
            TransitionGateKind, TransitionGateState, WorldRegion,
        },
        signs::OmenKind,
        village::{HerdingPhase, HerdingState, VillageState},
        world::BiomeKind,
    };

    use super::{
        biome_label, compass_text_for_context, control_hint, distance_band, facing_label_from_yaw,
        gate_status_hint, herding_status_hint, notebook_overlay_text, omen_label,
        process_pending_session_launch, return_path_hint, shift_notebook_category,
        time_of_day_label,
    };

    #[test]
    fn time_of_day_breakpoints_cover_full_cycle() {
        assert_eq!(time_of_day_label(0.02), "黎明");
        assert_eq!(time_of_day_label(0.22), "白昼");
        assert_eq!(time_of_day_label(0.44), "午后");
        assert_eq!(time_of_day_label(0.64), "黄昏");
        assert_eq!(time_of_day_label(0.88), "深夜");
    }

    #[test]
    fn labels_match_known_gameplay_terms() {
        assert_eq!(
            control_hint(SessionMode::Exploration),
            "WASD 移动  Shift 疾走  Space 跳跃  V 视角  Esc 暂停"
        );
        assert_eq!(biome_label(BiomeKind::Grove), "林地");
        assert_eq!(omen_label(Some(OmenKind::StillWater)), "止水");
        assert_eq!(omen_label(None), "未感知");
    }

    #[test]
    fn pending_launch_transitions_from_menu_into_game() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin);
        app.init_state::<AppScreen>();
        app.insert_resource(SessionMode::Exploration);
        app.insert_resource(PendingSessionLaunch(Some(SessionMode::Presentation)));
        app.add_systems(Update, process_pending_session_launch);

        app.update();
        app.update();

        assert_eq!(
            *app.world().resource::<State<AppScreen>>().get(),
            AppScreen::InGame
        );
        assert_eq!(
            *app.world().resource::<SessionMode>(),
            SessionMode::Presentation
        );
        assert_eq!(app.world().resource::<PendingSessionLaunch>().0, None);
    }

    #[test]
    fn notebook_category_shift_wraps_cleanly() {
        assert_eq!(
            shift_notebook_category(NotebookEntryKind::Dream, -1),
            NotebookEntryKind::PlayerNote
        );
        assert_eq!(
            shift_notebook_category(NotebookEntryKind::PlayerNote, 1),
            NotebookEntryKind::Dream
        );
    }

    #[test]
    fn notebook_overlay_filters_by_category() {
        let mut notebook = NotebookState::default();
        let _ = notebook.record(crate::game::notebook::NotebookRecord {
            kind: NotebookEntryKind::Place,
            at_seconds: 1.0,
            location: Some("海边".to_string()),
            source: NotebookSource::PlaceArrival,
            title: "抵达静水湾".to_string(),
            body: "你抵达了静水湾。".to_string(),
            tags: Vec::new(),
        });
        let _ = notebook.record(crate::game::notebook::NotebookRecord {
            kind: NotebookEntryKind::Sign,
            at_seconds: 2.0,
            location: Some("途中".to_string()),
            source: NotebookSource::Sign,
            title: "山鸣曾经显现".to_string(),
            body: "山风忽然朝一个方向收紧。".to_string(),
            tags: Vec::new(),
        });

        let place_text = notebook_overlay_text(Some(&notebook), NotebookEntryKind::Place);
        let sign_text = notebook_overlay_text(Some(&notebook), NotebookEntryKind::Sign);

        assert!(place_text.contains("抵达静水湾"));
        assert!(!place_text.contains("山鸣"));
        assert!(sign_text.contains("山鸣曾经显现"));
    }

    #[test]
    fn return_path_hint_uses_recorded_places_without_task_language() {
        let mut notebook = NotebookState::default();
        let _ = notebook.record(crate::game::notebook::NotebookRecord {
            kind: NotebookEntryKind::Place,
            at_seconds: 1.0,
            location: Some("海边".to_string()),
            source: NotebookSource::PlaceArrival,
            title: "静水湾".to_string(),
            body: "你记得潮声。".to_string(),
            tags: Vec::new(),
        });
        let ui_mode = crate::game::ui::UiModeState {
            return_paths_open: true,
            ..Default::default()
        };
        let text = return_path_hint(Some(&notebook), &ui_mode);

        assert!(text.contains("静水湾"));
        assert!(!text.contains("任务"));
        assert!(!text.contains("前往"));
    }

    #[test]
    fn compass_direction_labels_cardinal_yaw() {
        assert_eq!(facing_label_from_yaw(0.0), "北");
        assert_eq!(facing_label_from_yaw(std::f32::consts::FRAC_PI_2), "西");
        assert_eq!(distance_band(12.0), "身边");
        assert_eq!(distance_band(70.0), "远处");
    }

    #[test]
    fn compass_text_avoids_task_route_language() {
        let text = compass_text_for_context(None, None, None, None, None);

        assert!(!text.contains("任务"));
        assert!(!text.contains("前往"));
    }

    #[test]
    fn formal_context_hints_are_readable_and_non_task_like() {
        let mut village = VillageState {
            origin: bevy::prelude::Vec3::ZERO,
            spawn_point: bevy::prelude::Vec3::ZERO,
            areas: Vec::new(),
            houses: Vec::new(),
            actors: Vec::new(),
            nearest_actor: None,
            nearest_house: None,
            interaction_prompt: None,
            player_was_bootstrapped: true,
            herding: HerdingState {
                phase: HerdingPhase::FollowingToGrass,
                task_available: true,
                ..Default::default()
            },
        };
        let herding = herding_status_hint(&village).expect("herding hint");
        assert!(herding.contains("羊群"));
        assert!(!herding.contains('?'));
        assert!(!herding.contains("任务"));

        village.herding.phase = HerdingPhase::Completed;
        village.herding.first_task_completed = true;
        assert!(
            herding_status_hint(&village)
                .expect("completed hint")
                .contains("村外")
        );

        let graph = RegionGraphState {
            regions: vec![test_region(RegionId(1), RegionKind::VillageCoast)],
            gates: vec![TransitionGate {
                id: 7,
                from: RegionId(1),
                to: RegionId(2),
                kind: TransitionGateKind::MistRiverFord,
                position: bevy::prelude::Vec3::ZERO,
                radius: 20.0,
                condition: TransitionCondition::DreamAfterglowAndIntent,
                state: TransitionGateState::Open,
                hint: "雾里有旧水声。",
            }],
            current_region: RegionId(1),
            nearest_gate: Some(GateProximity {
                gate_id: 7,
                distance: 5.0,
                open: true,
            }),
            discovered_gates: Vec::new(),
            crossing: None,
            outpost: None,
            milestones: RegionJourneyMilestones {
                town_edge: test_milestone(RegionMilestoneKind::TownEdge, RegionId(2)),
                loss_crossroad: test_milestone(RegionMilestoneKind::LossCrossroad, RegionId(2)),
                desert_road: test_milestone(RegionMilestoneKind::DesertRoad, RegionId(2)),
            },
        };
        let gate = gate_status_hint(&graph);
        assert!(gate.contains("雾河"));
        assert!(!gate.contains('?'));
        assert!(!gate.contains("任务"));
    }

    fn test_region(id: RegionId, kind: RegionKind) -> WorldRegion {
        WorldRegion {
            id,
            kind,
            seed: id.0,
            center: bevy::prelude::Vec3::ZERO,
            radius: 120.0,
            landmark: None,
            boundary: RegionBoundaryKind::MistRiver,
            profile: RegionProfile {
                biome_bias: RegionBiomeBias::CoastalMeadow,
                weather_bias: RegionWeatherBias::ClearSeaMist,
                danger: 0.1,
                exploration_value: 0.4,
            },
        }
    }

    fn test_milestone(kind: RegionMilestoneKind, region: RegionId) -> RegionMilestoneState {
        RegionMilestoneState {
            kind,
            region,
            center: bevy::prelude::Vec3::ZERO,
            arrival_radius: 20.0,
            discovered: false,
            recorded: false,
        }
    }
}
