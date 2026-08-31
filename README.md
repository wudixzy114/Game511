# Game511

> **《道》—— Bevy 0.18 实现的"涌现式英雄之旅"3D 探索 demo**：以"开放沙漠 / 边境绿洲 / 游牧村庄"为骨架的 first-person 体验，把世界规律、征兆感知、英雄之旅剧情导演、玩家意图、笔记本、地区/路线规划、生态系统都做成 Bevy ECS plugin，性能/日志走 tracy + tracing + 文件观测全链路。

## 项目定位 / 背景

Game511 是 Game58date 的**技术预演版本**：先把"开放世界 + 世界规律 + 英雄之旅"这套机制在 Bevy 0.18 上跑通，再迁回 Stride 引擎。它是一个**"沙漠 / 边境 / 游牧"题材的第一/第三人称 3D 探索 demo**，不追求"完整游戏"，而是要把上层玩法系统（意向驱动剧情、生态群落行为、地区/路线状态机、征兆反馈、笔记本、村庄 NPC 行为）落地到 Bevy 的 ECS 调度里。

技术上是一套**纯 Rust + Bevy 0.18** 的模块化世界生成 demo：
- **2 个顶层 crate**：`core`（`CorePlugin`：配置 / 日志 / 性能埋点 / 自动退出 / 帧时统计）和 `game`（`GamePlugin`：22 个独立子 plugin 拼装）
- **22 个 Bevy Plugin** 涵盖：procedural assets / gallery / physics(avian3d) / materials / objects / UI / world / notebook / village / regions / landmarks / ecology / places / environment / signs / intent / player / journey / director / presentation
- **配置全 TOML 化**：`config/app.toml` 有 world / environment / player / camera / ecology / assets / desert / signs / quality 共 9 个 section，所有数值都可以热重载
- **可观测性优先**：`core/performance.rs` 26KB 的 frame time tracking + `bin/perf_report.rs` 80KB 的离线报告 + `bin/tracy_analyze.rs` 55KB 的 tracy CSV 分析 + `bin/log_report.rs` 18KB 的日志 HTML 生成 + `scripts/perf.ps1` 3KB 的 PowerShell 一键采样 + `scripts/profile.ps1` 10KB 的综合 profiling
- **物理走 avian3d 0.6.1**（parry3d f32 + debug-plugin + collider-from-mesh + parallel）
- **关键依赖**：`bevy = 0.18.1`（`bevy_pbr` / `bevy_gilrs` / `bevy_gizmos` / `bevy_state` / `bevy_ui_render` / `bevy_window` / `bevy_winit` / `bevy_scene` / `bevy_sprite` / `bevy_text` / `bevy_asset` / `bevy_color` / `bevy_core_pipeline` / `default_font` / `multi_threaded` / `png` / `tonemapping_luts` / `x11` / `wayland` / `zstd_rust`）/ `avian3d = 0.6.1` / `tracing = 0.1.41` / `tracing-tracy = 0.11.4` (optional `tracy-profile` feature) / `tracing-appender = 0.2.5` / `tracing-subscriber = 0.3.23` (env-filter + fmt + json + time) / `thiserror = 2.0.18` / `serde = 1.0.228` (derive) / `serde_json = 1.0.145` / `toml = 0.9.8` / `tempfile = 3.23.0` (dev)

## 仓库结构

