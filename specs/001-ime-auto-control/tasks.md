# Tasks: IME自动控制功能

**Input**: Design documents from `/specs/001-ime-auto-control/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Integration tests are included as they are required for code contributions per Helix Constitution.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., [US1], [US2], [US3])
- Include exact file paths in descriptions

## Path Conventions

- **helix-core**: `helix-core/src/`
- **helix-view**: `helix-view/src/`
- **helix-term**: `helix-term/src/`
- **Tests**: `helix-term/tests/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure

- [X] T001 Create directory structure for IME handler module in helix-term/src/handlers/ime/
- [X] T002 Create platform abstraction module directory helix-term/src/handlers/ime/platform.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Core Data Structures

- [X] T003 [P] Define ImeSensitiveRegion enum in helix-core/src/syntax.rs with variants: StringContent, CommentContent, Code, EntireFile
- [X] T004 [P] Define ImeState struct in helix-view/src/editor.rs with fields: saved_state: Option<bool>, current_region: Option<ImeSensitiveRegion>
- [X] T005 Add ime_states: HashMap<ViewId, ImeState> field to Editor struct in helix-view/src/editor.rs

### Platform Abstraction Layer

- [X] T006 [P] Define ImeController trait in helix-term/src/handlers/ime/platform.rs with methods: is_ime_enabled() -> Result<bool>, set_ime_enabled(enabled: bool) -> Result<()>
- [X] T007 [P] Implement WindowsImeController in helix-term/src/handlers/ime/platform.rs using ImmSetOpenStatus/ImmGetOpenStatus APIs
- [X] T008 [P] Implement LinuxImeController in helix-term/src/handlers/ime/platform.rs using IBus/FCITX D-Bus interfaces
- [X] T009 [P] Implement MacosImeController in helix-term/src/handlers/ime/platform.rs using TIS (Text Input Source) APIs
- [X] T010 Implement platform-specific ImeController selection logic using #[cfg(target_os = "...")] in helix-term/src/handlers/ime/platform.rs

### Core Detection Function

- [X] T011 Implement detect_ime_sensitive_region function in helix-core/src/syntax.rs that queries tree-sitter syntax tree to identify string/comment regions (FR-003)
- [X] T012 Add logic to detect_ime_sensitive_region to exclude leading quote symbols (FR-004) in helix-core/src/syntax.rs
- [X] T013 Add logic to detect_ime_sensitive_region to exclude comment header symbols (FR-005) in helix-core/src/syntax.rs
- [X] T014 Add logic to detect_ime_sensitive_region to include trailing quote first character (FR-006) in helix-core/src/syntax.rs
- [X] T015 Add logic to detect_ime_sensitive_region to include comment tail first character (FR-007) in helix-core/src/syntax.rs
- [X] T016 Add fallback logic to detect_ime_sensitive_region to return EntireFile when syntax parsing is unavailable, failed, or still in progress (FR-008, FR-009, FR-021) in helix-core/src/syntax.rs

### View Initialization

- [X] T017 Implement initialize_view_ime_state function in helix-term/src/handlers/ime.rs to create default ImeState and close IME if enabled
- [X] T018 Integrate initialize_view_ime_state call in view creation logic (hook into DocumentDidOpen or view creation)

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - IME在代码编辑中的自动切换 (Priority: P1) 🎯 MVP

**Goal**: 在Insert模式下根据光标位置自动切换IME状态：代码区域关闭IME，字符串/注释区域开启IME

**Independent Test**: 打开代码文件，在insert模式下将光标移动到代码区域、字符串区域和注释区域，验证IME状态是否正确切换

