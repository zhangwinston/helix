# Data Model: IME自动控制功能

**Date**: 2024-12-19  
**Feature**: IME自动控制功能

## Entities

### ImeContext

IME上下文信息，为每个Document+View组合独立维护，由终端层的注册表集中管理。

**Fields**:
- `mode: Mode` - 记录当前View的模式（Normal/Insert/Select）
- `saved_state: Option<bool>` - 保存的IME状态（None表示未保存，Some(true)表示开启，Some(false)表示关闭）
- `current_region: Option<ImeSensitiveRegion>` - 当前光标所在的IME敏感区域类型（用于缓存，避免重复检测）

**Location**: `helix-term/src/handlers/ime/engine.rs`（结构定义）+ `helix-term/src/handlers/ime/registry.rs`（注册表）

**Relationships**:
- 每个(DocumentId, ViewId)组合对应一个ImeContext
- 由 `ImeRegistry` (`HashMap<(DocumentId, ViewId), ImeContext>`) 统一管理
- 生命周期与Document+View组合绑定，当Document关闭时自动清理相关context
- **设计说明**: 使用Document+View组合作为key，确保当View切换Document时，IME状态自动隔离，符合helix架构中Document存储view-specific数据的模式

**State Transitions**:
1. Document+View初始化 → `mode = editor.mode()`, `saved_state: None`, `current_region: None`，关闭IME（如果系统IME开启）
2. 进入Insert模式且光标在敏感区域 → 如果`saved_state`有值则恢复，否则保持当前IME状态不变，设置`current_region`
3. 退出Insert模式 → 保存当前IME状态到`saved_state`（仅在敏感区域时保存），清除`current_region`，强制关闭IME
4. 光标移动到不同区域 → 更新`current_region`; 根据敏感/非敏感状态执行开启/关闭动作
5. 从非敏感区域移动到敏感区域 → 不保存非敏感区域的IME状态，如果`saved_state`有值则恢复，否则保持当前IME状态不变
6. View切换Document → 自动创建新的ImeContext（saved_state=None），确保不同Document的IME状态相互独立

### ImeSensitiveRegion

IME敏感区域类型枚举。

**Variants**:
- `StringContent` - 字符串内容区域（不包括前导引号）
- `CommentContent` - 注释内容区域（不包括首部符号）
- `Code` - 代码区域（非敏感）
- `EntireFile` - 整个文件（语法解析失败或未解析时）

**Location**: `helix-core/src/syntax.rs` (作为辅助类型)

**Usage**: 用于标识光标当前所在的区域类型，决定IME是否应该开启。

### ImeController (Trait)

平台特定的IME控制接口。

**Methods**:
- `fn is_ime_enabled() -> Result<bool>` - 查询当前IME是否开启
- `fn set_ime_enabled(enabled: bool) -> Result<()>` - 设置IME开启/关闭状态

**Location**: `helix-term/src/handlers/ime/platform.rs`

**Implementations**:
- `WindowsImeController` - Windows平台实现
- `LinuxImeController` - Linux平台实现（IBus/FCITX）
- `MacosImeController` - macOS平台实现

## Validation Rules

1. **View初始化规则**: 新view创建时，IME状态必须初始化为关闭（FR-001）。启动时也会为所有视图初始化IME状态。
2. **状态保存规则**: 从Insert模式退出时，无论IME开启或关闭都保存当前IME状态到`saved_state`。从敏感区域移动到非敏感区域时，保存敏感区域的IME状态。
3. **状态恢复规则**: 只有在进入Insert模式且光标位于敏感区域时才恢复状态。如果`saved_state`为`None`，保持当前IME状态不变（不强制开启）。
4. **区域检测规则**: 字符串前导引号和注释首部符号不属于敏感区域（FR-004, FR-005）
5. **边界规则**: 字符串尾部引号的第一个字符属于敏感区域（FR-006）
6. **非敏感区域状态保存规则**: 从非敏感区域移动到敏感区域时，不保存非敏感区域的IME状态，只使用之前保存的敏感区域状态

## State Machine

```
[View初始化]
    ↓
[IME关闭, saved_state=None]
    ↓
[进入Insert模式]
    ↓
[检测光标区域]
    ├─→ [敏感区域] → [如果saved_state有值则恢复，否则保持当前状态] → [设置IME]
    └─→ [非敏感区域] → [保持IME关闭]
    ↓
[光标移动]
    ├─→ [从非敏感→敏感] → [不保存非敏感区域状态] → [如果saved_state有值则恢复，否则保持当前状态]
    ├─→ [从敏感→非敏感] → [保存当前IME状态到saved_state] → [关闭IME]
    └─→ [区域未变化] → [保持当前状态]
    ↓
[退出Insert模式]
    ↓
[保存当前IME状态（无论开启或关闭）] → [关闭IME] → [清除区域缓存]
```

## Data Flow

1. **模式切换流程**:
   - OnModeSwitch事件触发
   - 检查old_mode和new_mode
   - 如果退出Insert模式：保存当前IME状态（无论开启或关闭）到`saved_state` → 关闭IME → 清除`current_region`
   - 如果进入Insert模式：检测区域 → 如果敏感区域且`saved_state`有值则恢复状态，否则保持当前IME状态不变

2. **光标移动流程**:
   - SelectionDidChange事件触发
   - 检查当前模式（必须是Insert）
   - 检测光标所在区域（使用语法解析和tree-sitter查询）
   - 如果区域变化：
     - 从敏感区域移动到非敏感区域：保存当前IME状态到`saved_state`并关闭IME
     - 从非敏感区域移动到敏感区域：不保存非敏感区域状态，如果`saved_state`有值则恢复，否则保持当前IME状态不变

3. **区域检测流程**:
   - 获取光标位置的字节偏移
   - 优先使用tree-sitter highlight查询检测`@comment`和`@string`捕获
   - 如果查询失败，回退到检查节点类型名称
   - 检查是否为字符串/注释节点
   - 处理边界情况（前导/尾部符号）

## Performance Considerations

- ImeContext使用HashMap存储在ImeRegistry中，使用(DocumentId, ViewId)作为key，O(1)查找
- current_region缓存避免重复语法查询
- 仅在Insert模式下进行检测
- 语法解析结果已缓存，查询开销小
- Document关闭时自动清理相关context，避免内存泄漏

## Architecture Design Decision

### 为什么使用(DocumentId, ViewId)作为key？

**问题背景**:
- 在helix中，一个View可以显示不同的Document（通过`:e`命令或`Action::Replace`）
- 如果只使用ViewId作为key，当View切换Document时，IME状态会被继承，导致不同Document的IME状态相互影响

**解决方案**:
- 使用(DocumentId, ViewId)组合作为唯一标识
- 这与helix架构中Document存储view-specific数据的模式一致（如`selections: HashMap<ViewId, Selection>`）
- 当View切换Document时，自动创建新的ImeContext，确保IME状态独立

**优势**:
1. 自动解决View切换Document时的状态隔离问题
2. 符合helix的架构设计理念
3. 语义更清晰：IME状态是"在特定Document的特定View中的状态"

