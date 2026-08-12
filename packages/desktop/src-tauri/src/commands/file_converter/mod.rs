/**
 * 文件格式转换命令
 *
 * 支持格式（5种：md/txt/html/docx/pdf，全互转 20 条路径）：
 * - Rust 处理 12 条非 PDF 路径（MD/TXT/HTML/DOCX 互转）
 * - Python 处理 8 条 PDF 路径（所有涉及 PDF 的操作）
 *
 * 模块结构：
 * - router: 格式检测 + 路由分发
 * - pdf_bridge: Python PDF 桥接
 * - converters: 各格式转换器（md/txt/html/docx）
 */

pub mod router;
pub mod pdf_bridge;
pub mod converters;

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::command;

// 临时文件碰撞防护（原子计数器）
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 在 UTF-8 字符边界处安全截断字符串
/// 避免多字节字符（如中文）被拦腰截断导致 panic
fn safe_truncate(text: &str, byte_cap: usize) -> &str {
    if text.len() <= byte_cap {
        return text;
    }
    let mut end = byte_cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConvertResult {
    pub success: bool,
    pub message: String,
    pub size: u64,
    pub content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileSizeResult {
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveFileResult {
    pub path: String,
}

/// 文档完整提取结果（供 AI 多模态对话使用）
#[derive(Debug, Serialize)]
pub struct DocumentExtract {
    pub html: String,
    pub images: Vec<DocumentImage>,
}

/// 文档中的单张图片
#[derive(Debug, Serialize)]
pub struct DocumentImage {
    pub name: String,
    pub mime: String,
    /// base64 编码的图片数据（可直接用于 data URI）
    pub base64: String,
}

/// Tauri 命令：获取文件大小
#[command]
pub fn get_file_size(path: String) -> Result<FileSizeResult, String> {
    let metadata = std::fs::metadata(&path)
        .map_err(|e| format!("无法读取文件: {}", e))?;
    Ok(FileSizeResult {
        size: metadata.len(),
    })
}

/// Tauri 命令：保存拖拽上传的文件到临时目录
#[command]
pub fn save_dropped_file(filename: String, data: Vec<u8>) -> Result<SaveFileResult, String> {
    let upload_dir = std::env::temp_dir().join("nahida-toolkit-uploads");
    std::fs::create_dir_all(&upload_dir)
        .map_err(|e| format!("创建上传目录失败: {}", e))?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let safe_name = format!("{}_{}", ts, &filename);
    let file_path = upload_dir.join(&safe_name);
    std::fs::write(&file_path, &data)
        .map_err(|e| format!("写入临时文件失败: {}", e))?;

    Ok(SaveFileResult {
        path: file_path.to_string_lossy().to_string(),
    })
}

/// Tauri 命令：转换文件（所有路径，含 PDF 自动走 Python 桥接）
#[command]
pub fn convert_file(src_path: String, dst_path: String) -> Result<ConvertResult, String> {
    let src = Path::new(&src_path);
    let dst = Path::new(&dst_path);

    if !src.exists() {
        return Err("源文件不存在".to_string());
    }

    // 确保目标目录存在
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建输出目录失败: {}", e))?;
    }

    // 通过专用转换引擎执行（16MB 大栈 + panic 捕获 + 自动恢复）
    let size = super::conversion_engine::run_on_worker(move || -> Result<u64, String> {
        let src = Path::new(&src_path);
        let dst = Path::new(&dst_path);
        router::route_conversion(src, dst)
    })?;

    Ok(ConvertResult {
        success: true,
        message: "转换成功".to_string(),
        size,
        content: None,
    })
}

/// Tauri 命令：转换文本内容
///
/// 若源或目标为 PDF 格式，自动转为文件模式（写临时文件后调用完整管道）。
/// 注意：PDF 源格式需传入 PDF 二进制数据（以字符串形式），不推荐使用。
#[command]
pub fn convert_content(
    content: String,
    from_fmt: String,
    to_fmt: String,
) -> Result<ConvertResult, String> {
    let temp_dir = std::env::temp_dir().join("nahida-toolkit-convert");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("创建临时目录失败: {}", e))?;

    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let src_ext = match from_fmt.as_str() {
        "md" | "markdown" => "md",
        "txt" => "txt",
        "html" | "htm" => "html",
        "pdf" => "pdf",
        _ => return Err(format!("不支持的内容源格式: {}", from_fmt)),
    };
    let dst_ext = match to_fmt.as_str() {
        "md" | "markdown" => "md",
        "txt" => "txt",
        "html" | "htm" => "html",
        "docx" => "docx",
        "pdf" => "pdf",
        _ => return Err(format!("不支持的内容目标格式: {}", to_fmt)),
    };

    let src_path = temp_dir.join(format!("content_{}.{}", counter, src_ext));
    let dst_path = temp_dir.join(format!("output_{}.{}", counter, dst_ext));

    std::fs::write(&src_path, &content)
        .map_err(|e| format!("写入临时内容失败: {}", e))?;

    // 若涉及 PDF（源或目标），走完整文件管道（含 Python 桥接）
    if from_fmt == "pdf" || to_fmt == "pdf" {
        let src_path_s = src_path.to_string_lossy().to_string();
        let dst_path_s = dst_path.to_string_lossy().to_string();
        let size = super::conversion_engine::run_on_worker(move || -> Result<u64, String> {
            let src = Path::new(&src_path_s);
            let dst = Path::new(&dst_path_s);
            router::route_conversion(src, dst)
        })?;

        // PDF 输出为二进制，返回 size 但不返回 content
        return Ok(ConvertResult {
            success: true,
            message: "转换成功".to_string(),
            size,
            content: None,
        });
    }

    let from = from_fmt.clone();
    let to = to_fmt.clone();
    let dst_path_clone = dst_path.clone();
    let size = super::conversion_engine::run_on_worker(move || -> Result<u64, String> {
        let src = &src_path;
        let dst = &dst_path_clone;
        match (from.as_str(), to.as_str()) {
            ("md", "txt") => converters::md::md_to_txt(src, dst),
            ("md", "html") => converters::md::md_to_html(src, dst),
            ("md", "docx") => converters::md::md_to_docx(src, dst),
            ("txt", "html") => converters::txt::txt_to_html(src, dst),
            ("txt", "md") => converters::txt::txt_to_md(src, dst),
            ("txt", "docx") => converters::txt::txt_to_docx(src, dst),
            ("html", "txt") => converters::html::html_to_txt(src, dst),
            ("html", "md") => converters::html::html_to_md(src, dst),
            ("html", "docx") => converters::html::html_to_docx(src, dst),
            ("md", "md") | ("txt", "txt") | ("html", "html") => {
                std::fs::copy(src, dst).map_err(|e| format!("复制失败: {}", e))?;
                std::fs::metadata(dst).map(|m| m.len()).map_err(|e| format!("读取文件失败: {}", e))
            }
            _ => Err(format!("不支持的内容转换: {} → {}", from, to)),
        }
    })?;

    let result = std::fs::read_to_string(&dst_path)
        .map_err(|e| format!("读取转换结果失败: {}", e))?;

    Ok(ConvertResult {
        success: true,
        message: "转换成功".to_string(),
        size,
        content: Some(result),
    })
}

/// Tauri 命令：从 Office 文档二进制数据提取纯文本
///
/// 供终端对话附件上传使用，支持 .docx / .pdf 格式。
/// 返回提取后的文本（截断至 8000 字以内）。
#[command]
pub fn extract_office_text(data: Vec<u8>, filename: String) -> Result<String, String> {
    // 防护：拒绝超过 5MB 的原始 IPC 数据（更大文件应走 extract_office_text_by_path）
    if data.len() > 5 * 1024 * 1024 {
        return Err("文件过大（>5MB），请通过路径方式读取".to_string())
    }

    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "docx" => {
            let text = converters::docx::extract_docx_md(&data)?;
            if text.len() > 8000 {
                Ok(format!("{}\n\n…(内容过长，已截断至前 8000 字)", safe_truncate(&text, 8000)))
            } else {
                Ok(text)
            }
        }
        "pdf" => {
            // PDF 需走 Python 桥接：写临时文件 → 调用 pdf_to_md → 读取结果
            let temp_dir = std::env::temp_dir().join("nahida-toolkit-pdf");
            std::fs::create_dir_all(&temp_dir)
                .map_err(|e| format!("创建临时目录失败: {}", e))?;
            let counter = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let pdf_path = temp_dir.join(format!("chat_pdf_{}.pdf", counter));
            let md_path = temp_dir.join(format!("chat_pdf_{}.md", counter));
            std::fs::write(&pdf_path, &data)
                .map_err(|e| format!("写入临时 PDF 失败: {}", e))?;

            pdf_bridge::call_pdf_python("pdf_to_md", &pdf_path, &md_path)?;

            let text = std::fs::read_to_string(&md_path)
                .map_err(|e| format!("读取 PDF 提取结果失败: {}", e))?;
            // 清理临时文件
            let _ = std::fs::remove_file(&pdf_path);
            let _ = std::fs::remove_file(&md_path);

            if text.len() > 8000 {
                Ok(format!("{}\n\n…(内容过长，已截断至前 8000 字)", safe_truncate(&text, 8000)))
            } else {
                Ok(text)
            }
        }
        "doc" => Err("暂不支持旧版 .doc 格式，请先用 Word 另存为 .docx 格式后再上传".to_string()),
        _ => Err(format!("不支持的文件格式: .{}，支持 .docx / .pdf", ext)),
    }
}

