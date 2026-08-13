/**
 * 纯文本转换器
 *
 * TXT→HTML: 空行分段→<p> 包裹，转义特殊字符
 * TXT→MD: 直接复制（TXT 是 MD 有效子集）
 * TXT→DOCX: docx-rs 按行构建 Paragraph
 */

use std::io::{BufRead, BufWriter, Cursor, Write};
use std::path::Path;

use crate::commands::file_converter::text_io::read_text_flexible;

use super::html_escape;

/// 纯文本 → HTML（逐行转义包裹；先读后写，读取失败不留孤儿目标文件）
pub fn txt_to_html(src: &Path, dst: &Path) -> Result<u64, String> {
    let content = read_text_flexible(src)?;
    let dst_file = std::fs::File::create(dst).map_err(|e| format!("无法创建目标文件: {}", e))?;

    let reader = Cursor::new(content.as_bytes());
    let mut writer = BufWriter::new(dst_file);

    let title = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Converted");

    writeln!(writer, "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">")
        .map_err(|e| format!("写入失败: {}", e))?;
    writeln!(writer, "<title>{}</title>", html_escape(title))
        .map_err(|e| format!("写入失败: {}", e))?;
    writeln!(writer, "</head>\n<body>\n<pre>")
        .map_err(|e| format!("写入失败: {}", e))?;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("读取失败: {}", e))?;
        writeln!(writer, "{}", html_escape(&line))
            .map_err(|e| format!("写入失败: {}", e))?;
    }

    writeln!(writer, "</pre>\n</body>\n</html>")
        .map_err(|e| format!("写入失败: {}", e))?;
    writer.flush().map_err(|e| format!("刷新缓冲区失败: {}", e))?;

    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// 纯文本 → Markdown（编码自适应读取后统一以 UTF-8 写出，保证输出编码一致；内容为纯子集复制，无损）
pub fn txt_to_md(src: &Path, dst: &Path) -> Result<u64, String> {
    let content = read_text_flexible(src)?;
    std::fs::write(dst, content.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// 纯文本 → DOCX
pub fn txt_to_docx(src: &Path, dst: &Path) -> Result<u64, String> {
    use docx_rs::*;

    let content = read_text_flexible(src)?;

    let mut doc = Docx::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            doc = doc.add_paragraph(Paragraph::new());
        } else {
            doc = doc.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(line)),
            );
        }
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    doc.build().pack(&mut buf).map_err(|e| format!("DOCX 生成失败: {}", e))?;
    let bytes = buf.into_inner();
    let written = bytes.len() as u64;
    std::fs::write(dst, &bytes).map_err(|e| format!("写入失败: {}", e))?;
    Ok(written)
}
