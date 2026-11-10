# reTranslate E2E Test Infrastructure Specification (TEST_INFRA.md)

**Document Version**: 1.0.0  
**Architect**: E2E Test Suite Architect (`test_writer_e2e_1`)  
**Target System**: `reTranslate` Desktop Bilingual Reader  
**Status**: APPROVED & PUBLISHED  

---

## 1. Executive Summary & Principles

The `reTranslate` End-to-End (E2E) Testing Track is designed as a strict **opaque-box verification system** validating all functional requirements, interface contracts, error resilience standards, and data integrity guarantees.

### 1.1 Core Principles
1. **Opaque-Box Verification**: Tests validate observable behaviors, output artifacts, API contracts, database states, and UI error states without depending on private internal refactorings.
2. **Deterministic Output Derivation**: Expected outputs are derived directly from authoritative specifications:
   - Cryptographic invariants (SHA-256 hex digests)
   - Unicode / Encoding RFCs & Standards (`chardetng`, `encoding_rs`, UTF-8, GBK, UTF-16)
   - Open Standard File Formats (EPUB 3 / OCF / OPF, DOCX OpenXML / ECMA-376, CommonMark 0.31, HTML5)
   - OpenAI Chat Completion & SSE Protocol (`text/event-stream`, `choices[0].delta`)
   - Strict Error Isolation Mandate (`ORIGINAL_REQUEST.md` Follow-up 2)
   - Dual-Mode Export Specification (`ORIGINAL_REQUEST.md` Follow-up 1)
3. **Progressive Testability & Isolation**: Every test is self-contained, creates its own isolated resources (in-memory SQLite, isolated temp directories, mock HTTP streams), and cleans up after itself.
4. **4-Tier Testing Methodology**:
   - **Tier 1: Feature Coverage** (Minimum 5 tests per feature covering nominal happy paths).
   - **Tier 2: Boundary & Corner Cases** (Minimum 5 tests per feature covering extreme, adversarial, and edge inputs).
   - **Tier 3: Cross-Feature Interactions** (Pairwise combination testing covering cross-cutting dependencies).
   - **Tier 4: Real-World Application Workloads** (Full end-to-end lifecycle simulation mimicking user workflows).

---

## 2. 4-Tier Test Architecture & Matrix

```
+---------------------------------------------------------------------------------------+
|                               Tier 4: Real-World Workloads                            |
|  - End-to-End E-Book Translation Journey (Import -> Viewport Stream -> Cache -> Export)|
|  - Interrupted Network with Partial Failure & One-Click Retry Workflow                |
|  - Multi-Document Library Management with SHA-256 Fingerprinting Across Path Moves     |
+---------------------------------------------------------------------------------------+
                                           ^
                                           |
+---------------------------------------------------------------------------------------+
|                           Tier 3: Cross-Feature Interactions                          |
|  - Format Parser x Batch Translation x Export Mode Pairwise Matrix                    |
|  - SSE Streaming x Think Filter x UI Popover x Glossary Replacement                   |
|  - SQLite Persistent Cache Hit x Viewport Scheduler Queue Cancellation                |
+---------------------------------------------------------------------------------------+
                                           ^
                                           |
+---------------------------------------------------------------------------------------+
|                           Tier 2: Boundary & Corner Cases                             |
|  - 0-byte, 100MB+, and malformed documents; split UTF-8 / GBK byte boundaries         |
|  - Split <think> tags across SSE chunk boundaries; unclosed reasoning tags             |
|  - Batch ID mismatch, merged segments, count errors, JSON syntax corruption           |
|  - Strict Error Isolation: HTTP 401/429/500/timeout -> "翻译失败，请稍后重试" + [Retry]|
|  - Keyring daemon disconnection -> 0600 file fallback security validation             |
+---------------------------------------------------------------------------------------+
                                           ^
                                           |
+---------------------------------------------------------------------------------------+
|                              Tier 1: Core Feature Coverage                            |
|  - Feature 1: SHA-256 Fingerprinting & Unified Document AST Hierarchy                 |
|  - Feature 2: Text Encoding Detection & TXT Parsing (UTF-8, GBK, UTF-16, Chapter RegEx) |
|  - Feature 3: Markdown CommonMark Parsing & Heading Chapter Partitioning              |
|  - Feature 4: EPUB / DOCX Package Traversal & Semantic Block Extraction               |
|  - Feature 5: Numbered Batch JSON Protocol & Count/ID Validator                       |
|  - Feature 6: SSE Streaming & Think-Tag Stripping State Machine                       |
|  - Feature 7: Strict Error Isolation & Sanitized Diagnostics                          |
|  - Feature 8: Bilingual & Target-Only Export Engines (EPUB, DOCX, MD, HTML)           |
|  - Feature 9: SQLite Schema Migrations & Keyring Credentials Storage                  |
+---------------------------------------------------------------------------------------+
```