### Tests for User Story 1

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T019 [P] [US1] Integration test: IME关闭当光标在代码区域 in helix-term/tests/integration.rs (test_ime_region_detection_code)
- [X] T020 [P] [US1] Integration test: IME开启当光标在字符串内容区域 in helix-term/tests/integration.rs (test_ime_region_detection_string_content)
- [X] T021 [P] [US1] Integration test: IME开启当光标在注释内容区域 in helix-term/tests/integration.rs (test_ime_region_detection_comment_content)
- [X] T022 [P] [US1] Integration test: IME关闭当光标在字符串前导引号 in helix-term/tests/integration.rs (test_ime_closed_at_string_leading_quote)
- [X] T023 [P] [US1] Integration test: IME关闭当光标在注释首部符号 in helix-term/tests/integration.rs (test_ime_closed_at_comment_header)
- [X] T024 [P] [US1] Integration test: IME开启当光标在字符串尾部引号第一个字符 in helix-term/tests/integration.rs (test_ime_enabled_at_string_trailing_quote_first_char)
- [X] T025 [P] [US1] Integration test: IME开启当光标在注释尾部符号第一个字符 in helix-term/tests/integration.rs (test_ime_enabled_at_comment_tail_first_char)

### Implementation for User Story 1

- [X] T026 [US1] Implement handle_cursor_move function in helix-term/src/handlers/ime.rs to detect region change and update IME state in real-time (FR-018)
- [X] T027 [US1] Add logic to handle_cursor_move to get primary cursor position from selection (FR-020) in helix-term/src/handlers/ime.rs
- [X] T028 [US1] Add logic to handle_cursor_move to call detect_ime_sensitive_region and cache result in current_region in helix-term/src/handlers/ime.rs
- [X] T029 [US1] Add logic to handle_cursor_move to close IME when moving from sensitive to non-sensitive region (FR-015) in helix-term/src/handlers/ime.rs
- [X] T030 [US1] Add logic to handle_cursor_move to restore IME when moving from non-sensitive to sensitive region (FR-016) in helix-term/src/handlers/ime.rs
- [X] T031 [US1] Add logic to handle_cursor_move to skip processing if region hasn't changed (FR-022) in helix-term/src/handlers/ime.rs
- [X] T032 [US1] Add logic to handle_cursor_move to skip processing if not in Insert mode (FR-002: Insert mode is IME-sensitive, other modes are not) in helix-term/src/handlers/ime.rs
- [X] T033 [US1] Register SelectionDidChange event hook in helix-term/src/handlers/ime.rs to call handle_cursor_move
- [X] T034 [US1] Add error handling to handle_cursor_move to silently fail and log errors (FR-019) in helix-term/src/handlers/ime.rs

**Checkpoint**: At this point, User Story 1 should be fully functional and testable independently

---

## Phase 4: User Story 2 - IME状态在模式切换时的保存与恢复 (Priority: P1)

**Goal**: 在模式切换时保存和恢复IME状态，确保用户在Insert模式间切换时IME状态能够正确恢复

**Independent Test**: 在Insert模式下开启IME，切换到Normal模式，再切换回Insert模式，验证IME状态是否正确恢复

### Tests for User Story 2

- [X] T035 [P] [US2] Integration test: 保存IME开启状态当退出Insert模式 in helix-term/tests/integration.rs (test_ime_state_saved_when_exiting_insert_mode)
- [X] T036 [P] [US2] Integration test: 不保存IME关闭状态当退出Insert模式 in helix-term/tests/integration.rs (test_ime_closed_state_not_saved_on_exit)
- [X] T037 [P] [US2] Integration test: 恢复IME状态当进入Insert模式且光标在敏感区域 in helix-term/tests/integration.rs (test_ime_state_restored_in_sensitive_region)
- [X] T038 [P] [US2] Integration test: 不恢复IME状态当进入Insert模式但光标在非敏感区域 in helix-term/tests/integration.rs (test_ime_state_not_restored_in_non_sensitive_region)

### Implementation for User Story 2