/// Tauri 命令：从磁盘路径读取 Office 文档并提取纯文本（大文件专用）
///
/// 与 extract_office_text 功能相同，但直接从磁盘读取文件，
/// 避免 IPC JSON 序列化瓶颈。支持任意大小文件。
#[command]
pub fn extract_office_text_by_path(path: String, filename: String) -> Result<String, String> {
    let data = std::fs::read(&path)
        .map_err(|e| format!("读取文件失败: {}", e))?;

    // 安全防护：拒绝超过 100MB 的文档
    if data.len() > 100 * 1024 * 1024 {
        return Err("文件过大（>100MB），无法处理".to_string())
    }

    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "docx" => {
            let text = converters::docx::extract_docx_md(&data)?;
            if text.len() > 8000 {
                Ok(format!("{}\n\n…(内容过长，已截断至前 8000 字)", safe_truncate(&text, 8000)))
            } else {
                Ok(text)
            }
        }
        "pdf" => {
            // 已有磁盘文件，直接走 Python 桥接
            let temp_dir = std::env::temp_dir().join("nahida-toolkit-pdf");
            std::fs::create_dir_all(&temp_dir)
                .map_err(|e| format!("创建临时目录失败: {}", e))?;
            let counter = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let md_path = temp_dir.join(format!("chat_pdf_{}.md", counter));
            let pdf_path = std::path::Path::new(&path);

            pdf_bridge::call_pdf_python("pdf_to_md", pdf_path, &md_path)?;

            let text = std::fs::read_to_string(&md_path)
                .map_err(|e| format!("读取 PDF 提取结果失败: {}", e))?;
            let _ = std::fs::remove_file(&md_path);

            if text.len() > 8000 {
                Ok(format!("{}\n\n…(内容过长，已截断至前 8000 字)", safe_truncate(&text, 8000)))
            } else {
                Ok(text)
            }
        }
        "doc" => Err("暂不支持旧版 .doc 格式，请先用 Word 另存为 .docx 格式后再上传".to_string()),
        _ => Err(format!("不支持的文件格式: .{}，支持 .docx / .pdf", ext)),
    }
}

