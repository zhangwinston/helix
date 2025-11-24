# IME Control API Contract

**Date**: 2024-12-19  
**Feature**: IME自动控制功能

## Overview

本文档定义了IME自动控制功能的内部API契约。这些是helix编辑器内部的函数接口，不是外部API。

## Platform Abstraction Layer

### Trait: ImeController

```rust
pub trait ImeController {
    /// 查询当前IME是否开启
    /// Returns: Ok(true) if IME is enabled, Ok(false) if disabled
    /// Errors: Platform-specific errors (permissions, API unavailable, etc.)
    fn is_ime_enabled() -> Result<bool>;
    
    /// 设置IME开启/关闭状态
    /// enabled: true to enable IME, false to disable
    /// Returns: Ok(()) on success
    /// Errors: Platform-specific errors (permissions, API unavailable, etc.)
    fn set_ime_enabled(enabled: bool) -> Result<()>;
}
```

**Error Handling**: 
- 所有错误应该被捕获并记录日志
- 不向上传播错误，静默失败
- 允许用户手动控制IME

## Core Functions

### Function: detect_ime_sensitive_region

```rust
pub fn detect_ime_sensitive_region(
    syntax: Option<&Syntax>,
    source: RopeSlice,
    loader: &Loader,
    cursor_pos: usize,  // byte offset
) -> ImeSensitiveRegion
```

**Purpose**: 检测光标位置所在的IME敏感区域类型

**Parameters**:
- `syntax`: 语法树（可选，如果为None则返回`EntireFile`）
- `source`: 文档文本的Rope切片
- `loader`: 语法加载器，用于获取语言配置和grammar
- `cursor_pos`: 光标位置的字节偏移

**Returns**: `ImeSensitiveRegion` 枚举值

**Behavior**:
1. 如果语法解析未就绪（`syntax`为None） → 返回 `EntireFile`
2. 如果无法找到光标位置的节点 → 返回 `Code`
3. 优先使用tree-sitter highlight查询检测`@comment`和`@string`捕获：
   - 编译查询 `(comment) @comment\n(string) @string\n...`
   - 执行查询找到包含光标位置的捕获
   - 检查是否为注释/字符串节点
4. 如果查询失败，回退到检查节点类型名称：
   - 字符串节点（包含"string"但不包含"string_start"/"string_end"） → 检查是否为前导引号 → 返回 `StringContent` 或 `Code`
   - 注释节点（包含"comment"或为"line_comment"/"block_comment"） → 检查是否为首部符号 → 返回 `CommentContent` 或 `Code`
   - 其他 → 返回 `Code`
5. 边界处理：
   - 前导引号/注释首部符号 → 返回 `Code`（非敏感）
   - 尾部引号/注释尾部的第一个字符 → 返回敏感区域类型

**Performance**: O(log n) where n is document size (tree-sitter query)

### Function: handle_mode_switch

```rust
pub fn handle_mode_switch(
    editor: &mut Editor,
    view_id: ViewId,
    old_mode: Mode,
    new_mode: Mode,
) -> Result<()>
```

**Purpose**: 处理模式切换时的IME状态管理

**Parameters**:
- `editor`: 编辑器实例
- `view_id`: 当前view ID
- `old_mode`: 之前的模式
- `new_mode`: 新的模式

**Behavior**:
1. 如果退出Insert模式（old_mode == Insert, new_mode != Insert）:
   - 查询当前IME状态
   - 保存当前IME状态（无论开启或关闭）到 `ImeRegistry[view_id].saved_state = Some(current_ime_enabled)`
   - 关闭IME
   - 清除 `current_region`
   - 更新 `mode` 为 `new_mode`
2. 如果进入Insert模式（old_mode != Insert, new_mode == Insert）:
   - 检测光标所在区域
   - 如果位于敏感区域：
     - 如果`saved_state`有值：恢复保存的IME状态
     - 如果`saved_state`为`None`：保持当前IME状态不变（不强制开启）
   - 如果位于非敏感区域：保持IME关闭
   - 更新 `mode` 为 `Insert`，设置 `current_region`

**Errors**: 静默处理，记录日志

### Function: handle_cursor_move

```rust
pub fn handle_cursor_move(
    editor: &mut Editor,
    view_id: ViewId,
) -> Result<()>
```

**Purpose**: 处理光标移动时的IME状态更新

**Parameters**:
- `editor`: 编辑器实例
- `view_id`: 当前view ID

**Preconditions**: 
- 当前模式必须是Insert模式
- 如果不在Insert模式，函数应该提前返回

**Behavior**:
1. 检查当前模式，如果不是Insert则返回
2. 获取主光标位置（FR-020）
3. 检测光标所在区域（使用`detect_ime_sensitive_region`）
4. 如果区域与缓存的 `current_region` 不同：
   - 更新 `current_region`
   - 根据区域类型更新IME状态：
     - 从非敏感区域移动到敏感区域：
       - 不保存非敏感区域的IME状态
       - 如果`saved_state`有值：恢复保存的IME状态
       - 如果`saved_state`为`None`：保持当前IME状态不变（不强制开启）
     - 从敏感区域移动到非敏感区域：
       - 保存当前IME状态到`saved_state`（这是敏感区域的IME状态）
       - 关闭IME

**Performance**: 
- 使用缓存的 `current_region` 避免重复检测
- 仅在区域变化时更新IME状态

### Function: initialize_view_ime_state

```rust
pub fn initialize_view_ime_state(
    editor: &mut Editor,
    view_id: ViewId,
) -> Result<()>
```

**Purpose**: 初始化新view的IME状态

**Parameters**:
- `editor`: 编辑器实例
- `view_id`: 新view的ID

**Behavior**:
1. 初始化 `ImeRegistry` 中的对应 `ImeContext`：`mode = editor.mode()`, `saved_state: None`, `current_region: None`
2. 如果当前系统IME处于开启状态，关闭IME（FR-001）

**Called when**: 
- View创建时（通过`DocumentDidOpen`事件）
- Helix启动时（在`Application::new`中为所有视图初始化IME状态）

## Event Handlers

### Handler: on_mode_switch

```rust
pub fn on_mode_switch(
    event: OnModeSwitch,
) -> Result<()>
```

**Purpose**: OnModeSwitch事件处理器

**Integration**: 注册到helix-event系统

**Behavior**: 调用 `handle_mode_switch`

### Handler: on_selection_change

```rust
pub fn on_selection_change(
    event: SelectionDidChange,
) -> Result<()>
```

**Purpose**: SelectionDidChange事件处理器

**Integration**: 注册到helix-event系统

**Behavior**: 调用 `handle_cursor_move`

## Error Contract

所有IME API调用错误应该：
1. 被捕获并记录到日志（使用 `log::error!`）
2. 不中断编辑流程
3. 不向上传播错误
4. 允许用户继续手动控制IME

## Performance Contract

- IME状态切换响应时间 < 100ms
- 光标移动时的区域检测延迟 < 100ms
- 对编辑器性能影响 < 5%
- 仅在Insert模式下进行检测
- 使用缓存避免重复查询

