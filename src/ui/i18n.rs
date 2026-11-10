use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    ZhCn = 0,
    EnUs = 1,
}

static CURRENT_LOCALE: AtomicU8 = AtomicU8::new(Locale::ZhCn as u8);

impl Locale {
    pub fn code(&self) -> &'static str {
        match self {
            Locale::ZhCn => "zh-CN",
            Locale::EnUs => "en-US",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code.to_lowercase().replace('_', "-").as_str() {
            "zh" | "zh-cn" | "zh-hans" => Some(Locale::ZhCn),
            "en" | "en-us" | "en-gb" => Some(Locale::EnUs),
            _ => None,
        }
    }
}

pub fn set_locale(locale: Locale) {
    CURRENT_LOCALE.store(locale as u8, Ordering::SeqCst);
}

pub fn current_locale() -> Locale {
    match CURRENT_LOCALE.load(Ordering::SeqCst) {
        1 => Locale::EnUs,
        _ => Locale::ZhCn,
    }
}

pub const UI_ERROR_MSG_ZH: &str = "翻译失败，请稍后重试";
pub const UI_ERROR_MSG_EN: &str = "Translation failed, please try again later";
pub const UI_RETRY_ACTION_ZH: &str = "重试";
pub const UI_RETRY_ACTION_EN: &str = "Retry";

pub fn translation_failed_text(locale: Option<Locale>) -> &'static str {
    match locale.unwrap_or_else(current_locale) {
        Locale::ZhCn => UI_ERROR_MSG_ZH,
        Locale::EnUs => UI_ERROR_MSG_EN,
    }
}

pub fn retry_action_text(locale: Option<Locale>) -> &'static str {
    match locale.unwrap_or_else(current_locale) {
        Locale::ZhCn => UI_RETRY_ACTION_ZH,
        Locale::EnUs => UI_RETRY_ACTION_EN,
    }
}

pub struct I18nRegistry {
    zh_cn: HashMap<&'static str, &'static str>,
    en_us: HashMap<&'static str, &'static str>,
}

impl I18nRegistry {
    fn new() -> Self {
        let mut zh = HashMap::new();
        let mut en = HashMap::new();

        macro_rules! insert_pair {
            ($key:expr, $zh_val:expr, $en_val:expr) => {
                zh.insert($key, $zh_val);
                en.insert($key, $en_val);
            };
        }

        // App & General
        insert_pair!("app.name", "reTranslate", "reTranslate");
        insert_pair!("app.tagline", "沉浸式双语阅读器", "Native Bilingual Reader");
        insert_pair!("app.ready", "就绪", "Ready");
        insert_pair!("app.loading", "加载中...", "Loading...");
        insert_pair!("app.retry", "重试", "Retry");
        insert_pair!("app.confirm", "确认", "Confirm");
        insert_pair!("app.cancel", "取消", "Cancel");
        insert_pair!("app.save", "保存", "Save");

        // Menu & Toolbar
        insert_pair!("menu.file", "文件", "File");
        insert_pair!("menu.open", "打开文档", "Open Document");
        insert_pair!("menu.export", "导出", "Export");
        insert_pair!("menu.settings", "设置", "Settings");
        insert_pair!("menu.toc", "目录", "Table of Contents");
        insert_pair!("menu.bookmarks", "书签", "Bookmarks");
        insert_pair!("menu.language", "界面语言", "Language");

        // View Modes (Follow-up 1)
        insert_pair!("view.mode.inline_muted", "行内对照 (灰色次级)", "Inline (Muted Gray)");
        insert_pair!("view.mode.inline_dotted", "行内对照 (虚线分隔)", "Inline (Dotted Divider)");
        insert_pair!("view.mode.side_by_side", "双栏对照 (等高对齐)", "Side-by-Side (Row Aligned)");
        insert_pair!("view.mode.target_only", "纯翻译视图", "Target-Only View");
        insert_pair!("view.mode.original_only", "纯原文视图", "Original-Only View");

        // Export Options (Follow-up 1)
        insert_pair!("export.dialog_title", "导出文档", "Export Document");
        insert_pair!("export.mode.bilingual", "双语对照导出", "Bilingual Export");
        insert_pair!("export.mode.target_only", "纯翻译导出", "Target-Only Export");
        insert_pair!("export.format.epub", "EPUB 电子书", "EPUB E-Book");
        insert_pair!("export.format.docx", "Word 文档 (.docx)", "Word Document (.docx)");
        insert_pair!("export.format.markdown", "Markdown 文档 (.md)", "Markdown (.md)");
        insert_pair!("export.format.html", "离线自包含网页 (.html)", "Self-Contained HTML (.html)");

        // Translation & Strict Error Isolation (Follow-up 2)
        insert_pair!("trans.translate_chapter", "翻译当前章", "Translate Chapter");
        insert_pair!("trans.translating", "翻译中...", "Translating...");
        insert_pair!("trans.failed_friendly", UI_ERROR_MSG_ZH, UI_ERROR_MSG_EN);
        insert_pair!("trans.retry_button", UI_RETRY_ACTION_ZH, UI_RETRY_ACTION_EN);

        // Selection Popover
        insert_pair!("popover.copy", "复制译文", "Copy Translation");
        insert_pair!("popover.copied", "已复制到剪贴板", "Copied to clipboard");
        insert_pair!("popover.add_glossary", "加入术语表", "Add to Glossary");

        // Onboarding Wizard
        insert_pair!("onboard.title", "欢迎使用 reTranslate", "Welcome to reTranslate");
        insert_pair!("onboard.subtitle", "请配置您的 AI 翻译服务商", "Configure your AI translation provider");
        insert_pair!("onboard.provider_deepseek", "DeepSeek (深度求索)", "DeepSeek");
        insert_pair!("onboard.provider_openai", "OpenAI (GPT-4o)", "OpenAI");
        insert_pair!("onboard.provider_ollama", "Ollama (本地模型)", "Ollama (Local)");
        insert_pair!("onboard.provider_siliconflow", "硅基流动 (SiliconFlow)", "SiliconFlow");
        insert_pair!("onboard.provider_custom", "自定义 OpenAI 兼容接口", "Custom OpenAI Compatible");
        insert_pair!("onboard.base_url", "API 基础地址 (Base URL)", "API Base URL");
        insert_pair!("onboard.api_key", "API 密钥 (API Key)", "API Key");
        insert_pair!("onboard.model", "模型名称 (Model)", "Model Name");
        insert_pair!("onboard.test_connection", "测试连通性", "Test Connection");
        insert_pair!("onboard.test_success", "连通成功！", "Connected successfully!");
        insert_pair!("onboard.test_failed", "连接失败，请检查配置", "Connection failed, check settings");
        insert_pair!("onboard.get_started", "进入阅读器", "Enter Reader");

        // Typography & Layout Settings (Follow-up 1)
        insert_pair!("settings.reading_width", "阅读区域宽度", "Reading Width");
        insert_pair!("settings.width_narrow", "窄版 (680px)", "Narrow (680px)");
        insert_pair!("settings.width_standard", "标准 (850px)", "Standard (850px)");
        insert_pair!("settings.width_wide", "宽版 (1100px)", "Wide (1100px)");
        insert_pair!("settings.width_full", "自适应全宽 (100%)", "Full Width (100%)");
        insert_pair!("settings.page_padding", "页面边距", "Page Padding");
        insert_pair!("settings.line_height", "行高", "Line Height");
        insert_pair!("settings.paragraph_spacing", "段落间距", "Paragraph Spacing");

        Self {
            zh_cn: zh,
            en_us: en,
        }
    }