// ============================================================
// 文档完整提取（HTML + 图片）— 供 AI 多模态对话使用
// ============================================================

/// Base64 编码（简单实现，不引入额外 crate）
fn base64_encode_mod(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 { result.push(CHARS[((n >> 6) & 63) as usize] as char); }
        else { result.push('='); }
        if chunk.len() > 2 { result.push(CHARS[(n & 63) as usize] as char); }
        else { result.push('='); }
    }
    result
}

/// Tauri 命令：从字节数据提取文档完整内容（DOCX/PDF → HTML + 图片）
///
/// 与 extract_office_text 不同，本命令：
/// 1. 输出 HTML 而非 Markdown（保留表格结构、格式标注）
/// 2. 提取内嵌图片并返回 base64 编码
/// 3. HTML 中图片已内嵌为 data URI
#[command]
pub fn extract_document_full(data: Vec<u8>, filename: String) -> Result<DocumentExtract, String> {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "docx" => {
            let (html, images) = converters::docx::extract_docx_full(&data)?;
            let doc_images: Vec<DocumentImage> = images
                .into_iter()
                .map(|img| DocumentImage {
                    name: img.name,
                    mime: img.mime,
                    base64: base64_encode_mod(&img.data),
                })
                .collect();
            Ok(DocumentExtract { html, images: doc_images })
        }
        "pdf" => {
            // PDF 走 Python 桥接：pdf_to_html 保留表格结构
            let temp_dir = std::env::temp_dir().join("nahida-toolkit-pdf");
            std::fs::create_dir_all(&temp_dir)
                .map_err(|e| format!("创建临时目录失败: {}", e))?;
            let counter = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let pdf_path = temp_dir.join(format!("chat_full_{}.pdf", counter));
            let html_path = temp_dir.join(format!("chat_full_{}.html", counter));
            std::fs::write(&pdf_path, &data)
                .map_err(|e| format!("写入临时 PDF 失败: {}", e))?;

            pdf_bridge::call_pdf_python("pdf_to_html", &pdf_path, &html_path)
                .map_err(|e| format!("PDF 提取失败: {}", e))?;

            let html = std::fs::read_to_string(&html_path)
                .map_err(|e| format!("读取 PDF 提取结果失败: {}", e))?;

            // 清理临时文件
            let _ = std::fs::remove_file(&pdf_path);
            let _ = std::fs::remove_file(&html_path);

            Ok(DocumentExtract { html, images: vec![] })
        }
        _ => Err(format!("不支持的文件格式: .{}，支持 .docx / .pdf", ext)),
    }
}