---

## 3. Comprehensive Feature Test Catalog

### Feature 1: Document Models & SHA-256 Fingerprinting
- **Authoritative Source**: `PROJECT.md` § Feature 1, 2, 3, 4, 5, 6; `retranslate_reader_plan.md` § Core.
- **Invariants**:
  - SHA-256 hash of file content must produce exact 64-character lowercase hex string.
  - Renaming or moving a file must NOT alter its `id`.
  - Block IDs follow `{chapter_id}_b{index}` strictly.
  - `BlockKind` covers Paragraph, Heading(1..=6), CodeBlock, Quote, ListItem, Table, Image, ThematicBreak.
  - `TranslationState` accurately represents `Untranslated`, `Translating`, `Translated`, and `Failed`.
- **Tier 1 Tests**:
  1. `test_sha256_identical_content_different_names`: Two files with identical bytes have identical IDs.
  2. `test_sha256_different_content_distinct_ids`: Minor byte difference yields completely distinct hashes.
  3. `test_ast_hierarchy_navigation`: Document containing multiple chapters and blocks traverses cleanly.
  4. `test_block_plain_text_extraction`: Extracts plain text combining all `InlineSpan`s correctly.
  5. `test_translation_state_transitions`: Transitions through `Untranslated` -> `Translating` -> `Translated`.
- **Tier 2 Tests**:
  1. `test_sha256_empty_file`: 0-byte file produces exact SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
  2. `test_ast_empty_document`: 0-block document instantiates without panic or out-of-bounds error.
  3. `test_block_kind_code_block_translatable_false`: Verifies code blocks default to `translatable: false`.
  4. `test_inline_span_nested_formatting`: Bold, italic, code, and footnote links in single block.
  5. `test_block_translation_failure_state`: `TranslationState::Failed` retains error message and retryable flag.

---

### Feature 2: Text Encodings & TXT Paragraph Parser
- **Authoritative Source**: `PROJECT.md` § Feature 25, 26, 27; `retranslate_reader_plan.md` § Core.
- **Invariants**:
  - UTF-8 with/without BOM, UTF-16LE, UTF-16BE correctly detected via BOM sniffing.
  - GBK / Windows-936 / CP936 encoded Chinese decoded cleanly without `\u{FFFD}` replacement chars.
  - CRLF (`\r\n`) and CR (`\r`) normalized to LF (`\n`).
  - Blank-line paragraph segmentation: merges soft wraps, joins Latin words with space, joins CJK characters without space.
  - Chinese chapter heading regex (`^第[0-9一二三四五六七八九十百千万]+[章回节卷集篇部]`) and English (`^Chapter [0-9IVXLCDM]+`) split chapters.
- **Tier 1 Tests**:
  1. `test_utf8_plain_and_bom`: Detects and decodes standard UTF-8 text with and without BOM.
  2. `test_gbk_chinese_literature`: Detects and decodes GBK-encoded Chinese text correctly.
  3. `test_utf16le_and_be_bom`: Detects BOM and decodes UTF-16LE and UTF-16BE files.
  4. `test_crlf_normalization`: Normalizes Windows CRLF to LF, splitting paragraphs on blank lines.
  5. `test_chapter_detection_chinese_and_english`: Accurately detects `第一章` and `Chapter 1`.
