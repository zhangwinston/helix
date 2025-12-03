# IME 自动控制需求检查清单：性能与全量覆盖

**Purpose**: 面向需求评审阶段，验证IME自动控制功能（全量范围）及性能相关要求的完整性、清晰度与一致性。  
**Created**: 2025-11-26  
**Feature**: [spec.md](../spec.md)

## Requirement Completeness

- [ ] CHK001 是否明确覆盖所有区域转换组合（代码↔字符串、代码↔注释、字符串↔注释），确保每种转移都有IME状态要求？ [Completeness, Spec §FR-003, §FR-015–§FR-016]
- [ ] CHK002 字符串与注释的前导/尾部符号在进入和退出场景下的行为是否完整罗列？ [Completeness, Spec §FR-004–§FR-007]
- [ ] CHK003 文档与视图组合（DocumentId+ViewId）在视图重用或文档替换时的IME状态初始化/清理流程要求是否完备？ [Completeness, Spec §FR-017, Spec §Edge Cases]
- [ ] CHK004 对无法解析、解析失败、以及语法缺少字符串/注释类型的文件，是否清楚描述何时切换为“整文件敏感”以及解析完成后的行为？ [Completeness, Spec §FR-008–§FR-010, §FR-021]
- [ ] CHK005 用户手动切换IME时，在模式与区域均不变的情况下维持现状的逻辑是否覆盖所有触发入口？ [Completeness, Spec §Clarifications 2024-12-19, §FR-022]

## Requirement Clarity

- [ ] CHK006 “主光标”在多光标场景的定义（如何选择最后活动光标）是否写明，避免实现歧义？ [Clarity, Spec §FR-020]
- [ ] CHK007 “快速移动光标”与“及时响应”是否量化频率与时延，否则SC-007难以客观验证？ [Clarity, Spec §Edge Cases, §SC-007, Gap]
- [ ] CHK008 “静默失败并记录日志”是否说明日志级别、必要字段及重试策略？ [Clarity, Spec §FR-019, §Clarifications]
- [ ] CHK009 平台特定IME API抽象是否描述在各OS上的具体调用与限制，避免实施层面理解不一致？ [Clarity, Plan §Technical Context, Spec §Dependencies]

## Requirement Consistency

- [ ] CHK010 规格中“从非敏感区域进入敏感区域只根据已保存状态恢复”与计划文档中“不强制开启IME”的说明是否一致，避免相互矛盾？ [Consistency, Spec §FR-016, Plan §Testing & Verification]
- [ ] CHK011 FR-018关于实时检测与Phase 8性能优化（T066–T068）对“只在必要时检测”的描述是否保持一致？ [Consistency, Spec §FR-018, Tasks §Phase 8]
- [ ] CHK012 FR-001要求启动时关闭IME是否与Edge Cases中“系统已关闭IME保持不变”相符，防止重复操作？ [Consistency, Spec §FR-001, Spec §Edge Cases]

## Acceptance Criteria Quality

- [ ] CHK013 SC-001~SC-008是否与可执行的度量/测试对应（例如T080~T085），并注明采样方法与阈值？ [Acceptance Criteria, Spec §SC-001–§SC-008, Tasks §T080–T085]
- [ ] CHK014 用户故事验收场景是否包含初始IME状态、光标位置和预期结果三要素，方便客观验证？ [Acceptance Criteria, Spec §US1–§US5]

## Scenario Coverage

- [ ] CHK015 异步语法加载期间以及首次语法树完成后的状态切换流程是否双向覆盖？ [Coverage, Spec §Clarifications 2024-12-19, §FR-021]
- [ ] CHK016 多view共享同一文档或通过Action::Replace替换文档时的IME上下文切换是否在需求中体现？ [Coverage, Spec §Edge Cases, Tasks §T073]
- [ ] CHK017 是否说明非标准模式切换（如命令/宏触发）同样遵循FR-011~FR-014的保存/恢复逻辑？ [Coverage, Spec §FR-011–§FR-014, Gap]

## Edge Case Coverage

- [ ] CHK018 多行字符串/注释或嵌套结构的敏感区边界是否明确（尤其是起止行、插入位置）？ [Edge Case Coverage, Spec §Edge Cases]
- [ ] CHK019 运行期间IME API失效（非启动时）是否有降级或重试要求，避免与FR-019仅在初始化阶段适用相矛盾？ [Edge Case Coverage, Spec §FR-019, Spec §Edge Cases]

## Non-Functional Requirements

- [ ] CHK020 性能预算（<100ms响应、<5%性能影响）是否拆分到不同操作路径并与性能测试任务映射？ [Non-Functional, Spec §SC-001 & §SC-008, Tasks §T066–T085]
- [ ] CHK021 是否规定采集指标/日志以验证快速光标移动准确率（SC-007），例如metrics字段与采样窗口？ [Non-Functional, Spec §SC-007, Tasks §T072, Gap]

## Dependencies & Assumptions

- [ ] CHK022 对tree-sitter语法支持和Loader可用性的前置假设是否列出验证或降级方案？ [Dependencies, Spec §Assumptions, Plan §Technical Context]
- [ ] CHK023 各平台IME控制API的可用性/权限依赖是否记录验证步骤与异常提示？ [Dependencies, Spec §Dependencies, Plan §Technical Context]

## Ambiguities & Conflicts

- [ ] CHK024 当进入Insert模式但光标仍在代码区域时，FR-014的不恢复要求与“用户手动切换后保持现状”说明是否存在冲突，需要澄清优先级？ [Ambiguity, Spec §FR-014, Clarifications 2024-12-19]