```
Game511/
├── Cargo.toml                        # 包元信息：bevy 0.18.1 + avian3d 0.6.1 + tracing/tracy
├── Cargo.lock                        # 152KB 锁文件
├── .cargo/config.toml                # rustflags = ["-Dwarnings"], RUST_BACKTRACE = "1"
├── config/
│   └── app.toml                      # 9-section 配置：world/environment/player/camera/ecology/assets/desert/signs/quality
├── scripts/                          # PowerShell 工具集
│   ├── run-dev.ps1                   # 启动开发模式
│   ├── run-presentation.ps1          # 启动展示模式
│   ├── run-material-gallery.ps1      # 启动材质陈列馆
│   ├── perf.ps1                      # 性能采样（12s baseline + compare + html）
│   ├── log.ps1                       # 日志 → HTML 报告
│   ├── profile.ps1                   # 综合 profiling
│   ├── ultimate-profile.ps1          # 终极 profiling pipeline
│   └── dev-check.ps1                 # 日常 dev 检查
├── src/
│   ├── main.rs                       # 入口：dao_game::build_app().run()
│   ├── lib.rs                        # build_app() 工厂
│   ├── core/                         # 基础设施
│   │   ├── mod.rs                    # CorePlugin：config + logging + 性能资源 + 调色 + 自动退出
│   │   ├── config.rs                 # AppConfig + 9 个子配置
│   │   ├── error.rs                  # DaoError
│   │   ├── logging.rs                # tracing-subscriber 初始化
│   │   └── performance.rs            # 帧时埋点 + RenderScheduleTiming + PerformanceAlert
│   ├── game/                         # 业务
│   │   ├── mod.rs                    # GamePlugin：22 个子 plugin 串装
│   │   ├── flow.rs                   # AppScreen (MainMenu/InGame) + SessionMode (Exploration/Presentation/MaterialGallery)
│   │   ├── director.rs               # DirectorPlugin：剧情管家异步任务
│   │   ├── journey.rs                # JourneyPlugin：英雄之旅 12 阶段 + DreamPhase + StoryArcStage
│   │   ├── intent.rs                 # IntentPlugin：玩家意图 + 感知状态
│   │   ├── notebook.rs               # NotebookPlugin：日志/记录
│   │   ├── world.rs                  # WorldPlugin：地形 chunk 流式 + 摄像机 + 光源 + impostor
│   │   ├── regions.rs                # RegionPlugin：地区图 + 过图门 + outpost
│   │   ├── landmarks.rs              # LandmarkPlugin
│   │   ├── places.rs                 # PlacesPlugin：MeaningfulPlaces + PlaceKind
│   │   ├── village.rs                # VillagePlugin：村庄 + 牧羊阶段
│   │   ├── ecology.rs                # EcologyPlugin：鸟 / 鱼 / 羊的 AI + 状态
│   │   ├── environment.rs            # EnvironmentPlugin：天气 / 风场 / 漫游
│   │   ├── signs.rs                  # SignPlugin：征兆 + OmenKind
│   │   ├── physics.rs                # DaoPhysicsPlugin：avian3d 集成 + 碰撞遥测
│   │   ├── materials.rs              # MaterialGalleryPlugin
│   │   ├── objects.rs                # ProceduralObjectPlugin
│   │   ├── assets.rs                 # ProceduralAssetPlugin：程序化资产生成
│   │   ├── gallery.rs                # GalleryPlugin
│   │   ├── presentation.rs           # PresentationPlugin：自动场景演示
│   │   ├── ui.rs                     # UiPlugin
│   │   └── objects/families/         # 物体族（rock / ruin_fragment / tree）
│   └── bin/                          # 离线分析工具
│       ├── log_report.rs             # tracing log → HTML
│       ├── perf_report.rs            # 性能 log → 报告
│       └── tracy_analyze.rs          # tracy CSV → 报告
├── 文档/                              # 中文策划文档（需 UTF-8 阅读）
└── 文档模板/                           # 任务清单 / 已完成模板
```

## 技术栈

| 领域 | 选型 | 用途 |
|---|---|---|
| 引擎 | Bevy 0.18.1（ECS + Plugins + States + SubStates + 系统调度） | 渲染、调度、状态机 |
| 物理 | avian3d 0.6.1（parry3d f32 + debug-plugin + collider-from-mesh + parallel） | 碰撞 / 高度场 |
| 配置 | toml 0.9.8 + serde 1.0.228 + serde_json 1.0.145 | AppConfig + 持久化 |
| 日志 | tracing 0.1.41 + tracing-subscriber 0.3.23（env-filter/fmt/json/time） + tracing-appender 0.2.5 | 结构化日志 |
| 性能 | tracing-tracy 0.11.4（可选 `tracy-profile` feature，ondemand / only-localhost / timer-fallback） | 实时 profiling |
| 错误 | thiserror 2.0.18 | DaoError 派生 |
| 测试 | tempfile 3.23.0 (dev) | 性能测试 |
| 工具 | PowerShell 7 (`scripts/*.ps1`) | 启动 / 性能 / 日志 / profiling |
| 编辑 | VS Code (`.vscode/`) | 开发 |

## 核心模块

**`CorePlugin`（基础设施）**
启动时从 `config/app.toml` 读 `AppConfig`，初始化 tracing（json + env-filter + appender），注入 5 个性能资源（`PerformanceSessionId` / `LatestPerformanceFrame` / `FramePerformance` / `MainScheduleTiming` / `PerformanceSessionReport`），把窗口标题 / `PresentMode::AutoVsync` 注入 `DefaultPlugins`，挂载 4 个调度系统（`First::begin_main_schedule_timing` / `Last::{end_main_schedule_timing, track_frame_timing, report_performance_session_summary, emit_tracy_frame_mark}`）。`DAO_AUTO_EXIT_SECONDS` 环境变量触发自动退出，方便 CI 跑 baseline。