- **Tier 2 Tests**:
  1. `test_mixed_ascii_header_gbk_body`: Detects GBK despite initial 512-byte ASCII metadata header.
  2. `test_soft_wrap_latin_spacing`: `"hello\nworld"` -> `"hello world"` (space inserted).
  3. `test_soft_wrap_cjk_no_spacing`: `"你好\n世界"` -> `"你好世界"` (no space inserted).
  4. `test_monolithic_single_line_chunking`: 5000-character single-line file splits cleanly at sentence punctuation.
  5. `test_thousands_consecutive_blank_lines`: Reduces 10,000 blank lines into single paragraph divider.

---

### Feature 3: Markdown Parser & CommonMark Conformance
- **Authoritative Source**: `PROJECT.md` § Feature 23, 24; `retranslate_reader_plan.md` § Core.
- **Invariants**:
  - CommonMark AST generation with GitHub-flavored Markdown extensions (tables, task lists, strikethrough).
  - Top-level Heading 1 (`# `) divides document into logical chapters.
  - Fenced code blocks preserve language tag and code verbatim; marked untranslatable.
  - Blockquotes, ordered lists, and unordered lists mapped to `BlockKind`.
  - Tables parsed with column headers and row alignments.
- **Tier 1 Tests**:
  1. `test_md_headings_h1_to_h6`: Correctly parses headings 1 through 6.
  2. `test_md_chapter_splitting_on_h1`: Divides multiple `# ` sections into separate `Chapter`s.
  3. `test_md_inline_formatting`: Bold, italic, strikethrough, and inline code formatting preserved.
  4. `test_md_gfm_tables`: Extracts header cells, body rows, and alignments (left/center/right).
  5. `test_md_code_block_untranslatable`: Code block language and syntax preserved; `translatable == false`.
- **Tier 2 Tests**:
  1. `test_md_raw_html_sanitization`: Script tags or unsafe HTML elements stripped or converted safely.
  2. `test_md_nested_lists_and_depth`: Nested list items preserve depth levels (0..=3).
  3. `test_md_empty_file_and_whitespace`: Empty markdown file parsed without errors.
  4. `test_md_escaped_characters`: Markdown meta-characters (`\*`, `\[`, `\#`) treated as plain text.
  5. `test_md_blockquotes_nested`: Multi-level blockquotes parsed into structured `BlockKind::Quote`.

---

### Feature 4: EPUB & DOCX Structural Conformance
- **Authoritative Source**: `PROJECT.md` § Feature 7, 8, 9, 10, 11, 18, 19, 20, 21, 22.
- **Invariants**:
  - EPUB `container.xml` -> `.opf` -> spine itemrefs -> TOC hierarchy parsed cleanly.
  - EPUB internal image relative paths resolved against chapter base directory.
  - EPUB footnotes (`epub:type="noteref"`) associated with footnote text.
  - DOCX `word/document.xml` paragraphs, runs, headings (`Heading1`..`Heading6`), and tables extracted.
  - Corrupt or non-DOCX/EPUB files return typed error without crash or panic.
- **Tier 1 Tests**:
  1. `test_epub_metadata_and_spine`: Reads title, author, language, and reading order from EPUB.
  2. `test_epub_xhtml_block_extraction`: Extracts paragraphs, headings, and lists from chapter XHTML.
  3. `test_epub_toc_ncx_and_nav`: Traverses hierarchical Table of Contents cleanly.
  4. `test_docx_paragraph_and_headings`: Maps Word styles (`Heading 1`, `Heading 2`) to `BlockKind::Heading`.
  5. `test_docx_tables_and_runs`: Extracts table cells and formatted runs (bold, italic, strike).