- [X] T039 [US2] Implement handle_mode_switch function in helix-term/src/handlers/ime.rs to handle mode transitions
- [X] T040 [US2] Add logic to handle_mode_switch to save IME state when exiting Insert mode (FR-011) in helix-term/src/handlers/ime.rs
- [X] T041 [US2] Add logic to handle_mode_switch to only save if IME is enabled when exiting Insert mode (FR-011) in helix-term/src/handlers/ime.rs
- [X] T042 [US2] Add logic to handle_mode_switch to close IME when exiting Insert mode (FR-012) in helix-term/src/handlers/ime.rs
- [X] T043 [US2] Add logic to handle_mode_switch to clear current_region when exiting Insert mode in helix-term/src/handlers/ime.rs
- [X] T044 [US2] Add logic to handle_mode_switch to restore IME state when entering Insert mode if cursor in sensitive region (FR-002, FR-013) in helix-term/src/handlers/ime.rs
- [X] T045 [US2] Add logic to handle_mode_switch to keep IME closed when entering Insert mode if cursor in non-sensitive region (FR-002, FR-014) in helix-term/src/handlers/ime.rs
- [X] T046 [US2] Register OnModeSwitch event hook in helix-term/src/handlers/ime.rs to call handle_mode_switch
- [X] T047 [US2] Add error handling to handle_mode_switch to silently fail and log errors (FR-019) in helix-term/src/handlers/ime.rs

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently

---

## Phase 5: User Story 3 - 无法语法解析文件的IME处理 (Priority: P2)

**Goal**: 对于无法语法解析的文件或语法解析出错的文件，整个文件视为IME敏感区域

**Independent Test**: 打开纯文本文件或语法解析失败的文件，在Insert模式下验证IME可以在任何位置开启

### Tests for User Story 3

- [X] T048 [P] [US3] Integration test: IME可开启在无法语法解析的文件任意位置 in helix-term/tests/test/ime.rs (test_ime_enabled_in_unparseable_file_anywhere)
- [X] T049 [P] [US3] Integration test: IME可开启在语法解析出错的文件任意位置 in helix-term/tests/test/ime.rs (test_ime_enabled_in_syntax_error_file_anywhere)

### Implementation for User Story 3

- [X] T050 [US3] Verify detect_ime_sensitive_region returns EntireFile when syntax parsing unavailable (already implemented in T016) in helix-core/src/syntax.rs
- [X] T051 [US3] Verify detect_ime_sensitive_region returns EntireFile when syntax parsing failed (already implemented in T016) in helix-core/src/syntax.rs
- [X] T052 [US3] Add test coverage for syntax parsing error scenarios in helix-term/tests/test/ime.rs (test_ime_region_detection_syntax_error_scenarios)

**Checkpoint**: User Story 3 should now work independently

---

## Phase 6: User Story 4 - 无字符串和注释类型文件的IME处理 (Priority: P2)

**Goal**: 对于语法中不包含字符串和注释类型的文件，整个文件视为IME敏感区域

**Independent Test**: 打开不包含字符串和注释类型的文件，在Insert模式下验证IME可以在任何位置开启

### Tests for User Story 4

- [X] T053 [P] [US4] Integration test: IME可开启在无字符串和注释类型文件的任意位置 in helix-term/tests/test/ime.rs (test_ime_enabled_in_file_without_string_comment_types)

### Implementation for User Story 4

- [X] T054 [US4] Add logic to detect_ime_sensitive_region to check if language has string/comment types in helix-core/src/syntax.rs (language_has_string_or_comment_types function)
- [X] T055 [US4] Return EntireFile from detect_ime_sensitive_region if language doesn't have string/comment types (FR-010) in helix-core/src/syntax.rs
- [X] T056 [US4] Add test coverage for languages without string/comment types in helix-term/tests/test/ime.rs (test_ime_region_detection_language_without_string_comment)

**Checkpoint**: User Story 4 should now work independently

---

## Phase 7: User Story 5 - View初始化时的IME状态 (Priority: P1)

