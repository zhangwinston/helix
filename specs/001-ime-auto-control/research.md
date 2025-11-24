# Research: IME自动控制功能

**Date**: 2024-12-19  
**Feature**: IME自动控制功能

## Research Tasks

### 1. 跨平台IME控制API研究

**Decision**: 使用平台特定的IME控制API，通过条件编译实现跨平台支持

**Rationale**: 
- Windows: 使用 `ImmSetOpenStatus` 和 `ImmGetOpenStatus` API (imm32.dll)
- Linux: 使用 IBus/FCITX D-Bus接口或X11 IME扩展
- macOS: 使用 Input Method Framework (TIS) API

**Alternatives considered**:
- 统一的IME控制库：目前没有成熟的跨平台Rust库，需要自己实现抽象层
- 仅支持单一平台：不符合helix的跨平台特性

**Implementation approach**:
- 创建平台抽象层 `helix-term/src/handlers/ime/platform.rs`
- 使用 `#[cfg(target_os = "...")]` 进行条件编译
- 提供统一的trait接口：`ImeController`

### 2. 语法解析区域检测方法

**Decision**: 使用helix现有的tree-sitter语法解析系统检测字符串和注释区域

**Rationale**:
- helix-core已经使用tree-house (tree-sitter wrapper)进行语法解析
- 可以通过查询语法树获取光标位置的节点类型
- 利用现有的 `syntax.rs` 模块中的 `named_descendant_for_byte_range` 方法
- 查询highlight scopes (`@string`, `@comment`) 来确定区域类型

**Alternatives considered**:
- 正则表达式匹配：不够准确，无法处理嵌套和转义
- 手动解析：重复实现，维护成本高

**Implementation approach**:
- 在 `helix-core/src/syntax.rs` 中添加辅助函数 `is_ime_sensitive_region`
- 查询光标位置的语法节点，检查是否为字符串或注释节点
- 处理边界情况：前导/尾部符号的排除逻辑

### 3. 事件系统集成方式

**Decision**: 使用helix现有的event系统（OnModeSwitch, SelectionDidChange）

**Rationale**:
- helix-event提供了事件分发机制
- `OnModeSwitch` 事件在模式切换时触发（helix-term/src/events.rs）
- `SelectionDidChange` 事件在光标位置变化时触发
- 可以注册事件处理器来响应这些事件

**Alternatives considered**:
- 轮询检查：性能开销大，不符合helix的事件驱动架构
- 直接修改命令执行逻辑：耦合度高，难以维护

**Implementation approach**:
- 在 `helix-term/src/handlers/ime.rs` 中注册事件处理器
- 监听 `OnModeSwitch` 事件处理模式切换
- 监听 `SelectionDidChange` 事件处理光标移动
- 在事件处理器中调用IME控制逻辑

### 4. IME状态存储和管理

**Decision**: 在Editor结构体中为每个view存储IME状态

**Rationale**:
- 每个view需要独立维护IME状态（FR-017）
- Editor结构体已经包含view管理逻辑
- 状态包括：当前IME开启/关闭状态、保存的IME状态

**Alternatives considered**:
- 全局状态：无法支持多view独立状态
- 文件级别状态：不符合view级别的需求

**Implementation approach**:
- 在 `helix-view/src/editor.rs` 的Editor结构体中添加 `ime_states: HashMap<ViewId, ImeState>`
- ImeState结构包含：`saved_state: Option<bool>` (保存的状态), `current_state: bool` (当前状态)
- 在view初始化时创建默认状态（关闭）

### 5. 性能优化策略

**Decision**: 使用延迟检测和缓存机制优化性能

**Rationale**:
- 语法解析查询可能有性能开销
- 光标快速移动时避免频繁IME切换
- 需要满足 <100ms响应时间要求（SC-001）

**Alternatives considered**:
- 每次光标移动都查询：可能造成性能问题
- 完全异步处理：增加复杂度，可能延迟响应

**Implementation approach**:
- 缓存当前IME敏感区域状态，只在区域变化时重新检测
- 使用防抖机制：快速光标移动时延迟IME切换
- 语法解析结果已缓存，查询开销小
- 仅在Insert模式下进行检测，其他模式跳过

### 6. 错误处理和降级策略

**Decision**: 静默失败并记录日志，允许用户手动控制

**Rationale**:
- IME API调用可能失败（权限、API不可用等）
- 不应该中断编辑流程
- 需要记录错误以便调试

**Alternatives considered**:
- 显示错误提示：干扰用户体验
- 完全禁用功能：功能不可用时无法降级

**Implementation approach**:
- 使用 `log::error!` 记录IME API调用失败
- 捕获所有错误，不向上传播
- 失败时保持当前IME状态不变
- 用户仍可手动切换IME

### 7. 多光标场景处理

**Decision**: 根据主光标（最后一个活动光标）的位置决定IME状态

**Rationale**:
- helix支持多选择编辑
- 主光标代表用户当前主要编辑位置
- 符合用户对"当前编辑位置"的预期

**Alternatives considered**:
- 任一光标在敏感区域就开启：可能不符合用户意图
- 所有光标都在敏感区域才开启：过于严格

**Implementation approach**:
- 使用 `doc.selection(view.id).primary()` 获取主选择
- 根据主选择的光标位置检测IME敏感区域
- 其他光标位置不影响IME状态

### 8. 语法解析延迟处理

**Decision**: 在解析完成前，将整个文件视为IME敏感区域

**Rationale**:
- 语法解析是异步的，可能有延迟
- 避免在解析期间频繁切换IME
- 提供更稳定的用户体验

**Alternatives considered**:
- 等待解析完成：可能延迟响应
- 视为非敏感区域：用户无法在解析期间使用IME

**Implementation approach**:
- 检查 `doc.syntax()` 是否为 `None` 或解析状态
- 如果语法未就绪，返回"整个文件敏感"
- 解析完成后自动切换到精确检测

## Summary

所有研究任务已完成，关键技术决策已确定：
- 跨平台IME API通过条件编译实现
- 使用现有tree-sitter语法解析系统
- 集成helix事件系统
- 在Editor中存储per-view IME状态
- 性能优化通过缓存和延迟检测
- 错误处理采用静默失败策略
- 多光标场景使用主光标位置
- 语法解析延迟时视为全文件敏感

所有决策都符合helix的架构原则和性能要求。