- **Tier 2 Tests**:
  1. `test_epub_drm_detection`: Returns `DocumentError::DrmProtected` when encryption XML is present.
  2. `test_epub_missing_manifest_resource`: Missing image does not crash parser; yields placeholder block.
  3. `test_epub_malformed_unclosed_xhtml`: Fault-tolerant HTML5 scraper repairs unclosed tags.
  4. `test_docx_corrupted_zip_archive`: Non-zip or truncated file returns `DocxError::InvalidPackage`.
  5. `test_docx_complex_merged_table_cells`: Correctly reads `gridSpan` and `vMerge` attributes.

---

### Feature 5: Numbered JSON Batch Translation Protocol & Single Fallback
- **Authoritative Source**: `PROJECT.md` § Feature 38, 39, 40; `ORIGINAL_REQUEST.md` § 2; `retranslate_reader_plan.md` § 3.
- **Invariants**:
  - Request format: numbered JSON payload `{"segments":[{"id":usize,"text":string}]}`.
  - Strip markdown fences: handles responses wrapped in ```` ```json ... ``` ````.
  - Strict validation: returned segment IDs must strictly match input IDs in count and order.
  - Automatic fallback: any mismatch (missing ID, disordered ID, extra/fewer count, parse error) immediately triggers single-segment fallback (1-by-1 request).
- **Tier 1 Tests**:
  1. `test_batch_prompt_generation`: Packs 5–15 segments into structured JSON prompt template.
  2. `test_batch_validation_success`: Perfectly matching IDs and counts return `BatchValidationResult::Success`.
  3. `test_batch_markdown_fence_stripping`: Extracts clean JSON from ```` ```json {"segments":...} ``` ````.
  4. `test_batch_count_mismatch_fallback`: 3 segments in, 2 segments out -> triggers `Mismatch`.
  5. `test_batch_id_order_mismatch_fallback`: IDs out of order ([2, 1]) -> triggers `Mismatch`.
- **Tier 2 Tests**:
  1. `test_batch_missing_intermediate_id`: IDs [1, 3] when [1, 2, 3] expected -> triggers `Mismatch`.
  2. `test_batch_unescaped_quotes_json_error`: Malformed JSON string returns `ParseError` and initiates fallback.
  3. `test_batch_merged_paragraphs_recovery`: Fallback translates each segment individually without losing text.
  4. `test_single_segment_fallback_retry`: Single segment retries once on network glitch before setting `Failed`.
  5. `test_batch_size_upper_and_lower_bounds`: Tests batch with 1 segment, 15 segments, and 50 segments.

---

### Feature 6: SSE Streaming & `<think>` Tag Stripping State Machine
- **Authoritative Source**: `PROJECT.md` § Feature 35, 36, 37; `retranslate_reader_plan.md` § 3.
- **Invariants**:
  - SSE parser consumes `text/event-stream`, extracts `data: {"choices":[{"delta":{"content":"..."}}]}`, terminates on `data: [DONE]`.
  - State machine strips inline `<think>...</think>` XML reasoning blocks from streaming delta chunks.
  - Handles split tags across chunk boundaries (e.g. `"<thi"`, `"nk> reasoning text...", "</th"`, `"ink>"`).
  - Discards API `choices[0].delta.reasoning_content` (DeepSeek-R1 API standard) without failing.
  - Preserves legitimate XML-like code (e.g. `"<stdio.h>"` or `"<table border='1'>"`) without dropping text.
- **Tier 1 Tests**:
  1. `test_sse_standard_stream_parsing`: Parses consecutive SSE deltas into assembled string.
  2. `test_sse_done_termination`: Terminates stream cleanly upon receiving `data: [DONE]`.
  3. `test_think_filter_nominal_stripping`: Strips complete `<think>thought</think>result` -> `"result"`.
  4. `test_reasoning_content_delta_discarded`: Discards `delta.reasoning_content` and keeps `delta.content`.
  5. `test_multiline_reasoning_stripping`: Strips multi-line reasoning blocks with line breaks.