**`WorldPlugin`（开放世界）**
`OnEnter(InGame)` 串行 `reset_world_cycle → configure_world_seed → generate_world_map → cache_world_showcase_spots → create_terrain_material_texture → spawn_camera → spawn_light → spawn_world`；`Update` 跑 `advance_world_cycle → apply_region_streaming_rebuild → update_visible_chunks → stream_terrain_chunks → update_terrain_impostor → update_collision_proxy`（仅 Exploration mode 跑 collision proxy）。资源：`WorldSeed(u64)` / `WorldCycle { normalized_time, daylight }` / `WorldMap` / `WandererPrototype` / `TerrainCollisionProxy` / `WorldShowcaseSpots`。配置里 `visible_chunk_radius = 4 / high_detail = 3 / low_detail = 7 / preload = 10 / impostor = 18`，impostor 用 8 radial bands × 96 angular segments，collision proxy `radius = 3 / subdivisions = 12 / cache = 96`，terrain 纹理图集 384 像素。

**`PlayerPlugin`（第一人称 / 第三人称）**
`FirstPersonState` 资源包含 yaw/pitch/vertical_velocity/grounded/cursor_locked/camera_mode (FirstPerson/ThirdPerson)/third_person_distance/animation_state。`OnEnter(InGame)` 启动 session；`OnEnter(InGameState::Running)` 锁鼠标；`OnEnter(InGameState::Paused)` 释放鼠标；`OnEnter(MainMenu)` 释放鼠标到菜单。Update 串行 `initialize_first_person_state → handle_camera_mode_toggle → apply_mouse_look → move_player_body → sync_camera_to_player → update_player_body_visibility`。配置里 walk 7.8 m/s、sprint ×1.7、eye height 1.68、capsule radius 0.44、max slope 47°、step 0.82、jump 6.5、gravity 18.5、contact_substeps 5。

**`PhysicsPlugin`（物理）**
`avian3d` 全套 + `PhysicsDebugPlugin`，4 substep 物理 + `PhysicsRoute` + `PhysicsDebugState::from_env()` + `PhysicsTelemetry`。`Update` 串行 `sync_physics_debug_config → ensure_player_body → ensure_procedural_asset_colliders → ensure_village_collider_entities → update_player_forward_query → record_physics_collision_events → report_physics_telemetry (on_timer 850ms)`。

**`DirectorPlugin`（剧情管家）**
`OnEnter(InGame)` 初始化 `DirectorState`；`Update` 串行 `poll_director_task → submit_director_task`，`AsyncComputeTaskPool` 后台跑大模型交互。`DirectorState` 维护 mode / last_input / last_output / last_validation / request_status / last_request_id / last_completed_request_id / elapsed_since_last_run。

**`JourneyPlugin`（英雄之旅）**
`JourneyState` + `DreamPhase` + `StoryArcStage`，联动 `EcologyState` / `SignState` / `VillageState` / `RegionGraphState` / `NotebookState` / `MeaningfulPlaces`。`JourneySessionResources` 用 `SystemParam` tuple 形式一次拿齐 7 个 `Res`/`Option<ResMut>`。

**`IntentPlugin`（玩家意图）**
`IntentState` + `IntentKind` + `PerceptionState`，玩家输入自然语言意图后调度征兆触发 + 行为画像更新。

**`NotebookPlugin`（笔记本）**
`NotebookState` 记录 `NotebookRecord`（kind/source/tag/text/timestamp），提供 `record_notebook_entry` + `dream_record` 工具函数。

**`RegionsPlugin`（地区图）**
`RegionGraphState` + `RegionKind` + `TransitionGateKind` + `TransitionGateState`，管过图门状态 + 视觉 + 远景 outpost。

**`VillagePlugin`（村庄）**
`VillageState` + `HerdingPhase` + `VillageAreaKind` + `VillageCollider`，`resolve_village_collision` 处理玩家与村庄的碰撞。

**`EcologyPlugin`（生态）**
`EcologyState` + `EcologySignal`，管鸟 / 鱼 / 羊（数量从配置 18/10/9 读）的 AI + 状态。`animate_wanderer` 系统让 wanderer 实体按 `wander_radius=6 / wander_speed=0.75` 漫游。

**`SignPlugin`（征兆）**
`SignState` + `OmenKind`，根据 `resonance_threshold=0.72` 触发环境征兆。`OmenBeacon` 用 `omen_beacon_height=3.4` 的高亮标记。

