/**
 * Markdown 转换器
 *
 * MD→TXT: 流式逐行剥离 MD 语法
 * MD→HTML: comrak（GFM 完整支持）
 * MD→DOCX: md_to_docx_bytes() 构建（含图片嵌入）
 */

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use super::{
    RE_BOLD, RE_BLOCKQUOTE, RE_CODE_FENCE, RE_HIGHLIGHT, RE_HR, RE_IMAGE,
    RE_IMAGE_SRC, RE_INLINE_CODE, RE_ITALIC, RE_LINK, RE_LIST, RE_ORDERED_LIST,
    RE_PDF_HEADING, RE_STRIKETHROUGH,
    html_escape, get_img_emu_size,
};

use docx_rs::{AlignmentType, Docx, Paragraph, Pic, Run, Table, TableBorder, TableBorderPosition, TableBorders, TableCell, TableRow, BorderType, Shading};
use regex::Regex;
use std::sync::LazyLock;

/// 内联格式片段匹配（加粗 / 斜体 / 删除线 / 高亮 / 行内代码）
static RE_INLINE_PARTS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\*\*(.+?)\*\*|\*(.+?)\*|~~(.+?)~~|==(.+?)==|`([^`]+)`").unwrap()
});

// ============================================================
// GFM 表格解析
// ============================================================

/// 判断一行是否为 GFM 表格分隔行（如 |---|---|）
fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|')
        && trimmed.chars().all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace())
}

/// 从表格行解析单元格内容（去除首尾管道符后按 | 分割）
fn parse_table_cells(line: &str) -> Vec<String> {
    let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

/// 解析内联 Markdown 为 docx-rs Run 序列（加粗/斜体/删除线/高亮/代码）
fn parse_inline_md_to_runs(text: &str) -> Vec<Run> {
    // 预处理：图片 → 占位、链接 → 仅保留文本
    let processed = RE_IMAGE.replace_all(text, "【图片】").to_string();
    let processed = RE_LINK.replace_all(&processed, "$1").to_string();

    let mut runs: Vec<Run> = Vec::new();
    let mut pos = 0;

    for caps in RE_INLINE_PARTS.captures_iter(&processed) {
        let m = caps.get(0).unwrap();
        // 匹配前的纯文本
        if pos < m.start() {
            let plain = &processed[pos..m.start()];
            if !plain.is_empty() {
                runs.push(Run::new().add_text(plain));
            }
        }

        // 按捕获组优先级：bold(1) > italic(2) > strike(3) > highlight(4) > code(5)
        if let Some(bold) = caps.get(1) {
            runs.push(Run::new().add_text(bold.as_str()).bold());
        } else if let Some(italic) = caps.get(2) {
            runs.push(Run::new().add_text(italic.as_str()).italic());
        } else if let Some(strike) = caps.get(3) {
            runs.push(Run::new().add_text(strike.as_str()).strike());
        } else if let Some(highlight) = caps.get(4) {
            runs.push(Run::new().add_text(highlight.as_str()).highlight("yellow"));
        } else if let Some(code) = caps.get(5) {
            runs.push(Run::new().add_text(code.as_str()));
        }

        pos = m.end();
    }

    // 尾部剩余纯文本
    if pos < processed.len() {
        runs.push(Run::new().add_text(&processed[pos..]));
    }

    if runs.is_empty() {
        runs.push(Run::new().add_text(text));
    }

    runs
}

/// Nahida 主题浅绿（表头底色）
const HEADER_FILL: &str = "D9EAD3";

/// 将已收集的表格行构建为 DOCX Table（带边框 + 表头底色 + 单元格格式化）
fn build_docx_table(mut doc: Docx, rows: &[Vec<String>]) -> Docx {
    if rows.is_empty() {
        return doc;
    }

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(1);

    // 表边框：外框稍深(sz=6)，内线较浅(sz=4)
    let borders = TableBorders::new()
        .set(TableBorder::new(TableBorderPosition::Top).border_type(BorderType::Single).size(6).color("999999"))
        .set(TableBorder::new(TableBorderPosition::Bottom).border_type(BorderType::Single).size(6).color("999999"))
        .set(TableBorder::new(TableBorderPosition::Left).border_type(BorderType::Single).size(6).color("999999"))
        .set(TableBorder::new(TableBorderPosition::Right).border_type(BorderType::Single).size(6).color("999999"))
        .set(TableBorder::new(TableBorderPosition::InsideH).border_type(BorderType::Single).size(4).color("CCCCCC"))
        .set(TableBorder::new(TableBorderPosition::InsideV).border_type(BorderType::Single).size(4).color("CCCCCC"));

    let docx_rows: Vec<TableRow> = rows.iter().enumerate().map(|(i, cells)| {
        let is_header = i == 0;
        let docx_cells: Vec<TableCell> = cells.iter().map(|text| {
            // 单元格内联格式解析（加粗/斜体/删除线/代码/高亮）
            let runs = parse_inline_md_to_runs(text);
            let mut para = Paragraph::new();
            for run in runs {
                para = para.add_run(run);
            }

            let mut cell = TableCell::new().add_paragraph(para);
            // 表头行加浅绿底色
            if is_header {
                cell = cell.shading(Shading::new().fill(HEADER_FILL));
            }
            cell
        }).collect();

        // 补齐列数不足的行
        let mut padded = docx_cells;
        while padded.len() < col_count {
            padded.push(TableCell::new().add_paragraph(Paragraph::new()));
        }

        TableRow::new(padded)
    }).collect();

    doc = doc.add_table(Table::new(docx_rows).set_borders(borders));
    // 表后空行，分隔后续内容
    doc.add_paragraph(Paragraph::new())
}

// ============================================================
// 转换实现
// ============================================================

/// Markdown → 纯文本（流式逐行处理，支持大文件）
pub fn md_to_txt(src: &Path, dst: &Path) -> Result<u64, String> {
    let src_file = std::fs::File::open(src).map_err(|e| format!("无法打开源文件: {}", e))?;
    let dst_file = std::fs::File::create(dst).map_err(|e| format!("无法创建目标文件: {}", e))?;

    let reader = BufReader::new(src_file);
    let mut writer = BufWriter::new(dst_file);
    let mut in_code_block = false;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("读取失败: {}", e))?;

        if RE_CODE_FENCE.is_match(&line) {
            in_code_block = !in_code_block;
            if !in_code_block {
                writeln!(writer).map_err(|e| format!("写入失败: {}", e))?;
            }
            continue;
        }
        if in_code_block {
            writeln!(writer, "    {}", line).map_err(|e| format!("写入失败: {}", e))?;
            continue;
        }

        if RE_HR.is_match(&line) {
            writeln!(writer, "────────────────────────────────").map_err(|e| format!("写入失败: {}", e))?;
            continue;
        }

        let mut result = line.clone();
        result = RE_LIST.replace_all(&result, "• ").to_string();
        result = RE_ORDERED_LIST.replace_all(&result, "").to_string();
        result = RE_BLOCKQUOTE.replace_all(&result, "│ ").to_string();
        result = RE_IMAGE.replace_all(&result, "【图片】").to_string();
        result = RE_LINK.replace_all(&result, "$1").to_string();
        result = RE_STRIKETHROUGH.replace_all(&result, "~~$1~~").to_string();
        result = RE_HIGHLIGHT.replace_all(&result, "«$1»").to_string();
        result = RE_BOLD.replace_all(&result, "$1").to_string();
        result = RE_ITALIC.replace_all(&result, "$1").to_string();
        result = RE_INLINE_CODE.replace_all(&result, "$1").to_string();

        writeln!(writer, "{}", result).map_err(|e| format!("写入失败: {}", e))?;
    }

    writer.flush().map_err(|e| format!("刷新缓冲区失败: {}", e))?;
    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// Markdown → HTML（使用 comrak，GFM 完整支持）
pub fn md_to_html(src: &Path, dst: &Path) -> Result<u64, String> {
    let content = std::fs::read_to_string(src)
        .map_err(|e| format!("无法读取源文件: {}", e))?;

    let mut options = comrak::Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.description_lists = true;
    options.extension.multiline_block_quotes = true;
    options.extension.math_dollars = true;
    options.extension.math_code = true;
    options.extension.superscript = true;
    options.parse.smart = true;
    options.parse.default_info_string = None;
    options.parse.relaxed_tasklist_matching = true;
    options.parse.relaxed_autolinks = true;
    options.render.hardbreaks = false;
    options.render.github_pre_lang = true;
    options.render.full_info_string = true;
    options.render.width = 0;
    options.render.unsafe_ = false;
    options.render.escape = false;
    options.render.list_style = comrak::ListStyleType::Dash;
    options.render.sourcepos = false;
    options.render.escaped_char_spans = true;

    let html_body = comrak::markdown_to_html(&content, &options);

    let title = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Converted");

    let full_html = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n{}\n</body>\n</html>",
        html_escape(title),
        html_body
    );

    std::fs::write(dst, full_html.as_bytes())
        .map_err(|e| format!("写入失败: {}", e))?;

    let metadata = std::fs::metadata(dst).map_err(|e| format!("读取输出文件失败: {}", e))?;
    Ok(metadata.len())
}

/// MD 文本构建为 DOCX 字节（含图片嵌入 + GFM 表格）
pub fn md_to_docx_bytes(content: &str, base_dir: &Path) -> Result<Vec<u8>, String> {
    let mut doc = Docx::new();
    let mut in_code_block = false;
    // 表格行缓冲区（遇到非表行时一次性构建为 DOCX Table）
    let mut table_buffer: Vec<Vec<String>> = Vec::new();

    for line in content.lines() {
        if RE_CODE_FENCE.is_match(line) {
            // 退出表格（代码块优先）
            if !table_buffer.is_empty() {
                doc = build_docx_table(doc, &table_buffer);
                table_buffer.clear();
            }
            in_code_block = !in_code_block;
            if !in_code_block {
                doc = doc.add_paragraph(Paragraph::new());
            }
            continue;
        }
        if in_code_block {
            doc = doc.add_paragraph(
                Paragraph::new().add_run(Run::new().add_text(line)),
            );
            continue;
        }

        // ── GFM 表格检测（必须在 heading/hr 之前，避免分隔行被 HR 误匹配）──
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            // 分隔行（|---|---|）跳过，但保留已收集的表头
            if is_table_separator(line) {
                continue;
            }
            table_buffer.push(parse_table_cells(line));
            continue;
        }

        // 非表格行：刷新缓冲区
        if !table_buffer.is_empty() {
            doc = build_docx_table(doc, &table_buffer);
            table_buffer.clear();
        }

        // 标题
        if let Some(cap) = RE_PDF_HEADING.captures(line) {
            let level = cap[1].len();
            let text = &cap[2];
            let font_size = match level {
                1 => 36,
                2 => 32,
                3 => 28,
                4 => 24,
                5 => 22,
                _ => 20,
            };
            doc = doc.add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text(text).bold().size(font_size))
                    .align(AlignmentType::Left),
            );
            continue;
        }

        // 无序列表项
        if let Some(cap) = RE_LIST.captures(line) {
            let m = cap.get(0).unwrap();
            let text = line[m.end()..].trim();
            if !text.is_empty() {
                let runs = parse_inline_md_to_runs(text);
                let mut para = Paragraph::new()
                    .add_run(Run::new().add_text("•  "));
                for run in runs {
                    para = para.add_run(run);
                }
                doc = doc.add_paragraph(para);
            }
            continue;
        }

        // 有序列表项
        if let Some(cap) = RE_ORDERED_LIST.captures(line) {
            let m = cap.get(0).unwrap();
            let prefix = line[..m.end()].to_string();
            let text = line[m.end()..].trim();
            if !text.is_empty() {
                let runs = parse_inline_md_to_runs(text);
                let mut para = Paragraph::new()
                    .add_run(Run::new().add_text(prefix));
                for run in runs {
                    para = para.add_run(run);
                }
                doc = doc.add_paragraph(para);
            }
            continue;
        }

        // 引用块
        if let Some(cap) = RE_BLOCKQUOTE.captures(line) {
            let m = cap.get(0).unwrap();
            let text = line[m.end()..].trim();
            if !text.is_empty() {
                let runs = parse_inline_md_to_runs(text);
                let mut para = Paragraph::new()
                    .add_run(Run::new().add_text("│ ").color("888888"));
                for run in runs {
                    para = para.add_run(run);
                }
                doc = doc.add_paragraph(para);
            }
            continue;
        }

        if RE_HR.is_match(line) {
            doc = doc.add_paragraph(Paragraph::new());
            continue;
        }

        // 图片嵌入：![]()
        if let Some(cap) = RE_IMAGE_SRC.captures(line) {
            let alt = &cap[1];
            let img_src = &cap[2];
            let full_path = base_dir.join(img_src);
            if let Ok(img_data) = std::fs::read(&full_path) {
                let (w_emu, h_emu) = get_img_emu_size(&img_data);
                let pic = Pic::new(&img_data).id("rId1").size(w_emu, h_emu);
                doc = doc.add_paragraph(
                    Paragraph::new().add_run(Run::new().add_image(pic)),
                );
            } else {
                doc = doc.add_paragraph(
                    Paragraph::new().add_run(Run::new().add_text(format!(
                        "【图片: {}】",
                        if alt.is_empty() { "未找到" } else { alt }
                    ))),
                );
            }
            continue;
        }

        // 普通段落：保留内联格式（加粗/斜体/删除线/代码/高亮）
        let runs = parse_inline_md_to_runs(line);
        // 去除格式标记后判断是否为空行
        let plain = RE_BOLD.replace_all(line, "$1");
        let plain = RE_ITALIC.replace_all(&plain, "$1");
        let plain = RE_STRIKETHROUGH.replace_all(&plain, "$1");
        let plain = RE_HIGHLIGHT.replace_all(&plain, "$1");
        let plain = RE_INLINE_CODE.replace_all(&plain, "$1");
        let plain = RE_IMAGE.replace_all(&plain, "");
        let plain = RE_LINK.replace_all(&plain, "$1");

        if !plain.trim().is_empty() && !runs.is_empty() {
            let mut para = Paragraph::new();
            for run in runs {
                para = para.add_run(run);
            }
            doc = doc.add_paragraph(para);
        } else {
            doc = doc.add_paragraph(Paragraph::new());
        }
    }

    // 文件末尾可能残留未刷新的表格
    if !table_buffer.is_empty() {
        doc = build_docx_table(doc, &table_buffer);
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    doc.build().pack(&mut buf).map_err(|e| format!("DOCX 生成失败: {}", e))?;
    Ok(buf.into_inner())
}

/// Markdown → DOCX（含图片嵌入）
pub fn md_to_docx(src: &Path, dst: &Path) -> Result<u64, String> {
    let content = std::fs::read_to_string(src)
        .map_err(|e| format!("无法读取源文件: {}", e))?;
    let base_dir = src.parent().unwrap_or(Path::new("."));
    let bytes = md_to_docx_bytes(&content, base_dir)?;
    let written = bytes.len() as u64;
    std::fs::write(dst, &bytes).map_err(|e| format!("写入失败: {}", e))?;
    Ok(written)
}