- **Tier 2 Tests**:
  1. `test_think_tag_split_opening_chunk`: Chunk 1: `"<thi"`, Chunk 2: `"nk>reasoning</think>output"` -> `"output"`.
  2. `test_think_tag_split_closing_chunk`: Chunk 1: `"<think>reasoning</th"`, Chunk 2: `"ink>output"` -> `"output"`.
  3. `test_unclosed_think_tag_recovery`: Model outputs `<think>reasoning` without `</think>` before ending; output is empty, does not crash.
  4. `test_legitimate_xml_passthrough`: `"<stdio.h> and <vector>"` passed through without dropping characters.
  5. `test_nested_or_consecutive_think_tags`: Multiple `<think>` blocks in single stream handled safely.

---

### Feature 7: Strict Error Isolation & One-Click Retry
- **Authoritative Source**: `ORIGINAL_REQUEST.md` Follow-up 2 (2026-11-09T15:36:49Z); `PROJECT.md` § Feature 73.
- **Invariants**:
  - **User-Facing UI Guarantee**:
    - For ANY error (HTTP 401, 403, 404, 429, 500, network timeout, DNS failure, JSON error, batch failure):
    - UI presentation state **ALWAYS and ONLY** displays:
      - `zh-CN`: `"翻译失败，请稍后重试"`
      - `en-US`: `"Translation failed, please try again later"`
    - Must provide an actionable `retryable: true` state that triggers the **[Retry] (重试)** button.
    - **ZERO leakage** of raw error messages, status codes, stack traces, or internal error types to UI state.
  - **Internal Logging Guarantee**:
    - Raw technical details (HTTP status, URL, error string, elapsed ms) written to internal diagnostics only.
    - Strict sanitization: API keys and authorization headers masked (`sk-***...`) in all log entries.
- **Tier 1 Tests**:
  1. `test_ui_error_message_zh_cn`: HTTP 500 error returns UI text `"翻译失败，请稍后重试"`.
  2. `test_ui_error_message_en_us`: HTTP 500 error returns UI text `"Translation failed, please try again later"`.
  3. `test_retry_flag_set_on_network_failure`: Network timeout sets `retryable: true` in block translation state.
  4. `test_sanitized_logger_receives_details`: Logger captures HTTP status and error cause for diagnostics.
  5. `test_api_key_masked_in_logs`: Logs mask Bearer token `sk-1234567890abcdef` to `sk-***cdef`.
- **Tier 2 Tests**:
  1. `test_http_429_rate_limit_ui_isolation`: HTTP 429 still displays `"翻译失败，请稍后重试"`, never exposes "429 Too Many Requests".
  2. `test_http_401_unauthorized_ui_isolation`: HTTP 401 invalid key shows friendly UI error and flags retry.
  3. `test_json_deserialization_error_ui_isolation`: Corrupt JSON from provider never leaks JSON syntax error to UI.
  4. `test_dns_timeout_ui_isolation`: Connection timeout shows friendly UI error with retry enabled.
  5. `test_one_click_retry_action_dispatch`: Invoking retry action re-enqueues block at Priority 0.

---

### Feature 8: Bilingual & Target-Only Export Verification
- **Authoritative Source**: `ORIGINAL_REQUEST.md` Follow-up 1 (2026-11-09T15:35:52Z); `PROJECT.md` § Feature 28, 29, 30, 31, 32, 68.
- **Invariants**:
  - **Bilingual Export**:
    - EPUB: Valid EPUB 3 zip archive; includes `.orig-text` and `.trans-text`; CSS styles for `.trans-text-muted` and `.trans-text-dotted`.
    - DOCX: Valid Word document; paired paragraphs with secondary styling (muted color `#555555`, italic, or dotted divider).
    - Markdown: Original paragraph followed by blockquote `> 译文` or inline translation.
    - HTML: Standalone single-file HTML with embedded responsive CSS, dark/light theme, and bilingual mode controls.
  - **Target-Only Export**:
    - Pure translated EPUB, DOCX, Markdown, HTML containing target text only.
    - Original source text completely omitted from body content.
    - Full chapter structure, headings, and images preserved.
  - Untranslated / partially translated documents:
    - 0-translated document export succeeds cleanly without placeholder text.
    - Partially translated document preserves original text for untranslated blocks.
