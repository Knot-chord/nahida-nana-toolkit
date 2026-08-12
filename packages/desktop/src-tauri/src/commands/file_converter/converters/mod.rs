/**
 * 转换器子模块
 *
 * 共享类型、正则表达式和工具函数。
 */

pub mod md;
pub mod txt;
pub mod html;
pub mod docx;

use regex::Regex;
use std::sync::LazyLock;
use image::GenericImageView;

// ============================================================
// 预编译正则表达式（LazyLock，编译一次，全局复用）
// ============================================================

// ── MD 解析 ──
pub static RE_CODE_FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^```").unwrap());
pub static RE_IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap());
pub static RE_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap());
pub static RE_BOLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*([^*]+)\*\*").unwrap());
pub static RE_ITALIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*([^*]+)\*").unwrap());
pub static RE_LIST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*[-*+]\s+").unwrap());
pub static RE_ORDERED_LIST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\d+\.\s+").unwrap());
pub static RE_BLOCKQUOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^>\s+").unwrap());
pub static RE_INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").unwrap());
pub static RE_HR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[-*_]{3,}\s*$").unwrap());
pub static RE_STRIKETHROUGH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~~([^~]+)~~").unwrap());
pub static RE_HIGHLIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"==([^=]+)==").unwrap());
// ── MD → HTML 辅助 ──
pub static RE_IMAGE_SRC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap());
pub static RE_LINK_SRC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());

// ── HTML → TXT ──
pub static RE_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
pub static RE_SCRIPT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<script[^>]*>[\s\S]*?</script>").unwrap());
pub static RE_STYLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<style[^>]*>[\s\S]*?</style>").unwrap());
pub static RE_BR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
pub static RE_BLOCK_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)</(p|div|h[1-6]|li|tr|blockquote|pre)>").unwrap());
pub static RE_MULTI_BLANK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

// ── HTML → MD 辅助 ──
pub static RE_IMG_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<img[^>]*/?>").unwrap());

// ── MD → DOCX ──
pub static RE_PDF_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.+)$").unwrap());


// ── 文本辅助 ──
pub static RE_ENTITY_NBSP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&nbsp;").unwrap());
pub static RE_ENTITY_AMP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&amp;").unwrap());
pub static RE_ENTITY_LT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&lt;").unwrap());
pub static RE_ENTITY_GT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&gt;").unwrap());

// ============================================================
// 共享工具函数
// ============================================================

/// HTML 特殊字符转义
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 计算图片在 DOCX 中的 EMU 尺寸（96 DPI，限制最大宽度 ~6.5 英寸）
pub fn get_img_emu_size(img_data: &[u8]) -> (u32, u32) {
    const DEFAULT_EMU: u32 = 2_743_200; // ~3 英寸
    const MAX_W_EMU: u32 = 5_943_600; // ~6.5 英寸
    const EMU_PER_PX: u32 = 9525; // 96 DPI

    match image::load_from_memory(img_data) {
        Ok(img) => {
            let (w, h) = img.dimensions();
            let w_emu = w * EMU_PER_PX;
            let h_emu = h * EMU_PER_PX;
            if w_emu > MAX_W_EMU {
                let ratio = MAX_W_EMU as f64 / w_emu as f64;
                (MAX_W_EMU, (h_emu as f64 * ratio) as u32)
            } else {
                (w_emu, h_emu)
            }
        }
        Err(_) => (DEFAULT_EMU, DEFAULT_EMU),
    }
}

/// 行内格式处理（粗体、斜体、代码、图片、链接、删除线、高亮）→ HTML
pub fn inline_format(text: &str) -> String {
    let mut result = text.to_string();
    result = RE_IMAGE_SRC
        .replace_all(&result, r#"<img src="$2" alt="$1">"#)
        .to_string();
    result = RE_LINK_SRC
        .replace_all(&result, r#"<a href="$2">$1</a>"#)
        .to_string();
    result = RE_INLINE_CODE
        .replace_all(&result, "<code>$1</code>")
        .to_string();
    result = RE_BOLD
        .replace_all(&result, "<strong>$1</strong>")
        .to_string();
    result = RE_ITALIC
        .replace_all(&result, "<em>$1</em>")
        .to_string();
    result = RE_STRIKETHROUGH
        .replace_all(&result, "<del>$1</del>")
        .to_string();
    result = RE_HIGHLIGHT
        .replace_all(&result, "<mark>$1</mark>")
        .to_string();
    result
}

/// 去除 Markdown 格式标记（用于测试及 DOCX 纯文本输出）
#[allow(dead_code)]
pub fn strip_md_formatting(s: &str) -> String {
    let mut result = s.to_string();
    result = RE_BOLD.replace_all(&result, "$1").to_string();
    result = RE_ITALIC.replace_all(&result, "$1").to_string();
    result = RE_INLINE_CODE.replace_all(&result, "$1").to_string();
    result = RE_STRIKETHROUGH.replace_all(&result, "$1").to_string();
    result = RE_HIGHLIGHT.replace_all(&result, "$1").to_string();
    result = RE_IMAGE.replace_all(&result, "【图片】").to_string();
    result = RE_LINK.replace_all(&result, "$1").to_string();
    result
}