/// Tauri 命令：从磁盘路径提取文档完整内容（大文件专用）
#[command]
pub fn extract_document_full_by_path(path: String, filename: String) -> Result<DocumentExtract, String> {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

    // PDF：直接用路径走 Python 桥接，避免 读→写临时文件→再读 的双份 I/O
    if ext == "pdf" {
        let file_size = std::fs::metadata(&path)
            .map(|m| m.len())
            .unwrap_or(0);
        if file_size > 100 * 1024 * 1024 {
            return Err("文件过大（>100MB），无法处理".to_string());
        }
        let temp_dir = std::env::temp_dir().join("nahida-toolkit-pdf");
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("创建临时目录失败: {}", e))?;
        let counter = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let html_path = temp_dir.join(format!("chat_full_{}.html", counter));

        pdf_bridge::call_pdf_python("pdf_to_html", std::path::Path::new(&path), &html_path)
            .map_err(|e| format!("PDF 提取失败: {}", e))?;

        let html = std::fs::read_to_string(&html_path)
            .map_err(|e| format!("读取 PDF 提取结果失败: {}", e))?;
        let _ = std::fs::remove_file(&html_path);

        return Ok(DocumentExtract { html, images: vec![] });
    }

    // DOCX 等：仍需读入内存解析
    let data = std::fs::read(&path)
        .map_err(|e| format!("读取文件失败: {}", e))?;

    if data.len() > 100 * 1024 * 1024 {
        return Err("文件过大（>100MB），无法处理".to_string());
    }

    extract_document_full(data, filename)
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_src(name: &str) -> PathBuf {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../临时/test_files/chain_conversions");
        base.join(name)
    }

    fn test_dst(ext: &str) -> PathBuf {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("test_conv_{}.{}", counter, ext))
    }

    #[test]
    fn test_detect_conversion() {
        assert_eq!(
            router::detect_conversion(Path::new("a.md"), Path::new("a.txt")).unwrap(),
            ("md", "txt")
        );
        assert_eq!(
            router::detect_conversion(Path::new("a.md"), Path::new("a.pdf")).unwrap(),
            ("md", "pdf")
        );
        assert!(router::detect_conversion(Path::new("a.md"), Path::new("a.md")).is_err());
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(converters::html_escape("hello"), "hello");
        assert_eq!(converters::html_escape("<hello>"), "&lt;hello&gt;");
        assert_eq!(converters::html_escape("a&b"), "a&amp;b");
    }

    #[test]
    fn test_strip_md_formatting() {
        assert_eq!(converters::strip_md_formatting("**bold**"), "bold");
        assert_eq!(converters::strip_md_formatting("*italic*"), "italic");
        assert_eq!(converters::strip_md_formatting("`code`"), "code");
    }

    // ============================================================
    // 集成测试：关键转换路径
    // ============================================================

    #[test]
    fn test_intg_md_to_html() {
        let src = test_src("test_sample.md");
        let dst = test_dst("html");
        let result = converters::md::md_to_html(&src, &dst);
        assert!(result.is_ok(), "MD→HTML 失败: {:?}", result.err());
        let content = std::fs::read_to_string(&dst).unwrap();
        assert!(content.contains("<html") || content.contains("<body"),
            "HTML 输出应包含 HTML 标签结构");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn test_intg_html_to_md() {
        let src = test_src("test_sample.html");
        let dst = test_dst("md");
        let result = converters::html::html_to_md(&src, &dst);
        assert!(result.is_ok(), "HTML→MD 失败: {:?}", result.err());
        let content = std::fs::read_to_string(&dst).unwrap();
        assert!(!content.is_empty(), "Markdown 输出不应为空");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn test_intg_md_to_txt() {
        let src = test_src("test_sample.md");
        let dst = test_dst("txt");
        let result = converters::md::md_to_txt(&src, &dst);
        assert!(result.is_ok(), "MD→TXT 失败: {:?}", result.err());
        let content = std::fs::read_to_string(&dst).unwrap();
        assert!(!content.is_empty(), "纯文本输出不应为空");
        assert!(content.contains("纳西妲"), "应包含原始文本内容");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn test_intg_md_to_docx() {
        let src = test_src("test_sample.md");
        let dst = test_dst("docx");
        let result = converters::md::md_to_docx(&src, &dst);
        assert!(result.is_ok(), "MD→DOCX 失败: {:?}", result.err());
        // DOCX 是有效的 ZIP 文件（PK 头）
        let data = std::fs::read(&dst).unwrap();
        assert!(data.len() > 22, "DOCX 文件过小");
        assert_eq!(&data[0..2], &[0x50, 0x4B], "应生成有效的 ZIP/DOCX");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn test_intg_html_to_docx() {
        let src = test_src("test_sample.html");
        let dst = test_dst("docx");
        let result = converters::html::html_to_docx(&src, &dst);
        assert!(result.is_ok(), "HTML→DOCX 失败: {:?}", result.err());
        let data = std::fs::read(&dst).unwrap();
        assert!(data.len() > 22, "DOCX 文件过小");
        assert_eq!(&data[0..2], &[0x50, 0x4B], "应生成有效的 ZIP/DOCX");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn test_intg_router_pdf_detection() {
        // 验证 PDF 路径被路由识别（实际转换需 Python 桥接）
        let src = test_src("test_sample.pdf");
        let dst = test_dst("txt");
        let (from, to) = router::detect_conversion(&src, &dst).unwrap();
        assert_eq!(from, "pdf");
        assert_eq!(to, "txt");

        let dst2 = test_dst("html");
        let result2 = router::detect_conversion(&src, &dst2);
        assert!(result2.is_ok());

        // 同格式应报错
        let dst3 = test_src("test_sample.pdf");
        let result3 = router::detect_conversion(&src, &dst3);
        assert!(result3.is_err());
    }

    #[test]
    fn test_intg_txt_to_html() {
        let src = test_src("test_sample.txt");
        let dst = test_dst("html");
        let result = converters::txt::txt_to_html(&src, &dst);
        assert!(result.is_ok(), "TXT→HTML 失败: {:?}", result.err());
        let content = std::fs::read_to_string(&dst).unwrap();
        assert!(content.contains("<pre>") || content.contains("<body>"),
            "TXT→HTML 应生成 HTML 标签结构");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn test_intg_docx_to_txt() {
        let src = test_src("test_sample.docx");
        let dst = test_dst("txt");
        let result = converters::docx::docx_to_txt(&src, &dst);
        assert!(result.is_ok(), "DOCX→TXT 失败: {:?}", result.err());
        let content = std::fs::read_to_string(&dst).unwrap();
        assert!(!content.is_empty(), "纯文本输出不应为空");
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn test_intg_full_route_non_pdf() {
        // 测试 route_conversion 非 PDF 路径
        let src = test_src("test_sample.md");
        let dst = test_dst("html");
        let result = router::route_conversion(&src, &dst);
        assert!(result.is_ok(), "route_conversion MD→HTML 失败: {:?}", result.err());
        assert!(result.unwrap() > 0, "结果文件大小应为正数");
        let _ = std::fs::remove_file(&dst);
    }
}
