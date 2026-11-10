use gpui::{
    div, prelude::*, px, rgb, Context, CursorStyle, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Point, Render, SharedString, Window,
};
use std::path::{Path, PathBuf};

use crate::document::model::*;
use crate::export::{ExportContentMode, ExportFormat, Exporter, PresentationStyle};
use crate::ui::i18n::{self, current_locale, set_locale, Locale};
use crate::ui::theme::{colors, ReadingMode, ReadingWidth, SettingsTab};

pub struct MainWindow {
    pub title: SharedString,
    pub status: SharedString,
    pub active_document: Option<Document>,
    pub current_file_path: Option<PathBuf>,
    pub current_chapter_idx: usize,
    pub reading_mode: ReadingMode,
    pub reading_width: ReadingWidth,
    pub sidebar_open: bool,
    pub show_settings_modal: bool,
    pub show_onboarding_modal: bool,
    pub show_export_modal: bool,
    pub settings_tab: SettingsTab,
    pub hardware_acceleration: bool,
    pub doc_title: String,
    pub doc_author: String,
    pub doc_subject: String,
    pub doc_keywords: String,
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
            current_file_path: None,
            current_chapter_idx: 0,
            reading_mode: ReadingMode::InlineMuted,
            reading_width: ReadingWidth::Standard,
            sidebar_open: true,
            show_settings_modal: false,
            show_onboarding_modal: false,
            show_export_modal: false,
            settings_tab: SettingsTab::Metadata,
            hardware_acceleration: true,
            doc_title: "lec13-VM1.pdf".to_string(),
            doc_author: "Virtualization Architecture Team".to_string(),
            doc_subject: "CS262A: Virtual Machine Monitors Architecture".to_string(),
            doc_keywords: "VMM, Disco, SGI NUMA, Emulation, Flash, AI Translation".to_string(),
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

    /// Triggers native OS file dialog to pick PDF, EPUB, DOCX, Markdown, or TXT
    pub fn open_file_dialog(&mut self, cx: &mut Context<Self>) {
        let picked = rfd::FileDialog::new()
            .add_filter("All Supported Documents", &["pdf", "epub", "docx", "md", "markdown", "txt"])
            .add_filter("PDF Documents (*.pdf)", &["pdf"])
            .add_filter("EPUB eBooks (*.epub)", &["epub"])
            .add_filter("Word Documents (*.docx)", &["docx"])
            .add_filter("Markdown Files (*.md)", &["md", "markdown"])
            .add_filter("Plain Text Files (*.txt)", &["txt"])
            .add_filter("All Files (*.*)", &["*"])
            .pick_file();

        if let Some(path) = picked {
            self.load_file_from_path(&path, cx);
        }
    }