    pub fn lookup(&self, key: &str, locale: Locale) -> &'static str {
        match locale {
            Locale::ZhCn => self
                .zh_cn
                .get(key)
                .copied()
                .or_else(|| self.en_us.get(key).copied())
                .unwrap_or(""),
            Locale::EnUs => self
                .en_us
                .get(key)
                .copied()
                .or_else(|| self.zh_cn.get(key).copied())
                .unwrap_or(""),
        }
    }

    pub fn all_keys(&self) -> Vec<&'static str> {
        let mut keys: Vec<_> = self.zh_cn.keys().copied().collect();
        keys.sort();
        keys
    }
}

static REGISTRY: LazyLock<I18nRegistry> = LazyLock::new(I18nRegistry::new);

/// Get localized string for current active locale
pub fn t(key: &str) -> &'static str {
    REGISTRY.lookup(key, current_locale())
}

/// Get localized string for an explicit locale
pub fn t_locale(key: &str, locale: Locale) -> &'static str {
    REGISTRY.lookup(key, locale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i18n_key_parity() {
        let reg = &REGISTRY;
        for key in reg.zh_cn.keys() {
            assert!(
                reg.en_us.contains_key(key),
                "Missing English translation for key: {}",
                key
            );
        }
        for key in reg.en_us.keys() {
            assert!(
                reg.zh_cn.contains_key(key),
                "Missing Chinese translation for key: {}",
                key
            );
        }
    }

    #[test]
    fn test_strict_error_isolation_strings() {
        set_locale(Locale::ZhCn);
        assert_eq!(t("trans.failed_friendly"), "翻译失败，请稍后重试");
        assert_eq!(t("trans.retry_button"), "重试");
        assert_eq!(translation_failed_text(None), "翻译失败，请稍后重试");
        assert_eq!(retry_action_text(None), "重试");

        set_locale(Locale::EnUs);
        assert_eq!(
            t("trans.failed_friendly"),
            "Translation failed, please try again later"
        );
        assert_eq!(t("trans.retry_button"), "Retry");
        assert_eq!(
            translation_failed_text(None),
            "Translation failed, please try again later"
        );
        assert_eq!(retry_action_text(None), "Retry");

        // Reset to default
        set_locale(Locale::ZhCn);
    }
}
