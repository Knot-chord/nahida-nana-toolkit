/**
 * HTML 转换器
 *
 * HTML→TXT: 剥离标签 + 解码实体
 * HTML→MD: html2md 直接转换
 * HTML→DOCX: 剥离标签后 docx-rs 构建（含图片嵌入）
 */

use std::io::{BufWriter, Write};
use std::path::Path;

use crate::commands::file_converter::text_io::read_text_flexible;

use super::{
    RE_BLOCK_END, RE_BR, RE_ENTITY_AMP, RE_ENTITY_GT, RE_ENTITY_LT, RE_ENTITY_NBSP,
    RE_IMG_TAG, RE_IMAGE, RE_MULTI_BLANK, RE_SCRIPT, RE_STYLE, RE_TAG,
    get_img_emu_size,
};

use docx_rs::{Docx, Paragraph, Pic, Run};

// ============================================================
// 转换实现
// ============================================================

/// 从 HTML img 标签中提取 src 和 alt 属性
fn extract_img_attrs(tag: &str) -> (String, String) {
    let src_pat = "src=\"";
    let alt_pat = "alt=\"";
    let src = if let Some(start) = tag.find(src_pat) {
        let val_start = start + src_pat.len();
        if let Some(end) = tag[val_start..].find('"') {
            tag[val_start..val_start + end].to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let alt = if let Some(start) = tag.find(alt_pat) {
        let val_start = start + alt_pat.len();
        if let Some(end) = tag[val_start..].find('"') {
            tag[val_start..val_start + end].to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    (src, alt)
}

/// HTML → 纯文本（剥离标签 + 解码实体；先读后写，读取失败不留孤儿目标文件）
pub fn html_to_txt(src: &Path, dst: &Path) -> Result<u64, String> {
    let mut content = read_text_flexible(src)?;
    let dst_file = std::fs::File::create(dst).map_err(|e| format!("无法创建目标文件: {}", e))?;

    let mut writer = BufWriter::new(dst_file);

    // 移除 script 和 style 块
    content = RE_SCRIPT.replace_all(&content, "").to_string();
    content = RE_STYLE.replace_all(&content, "").to_string();
    // <br> → 换行
    content = RE_BR.replace_all(&content, "\n").to_string();
    // 块级结束标签 → 换行
    content = RE_BLOCK_END.replace_all(&content, "\n").to_string();
    // 图片标签 → 占位符
    content = RE_IMG_TAG.replace_all(&content, "【图片】").to_string();
    // 移除所有 HTML 标签
    content = RE_TAG.replace_all(&content, "").to_string();
    // 解码 HTML 实体
    content = RE_ENTITY_NBSP.replace_all(&content, " ").to_string();
    content = RE_ENTITY_AMP.replace_all(&content, "&").to_string();
    content = RE_ENTITY_LT.replace_all(&content, "<").to_string();
    content = RE_ENTITY_GT.replace_all(&content, ">").to_string();
    // 清理多余空行
    content = RE_MULTI_BLANK.replace_all(&content, "\n\n").to_string();

    write!(writer, "{}", content.trim()).map_err(|e| format!("写入失败: {}", e))?;
    writer.flush().map_err(|e| format!("刷新缓冲区失败: {}", e))?;

    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// HTML → Markdown（使用 html2md，DOM 解析）
pub fn html_to_md(src: &Path, dst: &Path) -> Result<u64, String> {
    let content = read_text_flexible(src)?;

    let mut md = html2md::parse_html(&content);

    // 图片用占位符
    md = RE_IMAGE.replace_all(&md, "【图片】").to_string();

    std::fs::write(dst, md).map_err(|e| format!("写入失败: {}", e))?;

    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// HTML → DOCX（剥离标签后 docx-rs 构建，含图片嵌入）
pub fn html_to_docx(src: &Path, dst: &Path) -> Result<u64, String> {
    let content = read_text_flexible(src)?;
    let base_dir = src.parent().unwrap_or(Path::new("."));

    // Step 1: 提取所有 <img> 标签，替换为 {IMG:n} 占位符
    let mut images: Vec<(String, String)> = Vec::new(); // (src, alt)
    let mut text = String::new();
    let mut last_end = 0;

    for m in RE_IMG_TAG.find_iter(&content) {
        // 保留 <img> 前的文本
        text.push_str(&content[last_end..m.start()]);
        // 提取 src/alt
        let (src, alt) = extract_img_attrs(m.as_str());
        images.push((src, alt));
        let idx = images.len() - 1;
        text.push_str(&format!("{{IMG:{}}}", idx));
        last_end = m.end();
    }
    text.push_str(&content[last_end..]);

    // Step 2: 剥离剩余 HTML 标签 + 解码实体
    text = RE_SCRIPT.replace_all(&text, "").to_string();
    text = RE_STYLE.replace_all(&text, "").to_string();
    text = RE_TAG.replace_all(&text, "").to_string();
    text = RE_ENTITY_NBSP.replace_all(&text, " ").to_string();
    text = RE_ENTITY_AMP.replace_all(&text, "&").to_string();
    text = RE_ENTITY_LT.replace_all(&text, "<").to_string();
    text = RE_ENTITY_GT.replace_all(&text, ">").to_string();

    // Step 3: 构建 DOCX
    let mut doc = Docx::new();
    for line in text.lines() {
        let trimmed = line.trim();

        // 检查图片占位符（纯占位符行才嵌入图片）
        if trimmed.starts_with("{IMG:") && trimmed.ends_with('}') {
            let num_str = trimmed
                .strip_prefix("{IMG:")
                .and_then(|s| s.strip_suffix('}'))
                .unwrap_or("");
            if let Ok(idx) = num_str.parse::<usize>() {
                if idx < images.len() {
                    let (img_src, _alt) = &images[idx];
                    let full_path = base_dir.join(img_src);
                    if let Ok(img_data) = std::fs::read(&full_path) {
                        let (w_emu, h_emu) = get_img_emu_size(&img_data);
                        let pic = Pic::new(&img_data).id("rId1").size(w_emu, h_emu);
                        doc = doc.add_paragraph(
                            Paragraph::new().add_run(Run::new().add_image(pic)),
                        );
                    } else {
                        doc = doc.add_paragraph(
                            Paragraph::new().add_run(Run::new().add_text("【图片】")),
                        );
                    }
                    continue;
                }
            }
        }

        // 普通文本
        if trimmed.is_empty() {
            doc = doc.add_paragraph(Paragraph::new());
        } else {
            doc = doc.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(trimmed)),
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

