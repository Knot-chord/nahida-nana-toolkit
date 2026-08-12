/**
 * 纯文本转换器
 *
 * TXT→HTML: 空行分段→<p> 包裹，转义特殊字符
 * TXT→MD: 直接复制（TXT 是 MD 有效子集）
 * TXT→DOCX: docx-rs 按行构建 Paragraph
 */

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use super::html_escape;

/// 纯文本 → HTML（流式处理，支持大文件）
pub fn txt_to_html(src: &Path, dst: &Path) -> Result<u64, String> {
    let src_file = std::fs::File::open(src).map_err(|e| format!("无法打开源文件: {}", e))?;
    let dst_file = std::fs::File::create(dst).map_err(|e| format!("无法创建目标文件: {}", e))?;

    let reader = BufReader::new(src_file);
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

/// 纯文本 → Markdown（直接复制，纯文本已是 Markdown 子集）
pub fn txt_to_md(src: &Path, dst: &Path) -> Result<u64, String> {
    std::fs::copy(src, dst).map_err(|e| format!("复制失败: {}", e))?;
    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// 纯文本 → DOCX
pub fn txt_to_docx(src: &Path, dst: &Path) -> Result<u64, String> {
    use docx_rs::*;

    let content = std::fs::read_to_string(src)
        .map_err(|e| format!("无法读取源文件: {}", e))?;

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
