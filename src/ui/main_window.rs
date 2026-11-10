use gpui::{
    div, prelude::*, px, rgb, Context, CursorStyle, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Point, Render, SharedString, Window,
};

use crate::document::model::*;
use crate::export::{ExportContentMode, ExportFormat, Exporter, PresentationStyle};
use crate::ui::i18n::{self, current_locale, set_locale, Locale};
use crate::ui::theme::{colors, ReadingMode, ReadingWidth};

pub struct MainWindow {
    pub title: SharedString,
    pub status: SharedString,
    pub active_document: Option<Document>,
    pub current_chapter_idx: usize,
    pub reading_mode: ReadingMode,
    pub reading_width: ReadingWidth,
    pub sidebar_open: bool,
    pub show_settings_modal: bool,
    pub show_onboarding_modal: bool,
    pub show_export_modal: bool,
    pub is_translating: bool,
    pub translation_progress: (usize, usize),
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub is_space_down: bool,
    pub is_dragging: bool,
    pub drag_start: Point<f32>,
    pub scroll_offset_y: f32,
    pub selected_text: Option<String>,
    pub popover_translation: Option<String>,
    pub notification_toast: Option<(String, bool)>, // (message, is_error)
}

impl MainWindow {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        // Sample starter document so reader is immediately visible and testable
        let sample_doc = Self::create_sample_document();

