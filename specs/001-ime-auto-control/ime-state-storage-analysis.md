# IME状态存储方案分析

**状态**: ✅ 已实现（2024-12-24）

## 问题分析（历史）

### 原始实现的问题
- **Key**: `ViewId`
- **存储位置**: `helix-term/src/handlers/ime/registry.rs`
- **结构**: `HashMap<ViewId, ImeContext>`

### 问题场景

1. **场景1: 同一个View显示不同Document**
   - View1显示Document1 → IME context: saved_state=Some(true)
   - View1切换到Document2（通过`:e`命令）
   - **问题**: View1的IME context仍然保持saved_state=Some(true)，导致Document2继承了Document1的IME状态

2. **场景2: 同一个Document在不同View中显示**
   - View1显示Document1 → IME context: saved_state=Some(true)
   - View2显示Document1（通过split操作）
   - **合理**: View2有独立的IME context（saved_state=None）

## Document+View组合的唯一性分析

### Helix架构中的证据

Document已经使用ViewId作为key来存储view-specific数据：

```rust
pub struct Document {
    selections: HashMap<ViewId, Selection>,      // 每个View在Document中有独立的selection
    view_data: HashMap<ViewId, ViewData>,        // 每个View在Document中有独立的view_data
    inlay_hints: HashMap<ViewId, DocumentInlayHints>,  // 每个View在Document中有独立的inlay_hints
    // ...
}
```

这说明：**Document+View的组合确实是唯一的，并且helix已经在使用这个模式。**

### 关系分析

1. **View → Document**: 多对一（一个View只能同时显示一个Document）
   - View有`pub doc: DocumentId`字段
   - 但可以通过`:e`命令切换Document

2. **Document → View**: 一对多（一个Document可以被多个View同时显示）
   - Document存储了每个View的独立数据

3. **Document+View组合**: 唯一且稳定
   - 当View切换Document时，Document+View组合改变
   - 当Document在不同View中显示时，Document+View组合不同

## 方案对比

### 方案A: 当前实现（ViewId作为Key）

**优点**:
- 实现简单
- 符合"每个View独立维护IME状态"的直观理解

**缺点**:
- 当View切换Document时，IME状态会被继承（问题所在）
- 需要额外的逻辑来检测和重置（DocumentFocusLost事件处理）

### 方案B: DocumentId+ViewId组合作为Key（推荐）

**优点**:
- 符合helix架构模式（Document已经使用ViewId作为key）
- 自动解决View切换Document的问题
- 不需要额外的重置逻辑
- 更符合"每个Document+View组合独立维护IME状态"的语义

**缺点**:
- 需要修改存储结构
- 需要修改所有访问IME context的代码

## 推荐方案：DocumentId+ViewId组合

### 实现方案

1. **修改存储结构**:
   ```rust
   // 当前
   HashMap<ViewId, ImeContext>
   
   // 改为
   HashMap<(DocumentId, ViewId), ImeContext>
   ```

2. **修改访问接口**:
   ```rust
   // 当前
   with_context_mut(view_id, mode, |ctx| { ... })
   
   // 改为
   with_context_mut(doc_id, view_id, mode, |ctx| { ... })
   ```

3. **自动清理**:
   - 当Document关闭时，清理所有相关的IME context
   - 当View关闭时，清理所有相关的IME context

### 优势

1. **自动解决当前问题**:
   - View1显示Document1 → IME context: (Doc1, View1) → saved_state=Some(true)
   - View1切换到Document2 → IME context: (Doc2, View1) → saved_state=None（新创建）
   - 自动隔离，无需额外逻辑

2. **符合helix架构**:
   - 与Document中其他view-specific数据的存储方式一致
   - 更符合helix的设计理念

3. **更清晰的语义**:
   - IME状态是"在特定Document的特定View中的状态"
   - 而不是"特定View的状态"

## 实现复杂度

### 需要修改的地方

1. **registry.rs**: 修改存储结构和访问接口
2. **mod.rs**: 所有调用`with_context_mut`的地方需要传入`doc_id`
3. **测试**: 更新所有测试用例

### 工作量评估

- **中等复杂度**: 需要修改多个文件，但逻辑清晰
- **风险**: 低（主要是机械性的修改）
- **收益**: 高（彻底解决问题，符合架构）

## 结论

**✅ 已实现：使用DocumentId+ViewId组合作为IME状态的唯一标识**，这更符合helix的架构设计，并且能自动解决当前的问题。

## 实现状态（2024-12-24）

### 已完成的修改

1. **存储结构重构**:
   - ✅ 从 `HashMap<ViewId, ImeContext>` 改为 `HashMap<(DocumentId, ViewId), ImeContext>`
   - ✅ 更新了 `ImeRegistry` 结构

2. **接口更新**:
   - ✅ `with_context_mut` 现在需要 `doc_id` 和 `view_id` 两个参数
   - ✅ 所有调用点都已更新

3. **清理逻辑**:
   - ✅ 添加了 `remove_document` 函数
   - ✅ 在 `DocumentDidClose` 事件中自动清理

4. **测试更新**:
   - ✅ 所有测试用例都已更新，使用 `(doc_id, view_id)` 组合

### 验证结果

✅ **问题已解决**: 测试验证显示，当View切换Document时，IME状态自动隔离，不再相互影响。