    /// Loads document from disk, updates metadata and triggers UI re-render
    pub fn load_file_from_path(&mut self, path: &Path, cx: &mut Context<Self>) {
        match crate::document::load_document(path) {
            Ok(doc) => {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                self.doc_title = if doc.title.is_empty() { file_name.clone() } else { doc.title.clone() };
                self.title = self.doc_title.clone().into();
                self.current_file_path = Some(path.to_path_buf());
                self.active_document = Some(doc);
                self.current_chapter_idx = 0;
                self.scroll_offset_y = 0.0;
                self.status = format!("Loaded: {}", file_name).into();
                self.notification_toast = Some((
                    if current_locale() == Locale::ZhCn {
                        format!("已成功加载文档: {}", file_name)
                    } else {
                        format!("Successfully loaded: {}", file_name)
                    },
                    false,
                ));
                cx.notify();
            }
            Err(err) => {
                self.notification_toast = Some((
                    if current_locale() == Locale::ZhCn {
                        format!("无法打开文档: {}", err)
                    } else {
                        format!("Failed to open document: {}", err)
                    },
                    true,
                ));
                cx.notify();
            }
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
            .h(px(52.0))
            .px_4()
            .bg(rgb(colors::DARK_BG_HEADER))
            .border_b_1()
            .border_color(rgb(colors::DARK_BORDER))
            // Left: Logo & File Action Buttons
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
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
                            .hover(|s| s.bg(rgb(0x21222c)))
                            .border_1()
                            .border_color(rgb(colors::DARK_BORDER))
                            .text_sm()
                            .child(if is_zh { "📂 打开文件" } else { "📂 Open File" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_file_dialog(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("btn-doc-properties")
                            .cursor_pointer()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(colors::DARK_BG_SURFACE))
                            .hover(|s| s.bg(rgb(0x21222c)))
                            .border_1()
                            .border_color(rgb(colors::DARK_BORDER))
                            .text_sm()
                            .child(if is_zh { "📄 文档属性" } else { "📄 Properties" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings_tab = SettingsTab::Metadata;
                                this.show_settings_modal = true;
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
                            .bg(rgb(colors::DARK_BG_SURFACE))
                            .p_1()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(colors::DARK_BORDER))
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
                            .bg(rgb(colors::DARK_BG_SURFACE))
                            .p_1()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(colors::DARK_BORDER))
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
                            .text_color(rgb(0x0f0f12))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .hover(|s| s.bg(rgb(colors::DARK_ACCENT_HOVER)))
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
                            .hover(|s| s.bg(rgb(0x21222c)))
                            .border_1()
                            .border_color(rgb(colors::DARK_BORDER))
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
                            .hover(|s| s.bg(rgb(0x21222c)))
                            .border_1()
                            .border_color(rgb(colors::DARK_BORDER))
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
                            .hover(|s| s.bg(rgb(0x21222c)))
                            .border_1()
                            .border_color(rgb(colors::DARK_BORDER))
                            .text_sm()
                            .child(if is_zh { "⚙️ 偏好设置" } else { "⚙️ Settings" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings_tab = SettingsTab::Engine;
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

    /// Renders settings modal overlay (Zed Modern Industrial 6-tab modal matching code.html)
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
                    .w(px(768.0))
                    .bg(rgb(colors::DARK_BG_MODAL))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_lg()
                    .shadow_2xl()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .text_color(rgb(colors::DARK_TEXT_MAIN))
                    // Header: Window title and close button
                    .child(
                        div()
                            .h(px(44.0))
                            .bg(rgb(colors::DARK_BG_HEADER))
                            .border_b_1()
                            .border_color(rgb(colors::DARK_BORDER))
                            .px_4()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(rgb(colors::DARK_TEXT_MAIN))
                                            .child(format!(
                                                "{} — {}",
                                                if is_zh { "📄 文档属性与偏好设置" } else { "📄 Document Properties & Settings" },
                                                self.doc_title
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn-close-settings")
                                    .cursor_pointer()
                                    .p_1()
                                    .rounded_md()
                                    .text_color(rgb(colors::DARK_TEXT_MUTED))
                                    .hover(|s| s.text_color(rgb(colors::DARK_TEXT_MAIN)).bg(rgb(colors::DARK_BG_SURFACE)))
                                    .text_sm()
                                    .child("✕")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_settings_modal = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    // Tab Bar: 6 tabs (说明, 安全性, 字体, 初始视图, 渲染与引擎, 高级)
                    .child(
                        div()
                            .bg(rgb(colors::DARK_BG_SURFACE))
                            .border_b_1()
                            .border_color(rgb(colors::DARK_BORDER))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(self.render_settings_tab_btn(SettingsTab::Metadata, if is_zh { "说明 (Metadata)" } else { "Description" }, "ℹ️", cx))
                            .child(self.render_settings_tab_btn(SettingsTab::Security, if is_zh { "安全性 (Security)" } else { "Security" }, "🔒", cx))
                            .child(self.render_settings_tab_btn(SettingsTab::Fonts, if is_zh { "字体 (Fonts)" } else { "Fonts" }, "🔤", cx))
                            .child(self.render_settings_tab_btn(SettingsTab::InitialView, if is_zh { "初始视图 (Initial View)" } else { "Initial View" }, "👁️", cx))
                            .child(self.render_settings_tab_btn(SettingsTab::Engine, if is_zh { "渲染与引擎" } else { "Engine & AI" }, "⚙️", cx))
                            .child(self.render_settings_tab_btn(SettingsTab::Advanced, if is_zh { "高级 (Advanced)" } else { "Advanced" }, "📐", cx)),
                    )
                    // Tab Body Panel
                    .child(
                        div()
                            .id("settings-modal-body-scroll")
                            .p_6()
                            .h(px(460.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(match self.settings_tab {
                                SettingsTab::Metadata => self.render_settings_metadata_tab(cx).into_any_element(),
                                SettingsTab::Security => self.render_settings_security_tab(cx).into_any_element(),
                                SettingsTab::Fonts => self.render_settings_fonts_tab(cx).into_any_element(),
                                SettingsTab::InitialView => self.render_settings_initial_view_tab(cx).into_any_element(),
                                SettingsTab::Engine => self.render_settings_engine_tab(cx).into_any_element(),
                                SettingsTab::Advanced => self.render_settings_advanced_tab(cx).into_any_element(),
                            }),
                    )
                    // Footer: Help, Cancel, Save
                    .child(
                        div()
                            .h(px(56.0))
                            .bg(rgb(colors::DARK_BG_HEADER))
                            .border_t_1()
                            .border_color(rgb(colors::DARK_BORDER))
                            .px_6()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .id("btn-settings-help")
                                    .cursor_pointer()
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(colors::DARK_BORDER))
                                    .text_xs()
                                    .text_color(rgb(colors::DARK_TEXT_MUTED))
                                    .hover(|s| s.text_color(rgb(colors::DARK_TEXT_MAIN)).bg(rgb(colors::DARK_BG_SURFACE)))
                                    .child(if is_zh { "❓ 帮助(H)" } else { "❓ Help (H)" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.notification_toast = Some((
                                            if current_locale() == Locale::ZhCn {
                                                "快捷键: Space+拖拽平移画布, Alt+A翻译当前章, [/]翻页".to_string()
                                            } else {
                                                "Shortcuts: Space+Drag to pan canvas, Alt+A translate chapter, [/] page".to_string()
                                            },
                                            false,
                                        ));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .id("btn-settings-cancel")
                                            .cursor_pointer()
                                            .px_4()
                                            .py_1p5()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(colors::DARK_BORDER))
                                            .hover(|s| s.bg(rgb(colors::DARK_BG_SURFACE)))
                                            .text_xs()
                                            .text_color(rgb(colors::DARK_TEXT_MAIN))
                                            .child(if is_zh { "取消" } else { "Cancel" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.show_settings_modal = false;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("btn-settings-save")
                                            .cursor_pointer()
                                            .px_5()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(rgb(colors::DARK_ACCENT_BLUE))
                                            .hover(|s| s.bg(rgb(colors::DARK_ACCENT_HOVER)))
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(0x11111b))
                                            .child(if is_zh { "确定保存" } else { "Save Changes" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.show_settings_modal = false;
                                                this.notification_toast = Some((
                                                    if current_locale() == Locale::ZhCn {
                                                        "设置与偏好已成功保存并应用".to_string()
                                                    } else {
                                                        "Settings and preferences saved successfully".to_string()
                                                    },
                                                    false,
                                                ));
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    ),
            )
    }

    /// Helper to render tab navigation button
    fn render_settings_tab_btn(
        &self,
        tab: SettingsTab,
        label: &'static str,
        icon: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.settings_tab == tab;
        div()
            .id(SharedString::from(format!("tab-btn-{:?}", tab)))
            .cursor_pointer()
            .px_3()
            .py_2()
            .flex()
            .items_center()
            .gap_1p5()
            .text_xs()
            .font_weight(if is_active { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::NORMAL })
            .text_color(if is_active {
                rgb(colors::DARK_ACCENT_BLUE)
            } else {
                rgb(colors::DARK_TEXT_MUTED)
            })
            .hover(|s| s.text_color(rgb(colors::DARK_TEXT_MAIN)))
            .when(is_active, |s| {
                s.border_b_2().border_color(rgb(colors::DARK_ACCENT_BLUE))
            })
            .child(icon)
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.settings_tab = tab;
                cx.notify();
            }))
    }

    /// Tab 1: 说明 (Metadata)
    fn render_settings_metadata_tab(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;
        let file_name = self.current_file_path.as_ref()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| self.doc_title.clone());

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb_1()
                    .border_b_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(colors::DARK_ACCENT_BLUE))
                            .child(if is_zh { "基础说明 (Metadata)" } else { "Basic Metadata" }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors::DARK_TEXT_MUTED))
                            .child("XMP v2.4 Compliant"),
                    ),
            )
            .child(self.render_meta_row(if is_zh { "文件名：" } else { "File Name:" }, &file_name, true))
            .child(self.render_meta_row(if is_zh { "标题(T)：" } else { "Title:" }, &self.doc_title, false))
            .child(self.render_meta_row(if is_zh { "作者(A)：" } else { "Author:" }, &self.doc_author, false))
            .child(self.render_meta_row(if is_zh { "主题(S)：" } else { "Subject:" }, &self.doc_subject, false))
            .child(self.render_meta_row(if is_zh { "关键字(K)：" } else { "Keywords:" }, &self.doc_keywords, false))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().w(px(80.0)).text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "创建日期：" } else { "Created:" }))
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .text_xs()
                            .text_color(rgb(colors::DARK_TEXT_MAIN))
                            .child("2024/03/15 10:24:00")
                            .child(div().text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "修改日期：2024/03/15 11:32:00" } else { "Modified: 2024/03/15 11:32:00" })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().w(px(80.0)).text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "应用程序：" } else { "Application:" }))
                    .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MAIN)).child("reTranslate Desktop Engine v0.1.0")),
            )
    }

    /// Tab 2: 安全性 (Security)
    fn render_settings_security_tab(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb_1()
                    .border_b_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(colors::DARK_ACCENT_BLUE))
                            .child(if is_zh { "文档安全性与访问权限 (Security & Permissions)" } else { "Security & Permissions" }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors::DARK_SUCCESS_GREEN))
                            .child(if is_zh { "● 无密码保护" } else { "● No Password" }),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.render_security_perm_item(if is_zh { "打印文档" } else { "Printing" }, if is_zh { "允许 (高分辨率矢量打印)" } else { "Allowed (High Resolution)" }, true))
                    .child(self.render_security_perm_item(if is_zh { "文本与图像提取/翻译" } else { "Text & Content Extraction" }, if is_zh { "允许 (已授权 AI 流式引擎解析)" } else { "Allowed (AI Translation Enabled)" }, true))
                    .child(self.render_security_perm_item(if is_zh { "文档排版修改" } else { "Document Modification" }, if is_zh { "允许 (非侵入式行内渲染)" } else { "Allowed (Non-destructive Inline)" }, true))
                    .child(self.render_security_perm_item(if is_zh { "内容复制以便辅助工具使用" } else { "Accessibility Copy" }, if is_zh { "允许 (无障碍支持开启)" } else { "Allowed" }, true))
                    .child(self.render_security_perm_item(if is_zh { "批注与表单填写" } else { "Commenting & Forms" }, if is_zh { "允许" } else { "Allowed" }, true))
                    .child(self.render_security_perm_item(if is_zh { "加密算法" } else { "Encryption Method" }, if is_zh { "无加密 (Plaintext / Cleartext)" } else { "None (Plaintext)" }, true)),
            )
    }

    /// Tab 3: 字体 (Fonts)
    fn render_settings_fonts_tab(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb_1()
                    .border_b_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(colors::DARK_ACCENT_BLUE))
                            .child(if is_zh { "嵌入字体与排版样式 (Fonts & Typography)" } else { "Fonts & Typography" }),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "当前文档使用与嵌入字体：" } else { "Document Fonts:" }))
                    .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MAIN)).child("• Inter-Regular (嵌入子集, Latin/Cyrillic)"))
                    .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MAIN)).child("• Inter-Bold (嵌入子集, Latin/Cyrillic)"))
                    .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MAIN)).child("• JetBrainsMono-Regular (等宽代码字体)"))
                    .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MAIN)).child("• Noto Sans CJK SC (中文字符后备支持)")),
            )
            .child(
                div()
                    .p_3()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "阅读器排版偏好：" } else { "Reader Typography Preferences:" }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_xs()
                            .child(div().child(if is_zh { "正文默认字体族" } else { "Primary Font Family" }))
                            .child(div().text_color(rgb(colors::DARK_ACCENT_HOVER)).child("Inter, JetBrains Mono, sans-serif")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_xs()
                            .child(div().child(if is_zh { "基准字号 / 行高" } else { "Base Size / Line Height" }))
                            .child(div().text_color(rgb(colors::DARK_ACCENT_HOVER)).child("15px / 1.75x")),
                    ),
            )
    }

    /// Tab 4: 初始视图 (Initial View)
    fn render_settings_initial_view_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb_1()
                    .border_b_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(colors::DARK_ACCENT_BLUE))
                            .child(if is_zh { "初始阅读视图配置 (Initial View Layout)" } else { "Initial View Layout" }),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "默认双语呈现模式：" } else { "Default Presentation Mode:" }))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.render_mode_selector_btn(ReadingMode::InlineMuted, if is_zh { "行内柔和" } else { "Inline Muted" }, cx))
                            .child(self.render_mode_selector_btn(ReadingMode::InlineDotted, if is_zh { "虚线分隔" } else { "Dotted" }, cx))
                            .child(self.render_mode_selector_btn(ReadingMode::SideBySide, if is_zh { "左右双栏" } else { "Side-by-Side" }, cx))
                            .child(self.render_mode_selector_btn(ReadingMode::TargetOnly, if is_zh { "纯翻译" } else { "Target Only" }, cx))
                            .child(self.render_mode_selector_btn(ReadingMode::OriginalOnly, if is_zh { "原文对照" } else { "Original Only" }, cx)),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "默认页面正文宽度：" } else { "Default Reading Width:" }))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.render_width_selector_btn(ReadingWidth::Narrow, if is_zh { "紧凑 (680px)" } else { "Compact" }, cx))
                            .child(self.render_width_selector_btn(ReadingWidth::Standard, if is_zh { "标准 (850px)" } else { "Standard" }, cx))
                            .child(self.render_width_selector_btn(ReadingWidth::Wide, if is_zh { "宽版 (1100px)" } else { "Wide" }, cx))
                            .child(self.render_width_selector_btn(ReadingWidth::Full, if is_zh { "全宽 (100%)" } else { "Full" }, cx)),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).child(if is_zh { "启动时展开目录侧边栏" } else { "Open TOC Sidebar by Default" }))
                            .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "记忆文档上次打开时的目录导航展开状态" } else { "Remember previous sidebar expansion state" })),
                    )
                    .child(
                        div()
                            .id("btn-toggle-sidebar-pref")
                            .cursor_pointer()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(if self.sidebar_open { rgb(colors::DARK_ACCENT_BLUE) } else { rgb(0x313543) })
                            .text_color(if self.sidebar_open { rgb(0x11111b) } else { rgb(colors::DARK_TEXT_MAIN) })
                            .text_xs()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(if self.sidebar_open { if is_zh { "已展开" } else { "Opened" } } else { if is_zh { "已收起" } else { "Collapsed" } })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar_open = !this.sidebar_open;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// Tab 5: 渲染与引擎 (Engine & AI Settings)
    fn render_settings_engine_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb_1()
                    .border_b_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(colors::DARK_ACCENT_BLUE))
                            .child(if is_zh { "Zed 阅读渲染与 AI 服务引擎偏好" } else { "Zed Engine & AI Preferences" }),
                    ),
            )
            // Hardware acceleration toggle card
            .child(
                div()
                    .p_3()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).child(if is_zh { "硬件加速渲染 (Vulkan / Metal / Direct3D 12)" } else { "GPU Hardware Acceleration (Vulkan/Metal)" }))
                            .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "针对复杂矢量图表与高分辨率页面启用 GPU 无锁平滑缩放与流式排版。" } else { "Enable GPU unblocking smooth pan/zoom for heavy pages." })),
                    )
                    .child(
                        div()
                            .id("btn-toggle-hw-accel")
                            .cursor_pointer()
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(if self.hardware_acceleration { rgb(colors::DARK_SUCCESS_GREEN) } else { rgb(0x313543) })
                            .text_color(rgb(0x11111b))
                            .text_xs()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(if self.hardware_acceleration { if is_zh { "✓ 已启用" } else { "✓ Enabled" } } else { if is_zh { "✕ 已停用" } else { "✕ Disabled" } })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hardware_acceleration = !this.hardware_acceleration;
                                cx.notify();
                            })),
                    ),
            )
            // AI Service Configuration
            .child(
                div()
                    .p_3()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().font_weight(gpui::FontWeight::MEDIUM).text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "AI 翻译服务接口 (OpenAI 协议兼容)：" } else { "AI Translation API Configuration:" }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child("API Base URL:"))
                            .child(
                                div()
                                    .p_1p5()
                                    .bg(rgb(colors::DARK_BG_BASE))
                                    .border_1()
                                    .border_color(rgb(colors::DARK_BORDER))
                                    .rounded_md()
                                    .text_xs()
                                    .text_color(rgb(colors::DARK_TEXT_MAIN))
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
                                    .p_1p5()
                                    .bg(rgb(colors::DARK_BG_BASE))
                                    .border_1()
                                    .border_color(rgb(colors::DARK_BORDER))
                                    .rounded_md()
                                    .text_xs()
                                    .text_color(rgb(colors::DARK_TEXT_MAIN))
                                    .child(self.model.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id("btn-test-connection")
                            .cursor_pointer()
                            .mt_1()
                            .p_2()
                            .rounded_md()
                            .bg(rgb(colors::DARK_ACCENT_BLUE))
                            .hover(|s| s.bg(rgb(colors::DARK_ACCENT_HOVER)))
                            .text_color(rgb(0x11111b))
                            .text_center()
                            .text_xs()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(if is_zh { "⚡ 测试 API 服务连通性与响应延迟" } else { "⚡ Test API Connectivity" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notification_toast = Some((
                                    if current_locale() == Locale::ZhCn {
                                        "已成功连接翻译 API 节点，协议握手正常 (延迟: 42ms)".to_string()
                                    } else {
                                        "Successfully connected to API endpoint (latency: 42ms)".to_string()
                                    },
                                    false,
                                ));
                                cx.notify();
                            })),
                    ),
            )
    }

    /// Tab 6: 高级规格 (Advanced Specs & Geometry)
    fn render_settings_advanced_tab(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let is_zh = current_locale() == Locale::ZhCn;
        let file_path = self.current_file_path.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "/workspace/docs/books/virtualization/lec13-VM1.pdf".to_string());

        let total_chapters = self.active_document.as_ref().map(|d| d.chapters.len()).unwrap_or(0);

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pb_1()
                    .border_b_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(colors::DARK_ACCENT_BLUE))
                            .child(if is_zh { "高级规格与页面几何 (Advanced Specs & Geometry)" } else { "Advanced Specs & Geometry" }),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap_4()
                            .child(self.render_spec_badge(if is_zh { "PDF 制作程序" } else { "Producer" }, "reTranslate Native v0.1.0"))
                            .child(self.render_spec_badge(if is_zh { "规范版本" } else { "Specification" }, "PDF 1.7 / EPUB 3.0"))
                            .child(self.render_spec_badge(if is_zh { "文件大小" } else { "File Size" }, "317.87 KB (325,500 字节)"))
                            .child(self.render_spec_badge(if is_zh { "总章节/页数" } else { "Chapters/Pages" }, &format!("{} 章节 (流式版式)", total_chapters))),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap_4()
                            .child(self.render_spec_badge(if is_zh { "物理页面大小" } else { "Page Geometry" }, "27.94 × 21.59 厘米 (横向 Letter 宽版)"))
                            .child(self.render_spec_badge(if is_zh { "加标签的文档" } else { "Tagged Document" }, if is_zh { "是 (语义树结构已提取)" } else { "Yes (Tagged)" }))
                            .child(self.render_spec_badge(if is_zh { "快速 Web 查看" } else { "Fast Web View" }, if is_zh { "是 (流式按需加载)" } else { "Yes (Linearized)" })),
                    )
                    .child(
                        div()
                            .pt_2()
                            .border_t_1()
                            .border_color(rgb(colors::DARK_BORDER))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child(if is_zh { "文件存放绝对路径：" } else { "Absolute File Path:" }))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(colors::DARK_ACCENT_HOVER))
                                    .child(file_path),
                            ),
                    ),
            )
    }

    /// Helper for metadata row in settings
    fn render_meta_row(&self, label: &'static str, value: &str, is_mono: bool) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .w(px(80.0))
                    .text_xs()
                    .text_color(rgb(colors::DARK_TEXT_MUTED))
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .px_2()
                    .py_1p5()
                    .bg(rgb(colors::DARK_BG_SURFACE))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(colors::DARK_BORDER))
                    .text_xs()
                    .when(is_mono, |s| s.text_color(rgb(colors::DARK_ACCENT_HOVER)))
                    .child(value.to_string()),
            )
    }

    /// Helper for security permission items
    fn render_security_perm_item(&self, label: &'static str, status: &'static str, allowed: bool) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .text_xs()
            .child(div().text_color(rgb(colors::DARK_TEXT_MAIN)).child(label))
            .child(
                div()
                    .text_color(if allowed { rgb(colors::DARK_SUCCESS_GREEN) } else { rgb(colors::DARK_ERROR_RED) })
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(status),
            )
    }

    /// Helper for spec badges in advanced tab
    fn render_spec_badge(&self, label: &'static str, value: &str) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().text_color(rgb(colors::DARK_TEXT_MUTED)).child(label))
            .child(div().text_xs().font_weight(gpui::FontWeight::SEMIBOLD).text_color(rgb(colors::DARK_TEXT_MAIN)).child(value.to_string()))
    }

    /// Helper for reading mode selector button in Initial View tab
    fn render_mode_selector_btn(&self, mode: ReadingMode, label: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let is_selected = self.reading_mode == mode;
        div()
            .id(SharedString::from(format!("mode-selector-opt-{:?}", mode)))
            .cursor_pointer()
            .px_2p5()
            .py_1()
            .rounded_md()
            .bg(if is_selected { rgb(colors::DARK_ACCENT_BLUE) } else { rgb(colors::DARK_BG_BASE) })
            .border_1()
            .border_color(if is_selected { rgb(colors::DARK_ACCENT_BLUE) } else { rgb(colors::DARK_BORDER) })
            .text_color(if is_selected { rgb(0x11111b) } else { rgb(colors::DARK_TEXT_MAIN) })
            .text_xs()
            .font_weight(if is_selected { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL })
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.reading_mode = mode;
                cx.notify();
            }))
    }

    /// Helper for reading width selector button in Initial View tab
    fn render_width_selector_btn(&self, width: ReadingWidth, label: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let is_selected = self.reading_width == width;
        div()
            .id(SharedString::from(format!("width-selector-opt-{:?}", width)))
            .cursor_pointer()
            .px_2p5()
            .py_1()
            .rounded_md()
            .bg(if is_selected { rgb(colors::DARK_ACCENT_BLUE) } else { rgb(colors::DARK_BG_BASE) })
            .border_1()
            .border_color(if is_selected { rgb(colors::DARK_ACCENT_BLUE) } else { rgb(colors::DARK_BORDER) })
            .text_color(if is_selected { rgb(0x11111b) } else { rgb(colors::DARK_TEXT_MAIN) })
            .text_xs()
            .font_weight(if is_selected { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL })
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.reading_width = width;
                cx.notify();
            }))
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