- **Tier 1 Tests**:
  1. `test_export_bilingual_epub_structure`: Verifies valid EPUB 3 zip container, OPF, and paired classes.
  2. `test_export_bilingual_docx_paired_paragraphs`: Verifies paired runs and secondary color formatting.
  3. `test_export_bilingual_markdown_blockquotes`: Verifies CommonMark format with `> ` translation quotes.
  4. `test_export_bilingual_html_standalone`: Single self-contained HTML file with embedded CSS and layout switcher.
  5. `test_export_target_only_all_formats`: Generates target-only EPUB, DOCX, MD, and HTML without source text.
- **Tier 2 Tests**:
  1. `test_export_untranslated_document`: Exporting document with 0 translations generates clean original document.
  2. `test_export_partially_translated`: Only translated blocks have translations; pending blocks show original.
  3. `test_export_xml_html_entity_escaping`: Characters `&`, `<`, `>`, `"`, `'` properly escaped in EPUB/HTML.
  4. `test_export_style_subtle_muted_vs_dotted`: Verifies correct CSS classes/Word styles for both presentation modes.
  5. `test_export_target_only_preserves_images_and_headings`: Target-only export retains images and heading hierarchy.

---

### Feature 9: Storage CRUD & Secure Keyring Fallback
- **Authoritative Source**: `PROJECT.md` § Feature 46, 47, 48, 49; `retranslate_reader_plan.md` § Store.
- **Invariants**:
  - SQLite tables: `translations`, `reading_progress`, `bookmarks`, `glossary`, `app_preferences`.
  - Composite primary key `(source_hash, provider, model, target_lang)` on `translations`.
  - Cache lookup hit returns translation; cache miss returns `None`.
  - Keyring storage with fallback: falls back to restricted permission file (`~/.config/retranslate/credentials.json` with chmod 0600) when OS keyring daemon is absent.
- **Tier 1 Tests**:
  1. `test_sqlite_migration_execution`: Creates all 5 tables and indexes cleanly.
  2. `test_translation_cache_crud`: Inserts, retrieves, and updates cached translation.
  3. `test_reading_progress_save_and_restore`: Saves scroll offset and restores exact block index.
  4. `test_glossary_crud_and_case_sensitivity`: Inserts terminology pairs with case sensitivity.
  5. `test_keyring_store_and_retrieve`: Stores and retrieves provider API key.
- **Tier 2 Tests**:
  1. `test_translation_cache_model_isolation`: Different models for identical text store distinct translations.
  2. `test_translation_cache_provider_isolation`: Different providers store distinct translations.
  3. `test_keyring_daemon_unavailable_fallback`: Headless environment without D-Bus falls back to 0600 file.
  4. `test_credentials_file_permission_enforcement`: Unix permissions verify `0600` (read/write by owner only).
  5. `test_sqlite_concurrent_connections`: Multi-threaded access to SQLite cache functions safely.

---

## 4. Tier 3: Cross-Feature Interaction & Pairwise Testing

The Tier 3 matrix tests combinations of features to catch integration anomalies across document formats, translation models, UI comparison styles, and export engines:

