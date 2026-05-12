# 《道》独立游戏开发流程

## 基本流程

1. 明确最终游戏目标；需要背景时阅读 `文档/游戏介绍.md`。
2. 明确质量要求；需要标准时阅读 `文档/质量要求.md`。
3. 明确展示效果要求；需要验收口径时阅读 `文档/效果展示要求文档.md`。
4. 查看当前任务清单；如无任务清单，在 `文档/任务清单/` 下创建。
5. 选择应该首先做的任务清单，实施代码、配置、文档或工具变更。
6. 按风险补充测试，并运行必要的编译、测试、格式化、clippy 检查。
7. 如涉及性能、日志、运行行为或调试能力，必须按下面的观测规则验证。
8. 更新相关任务清单；任务完成后移动到 `文档/已经完成的任务文件/`。

## 性能观测规则

性能相关任务包括但不限于：降低帧耗时、优化加载、优化流式生成、优化渲染准备、减少卡顿、调整缓存/预算、修改性能埋点或性能工具。

处理性能优化时必须遵守：

1. 优化前先建立 baseline：

```powershell
.\scripts\perf.ps1 -Seconds 12
```

2. 完成优化后再次运行同一采样命令：

```powershell
.\scripts\perf.ps1 -Seconds 12
```

3. 必须运行对比工具确认效果：

```powershell
.\scripts\perf.ps1 -Action compare
```

4. 需要可视化时生成 HTML：

```powershell
.\scripts\perf.ps1 -Action html
```

5. 最终回复必须说明性能对比结果。至少包含平均帧耗时、p95/p99、超预算帧数、主要瓶颈阶段是否改善。如果没有改善，必须明确说明并给出下一步判断。

6. 如果报告出现 `low instrumentation coverage`，不能直接认定已埋点阶段就是全局瓶颈；应优先补充缺失阶段的埋点，或在结论中说明当前数据覆盖不足。

## 日志观测规则

调试运行问题、启动问题、异常、状态机问题、资源加载问题、性能异常时，优先生成日志 HTML：

```powershell
.\scripts\log.ps1
```

默认输出：

```text
logs/log-report.html
```

查看时按以下维度过滤：

- 类型：`application` / `error` / `performance`
- 级别：`ERROR` / `WARN` / `INFO` / `DEBUG` / `TRACE`
- 系统 target：例如 `dao_game::world::streaming`
- 全文搜索：message、fields、target、source

日志只保留当前和上一次运行。需要对比运行前后行为时，先保留当前 `*.log.1` 和 `*.log` 的语义，不要手工复制出新的重复日志文件，除非用户明确要求归档。

## 常规质量检查

提交或收尾前通常运行：

```powershell
cargo fmt --all --check
cargo test
cargo clippy --all-targets -- -D warnings
```

如果只做文档改动，可说明未运行 Rust 检查的原因。