**`ObjectsPlugin` + `AssetsPlugin`（程序化资产）**
`ProceduralAsset` + `ProceduralAssetKind` + `ProceduralAssetLod` + `ProceduralAssetMaterials` + `ProceduralSpawnRequest`；`stable_asset_id` 派生确定性资产 ID，`spawn_procedural_asset_entity` 异步生成。物体族分 rock / ruin_fragment / tree 三类。

**`EnvironmentPlugin`（环境）**
`EnvironmentSnapshot` + `WindField`，`day_length_seconds=240` 模拟昼夜循环；`WeatherKind` 接入 desert 沙尘暴（`sandstorm_visibility=46 / particle_strength=1.0 / wind_speed=4.2`）。

**`Desert` 主题配置**
`dune_height=3.2 / dune_frequency=0.22 / gobi_flatness=0.48 / oasis_radius=38 / oasis_moisture=0.86 / sandstorm_visibility=46 / sandstorm_particle_strength=1.0 / sandstorm_wind_speed=4.2` 决定沙漠体验基调。

**`PresentationPlugin`（自动展示）**
`scene_duration_seconds=7` 自动循环演示场景，`camera_blend_speed=2.0` 平滑切镜，DAO_PRESENTATION_MODE 环境变量触发。

**`bin/` 离线分析**
- `log_report.rs`：tracing 文件日志 → `logs/log-report.html`，按 type/level/target 过滤
- `perf_report.rs`：性能日志 → 报告（p50/p95/p99 / 主要瓶颈阶段）
- `tracy_analyze.rs`：tracy 导出的 CSV → 对比报告

**`scripts/perf.ps1` 一键 baseline**
`perf.ps1 -Seconds 12` 跑 12 秒采样；`perf.ps1 -Action compare` 对比历史；`perf.ps1 -Action html` 输出可视化。

## 已完成 / 进行中

- ✅ 22 个独立 Bevy Plugin，模块边界清晰
- ✅ 完整可观测性：tracing + tracy + 帧时埋点 + 日志/性能/tracy 离线分析
- ✅ WorldPlugin 流式 chunk 加载 + impostor + 碰撞代理
- ✅ PlayerPlugin 第一/第三人称切换
- ✅ PhysicsPlugin 完整碰撞链
- ✅ DirectorPlugin 异步剧情调度
- ✅ JourneyPlugin 12 阶段英雄之旅
- ✅ IntentPlugin / SignPlugin / NotebookPlugin / VillagePlugin / EcologyPlugin / RegionsPlugin 全部跑通
- ✅ 配置化（9 个 TOML section）
- ✅ PresentationPlugin 自动化展示
- ⏳ 持久化（场景流式 chunk cache 之外还没有完整 save/load）
- ⏳ LLM 后端真实接入（DirectorPlugin 框架就位，runtime 大模型适配未完成）
- ❌ 完整 NPC 对话（仅有村庄 + 漫游者，缺少 AI 调度对话）
- ❌ 跨地图意图驱动过渡

## 本地运行 / 构建

```powershell
# 默认开发模式
cargo run --release

# 启动展示模式
.\scripts\run-presentation.ps1

# 启动材质陈列馆
.\scripts\run-material-gallery.ps1

# 性能 baseline（先跑 12s）
.\scripts\perf.ps1 -Seconds 12
# 优化后再跑 + 对比
.\scripts\perf.ps1 -Seconds 12
.\scripts\perf.ps1 -Action compare
.\scripts\perf.ps1 -Action html

# 常规质量检查
cargo fmt --all --check
cargo test
cargo clippy --all-targets -- -D warnings

# 启用 tracy profiling
cargo run --release --features tracy-profile
```

环境变量：
- `DAO_AUTO_START_MODE=exploration|presentation|material_gallery`
- `DAO_PRESENTATION_MODE=1` 强制进展示
- `DAO_AUTO_EXIT_SECONDS=12` 自动退出
- `RUST_LOG=dao_game=debug,info` 控制日志级别

## 状态

**v0.1 milestone（"8 milestone"，commit "8 milestone"）**。Bevy 0.18 + avian3d 0.6.1 的技术预演已经稳定：能 demo 第一/第三人称 3D 探索、开放世界流式 chunk 加载、村庄生态、征兆感知、世界规律 tick、剧情管家异步任务。**演示可用，但还不算完整游戏**——剧情管家还在用 mock 输入，意图系统缺真实 LLM 后端，跨地图过渡还没接上。

## License

未指定 License。所有 commit 都来自 `xiezongyu` 个人实验。