**Goal**: View初始化时IME状态应该关闭，如果系统IME开启则主动关闭

**Independent Test**: 在系统IME开启状态下启动helix编辑器，验证IME自动关闭

### Tests for User Story 5

- [X] T057 [P] [US5] Integration test: IME自动关闭当view初始化且系统IME开启 in helix-term/tests/test/ime.rs (test_ime_auto_closed_on_view_init_when_system_ime_enabled)
- [X] T058 [P] [US5] Integration test: IME保持关闭当view初始化且系统IME关闭 in helix-term/tests/test/ime.rs (test_ime_stays_closed_on_view_init_when_system_ime_disabled)

### Implementation for User Story 5

- [X] T059 [US5] Verify initialize_view_ime_state closes IME if system IME is enabled (already implemented in T017) in helix-term/src/handlers/ime.rs
- [X] T060 [US5] Verify initialize_view_ime_state creates default ImeState with saved_state=None (already implemented in T017) in helix-term/src/handlers/ime.rs
- [X] T061 [US5] Ensure initialize_view_ime_state is called for all new views (verify T018 integration) - verified in DocumentDidOpen hook

**Checkpoint**: User Story 5 should now work independently

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

### Handler Registration

- [X] T062 Register IME handler hooks in helix-term/src/handlers.rs setup function
- [X] T063 Add IME handler module declaration in helix-term/src/handlers.rs

### Error Handling & Logging

- [ ] T064 [P] Add comprehensive error logging for all IME API failures in helix-term/src/handlers/ime.rs
- [ ] T065 [P] Verify all error paths are handled silently without interrupting editor flow in helix-term/src/handlers/ime.rs

### Performance Optimization

- [ ] T066 Verify current_region caching prevents redundant syntax queries in helix-term/src/handlers/ime.rs
- [ ] T067 Verify IME detection only runs in Insert mode to minimize performance impact in helix-term/src/handlers/ime.rs
- [ ] T068 Add performance benchmarks to verify <100ms response time requirement (SC-001) in helix-term/tests/integration.rs

### Edge Cases

- [ ] T069 [P] Add test for multi-cursor scenario using primary cursor (FR-020) in helix-term/tests/integration.rs
- [ ] T070 [P] Add test for syntax parsing delay scenario (FR-021: entire file treated as sensitive until first syntax tree is built) in helix-term/tests/integration.rs
- [ ] T071 [P] Add test for multi-line string/comment scenarios in helix-term/tests/integration.rs
- [ ] T072 [P] Add test for rapid cursor movement scenario (SC-007) in helix-term/tests/integration.rs
- [X] T073 [P] Add test for per-view IME state independence (FR-017) in helix-term/tests/test/ime.rs (test_ime_context_independence_per_view)

### Documentation

- [ ] T074 [P] Update code documentation for IME handler functions in helix-term/src/handlers/ime.rs
- [ ] T075 [P] Update code documentation for detect_ime_sensitive_region in helix-core/src/syntax.rs
- [ ] T076 [P] Add inline comments explaining IME state transitions in helix-view/src/editor.rs

### Success Criteria Testing

- [ ] T080 [P] Add test to verify 100% success rate for IME auto-enable when moving from code to string region (SC-002) in helix-term/tests/integration.rs
- [ ] T081 [P] Add test to verify 100% success rate for IME auto-disable when moving from string to code region (SC-003) in helix-term/tests/integration.rs
- [ ] T082 [P] Add test to verify 100% accuracy for IME state save/restore during mode switch (SC-004) in helix-term/tests/integration.rs
- [ ] T083 [P] Add test to verify 100% availability for IME in unparseable files at any position (SC-005) in helix-term/tests/integration.rs
- [ ] T084 [P] Add test to verify 100% success rate for IME closure during view initialization regardless of system IME state (SC-006) in helix-term/tests/integration.rs
- [ ] T085 [P] Add performance benchmark to verify <5% editor performance impact (SC-008) in helix-term/tests/integration.rs

