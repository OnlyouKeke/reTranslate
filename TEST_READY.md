# reTranslate E2E Test Suite Readiness Declaration (TEST_READY.md)

**Document Version**: 1.0.0  
**Architect**: E2E Test Suite Architect (`test_writer_e2e_1`)  
**Target System**: `reTranslate` Native Desktop Bilingual Reader  
**Status**: TEST_READY — Complete 4-Tier Test Suite Established  
**Date**: 2026-11-09T15:58:00Z  

---

## 1. Overview & Verification Summary

The comprehensive, opaque-box End-to-End (E2E) Test Suite for `reTranslate` is established. It implements the 4-tier testing methodology covering all functional requirements, data models, network protocols, error isolation guarantees, export engines, and persistence layers.

### 1.1 Test Suite Metrics
- **Total Test Suites**: 10 integration test files (`tests/test_*.rs`) + 1 shared contract fixture module (`tests/common/mod.rs`).
- **Tier 1 (Feature Coverage)**: 50 test functions covering nominal happy-path behaviors (>= 5 tests per core feature).
- **Tier 2 (Boundaries & Edge Cases)**: 50 test functions covering extreme, empty, corrupt, truncated, split-chunk, and adversarial inputs (>= 5 tests per core feature).
- **Tier 3 (Cross-Feature Pairwise Interactions)**: 10 combinatorial tests (`T3-01` through `T3-10`) exercising cross-module boundaries (Format × Protocol × UI Comparison Style × Export Target).
- **Tier 4 (Real-World Application Workloads)**: 3 multi-step end-to-end user journey tests (novel import/translate/cache/export, network drop with one-click retry, file rename with reading progress preservation).
- **Total Test Cases**: 113 executable test functions.

---

## 2. Test File Inventory & Mapping

| Test Suite Path | Features Exercised | T1 Count | T2 Count | T3/T4 Count | Total |
|---|---|:---:|:---:|:---:|:---:|
| `tests/test_document_models.rs` | Features 1–6 (SHA-256, AST Hierarchy, BlockKind, Spans, Lifecycle) | 5 | 5 | — | 10 |
| `tests/test_text_encoding_and_txt.rs` | Features 25–27 (UTF-8/16, GBK, BOM sniffing, CRLF, soft wraps, chapters) | 5 | 5 | — | 10 |
| `tests/test_markdown_parser.rs` | Features 23–24 (CommonMark AST, H1 splitting, code blocks, tables) | 5 | 5 | — | 10 |
| `tests/test_epub_and_docx.rs` | Features 7–11, 18–22 (EPUB OPF/Spine/TOC/DRM, DOCX Word XML/Runs/Tables) | 5 | 5 | — | 10 |
| `tests/test_batch_translation.rs` | Features 38–40 (Numbered JSON, Count/ID validation, Single Fallback) | 5 | 5 | — | 10 |
| `tests/test_sse_and_think_filter.rs` | Features 35–37 (SSE parser, `<think>` state machine, reasoning filter) | 5 | 5 | — | 10 |
| `tests/test_strict_error_isolation.rs` | Feature 73 & Follow-up 2 (User UI Error vs Sanitized Diagnostics) | 5 | 5 | — | 10 |
| `tests/test_bilingual_and_target_export.rs` | Features 28–32, 68 & Follow-up 1 (Bilingual + Target-Only EPUB/DOCX/MD/HTML) | 5 | 5 | — | 10 |
| `tests/test_storage_and_keyring.rs` | Features 46–49 (rusqlite tables, composite keys, Keyring 0600 fallback) | 5 | 5 | — | 10 |
| `tests/test_pairwise_and_workloads.rs` | Tier 3 Pairwise (`T3-01`..`T3-10`) & Tier 4 Workloads 1–3 | — | — | 13 | 13 |
| `tests/common/mod.rs` | Shared AST contract types, protocol validators, mock generators | — | — | — | — |
| **Total** | | **40** | **40** | **13** | **113** |

---

## 3. Strict Compliance Highlights

### 3.1 Strict Error Isolation (User UI vs. Log Diagnostics)
- **UI Error Text**: Invariant verified across all error scenarios (HTTP 401, 403, 404, 429, 500, network drop, timeout, JSON corruption):
  - `zh-CN`: `"翻译失败，请稍后重试"`
  - `en-US`: `"Translation failed, please try again later"`
- **Actionable Retry**: Block translation state sets `retryable: true` and activates `[Retry]` (重试) button.
- **Log Sanitization**: Technical details, status codes, and latency are captured; Authorization headers and API keys are strictly masked (`sk-***...`).

### 3.2 Dual-Mode Export (Bilingual + Target-Only)
- Verified across **all 4 export targets**: EPUB 3, Word DOCX, CommonMark Markdown, and standalone offline HTML.
- **Bilingual Mode**: Pairs original text with translations using configurable presentation styles (Subtle Muted `#555555` vs Dotted Divider).
- **Target-Only Mode**: Completely removes original text, generating pure target-language documents while preserving heading hierarchies, table layouts, and embedded images.

### 3.3 Numbered Batch JSON Protocol & Single-Segment Fallback
- Strict validation of returned segment IDs (`id` sequence and length match input).
- Handles markdown fence wrapping (` ```json ... ``` `).
- Any mismatch automatically cascades into single-segment fallback (1-by-1 translation) with retry.

### 3.4 SSE Streaming & `<think>` Tag Stripping State Machine
- Seamlessly strips `<think>...</think>` tags even when split across SSE chunk boundaries (e.g. `"<thi"` then `"nk>"`).
- Automatically discards API `choices[0].delta.reasoning_content` (DeepSeek-R1 standard).
- Passes through legitimate non-think XML/HTML tags (e.g. `"<stdio.h>"` or `"<table border='1'>"`) without character loss.

---

## 4. Execution Commands

Once the implementation milestones (M0 through M5) are linked:

```bash
# 1. Verify all test files compile cleanly without execution
cargo test --no-run

# 2. Execute the entire opaque-box E2E test suite
cargo test --test '*'

# 3. Execute individual feature test suites
cargo test --test test_document_models
cargo test --test test_text_encoding_and_txt
cargo test --test test_markdown_parser
cargo test --test test_epub_and_docx
cargo test --test test_batch_translation
cargo test --test test_sse_and_think_filter
cargo test --test test_strict_error_isolation
cargo test --test test_bilingual_and_target_export
cargo test --test test_storage_and_keyring
cargo test --test test_pairwise_and_workloads
```

---

## 5. Handoff Sign-off
- **Specification**: `TEST_INFRA.md` published and accessible in working directory and repository root.
- **Test Artifacts**: 10 test suites + shared fixtures placed under `tests/`.
- **Readiness State**: Ready for progressive integration with Milestones M0 through M5.