| Test ID | Document Format | Translation Protocol | Comparison Style | Export Mode | Invalidation / Stress Condition |
|---------|-----------------|----------------------|------------------|-------------|---------------------------------|
| `T3-01` | EPUB | Numbered Batch JSON | Inline Subtle Muted | Bilingual EPUB | Batch ID Mismatch -> Fallback |
| `T3-02` | DOCX | Numbered Batch JSON | Inline Dotted Divider | Bilingual DOCX | Merged Segments -> Fallback |
| `T3-03` | Markdown | SSE Streaming | Side-by-Side Equal Height | Bilingual Markdown | Split `<think>` Tag |
| `T3-04` | TXT (GBK) | Numbered Batch JSON | Target Only | Target-Only HTML | HTTP 429 Rate Limit -> Backoff |
| `T3-05` | TXT (UTF-8) | SSE Streaming | Target Only | Target-Only EPUB | Unclosed `<think>` Tag |
| `T3-06` | EPUB | Single Segment | Inline Subtle Muted | Target-Only DOCX | HTTP 500 -> User UI Error + Retry |
| `T3-07` | DOCX | Numbered Batch JSON | Original Only | Bilingual HTML | Partial Translation (50% done) |
| `T3-08` | Markdown | Single Segment | Inline Dotted Divider | Target-Only Markdown | Untranslated Document (0% done) |
| `T3-09` | TXT (UTF-16) | Numbered Batch JSON | Side-by-Side Equal Height | Bilingual DOCX | Soft Wrap CJK + Latin Mixed |
| `T3-10` | EPUB | Numbered Batch JSON | Target Only | Bilingual Markdown | Missing manifest image + Footnote |

---

## 5. Tier 4: Real-World Application Workload Scenarios

### Workload 1: End-to-End E-Book Reading & Translation Lifecycle
- **Scenario**: User imports a 3-chapter novel (EPUB / TXT).
- **Execution Flow**:
  1. Import document: SHA-256 fingerprint generated; chapters and blocks indexed.
  2. Viewport translation: User opens Chapter 1; visible blocks (P0) immediately translated via batch JSON; forward prefetch (P1: +10 blocks) queued.
  3. Cache persistence: All translated blocks stored in SQLite cache.
  4. Chapter navigation: User jumps to Chapter 3; pending Chapter 1 prefetch tasks cancelled; Chapter 3 visible blocks prioritized.
  5. Reopen Chapter 1: Blocks instantly loaded from SQLite cache without network calls.
  6. Dual Export: User exports both Bilingual EPUB and Target-Only EPUB; outputs structurally verified.

### Workload 2: Network Interruption, Strict Error Isolation & UI Retry
- **Scenario**: User translates a chapter during an unstable network connection.
- **Execution Flow**:
  1. Block 1 translates successfully.
  2. Block 2 encounters HTTP 500 error from upstream LLM.
  3. System isolates error: UI BlockView receives state `Failed { error, retryable: true }`; rendered text is `"翻译失败，请稍后重试"`; [Retry] button displayed.
  4. Raw error details (`HTTP 500: Internal Server Error`) routed to sanitized logger; API key masked.
  5. User clicks [Retry] button; block re-scheduled at Priority 0.
  6. Upstream succeeds; UI updates with translated text and scroll anchor preserved.

### Workload 3: Library File Migration & Reading Progress Preservation
- **Scenario**: User moves/renames document file on disk.
- **Execution Flow**:
  1. User reads to Chapter 2, Block 15 (offset 124.5px); reading progress saved in SQLite under document SHA-256 ID.
  2. User renames `book.txt` to `renamed_book_v2.txt` and moves to another directory.
  3. User opens file from new path: SHA-256 matches existing document ID.
  4. Reading progress restored to Chapter 2, Block 15; cached translations loaded instantly.

---

## 6. Execution Commands & Verification Harness

All E2E test suites are located under `tests/` and can be executed via standard Cargo commands:

```bash
# 1. Check all test suites compile without errors
cargo test --no-run

# 2. Run all E2E integration test suites
cargo test --test '*'

# 3. Run specific feature test suites
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

## 7. Deliverables & Sign-Off Checklist
- [x] 4-Tier Test Architecture defined.
- [x] Opaque-box verification principles established.
- [x] Expected output derivation rules codified.
- [x] Comprehensive feature test catalog (>=5 Tier 1 & >=5 Tier 2 tests per feature).
- [x] Tier 3 Pairwise Interaction Matrix documented.
- [x] Tier 4 Real-World Application Workload Scenarios specified.
- [x] Test suite files scaffolded under `tests/`.
