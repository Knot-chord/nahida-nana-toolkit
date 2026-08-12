/**
 * 格式检测 + 路由分发
 *
 * 含 PDF → 一律走 Python（pdf_bridge）
 * 非 PDF → Rust 各转换器
 */

use std::path::Path;

use super::pdf_bridge;
use super::converters;

/// 根据扩展名确定转换类型
pub fn detect_conversion(src: &Path, dst: &Path) -> Result<(&'static str, &'static str), String> {
    let src_ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or_else(|| "无法识别源文件扩展名".to_string())?;
    let dst_ext = dst
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or_else(|| "无法识别目标文件扩展名".to_string())?;

    match (src_ext.as_str(), dst_ext.as_str()) {
        // Markdown
        ("md" | "markdown", "txt") => Ok(("md", "txt")),
        ("md" | "markdown", "html" | "htm") => Ok(("md", "html")),
        ("md" | "markdown", "docx") => Ok(("md", "docx")),
        ("md" | "markdown", "pdf") => Ok(("md", "pdf")),
        // HTML
        ("html" | "htm", "txt") => Ok(("html", "txt")),
        ("html" | "htm", "md") => Ok(("html", "md")),
        ("html" | "htm", "docx") => Ok(("html", "docx")),
        ("html" | "htm", "pdf") => Ok(("html", "pdf")),
        // TXT
        ("txt", "html" | "htm") => Ok(("txt", "html")),
        ("txt", "md") => Ok(("txt", "md")),
        ("txt", "docx") => Ok(("txt", "docx")),
        ("txt", "pdf") => Ok(("txt", "pdf")),
        // DOCX
        ("docx", "txt") => Ok(("docx", "txt")),
        ("docx", "html" | "htm") => Ok(("docx", "html")),
        ("docx", "md" | "markdown") => Ok(("docx", "md")),
        ("docx", "pdf") => Ok(("docx", "pdf")),
        // PDF
        ("pdf", "txt") => Ok(("pdf", "txt")),
        ("pdf", "html" | "htm") => Ok(("pdf", "html")),
        ("pdf", "md" | "markdown") => Ok(("pdf", "md")),
        ("pdf", "docx") => Ok(("pdf", "docx")),
        // 相同格式
        (s, d) if s == d => Err("源格式和目标格式相同，无需转换".to_string()),
        // 不支持的组合
        (_, _) => Err(format!("暂不支持 .{} → .{} 转换", src_ext, dst_ext)),
    }
}

/// 路由转换：含 PDF 走 Python，非 PDF 走 Rust 转换器
pub fn route_conversion(src: &Path, dst: &Path) -> Result<u64, String> {
    let (from_fmt, to_fmt) = detect_conversion(src, dst)?;

    // 含 PDF → 一律走 Python
    if from_fmt == "pdf" || to_fmt == "pdf" {
        let op = format!("{}_to_{}", from_fmt, to_fmt);
        return pdf_bridge::call_pdf_python(&op, src, dst);
    }

    // 非 PDF → Rust 各转换器
    match (from_fmt, to_fmt) {
        ("md", "txt") => converters::md::md_to_txt(src, dst),
        ("md", "html") => converters::md::md_to_html(src, dst),
        ("md", "docx") => converters::md::md_to_docx(src, dst),
        ("txt", "html") => converters::txt::txt_to_html(src, dst),
        ("txt", "md") => converters::txt::txt_to_md(src, dst),
        ("txt", "docx") => converters::txt::txt_to_docx(src, dst),
        ("html", "txt") => converters::html::html_to_txt(src, dst),
        ("html", "md") => converters::html::html_to_md(src, dst),
        ("html", "docx") => converters::html::html_to_docx(src, dst),
        ("docx", "txt") => converters::docx::docx_to_txt(src, dst),
        ("docx", "html") => converters::docx::docx_to_html(src, dst),
        ("docx", "md") => converters::docx::docx_to_md(src, dst),
        _ => Err(format!("不支持的转换: {} → {}", from_fmt, to_fmt)),
    }
}