### Validation

- [ ] T086 Run quickstart.md validation scenarios
- [ ] T087 Verify all functional requirements (FR-001 through FR-022) are implemented
- [ ] T088 Verify all success criteria (SC-001 through SC-008) are met

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3+)**: All depend on Foundational phase completion
  - User stories can then proceed in parallel (if staffed)
  - Or sequentially in priority order (P1 → P2)
- **Polish (Final Phase)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P1)**: Can start after Foundational (Phase 2) - Depends on US1 for cursor move handling, but independently testable
- **User Story 3 (P2)**: Can start after Foundational (Phase 2) - Depends on detect_ime_sensitive_region (T016), independently testable
- **User Story 4 (P2)**: Can start after Foundational (Phase 2) - Depends on detect_ime_sensitive_region enhancements, independently testable
- **User Story 5 (P1)**: Can start after Foundational (Phase 2) - Depends on initialize_view_ime_state (T017), independently testable

### Within Each User Story

- Tests MUST be written and FAIL before implementation
- Core detection functions before event handlers
- Event handlers before integration
- Story complete before moving to next priority

### Parallel Opportunities

- **Phase 2**: T003-T010 can run in parallel (different files/modules)
- **Phase 3 (US1)**: T019-T025 (tests) can run in parallel
- **Phase 4 (US2)**: T035-T038 (tests) can run in parallel
- **Phase 5 (US3)**: T048-T049 (tests) can run in parallel
- **Phase 6 (US4)**: T053 (test) can run independently
- **Phase 7 (US5)**: T057-T058 (tests) can run in parallel
- **Phase 8**: T064-T076 (documentation and edge cases), T080-T085 (success criteria tests) can run in parallel
- Once Foundational phase completes, User Stories 1, 2, 3, 4, 5 can start in parallel (if team capacity allows)

---

## Parallel Example: User Story 1

```bash
# Launch all tests for User Story 1 together:
Task: T019 [US1] Integration test: IME关闭当光标在代码区域
Task: T020 [US1] Integration test: IME开启当光标在字符串内容区域
Task: T021 [US1] Integration test: IME开启当光标在注释内容区域
Task: T022 [US1] Integration test: IME关闭当光标在字符串前导引号
Task: T023 [US1] Integration test: IME关闭当光标在注释首部符号
Task: T024 [US1] Integration test: IME开启当光标在字符串尾部引号第一个字符
Task: T025 [US1] Integration test: IME开启当光标在注释尾部符号第一个字符

# Then implement core functionality:
Task: T026 [US1] Implement handle_cursor_move function
Task: T027 [US1] Add logic to get primary cursor position
Task: T028 [US1] Add logic to call detect_ime_sensitive_region
# ... continue with remaining implementation tasks
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: Test User Story 1 independently
5. Deploy/demo if ready

### Incremental Delivery

1. Complete Setup + Foundational → Foundation ready
2. Add User Story 1 → Test independently → Deploy/Demo (MVP!)
3. Add User Story 2 → Test independently → Deploy/Demo
4. Add User Story 5 → Test independently → Deploy/Demo (Complete P1 stories)
5. Add User Story 3 → Test independently → Deploy/Demo
6. Add User Story 4 → Test independently → Deploy/Demo
7. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together
2. Once Foundational is done:
   - Developer A: User Story 1 (cursor move handling)
   - Developer B: User Story 2 (mode switch handling)
   - Developer C: User Story 5 (view initialization)
3. Stories complete and integrate independently
4. Then proceed with P2 stories (US3, US4) in parallel

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Verify tests fail before implementing
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same file conflicts, cross-story dependencies that break independence
- All IME API errors must be handled silently with logging (FR-019)
- Performance requirement: <100ms response time (SC-001)
- Performance requirement: <5% editor performance impact (SC-008)