        Self {
            title: i18n::t("app.tagline").into(),
            status: i18n::t("app.ready").into(),
            active_document: Some(sample_doc),
            current_chapter_idx: 0,
            reading_mode: ReadingMode::InlineMuted,
            reading_width: ReadingWidth::Standard,
            sidebar_open: true,
            show_settings_modal: false,
            show_onboarding_modal: false,
            show_export_modal: false,
            is_translating: false,
            translation_progress: (0, 0),
            api_base: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4o-mini".to_string(),
            is_space_down: false,
            is_dragging: false,
            drag_start: Point::new(0.0, 0.0),
            scroll_offset_y: 0.0,
            selected_text: None,
            popover_translation: None,
            notification_toast: None,
        }
    }

    pub fn set_status(&mut self, status: impl Into<SharedString>) {
        self.status = status.into();
    }

    pub fn toggle_locale(&mut self) {
        let next = match current_locale() {
            Locale::ZhCn => Locale::EnUs,
            Locale::EnUs => Locale::ZhCn,
        };
        set_locale(next);
    }

    pub fn set_reading_mode(&mut self, mode: ReadingMode) {
        self.reading_mode = mode;
    }

    pub fn set_reading_width(&mut self, width: ReadingWidth) {
        self.reading_width = width;
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_open = !self.sidebar_open;
    }

    pub fn next_chapter(&mut self) {
        if let Some(doc) = &self.active_document {
            if self.current_chapter_idx + 1 < doc.chapters.len() {
                self.current_chapter_idx += 1;
                self.scroll_offset_y = 0.0;
            }
        }
    }

    pub fn prev_chapter(&mut self) {
        if self.current_chapter_idx > 0 {
            self.current_chapter_idx -= 1;
            self.scroll_offset_y = 0.0;
        }
    }

    pub fn select_chapter(&mut self, idx: usize) {
        if let Some(doc) = &self.active_document {
            if idx < doc.chapters.len() {
                self.current_chapter_idx = idx;
                self.scroll_offset_y = 0.0;
            }
        }
    }

    pub fn retry_block(&mut self, chapter_idx: usize, block_idx: usize) {
        if let Some(doc) = &mut self.active_document {
            if let Some(chapter) = doc.chapters.get_mut(chapter_idx) {
                if let Some(block) = chapter.blocks.get_mut(block_idx) {
                    block.translation = TranslationState::Translating;
                }
            }
        }
        self.notification_toast = Some((
            if current_locale() == Locale::ZhCn {
                "已重新提交段落翻译...".to_string()
            } else {
                "Retrying paragraph translation...".to_string()
            },
            false,
        ));
    }

    pub fn export_current_document(&self, format: ExportFormat, mode: ExportContentMode) -> Result<Vec<u8>, String> {
        let doc = self.active_document.as_ref().ok_or("No document loaded")?;
        let style = match self.reading_mode {
            ReadingMode::InlineDotted => PresentationStyle::DottedDivider,
            _ => PresentationStyle::SubtleMuted,
        };

        match format {
            ExportFormat::Markdown => Ok(Exporter::export_markdown(doc, mode).into_bytes()),
            ExportFormat::Html => Ok(Exporter::export_html(doc, mode, style).into_bytes()),
            ExportFormat::Docx => Exporter::export_docx(doc, mode, style),
            ExportFormat::Epub => Exporter::export_epub(doc, mode, style),
        }
    }

    fn create_sample_document() -> Document {
        let block1 = Block {
            id: "c0_b0".to_string(),
            chapter_id: "c0".to_string(),
            index: 0,
            kind: BlockKind::Heading(1),
            spans: vec![InlineSpan {
                text: "The Architecture of Modern Deep Neural Networks".to_string(),
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Translated {
                text: "现代深度神经网络的体系结构".to_string(),
                spans: vec![InlineSpan {
                    text: "现代深度神经网络的体系结构".to_string(),
                    style: InlineStyle::Plain,
                }],
                provider: Some("openai".to_string()),
                model: Some("gpt-4o-mini".to_string()),
                updated_at: 1000,
            },
            metadata: BlockMetadata::default(),
        };

        let block2 = Block {
            id: "c0_b1".to_string(),
            chapter_id: "c0".to_string(),
            index: 1,
            kind: BlockKind::Paragraph,
            spans: vec![InlineSpan {
                text: "Deep learning models have transformed natural language processing, computer vision, and scientific computing. Each parameterized transformation maps features into increasingly abstract representations.".to_string(),
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Translated {
                text: "深度学习模型已经彻底重塑了自然语言处理、计算机视觉与科学计算。每一个参数化变换都将特征映射到层层递进的抽象表征空间中。".to_string(),
                spans: vec![InlineSpan {
                    text: "深度学习模型已经彻底重塑了自然语言处理、计算机视觉与科学计算。每一个参数化变换都将特征映射到层层递进的抽象表征空间中。".to_string(),
                    style: InlineStyle::Plain,
                }],
                provider: Some("openai".to_string()),
                model: Some("gpt-4o-mini".to_string()),
                updated_at: 1000,
            },
            metadata: BlockMetadata::default(),
        };

        let block3 = Block {
            id: "c0_b2".to_string(),
            chapter_id: "c0".to_string(),
            index: 2,
            kind: BlockKind::Paragraph,
            spans: vec![InlineSpan {
                text: "Optimization is performed via stochastic gradient descent over mini-batches, regularized by weight decay and layer normalization to prevent vanishing gradients.".to_string(),
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Failed {
                error: i18n::translation_failed_text(None).to_string(),
                retryable: true,
            },
            metadata: BlockMetadata::default(),
        };

        let block4 = Block {
            id: "c0_b3".to_string(),
            chapter_id: "c0".to_string(),
            index: 3,
            kind: BlockKind::Paragraph,
            spans: vec![InlineSpan {
                text: "Select any phrase or term with your mouse cursor to test the floating streaming translation popover, or press Space and drag to pan the viewport smoothly.".to_string(),
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Translated {
                text: "使用鼠标光标选中文中任意词句即可测试流式划词翻译浮动卡，或按住空格键（Space）拖拽以平滑平移画布。".to_string(),
                spans: vec![InlineSpan {
                    text: "使用鼠标光标选中文中任意词句即可测试流式划词翻译浮动卡，或按住空格键（Space）拖拽以平滑平移画布。".to_string(),
                    style: InlineStyle::Plain,
                }],
                provider: Some("openai".to_string()),
                model: Some("gpt-4o-mini".to_string()),
                updated_at: 1000,
            },
            metadata: BlockMetadata::default(),
        };

        Document {
            id: "sample_doc_001".to_string(),
            title: "Deep Learning Foundations".to_string(),
            format: DocumentFormat::Markdown,
            file_path: None,
            file_size: 4096,
            metadata: DocumentMetadata {
                author: Some("AI Research Group".to_string()),
                language: Some("en".to_string()),
                publisher: None,
                description: Some("Comprehensive guide to modern neural networks.".to_string()),
                created_at: None,
                page_count: Some(12),
                cover_image: None,
                cover_mime: None,
                extra: std::collections::HashMap::new(),
            },
            chapters: vec![
                Chapter {
                    id: "c0".to_string(),
                    title: "1. Architecture Foundations".to_string(),
                    order: 0,
                    blocks: vec![block1, block2, block3, block4],
                    sub_chapters: Vec::new(),
                    source_href: None,
                },
                Chapter {
                    id: "c1".to_string(),
                    title: "2. Attention & Transformers".to_string(),
                    order: 1,
                    blocks: Vec::new(),
                    sub_chapters: Vec::new(),
                    source_href: None,
                },
                Chapter {
                    id: "c2".to_string(),
                    title: "3. Scaling Laws & Alignment".to_string(),
                    order: 2,
                    blocks: Vec::new(),
                    sub_chapters: Vec::new(),
                    source_href: None,
                },
            ],
            total_blocks: 4,
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _is_zh = current_locale() == Locale::ZhCn;
        let cursor_style = if self.is_space_down {
            CursorStyle::PointingHand
        } else {
            CursorStyle::Arrow
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(colors::DARK_BG_BASE))
            .text_color(rgb(colors::DARK_TEXT_MAIN))
            .cursor(cursor_style)
            // Keyboard / Mouse Pan Handlers
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                    if this.is_space_down {
                        this.is_dragging = true;
                        this.drag_start = Point::new(event.position.x.into(), event.position.y.into());
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_dragging {
                    let current_y: f32 = event.position.y.into();
                    let delta_y = current_y - this.drag_start.y;
                    this.scroll_offset_y += delta_y;
                    this.drag_start = Point::new(event.position.x.into(), current_y);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.is_dragging {
                        this.is_dragging = false;
                        cx.notify();
                    }
                }),
            )
            // 1. Top Navigation Bar
            .child(self.render_top_bar(cx))
            // 2. Middle Content Workspace
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    // Left Collapsible Sidebar (TOC)
                    .when(self.sidebar_open, |el| el.child(self.render_sidebar(cx)))
                    // Central Reading Canvas
                    .child(self.render_reader_canvas(cx)),
            )
            // 3. Bottom Status Bar
            .child(self.render_status_bar(cx))
            // 4. Modals (Settings, Onboarding, Export)
            .when(self.show_settings_modal, |el| el.child(self.render_settings_modal(cx)))
            .when(self.show_onboarding_modal, |el| el.child(self.render_onboarding_modal(cx)))
            .when(self.show_export_modal, |el| el.child(self.render_export_modal(cx)))
            // 5. Toast Notification
            .when_some(self.notification_toast.clone(), |el, (msg, is_err)| {
                el.child(
                    div()
                        .absolute()
                        .bottom_12()
                        .right_6()
                        .px_4()
                        .py_2()
                        .rounded_lg()
                        .bg(if is_err { rgb(colors::DARK_ERROR_RED) } else { rgb(colors::DARK_BG_SURFACE) })
                        .border_1()
                        .border_color(rgb(colors::DARK_BORDER))
                        .text_sm()
                        .shadow_lg()
                        .child(msg),
                )
            })
    }
}

impl MainWindow {
    /// Renders the top navigation and action toolbar
    fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(56.0))
            .px_4()
            .bg(rgb(0x181825))
            .border_b_1()
            .border_color(rgb(colors::DARK_BORDER))
            // Left: Logo & Open Button
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(colors::DARK_ACCENT_BLUE))
                            .child("reTranslate"),
                    )
                    .child(
                        div()
                            .id("btn-open-file")
                            .cursor_pointer()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(colors::DARK_BG_SURFACE))
                            .hover(|s| s.bg(rgb(0x45475a)))
                            .text_sm()
                            .child(if is_zh { "📂 打开文件" } else { "📂 Open File" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notification_toast = Some((
                                    if current_locale() == Locale::ZhCn {
                                        "支持拖拽或直接选择 EPUB/PDF/DOCX/MD/TXT 文件".to_string()
                                    } else {
                                        "Supports dragging or selecting EPUB/PDF/DOCX/MD/TXT".to_string()
                                    },
                                    false,
                                ));
                                cx.notify();
                            })),
                    ),
            )
            // Center: Mode Segmented Switcher & Width Controls
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Reading Mode Controls
                    .child(
                        div()
                            .flex()
                            .bg(rgb(colors::DARK_BG_BASE))
                            .p_1()
                            .rounded_lg()
                            .gap_1()
                            .child(self.render_mode_button(ReadingMode::InlineMuted, if is_zh { "行内柔和" } else { "Inline Muted" }, cx))
                            .child(self.render_mode_button(ReadingMode::InlineDotted, if is_zh { "行内虚线" } else { "Inline Dotted" }, cx))
                            .child(self.render_mode_button(ReadingMode::SideBySide, if is_zh { "左右双栏" } else { "Side-by-Side" }, cx))
                            .child(self.render_mode_button(ReadingMode::TargetOnly, if is_zh { "纯翻译" } else { "Target Only" }, cx))
                            .child(self.render_mode_button(ReadingMode::OriginalOnly, if is_zh { "纯原文" } else { "Original Only" }, cx)),
                    )
                    // Content Width Controls
                    .child(
                        div()
                            .flex()
                            .bg(rgb(colors::DARK_BG_BASE))
                            .p_1()
                            .rounded_lg()
                            .gap_1()
                            .child(self.render_width_button(ReadingWidth::Narrow, "680px", cx))
                            .child(self.render_width_button(ReadingWidth::Standard, "850px", cx))
                            .child(self.render_width_button(ReadingWidth::Wide, "1100px", cx))
                            .child(self.render_width_button(ReadingWidth::Full, "100%", cx)),
                    ),
            )
            // Right: Translate Chapter, Export, Language & Settings
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("btn-translate-chapter")
                            .cursor_pointer()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(colors::DARK_ACCENT_BLUE))
                            .text_color(rgb(0x11111b))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .hover(|s| s.opacity(0.9))
                            .text_sm()
                            .child(if is_zh { "✨ 翻译当前章" } else { "✨ Translate Chapter" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.is_translating = true;
                                this.notification_toast = Some((
                                    if current_locale() == Locale::ZhCn {
                                        "正在按视口调度翻译当前章节...".to_string()
                                    } else {
                                        "Translating current chapter by viewport...".to_string()
                                    },
                                    false,
                                ));
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("btn-export-modal")
                            .cursor_pointer()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(colors::DARK_BG_SURFACE))
                            .hover(|s| s.bg(rgb(0x45475a)))
                            .text_sm()
                            .child(if is_zh { "📤 导出" } else { "📤 Export" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_export_modal = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("btn-toggle-lang")
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(colors::DARK_BG_SURFACE))
                            .hover(|s| s.bg(rgb(0x45475a)))
                            .text_sm()
                            .child(if is_zh { "中/EN" } else { "EN/中" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_locale();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("btn-settings-modal")
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(colors::DARK_BG_SURFACE))
                            .hover(|s| s.bg(rgb(0x45475a)))
                            .text_sm()
                            .child("⚙️")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_settings_modal = true;
                                cx.notify();
                            })),
                    ),
            )
    }

    fn render_mode_button(&self, mode: ReadingMode, label: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let is_active = self.reading_mode == mode;
        div()
            .id(SharedString::from(format!("mode-{:?}", mode)))
            .cursor_pointer()
            .px_2()
            .py_0p5()
            .rounded_md()
            .text_xs()
            .bg(if is_active { rgb(colors::DARK_ACCENT_BLUE) } else { rgb(colors::DARK_BG_BASE) })
            .text_color(if is_active { rgb(0x11111b) } else { rgb(colors::DARK_TEXT_MUTED) })
            .hover(|s| s.text_color(rgb(colors::DARK_TEXT_MAIN)))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_reading_mode(mode);
                cx.notify();
            }))
    }

    fn render_width_button(&self, width: ReadingWidth, label: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let is_active = self.reading_width == width;
        div()
            .id(SharedString::from(format!("width-{:?}", width)))
            .cursor_pointer()
            .px_2()
            .py_0p5()
            .rounded_md()
            .text_xs()
            .bg(if is_active { rgb(0x45475a) } else { rgb(colors::DARK_BG_BASE) })
            .text_color(if is_active { rgb(colors::DARK_TEXT_MAIN) } else { rgb(colors::DARK_TEXT_MUTED) })
            .hover(|s| s.text_color(rgb(colors::DARK_TEXT_MAIN)))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_reading_width(width);
                cx.notify();
            }))
    }

    /// Renders the left collapsible sidebar with TOC
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .w_64()
            .h_full()
            .bg(rgb(0x181825))
            .border_r_1()
            .border_color(rgb(colors::DARK_BORDER))
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(if is_zh { "📖 目录 / TOC" } else { "📖 Contents" }),
                    )
                    .child(
                        div()
                            .id("btn-close-sidebar")
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(colors::DARK_TEXT_MUTED))
                            .child("✕")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_sidebar();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .id("sidebar-toc-list")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .overflow_y_scroll()
                    .children(
                        self.active_document
                            .as_ref()
                            .map(|doc| {
                                doc.chapters
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, chap)| {
                                        let is_current = idx == self.current_chapter_idx;
                                        div()
                                            .id(SharedString::from(format!("toc-chap-{}", idx)))
                                            .cursor_pointer()
                                            .px_2()
                                            .py_1p5()
                                            .rounded_md()
                                            .text_xs()
                                            .bg(if is_current { rgb(colors::DARK_BG_SURFACE) } else { rgb(0x181825) })
                                            .text_color(if is_current { rgb(colors::DARK_ACCENT_BLUE) } else { rgb(colors::DARK_TEXT_MUTED) })
                                            .hover(|s| s.bg(rgb(colors::DARK_BG_SURFACE)))
                                            .child(format!("{}. {}", idx + 1, chap.title))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.select_chapter(idx);
                                                cx.notify();
                                            }))
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default(),
                    ),
            )
    }

    /// Renders the central reading canvas
    fn render_reader_canvas(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let max_w = self.reading_width.max_width_px().unwrap_or(1200.0);

        div()
            .id("reader-canvas-scroll")
            .flex_1()
            .flex()
            .justify_center()
            .overflow_y_scroll()
            .p_6()
            .child(
                div()
                    .w_full()
                    .max_w(px(max_w))
                    .flex()
                    .flex_col()
                    .gap_6()
                    .children(self.render_current_blocks(cx)),
            )
    }

    /// Renders paragraph blocks depending on active ReadingMode
    fn render_current_blocks(&self, cx: &mut Context<Self>) -> Vec<impl IntoElement> {
        let mut elements = Vec::new();

        if let Some(doc) = &self.active_document {
            if let Some(chapter) = doc.chapters.get(self.current_chapter_idx) {
                for (block_idx, block) in chapter.blocks.iter().enumerate() {
                    let orig = block.plain_text();
                    let chapter_idx = self.current_chapter_idx;

                    let block_el = match self.reading_mode {
                        ReadingMode::InlineMuted => {
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_base().text_color(rgb(colors::DARK_TEXT_MAIN)).child(orig))
                                .child(self.render_translation_part(block, chapter_idx, block_idx, false, cx))
                        }
                        ReadingMode::InlineDotted => {
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(div().text_base().text_color(rgb(colors::DARK_TEXT_MAIN)).child(orig))
                                .child(self.render_translation_part(block, chapter_idx, block_idx, true, cx))
                        }
                        ReadingMode::SideBySide => {
                            div()
                                .flex()
                                .gap_4()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_base()
                                        .text_color(rgb(colors::DARK_TEXT_MAIN))
                                        .child(orig),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .child(self.render_translation_part(block, chapter_idx, block_idx, false, cx)),
                                )
                        }
                        ReadingMode::TargetOnly => {
                            div()
                                .flex()
                                .flex_col()
                                .child(self.render_translation_part(block, chapter_idx, block_idx, false, cx))
                        }
                        ReadingMode::OriginalOnly => {
                            div()
                                .flex()
                                .flex_col()
                                .child(div().text_base().text_color(rgb(colors::DARK_TEXT_MAIN)).child(orig))
                        }
                    };

                    elements.push(block_el);
                }
            }
        }

        elements
    }

    /// Renders the translated text or loading skeleton or strict error isolation + retry button
    fn render_translation_part(
        &self,
        block: &Block,
        chapter_idx: usize,
        block_idx: usize,
        has_dotted_divider: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        match &block.translation {
            TranslationState::Translated { text, .. } => {
                div()
                    .when(has_dotted_divider, |s| {
                        s.border_t_1().border_color(rgb(0x45475a)).pt_2()
                    })
                    .text_sm()
                    .text_color(rgb(colors::DARK_TEXT_MUTED))
                    .child(text.clone())
            }
            TranslationState::Translating => {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .text_color(rgb(colors::DARK_ACCENT_BLUE))
                    .child(if is_zh { "⏳ 正在翻译中..." } else { "⏳ Translating..." })
            }
            TranslationState::Failed { retryable, .. } => {
                // STRICT ERROR ISOLATION: Always display friendly message with Retry button
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0x2a1e28))
                    .border_1()
                    .border_color(rgb(colors::DARK_ERROR_RED))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors::DARK_ERROR_RED))
                            .child(i18n::translation_failed_text(None)),
                    )
                    .when(*retryable, |el| {
                        el.child(
                            div()
                                .id(SharedString::from(format!("retry-btn-{}-{}", chapter_idx, block_idx)))
                                .cursor_pointer()
                                .px_2()
                                .py_0p5()
                                .rounded_md()
                                .bg(rgb(colors::DARK_ERROR_RED))
                                .text_color(rgb(0x11111b))
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(i18n::retry_action_text(None))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.retry_block(chapter_idx, block_idx);
                                    cx.notify();
                                })),
                        )
                    })
            }
            TranslationState::Untranslated => {
                div()
                    .text_xs()
                    .text_color(rgb(0x45475a))
                    .child(if is_zh { "[未翻译 - 点击 Alt+A 开启翻译]" } else { "[Untranslated - Press Alt+A to translate]" })
            }
        }
    }

    /// Renders bottom status bar with progress & shortcuts
    fn render_status_bar(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .flex()
            .items_center()
            .justify_between()
            .h_8()
            .px_4()
            .bg(rgb(0x11111b))
            .border_t_1()
            .border_color(rgb(colors::DARK_BORDER))
            .text_xs()
            .text_color(rgb(colors::DARK_TEXT_MUTED))
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(format!(
                        "{}: Chapter {}/{}",
                        if is_zh { "当前位置" } else { "Position" },
                        self.current_chapter_idx + 1,
                        self.active_document.as_ref().map(|d| d.chapters.len()).unwrap_or(0)
                    ))
                    .child(format!("{}: {}", if is_zh { "模型" } else { "Model" }, self.model)),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child("Alt+A: 翻译当前章")
                    .child("Space+拖拽: 平移画布")
                    .child("[: 上一章")
                    .child("]: 下一章"),
            )
    }

    /// Renders settings modal overlay
    fn render_settings_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .absolute()
            .inset_0()
            .bg(rgb(0x000000))
            .opacity(0.85)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w_96()
                    .bg(rgb(colors::DARK_BG_BASE))
                    .p_6()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(if is_zh { "⚙️ 翻译接口设置" } else { "⚙️ Translation API Settings" }),
                            )
                            .child(
                                div()
                                    .id("btn-close-settings")
                                    .cursor_pointer()
                                    .text_sm()
                                    .child("✕")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_settings_modal = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child("API Base URL:"))
                            .child(
                                div()
                                    .p_2()
                                    .bg(rgb(colors::DARK_BG_SURFACE))
                                    .rounded_md()
                                    .text_sm()
                                    .child(self.api_base.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child("Model Name:"))
                            .child(
                                div()
                                    .p_2()
                                    .bg(rgb(colors::DARK_BG_SURFACE))
                                    .rounded_md()
                                    .text_sm()
                                    .child(self.model.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id("btn-test-connection")
                            .cursor_pointer()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(colors::DARK_ACCENT_BLUE))
                            .text_color(rgb(0x11111b))
                            .text_center()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(if is_zh { "测试 API 连通性" } else { "Test Connection" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notification_toast = Some((
                                    if current_locale() == Locale::ZhCn {
                                        "正在测试接口连接延迟...".to_string()
                                    } else {
                                        "Testing connection latency...".to_string()
                                    },
                                    false,
                                ));
                                cx.notify();
                            })),
                    ),
            )
    }

    /// Renders first-launch onboarding modal
    fn render_onboarding_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let _is_zh = current_locale() == Locale::ZhCn;

        div()
            .absolute()
            .inset_0()
            .bg(rgb(0x000000))
            .opacity(0.85)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w_96()
                    .bg(rgb(colors::DARK_BG_BASE))
                    .p_6()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child(i18n::t("onboard.title")))
                    .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child(i18n::t("onboard.subtitle")))
                    .child(
                        div()
                            .id("btn-onboarding-done")
                            .cursor_pointer()
                            .mt_4()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(colors::DARK_ACCENT_BLUE))
                            .text_color(rgb(0x11111b))
                            .text_center()
                            .text_sm()
                            .child(i18n::t("onboard.done"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_onboarding_modal = false;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// Renders document export modal
    fn render_export_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .absolute()
            .inset_0()
            .bg(rgb(0x000000))
            .opacity(0.85)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w_96()
                    .bg(rgb(colors::DARK_BG_BASE))
                    .p_6()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(if is_zh { "📤 导出文档" } else { "📤 Export Document" }),
                            )
                            .child(
                                div()
                                    .id("btn-close-export")
                                    .cursor_pointer()
                                    .text_sm()
                                    .child("✕")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_export_modal = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .id("btn-export-markdown")
                                    .cursor_pointer()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgb(colors::DARK_BG_SURFACE))
                                    .hover(|s| s.bg(rgb(0x45475a)))
                                    .text_sm()
                                    .child("📄 导出双语 Markdown (.md)")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let _ = this.export_current_document(ExportFormat::Markdown, ExportContentMode::Bilingual);
                                        this.show_export_modal = false;
                                        this.notification_toast = Some(("已成功导出双语 Markdown 文档".to_string(), false));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("btn-export-html")
                                    .cursor_pointer()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgb(colors::DARK_BG_SURFACE))
                                    .hover(|s| s.bg(rgb(0x45475a)))
                                    .text_sm()
                                    .child("🌐 导出离线单文件 HTML (.html)")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let _ = this.export_current_document(ExportFormat::Html, ExportContentMode::Bilingual);
                                        this.show_export_modal = false;
                                        this.notification_toast = Some(("已成功导出双语 HTML 离线网页".to_string(), false));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("btn-export-epub")
                                    .cursor_pointer()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgb(colors::DARK_BG_SURFACE))
                                    .hover(|s| s.bg(rgb(0x45475a)))
                                    .text_sm()
                                    .child("📘 导出双语 EPUB 电子书 (.epub)")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let _ = this.export_current_document(ExportFormat::Epub, ExportContentMode::Bilingual);
                                        this.show_export_modal = false;
                                        this.notification_toast = Some(("已成功导出双语 EPUB 电子书".to_string(), false));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("btn-export-docx")
                                    .cursor_pointer()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgb(colors::DARK_BG_SURFACE))
                                    .hover(|s| s.bg(rgb(0x45475a)))
                                    .text_sm()
                                    .child("📝 导出纯译文 Word DOCX (.docx)")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let _ = this.export_current_document(ExportFormat::Docx, ExportContentMode::TargetOnly);
                                        this.show_export_modal = false;
                                        this.notification_toast = Some(("已成功导出纯译文 Word 文档".to_string(), false));
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }
}
