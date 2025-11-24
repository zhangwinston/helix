# Implementation Plan: IME自动控制功能

**Branch**: `001-ime-auto-control` | **Date**: 2024-12-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-ime-auto-control/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

实现IME（输入法）自动控制功能，根据编辑模式和光标位置自动开启或关闭输入法。核心功能包括：在Insert模式下根据光标所在区域（字符串/注释 vs 代码）自动切换IME状态，在模式切换时保存和恢复IME状态，支持多光标场景和语法解析延迟处理。技术实现基于helix现有的事件系统（OnModeSwitch, SelectionDidChange）和tree-sitter语法解析系统，通过平台特定的IME控制API实现跨平台支持。

## Technical Context

**Language/Version**: Rust 1.82 (MSRV per Firefox policy)  
**Primary Dependencies**: tree-house (tree-sitter wrapper), ropey (rope data structure), helix-event (事件系统), 平台特定IME API (imm32.dll for Windows, IBus/FCITX D-Bus for Linux, TIS for macOS)  
**Storage**: N/A (IME状态存储在Editor结构体的HashMap中，内存存储)  
**Testing**: `cargo test --workspace` (单元测试), `cargo integration-test` (集成测试，使用helix-term/tests/test/helpers.rs)  
**Target Platform**: Terminal-based editor (Linux, macOS, Windows)  
**Project Type**: single (helix编辑器内部功能)  
**Performance Goals**: IME状态切换响应时间 < 100ms (SC-001), 光标移动时区域检测延迟 < 100ms, 对编辑器性能影响 < 5% (SC-008)  
**Constraints**: 仅在Insert模式下进行检测，其他模式无开销；错误处理采用静默失败策略，不中断编辑流程；支持多光标场景（根据主光标位置）；语法解析延迟时降级为全文件敏感区域  
**Scale/Scope**: 所有helix用户，支持所有文件类型（代码文件、纯文本文件、配置文件等），每个view独立维护IME状态

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Verify compliance with Helix Constitution principles:

- **Rust-First & MSRV**: ✅ 功能完全使用Rust实现，符合MSRV 1.82要求。平台特定代码使用条件编译（`#[cfg(target_os = "...")]`），不违反MSRV政策。
- **Modular Architecture**: ✅ 功能符合现有crate边界：
  - `helix-core/src/syntax.rs`: 添加IME敏感区域检测函数（功能性，不修改核心数据结构）
  - `helix-view/src/editor.rs`: 添加IME状态存储（HashMap字段）
  - `helix-term/src/handlers/ime.rs`: 新增IME事件处理器模块
  - `helix-term/src/handlers/ime/platform.rs`: 平台抽象层（trait和平台特定实现）
  不需要新crate，功能清晰分离。
- **Integration Testing**: ✅ 计划编写集成测试：
  - 测试模式切换时的IME状态保存和恢复
  - 测试光标移动时的IME自动切换
  - 测试多光标场景
  - 测试语法解析失败时的降级行为
  - 测试IME API调用失败时的错误处理
  使用 `cargo integration-test` 和 `helix-term/tests/test/helpers.rs`。
- **Functional Core**: ✅ 对`helix-core`的修改是功能性的：
  - `detect_ime_sensitive_region`函数是纯函数，不修改状态
  - 仅添加辅助类型和查询函数，不修改现有核心数据结构
  符合functional core原则。
- **Performance & Simplicity**: ✅ 复杂度合理且必要：
  - 使用现有事件系统，避免重复实现
  - 使用现有语法解析系统，避免手动解析
  - 通过缓存机制优化性能（current_region缓存）
  - 仅在Insert模式下检测，其他模式无开销
  - 平台抽象层最小化，仅提供必要的trait接口
  复杂度由功能需求（跨平台IME控制、语法感知）决定，无法进一步简化。

**Gate Status**: ✅ PASS - 所有原则均符合，无需在Complexity Tracking中记录违规。

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
helix-core/src/
├── syntax.rs                    # 添加 detect_ime_sensitive_region() 函数和 ImeSensitiveRegion 枚举

helix-term/src/
├── application.rs               # 启动时为所有视图初始化IME状态
├── handlers/
│   ├── ime.rs                   # IME事件处理器主模块
│   └── ime/
│       ├── engine.rs            # 新增：语法区域/模式状态机 (ImeEngine)
│       ├── registry.rs          # 新增：ImeContext注册表，管理每个View的上下文
│       └── platform.rs          # 平台抽象层（ImeController trait和平台特定实现）
└── handlers.rs                  # 注册IME事件处理器

helix-term/tests/
└── integration.rs               # 可选端到端用例（engine层已有单测）
```

**Structure Decision**: 功能分布在现有crate中，符合helix的模块化架构：
- `helix-core`: 添加语法相关的IME区域检测逻辑（功能性）
- `helix-term`: 添加IME事件处理器、平台抽象层，以及负责IME上下文与状态机的子模块
- 不需要新crate，功能清晰分离且符合现有架构模式

### Testing & Verification

- `helix-term/src/handlers/ime/engine.rs` 内置单元测试覆盖核心FR（敏感/非敏感区域切换、模式切换保存/恢复、状态恢复策略等），无需依赖终端事件即可快速验证状态机。
- **状态恢复策略**：从非敏感区域移动到敏感区域时，如果`saved_state`为`None`，保持当前IME状态不变（不强制开启）。这是根据用户需求调整的行为。
- 日常回归优先运行 `cargo test -p helix-term --lib engine::...`；在需要端到端验证时，可通过 `cargo test -p helix-term` 或 integration 特性运行更完整的场景测试。
- 旧的 `helix-term/tests/test/ime.rs` 集成测试已废弃，避免与新架构的引擎/注册表模型产生冲突，后续若需 UI 层回归可在 integration harness 中补充。
- **启动时初始化**：在`Application::new`中为所有视图初始化IME状态，确保启动时IME关闭（FR-001）。

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

无违规项。所有设计决策均符合Helix Constitution原则。
